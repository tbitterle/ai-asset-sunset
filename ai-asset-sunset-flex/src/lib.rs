// Copyright 2026 The ai-asset-sunset authors. Licensed under Apache-2.0.
// Community policy published via p4a.ai. NOT a MuleSoft product, NOT supported by MuleSoft.
//! # AI Asset Sunset
//!
//! Date-phased deprecation for agents (A2A), MCP servers, LLM routes and APIs on Omni Gateway:
//!
//! | phase      | window                       | behaviour                                              |
//! |------------|------------------------------|--------------------------------------------------------|
//! | active     | now < deprecationAt          | pass-through                                           |
//! | heads-up   | deprecationAt ≤ now < sunset | pass-through + RFC 9745/8594 headers + JSON metadata   |
//! | brownout   | configured windows in heads-up | tombstone (pre-announced)                            |
//! | tombstone  | sunsetAt ≤ now               | answered locally, upstream NOT called                  |
//!
//! Identity comes only from the PDK `Authentication` object (Client ID Enforcement, JWT, …
//! placed BEFORE this policy) — never from raw request headers.

mod generated;
pub(crate) mod notice;
pub(crate) mod phase;

#[cfg(test)]
mod tests;

use std::rc::Rc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use pdk::authentication::{Authentication, AuthenticationHandler};
use pdk::hl::timer::{Clock, Timer};
use pdk::hl::*;
use pdk::logger;
use pdk::policy_violation::PolicyViolations;
use serde_json::{json, Value};

use crate::generated::config::Config;
use crate::notice::{Proto, ReqInfo};
use crate::phase::{CallKind, Decision, Phase, Settings};

const ACK_HEADER: &str = "x-p4a-sunset-ack";

/// Carried from the request to the response filter (heads-up only).
#[derive(Clone, Default)]
pub struct Ctx {
    decorate: bool,
    phase: Option<Phase>,
    identified: bool,
    a2a: bool,
    /// JSON-RPC style asset (MCP/A2A): only these responses are buffered for body decoration.
    rpc: bool,
    agent_card: bool,
}

struct Caller {
    client_id: Option<String>,
    client_name: Option<String>,
    source: &'static str,
}

fn caller(auth: &Authentication) -> Caller {
    match auth.authentication() {
        Some(a) => {
            let source = if a.client_id.is_some() {
                "client-id"
            } else if a.principal.is_some() {
                "principal"
            } else {
                "none"
            };
            // A principal without a client_id (e.g. a JWT without client claim) identifies the
            // caller for logging, but exemptions only ever match on client_id.
            Caller { client_id: a.client_id, client_name: a.client_name.or(a.principal), source }
        }
        None => Caller { client_id: None, client_name: None, source: "none" },
    }
}

fn now_of(timer: &Timer) -> DateTime<Utc> {
    timer.now().into()
}

/// One structured evidence line per governed call. No bodies, no tokens.
fn evidence(s: &Settings, phase: Phase, outcome: &str, c: &Caller, method: &str, ack: Option<String>, extra: Option<&str>) {
    let line = json!({
        "phase": phase.as_str(),
        "outcome": outcome,
        "clientId": c.client_id,
        "clientName": c.client_name,
        "identitySource": c.source,
        "method": notice::header_safe(method, 64),
        "ack": ack.map(|a| notice::header_safe(&a, 128)),
        "detail": extra,
        "sunsetAt": s.sunset_at.to_rfc3339(),
    });
    if phase == Phase::PastSwitchOff {
        logger::warn!("[ai-asset-sunset] {line} -- switchOffAt has passed: remove this API instance");
    } else {
        logger::info!("[ai-asset-sunset] {line}");
    }
}

fn violation(s: &Settings, pv: &PolicyViolations, c: &Caller) {
    // Never overwrite a violation another policy already reported for this request
    // (only one violation per request survives — red-team R9).
    if !s.emit_violation || pv.policy_violation().is_some() {
        return;
    }
    match (&c.client_id, &c.client_name) {
        (Some(id), name) => pv.generate_policy_violation_for_client_app(name.clone().unwrap_or_default(), id.clone()),
        _ => pv.generate_policy_violation(),
    }
}

fn to_response(r: notice::Reply) -> Response {
    Response::new(r.status).with_headers(r.headers).with_body(r.body)
}

fn tombstone(s: &Settings, proto: Proto, phase: Phase, method: &str, body: Option<&Value>, identified: bool, now: DateTime<Utc>) -> Response {
    let n = notice::notice(s, phase, identified);
    let base = notice::deprecation_headers(s, phase, identified);
    let reply = match proto {
        Proto::Mcp => notice::mcp_tombstone(method, body, &n, s.mcp_tool_result, &base),
        Proto::A2aJsonRpcLegacy | Proto::A2aJsonRpcV1 | Proto::A2aHttpJson => notice::a2a_tombstone(proto, body, &n, &base),
        Proto::Llm => notice::llm_tombstone(body, &n, s.llm_assistant_message, now, &base),
        Proto::Http => notice::http_tombstone(&n, &base),
    };
    to_response(reply)
}

async fn request_filter(
    state: RequestState,
    auth: Authentication,
    s: Rc<Settings>,
    timer: &Timer,
    pv: &PolicyViolations,
) -> Flow<Ctx> {
    let now = now_of(timer);
    let phase = s.phase_at(now);
    if phase == Phase::Active {
        return Flow::Continue(Ctx::default());
    }
    let c = caller(&auth);
    let identified = c.client_id.is_some();

    // Decide the buffering strategy BEFORE touching the request: a policy that may answer
    // locally must buffer headers+body atomically, otherwise Envoy starts proxying upstream
    // in parallel and the upstream reply can win the race (pdk-stop-execution).
    if phase::may_tombstone(&s, phase, c.client_id.as_deref(), now) {
        let hb = state.into_headers_body_state().await;
        let h = hb.handler();
        let method = h.header(":method").unwrap_or_default();
        let path = h.header(":path").unwrap_or_default();
        let a2a_version = h.header("a2a-version");
        let mcp_header = h.header("mcp-protocol-version").is_some() || h.header("mcp-session-id").is_some();
        let ack = h.header(ACK_HEADER);
        let body: Option<Value> = if hb.contains_body() { serde_json::from_slice(&h.body()).ok() } else { None };
        let info = ReqInfo { method: &method, path: &path, a2a_version: a2a_version.as_deref(), mcp_header, body: body.as_ref() };
        let proto = notice::detect(&s.protocol, &info);
        // Task grace is a protocol feature: in 'auto' mode a caller could forge `a2a-version` on a
        // REST API to look like an A2A follow-up, so HTTP+JSON path heuristics only count when the
        // instance is explicitly configured as protocol 'a2a' (red-team R6).
        // Only on instances explicitly configured as an MCP server or agent: in 'auto' a JSON-RPC
        // body could be forged on a REST upstream to get the grace window (red-team R6b).
        let grace_allowed = match proto {
            Proto::A2aHttpJson => s.protocol == "a2a",
            _ => matches!(s.protocol.as_str(), "mcp" | "a2a"),
        };
        let kind = if grace_allowed && notice::is_task_follow_up(proto, &info) {
            CallKind::TaskFollowUp
        } else {
            CallKind::NewWork
        };
        let rpc = notice::jsonrpc_method(body.as_ref()).unwrap_or(method.clone());

        match phase::decide(&s, now, c.client_id.as_deref(), kind) {
            Decision::Tombstone { why } => {
                evidence(&s, phase, "tombstone", &c, &rpc, ack, Some(why));
                violation(&s, pv, &c);
                return Flow::Break(tombstone(&s, proto, phase, &method, body.as_ref(), identified, now));
            }
            Decision::HeadsUp { exempt, reason } => {
                let outcome = if exempt { "exempt" } else { "forwarded" };
                evidence(&s, phase, outcome, &c, &rpc, ack, reason.as_deref());
                if !exempt && reason.is_none() {
                    violation(&s, pv, &c);
                }
                let a2a = matches!(proto, Proto::A2aJsonRpcLegacy | Proto::A2aJsonRpcV1 | Proto::A2aHttpJson);
                return Flow::Continue(Ctx {
                    decorate: true,
                    phase: Some(phase),
                    identified,
                    a2a,
                    rpc: a2a || proto == Proto::Mcp,
                    agent_card: path.contains("/.well-known/agent"),
                });
            }
            Decision::Pass => return Flow::Continue(Ctx::default()),
        }
    }

    // Heads-up path: headers only, never buffer the request (no latency for streaming clients).
    let hs = state.into_headers_state().await;
    let h = hs.handler();
    let method = h.header(":method").unwrap_or_default();
    let path = h.header(":path").unwrap_or_default();
    let agent_card = path.contains("/.well-known/agent");
    let a2a = s.protocol == "a2a" || (s.protocol == "auto" && (h.header("a2a-version").is_some() || agent_card));
    let mcp = s.protocol == "mcp"
        || (s.protocol == "auto" && (h.header("mcp-protocol-version").is_some() || h.header("mcp-session-id").is_some()));
    let exempt = s.exemption_for(c.client_id.as_deref(), now).is_some();
    evidence(&s, phase, if exempt { "exempt" } else { "forwarded" }, &c, &method, h.header(ACK_HEADER), None);
    if !exempt {
        violation(&s, pv, &c);
    }
    Flow::Continue(Ctx { decorate: true, phase: Some(phase), identified, a2a, rpc: a2a || mcp, agent_card })
}

async fn response_filter(state: ResponseState, data: RequestData<Ctx>, s: Rc<Settings>) {
    let ctx = match data {
        RequestData::Continue(c) if c.decorate => c,
        _ => return,
    };
    let phase = ctx.phase.unwrap_or(Phase::HeadsUp);
    let hs = state.into_headers_state().await;
    let h = hs.handler();
    for (k, v) in notice::deprecation_headers(&s, phase, ctx.identified) {
        h.set_header(&k, &v);
    }
    if ctx.agent_card {
        // Clamp agent-card caching so the tombstone is seen soon after sunset.
        h.set_header("cache-control", &format!("max-age={}", s.cache_clamp_seconds));
    }

    // Only JSON-RPC assets are buffered; REST/LLM responses keep streaming untouched.
    if !ctx.rpc || !s.inject_body_metadata || !hs.contains_body() {
        return;
    }
    let ctype = h.header("content-type").unwrap_or_default().to_ascii_lowercase();
    if !ctype.starts_with("application/json") {
        return; // SSE / streams / anything else: headers only, never buffer.
    }
    let len = h.header("content-length").and_then(|v| v.parse::<usize>().ok());
    match len {
        Some(l) if l <= s.max_body_rewrite_bytes => {}
        _ => return, // unknown (chunked) or too large: headers only.
    }
    if h.header("content-encoding").map_or(false, |e| !e.eq_ignore_ascii_case("identity")) {
        return; // compressed body: do not touch.
    }
    // Headers cannot change after the body state; drop content-length first.
    h.remove_header("content-length");
    let bs = hs.into_body_state().await;
    let body = bs.handler().body();
    let proto = if ctx.a2a { Proto::A2aJsonRpcV1 } else { Proto::Mcp };
    let n = notice::notice(&s, phase, ctx.identified);
    if let Some(new_body) = notice::decorate_jsonrpc(&body, proto, &s, &n) {
        if let Err(e) = bs.handler().set_body(&new_body) {
            logger::warn!("[ai-asset-sunset] could not decorate response body: {e:?}");
        }
    }
}

#[entrypoint]
async fn configure(
    launcher: Launcher,
    Configuration(bytes): Configuration,
    clock: Clock,
    policy_violations: PolicyViolations,
) -> Result<()> {
    let config: Config = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow!("ai-asset-sunset: cannot parse configuration: {e}"))?;
    let timer = clock.period(Duration::from_secs(1));

    let settings = match Settings::from_config(&config) {
        Ok(s) => Rc::new(s),
        Err(errors) => {
            // Invalid timeline. Before sunset: fail OPEN (a typo must never take a live asset down).
            // If sunsetAt itself still parses and has passed: fail CLOSED with a minimal 410, so
            // breaking the config cannot be used to re-open a retired asset (red-team R3).
            let sunset = phase::parse_sunset_only(&config.sunset_at);
            logger::error!("[ai-asset-sunset] invalid configuration: {}", errors.join("; "));
            let filter = on_request(move |rs: RequestState| {
                let closed = sunset.map_or(false, |t| now_of(&timer) >= t);
                async move {
                    if !closed {
                        let _ = rs.into_headers_state().await;
                        return Flow::Continue(Ctx::default());
                    }
                    let _hb = rs.into_headers_body_state().await;
                    let body = br#"{"title":"Asset sunset","status":410}"#.to_vec();
                    Flow::Break(
                        Response::new(410)
                            .with_headers(vec![
                                ("content-type".to_string(), "application/problem+json".to_string()),
                                ("cache-control".to_string(), "no-store".to_string()),
                                ("content-length".to_string(), body.len().to_string()),
                            ])
                            .with_body(body),
                    )
                }
            });
            launcher.launch(filter).await?;
            return Ok(());
        }
    };

    let s_req = Rc::clone(&settings);
    let s_res = Rc::clone(&settings);
    let filter = on_request(|rs, auth| request_filter(rs, auth, Rc::clone(&s_req), &timer, &policy_violations))
        .on_response(|rs, data| response_filter(rs, data, Rc::clone(&s_res)));
    launcher.launch(filter).await?;
    Ok(())
}
