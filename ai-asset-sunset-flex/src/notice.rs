// Copyright 2026 The ai-asset-sunset authors. Licensed under Apache-2.0.
//! Pure builders for deprecation headers and protocol-aware tombstone responses.
//! No PDK types here — `lib.rs` turns `Reply` into a PDK `Response`.

use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};

use crate::phase::{Disclosure, Phase, Settings};

pub const META_KEY: &str = "ai.p4a/deprecation";
/// Custom JSON-RPC code: outside the JSON-RPC reserved range (-32768..-32000).
pub const JSONRPC_SUNSET_CODE: i64 = -31001;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proto {
    Mcp,
    A2aJsonRpcLegacy,
    A2aJsonRpcV1,
    A2aHttpJson,
    Llm,
    Http,
}

pub struct Reply {
    pub status: u32,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Header values: visible ASCII only (no CR/LF, no non-ASCII), capped.
pub fn header_safe(v: &str, max: usize) -> String {
    v.chars()
        .map(|c| if (' '..='~').contains(&c) { c } else { ' ' })
        .take(max)
        .collect::<String>()
        .trim()
        .to_string()
}

fn http_date(t: DateTime<Utc>) -> String {
    t.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

/// RFC 9745 `Deprecation: @<epoch>`, RFC 8594 `Sunset: <HTTP-date>`, Link relations.
pub fn deprecation_headers(s: &Settings, phase: Phase) -> Vec<(String, String)> {
    let mut h = vec![
        ("deprecation".to_string(), format!("@{}", s.deprecation_at.timestamp())),
        ("sunset".to_string(), http_date(s.sunset_at)),
        ("x-p4a-sunset-phase".to_string(), phase.as_str().to_string()),
        ("x-p4a-sunset-notice".to_string(), header_safe(&s.message, 300)),
    ];
    let mut links = Vec::new();
    if !s.docs_url.is_empty() {
        links.push(format!("<{}>; rel=\"deprecation\"", header_safe(&s.docs_url, 400)));
    }
    if !s.successor_url.is_empty() {
        links.push(format!("<{}>; rel=\"successor-version\"", header_safe(&s.successor_url, 400)));
    }
    if !links.is_empty() {
        h.push(("link".to_string(), links.join(", ")));
    }
    h
}

/// The machine-readable deprecation object (shared by every protocol).
pub fn notice(s: &Settings, phase: Phase, identified: bool) -> Value {
    let show_details = match s.disclosure {
        Disclosure::Full => true,
        Disclosure::Standard => identified,
        Disclosure::Minimal => false,
    };
    let mut m = Map::new();
    m.insert("status".into(), json!(match phase {
        Phase::HeadsUp | Phase::Active => "deprecated",
        Phase::Brownout => "brownout",
        _ => "sunset",
    }));
    m.insert("phase".into(), json!(phase.as_str()));
    m.insert("message".into(), json!(s.message));
    m.insert("deprecationAt".into(), json!(s.deprecation_at.to_rfc3339()));
    m.insert("sunsetAt".into(), json!(s.sunset_at.to_rfc3339()));
    m.insert("switchOffAt".into(), json!(s.switch_off_at.to_rfc3339()));
    if !s.docs_url.is_empty() {
        m.insert("docsUrl".into(), json!(s.docs_url));
    }
    if show_details {
        if !s.successor_name.is_empty() || !s.successor_url.is_empty() {
            m.insert("successor".into(), json!({"name": s.successor_name, "url": s.successor_url}));
        }
        if !s.contact_name.is_empty() || !s.contact_email.is_empty() {
            m.insert("contact".into(), json!({"name": s.contact_name, "email": s.contact_email}));
        }
    }
    Value::Object(m)
}

/// One plain-text sentence an LLM can act on.
pub fn notice_text(n: &Value) -> String {
    let mut t = format!(
        "[DEPRECATED SERVICE] {} Sunset: {}.",
        n["message"].as_str().unwrap_or(""),
        n["sunsetAt"].as_str().unwrap_or("")
    );
    if let Some(name) = n["successor"]["name"].as_str().filter(|v| !v.is_empty()) {
        t.push_str(&format!(" Use the replacement '{name}'"));
        if let Some(url) = n["successor"]["url"].as_str().filter(|v| !v.is_empty()) {
            t.push_str(&format!(" ({url})"));
        }
        t.push('.');
    }
    if let Some(url) = n["docsUrl"].as_str() {
        t.push_str(&format!(" Details: {url}"));
    }
    if let Some(mail) = n["contact"]["email"].as_str().filter(|v| !v.is_empty()) {
        t.push_str(&format!(" Contact: {mail}"));
    }
    t
}

// ---------------------------------------------------------------- detection

pub struct ReqInfo<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub a2a_version: Option<&'a str>,
    pub mcp_header: bool,
    pub body: Option<&'a Value>,
}

const LLM_PATHS: [&str; 5] = ["/chat/completions", "/completions", "/responses", "/messages", "/embeddings"];

pub fn detect(configured: &str, r: &ReqInfo) -> Proto {
    let is_jsonrpc = r.body.map_or(false, |b| {
        b.get("jsonrpc").is_some() || b.as_array().map_or(false, |a| a.iter().any(|e| e.get("jsonrpc").is_some()))
    });
    let a2a = |r: &ReqInfo| {
        if is_jsonrpc {
            if r.a2a_version.is_some() { Proto::A2aJsonRpcV1 } else { Proto::A2aJsonRpcLegacy }
        } else {
            Proto::A2aHttpJson
        }
    };
    match configured {
        "mcp" => Proto::Mcp,
        "a2a" => a2a(r),
        "llm" => Proto::Llm,
        "http" => Proto::Http,
        _ => {
            if r.a2a_version.is_some() || r.path.contains("/.well-known/agent") {
                return a2a(r);
            }
            if r.mcp_header {
                return Proto::Mcp;
            }
            if is_jsonrpc {
                let m = jsonrpc_method(r.body).unwrap_or_default();
                if m.starts_with("message/") || m == "SendMessage" || m == "SendStreamingMessage" {
                    return a2a(r);
                }
                return Proto::Mcp;
            }
            if r.method == "POST" && LLM_PATHS.iter().any(|p| r.path.ends_with(p) || r.path.contains(&format!("{p}?"))) {
                return Proto::Llm;
            }
            Proto::Http
        }
    }
}

pub fn jsonrpc_method(body: Option<&Value>) -> Option<String> {
    body.and_then(|b| b.get("method")).and_then(Value::as_str).map(str::to_string)
}

/// Follow-ups on already running tasks. Only meaningful for A2A / MCP (red-team R6):
/// explicit allow-list of read/cancel/resubscribe methods — no prefix matching, no batches,
/// and path heuristics only for A2A HTTP+JSON requests without a JSON-RPC body.
pub fn is_task_follow_up(proto: Proto, r: &ReqInfo) -> bool {
    const FOLLOW_UPS: [&str; 9] = [
        "tasks/get", "tasks/cancel", "tasks/resubscribe", "tasks/result",
        "GetTask", "CancelTask", "SubscribeToTask",
        "GetTaskPushNotificationConfig", "ListTaskPushNotificationConfig",
    ];
    match proto {
        Proto::Mcp | Proto::A2aJsonRpcLegacy | Proto::A2aJsonRpcV1 => match r.body {
            Some(Value::Object(_)) => jsonrpc_method(r.body).map_or(false, |m| FOLLOW_UPS.contains(&m.as_str())),
            _ => false, // batches and non-JSON bodies never get grace
        },
        Proto::A2aHttpJson => {
            r.body.is_none()
                && (r.method == "GET" || (r.method == "POST" && r.path.contains(":cancel")))
                && r.path.contains("/tasks/")
        }
        Proto::Llm | Proto::Http => false,
    }
}

// ---------------------------------------------------------------- replies

fn json_reply(status: u32, ctype: &str, body: Value, base: &[(String, String)]) -> Reply {
    let bytes = body.to_string().into_bytes();
    let mut headers = base.to_vec();
    headers.push(("content-type".into(), ctype.into()));
    headers.push(("cache-control".into(), "no-store".into()));
    headers.push(("content-length".into(), bytes.len().to_string()));
    Reply { status, headers, body: bytes }
}

fn mcp_single(req: &Value, n: &Value, tool_result: bool) -> Option<Value> {
    let id = req.get("id")?.clone(); // notifications (no id) never get an answer
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let text = notice_text(n);
    let meta = json!({ META_KEY: n });
    let result = match method {
        "initialize" => Some(json!({
            "protocolVersion": req["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "sunset-notice", "version": "1.0.0"},
            "instructions": text,
            "_meta": meta
        })),
        // MCP 2026-07-28 stateless discovery. Shape is best effort; verify against your clients.
        "server/discover" => Some(json!({
            "capabilities": {"tools": {}},
            "instructions": text,
            "_meta": meta
        })),
        "ping" => Some(json!({})),
        "tools/list" => Some(json!({
            "tools": [{
                "name": "get_deprecation_info",
                "description": text,
                "inputSchema": {"type": "object", "properties": {}}
            }],
            "_meta": meta
        })),
        "tools/call" if tool_result => Some(json!({
            "content": [{"type": "text", "text": text}],
            "structuredContent": n,
            "isError": req["params"]["name"].as_str() != Some("get_deprecation_info"),
            "_meta": meta
        })),
        "resources/list" => Some(json!({"resources": [], "_meta": meta})),
        "prompts/list" => Some(json!({"prompts": [], "_meta": meta})),
        _ => None,
    };
    Some(match result {
        Some(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        None => json!({"jsonrpc": "2.0", "id": id,
                       "error": {"code": JSONRPC_SUNSET_CODE, "message": text, "data": n}}),
    })
}

pub fn mcp_tombstone(method: &str, body: Option<&Value>, n: &Value, tool_result: bool, base: &[(String, String)]) -> Reply {
    // Streamable HTTP: GET (server stream) / DELETE (session end) on a retired server -> 405,
    // which MCP clients handle cleanly. Deprecation headers are still attached.
    if method == "GET" || method == "DELETE" {
        let mut headers = base.to_vec();
        headers.push(("allow".into(), "POST".into()));
        headers.push(("cache-control".into(), "no-store".into()));
        headers.push(("content-length".into(), "0".into()));
        return Reply { status: 405, headers, body: Vec::new() };
    }
    let answer = match body {
        Some(Value::Array(items)) => {
            let v: Vec<Value> = items.iter().filter_map(|r| mcp_single(r, n, tool_result)).collect();
            if v.is_empty() { None } else { Some(Value::Array(v)) }
        }
        Some(r @ Value::Object(_)) => mcp_single(r, n, tool_result),
        // Not JSON-RPC (e.g. GET for a legacy SSE stream): JSON-RPC error with null id,
        // never a bare 4xx (bare 4xx makes dual-era clients fall back to the legacy transport).
        _ => Some(json!({"jsonrpc": "2.0", "id": null,
                         "error": {"code": JSONRPC_SUNSET_CODE, "message": notice_text(n), "data": n}})),
    };
    match answer {
        Some(v) => json_reply(200, "application/json", v, base),
        None => {
            let mut headers = base.to_vec();
            headers.push(("content-length".into(), "0".into()));
            Reply { status: 202, headers, body: Vec::new() }
        }
    }
}

fn error_info(n: &Value) -> Value {
    let mut md = Map::new();
    for k in ["sunsetAt", "switchOffAt", "docsUrl"] {
        if let Some(v) = n[k].as_str() {
            md.insert(k.into(), json!(v));
        }
    }
    if let Some(v) = n["successor"]["name"].as_str() {
        md.insert("successorName".into(), json!(v));
    }
    if let Some(v) = n["successor"]["url"].as_str() {
        md.insert("successorUrl".into(), json!(v));
    }
    json!({"@type": "type.googleapis.com/google.rpc.ErrorInfo",
           "reason": "AGENT_SUNSET", "domain": "p4a.ai", "metadata": md})
}

pub fn a2a_tombstone(proto: Proto, body: Option<&Value>, n: &Value, base: &[(String, String)]) -> Reply {
    let id = body.and_then(|b| b.get("id")).cloned().unwrap_or(Value::Null);
    let text = notice_text(n);
    match proto {
        Proto::A2aJsonRpcLegacy => json_reply(200, "application/json",
            json!({"jsonrpc": "2.0", "id": id,
                   "error": {"code": JSONRPC_SUNSET_CODE, "message": text, "data": n.clone()}}), base),
        Proto::A2aJsonRpcV1 => json_reply(200, "application/json",
            json!({"jsonrpc": "2.0", "id": id,
                   "error": {"code": JSONRPC_SUNSET_CODE, "message": text, "data": [error_info(n)]}}), base),
        _ => json_reply(410, "application/json",
            json!({"error": {"code": 410, "message": text, "details": [error_info(n)]}}), base),
    }
}

pub fn llm_tombstone(body: Option<&Value>, n: &Value, assistant_message: bool, now: DateTime<Utc>, base: &[(String, String)]) -> Reply {
    let text = notice_text(n);
    let streaming = body.and_then(|b| b.get("stream")).and_then(Value::as_bool).unwrap_or(false);
    if assistant_message && !streaming {
        let model = body.and_then(|b| b.get("model")).and_then(Value::as_str).unwrap_or("deprecated");
        return json_reply(200, "application/json", json!({
            "id": "sunset-notice", "object": "chat.completion", "created": now.timestamp(), "model": model,
            "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
            "p4a_deprecation": n
        }), base);
    }
    json_reply(410, "application/json", json!({
        "error": {"message": text, "type": "model_sunset", "code": "asset_sunset", "param": null},
        "p4a_deprecation": n
    }), base)
}

/// RFC 9457 problem details.
pub fn http_tombstone(n: &Value, base: &[(String, String)]) -> Reply {
    let mut p = json!({
        "type": n["docsUrl"].as_str().unwrap_or("about:blank"),
        "title": "Asset sunset",
        "status": 410,
        "detail": n["message"],
        "sunset": n["sunsetAt"],
        "switchOff": n["switchOffAt"]
    });
    if let Some(s) = n.get("successor") {
        p["successor"] = s.clone();
    }
    if let Some(c) = n.get("contact") {
        p["contact"] = c.clone();
    }
    json_reply(410, "application/problem+json", p, base)
}

// ---------------------------------------------------------------- heads-up body decoration

/// Decorates a buffered JSON-RPC response in heads-up. Returns None when nothing changed.
pub fn decorate_jsonrpc(body: &[u8], proto_hint: Proto, s: &Settings, n: &Value) -> Option<Vec<u8>> {
    let mut v: Value = serde_json::from_slice(body).ok()?;
    let changed = match &mut v {
        Value::Array(items) => items.iter_mut().fold(false, |acc, i| decorate_one(i, proto_hint, s, n) || acc),
        one => decorate_one(one, proto_hint, s, n),
    };
    if changed { Some(v.to_string().into_bytes()) } else { None }
}

fn decorate_one(msg: &mut Value, proto_hint: Proto, s: &Settings, n: &Value) -> bool {
    if msg.get("jsonrpc").is_none() {
        return false;
    }
    let result = match msg.get_mut("result") {
        Some(Value::Object(r)) => r,
        _ => return false,
    };
    let a2a = matches!(proto_hint, Proto::A2aJsonRpcLegacy | Proto::A2aJsonRpcV1);
    let field = if a2a { "metadata" } else { "_meta" };
    let slot = result.entry(field.to_string()).or_insert_with(|| json!({}));
    if let Value::Object(o) = slot {
        o.insert(META_KEY.into(), n.clone());
    }
    if !a2a {
        // Clamp tools/list cache lifetime so a later phase change propagates (MCP 2026-07-28 ttlMs).
        if let Some(ttl) = result.get_mut("ttlMs") {
            let max = s.cache_clamp_seconds.saturating_mul(1000);
            if ttl.as_i64().map_or(false, |t| t > max) {
                *ttl = json!(max);
            }
        }
        if s.annotate_tool_descriptions {
            if let Some(Value::Array(tools)) = result.get_mut("tools") {
                let suffix = format!(" {}", notice_text(n));
                for t in tools.iter_mut() {
                    if let Some(o) = t.as_object_mut() {
                        let d = o.get("description").and_then(Value::as_str).unwrap_or("").to_string();
                        if !d.contains("[DEPRECATED SERVICE]") {
                            o.insert("description".into(), json!(format!("{d}{suffix}").trim().to_string()));
                        }
                    }
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::config::Config;

    fn settings(disclosure: &str) -> Settings {
        let c: Config = serde_json::from_value(json!({
            "deprecationAt": "2026-12-01T00:00:00Z",
            "sunsetAt": "2027-01-01T00:00:00Z",
            "switchOffAt": "2027-01-15T00:00:00Z",
            "message": "Orders agent v1 is retired.",
            "successorName": "orders-agent-v2",
            "successorUrl": "https://gw.example.com/orders-v2",
            "docsUrl": "https://example.com/sunset/orders-v1",
            "contactEmail": "ai-platform@example.com",
            "disclosure": disclosure
        })).unwrap();
        Settings::from_config(&c).unwrap()
    }

    #[test]
    fn headers_follow_rfcs() {
        let s = settings("standard");
        let h = deprecation_headers(&s, Phase::HeadsUp);
        let get = |k: &str| h.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap();
        assert_eq!(get("deprecation"), format!("@{}", s.deprecation_at.timestamp()));
        assert_eq!(get("sunset"), "Fri, 01 Jan 2027 00:00:00 GMT");
        assert!(get("link").contains("rel=\"deprecation\""));
        assert!(get("link").contains("rel=\"successor-version\""));
    }

    #[test]
    fn disclosure_hides_contact_from_anonymous() {
        let s = settings("standard");
        assert!(notice(&s, Phase::Tombstone, false).get("contact").is_none());
        assert!(notice(&s, Phase::Tombstone, true).get("contact").is_some());
        assert!(notice(&settings("minimal"), Phase::Tombstone, true).get("successor").is_none());
        assert!(notice(&settings("full"), Phase::Tombstone, false).get("contact").is_some());
    }

    #[test]
    fn mcp_tools_call_is_llm_visible_tool_error() {
        let s = settings("standard");
        let n = notice(&s, Phase::Tombstone, true);
        let req = json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"get_order"}});
        let r = mcp_tombstone("POST", Some(&req), &n, true, &[]);
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(v["id"], 7);
        assert_eq!(v["result"]["isError"], true);
        assert!(v["result"]["content"][0]["text"].as_str().unwrap().contains("orders-agent-v2"));
    }

    #[test]
    fn mcp_notification_gets_202_and_unknown_method_custom_code() {
        let s = settings("standard");
        let n = notice(&s, Phase::Tombstone, false);
        let note = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        assert_eq!(mcp_tombstone("POST", Some(&note), &n, true, &[]).status, 202);
        let req = json!({"jsonrpc":"2.0","id":"a","method":"resources/read"});
        let v: Value = serde_json::from_slice(&mcp_tombstone("POST", Some(&req), &n, true, &[]).body).unwrap();
        assert_eq!(v["error"]["code"], JSONRPC_SUNSET_CODE);
    }

    #[test]
    fn a2a_v1_uses_error_info() {
        let s = settings("standard");
        let n = notice(&s, Phase::Tombstone, true);
        let req = json!({"jsonrpc":"2.0","id":1,"method":"SendMessage"});
        let v: Value = serde_json::from_slice(&a2a_tombstone(Proto::A2aJsonRpcV1, Some(&req), &n, &[]).body).unwrap();
        assert_eq!(v["error"]["data"][0]["reason"], "AGENT_SUNSET");
        assert_eq!(a2a_tombstone(Proto::A2aHttpJson, None, &n, &[]).status, 410);
    }

    #[test]
    fn llm_default_is_error_and_stream_never_gets_fake_message() {
        let s = settings("standard");
        let n = notice(&s, Phase::Tombstone, true);
        let now = s.sunset_at;
        assert_eq!(llm_tombstone(None, &n, false, now, &[]).status, 410);
        let stream = json!({"model":"m","stream":true});
        assert_eq!(llm_tombstone(Some(&stream), &n, true, now, &[]).status, 410);
        assert_eq!(llm_tombstone(Some(&json!({"model":"m"})), &n, true, now, &[]).status, 200);
    }

    #[test]
    fn detection() {
        let mcp = json!({"jsonrpc":"2.0","id":1,"method":"tools/call"});
        let a2a = json!({"jsonrpc":"2.0","id":1,"method":"message/send"});
        fn r<'a>(b: Option<&'a Value>, path: &'a str, a2a_v: Option<&'a str>) -> ReqInfo<'a> {
            ReqInfo { method: "POST", path, a2a_version: a2a_v, mcp_header: false, body: b }
        }
        assert_eq!(detect("auto", &r(Some(&mcp), "/mcp", None)), Proto::Mcp);
        assert_eq!(detect("auto", &r(Some(&a2a), "/", None)), Proto::A2aJsonRpcLegacy);
        assert_eq!(detect("auto", &r(Some(&a2a), "/", Some("1.0"))), Proto::A2aJsonRpcV1);
        assert_eq!(detect("auto", &r(None, "/v1/chat/completions", None)), Proto::Llm);
        assert_eq!(detect("auto", &r(None, "/orders", None)), Proto::Http);
    }

    #[test]
    fn task_follow_up_allow_list_is_strict() {
        let get = json!({"jsonrpc":"2.0","id":1,"method":"tasks/get"});
        let send = json!({"jsonrpc":"2.0","id":1,"method":"tasks/sendSubscribe"}); // legacy "new work"
        let batch = json!([{"jsonrpc":"2.0","id":1,"method":"tasks/get"}]);
        fn r<'a>(b: Option<&'a Value>) -> ReqInfo<'a> {
            ReqInfo { method: "POST", path: "/", a2a_version: None, mcp_header: false, body: b }
        }
        assert!(is_task_follow_up(Proto::A2aJsonRpcLegacy, &r(Some(&get))));
        assert!(!is_task_follow_up(Proto::A2aJsonRpcLegacy, &r(Some(&send))), "prefix tasks/ must not grant grace");
        assert!(!is_task_follow_up(Proto::A2aJsonRpcLegacy, &r(Some(&batch))), "batches never get grace");
        let rest = ReqInfo { method: "GET", path: "/v1/tasks/abc", a2a_version: None, mcp_header: false, body: None };
        assert!(!is_task_follow_up(Proto::Http, &rest));
    }

    #[test]
    fn heads_up_decoration_and_ttl_clamp() {
        let s = settings("standard");
        let n = notice(&s, Phase::HeadsUp, true);
        let body = br#"{"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"a","description":"A"}],"ttlMs":86400000}}"#;
        let out: Value = serde_json::from_slice(&decorate_jsonrpc(body, Proto::Mcp, &s, &n).unwrap()).unwrap();
        assert!(out["result"]["_meta"][META_KEY].is_object());
        assert_eq!(out["result"]["ttlMs"], 3_600_000);
        let a2a = br#"{"jsonrpc":"2.0","id":1,"result":{"kind":"task","id":"t1"}}"#;
        let out: Value = serde_json::from_slice(&decorate_jsonrpc(a2a, Proto::A2aJsonRpcV1, &s, &n).unwrap()).unwrap();
        assert!(out["result"]["metadata"][META_KEY].is_object());
        assert!(decorate_jsonrpc(br#"{"ok":true}"#, Proto::Http, &s, &n).is_none());
    }
}
