// Copyright 2026 The ai-asset-sunset authors. Licensed under Apache-2.0.
//! pdk-unit lifecycle tests (no Envoy, no Docker). Pure phase/notice logic is tested in
//! `phase.rs` / `notice.rs`; these tests prove the PDK wiring: buffering, Flow::Break,
//! upstream not called, response decoration, local mode.
//!
//! VERIFIED 2026-11-12 (pdk-unit 1.10.0, host/implementation.rs): the unit clock starts at
//! the UNIX epoch (1970-01-01T00:00:00Z). Test dates therefore bracket 1970: PAST = 1969,
//! FUTURE = 1990. Use `tester.sleep(..)` to advance across a boundary.

mod tests {
    use std::rc::Rc;

    use pdk_unit::{TraceBackend, UnitHttpMessage, UnitHttpRequest, UnitHttpResponse, UnitTestBuilder};
    use serde_json::{json, Value};

    const PAST: &str = "1969-01-01T00:00:00Z";
    const PAST2: &str = "1969-06-01T00:00:00Z";
    const FUTURE: &str = "1990-01-01T00:00:00Z";
    const FUTURE2: &str = "1991-01-01T00:00:00Z";

    fn cfg(dep: &str, sun: &str, off: &str, extra: Value) -> String {
        let mut c = json!({
            "deprecationAt": dep, "sunsetAt": sun, "switchOffAt": off,
            "message": "Orders agent v1 is retired.",
            "successorName": "orders-agent-v2",
            "docsUrl": "https://example.com/sunset/orders-v1"
        });
        if let (Some(o), Some(e)) = (c.as_object_mut(), extra.as_object()) {
            for (k, v) in e {
                o.insert(k.clone(), v.clone());
            }
        }
        c.to_string()
    }

    fn heads_up() -> String { cfg(PAST, FUTURE, FUTURE2, json!({})) }
    fn tombstone() -> String { cfg(PAST, PAST2, FUTURE, json!({})) }

    fn upstream_json(body: &str) -> Rc<TraceBackend<UnitHttpResponse>> {
        Rc::new(TraceBackend::new(
            UnitHttpResponse::new(200)
                .with_header("content-type", "application/json")
                .with_header("content-length", &body.len().to_string())
                .with_body(body.to_string()),
        ))
    }

    fn mcp(method: &str) -> UnitHttpRequest {
        UnitHttpRequest::post()
            .with_path("/mcp")
            .with_header("content-type", "application/json")
            .with_header("mcp-protocol-version", "2025-06-18")
            .with_body(json!({"jsonrpc":"2.0","id":42,"method":method,"params":{"name":"get_order"}}).to_string())
    }

    fn body_json(r: &UnitHttpResponse) -> Value {
        serde_json::from_slice(r.body()).expect("json body")
    }

    // T01 — before deprecationAt nothing changes
    #[test]
    fn t01_active_is_pass_through() {
        let backend = upstream_json(r#"{"ok":true}"#);
        let mut t = UnitTestBuilder::default()
            .with_config(cfg(FUTURE, FUTURE2, "1992-01-01T00:00:00Z", json!({})))
            .with_backend(Rc::clone(&backend))
            .with_entrypoint(crate::configure);
        let r = t.request(UnitHttpRequest::get().with_path("/orders"));
        assert_eq!(r.status_code(), 200);
        assert!(r.header("deprecation").is_none());
        assert!(backend.next().is_some(), "upstream must be called");
    }

    // T02 — heads-up adds RFC 9745 / 8594 headers and still calls upstream
    #[test]
    fn t02_heads_up_headers_and_upstream_called() {
        let backend = upstream_json(r#"{"ok":true}"#);
        let mut t = UnitTestBuilder::default()
            .with_config(heads_up())
            .with_backend(Rc::clone(&backend))
            .with_entrypoint(crate::configure);
        let r = t.request(UnitHttpRequest::get().with_path("/orders"));
        assert_eq!(r.status_code(), 200);
        assert!(r.header("deprecation").unwrap().starts_with('@'));
        assert_eq!(r.header("sunset").unwrap(), "Mon, 01 Jan 1990 00:00:00 GMT");
        assert!(r.header("link").unwrap().contains("rel=\"deprecation\""));
        assert_eq!(r.header("x-p4a-sunset-phase").unwrap(), "heads-up");
        let upstream_req = backend.next().expect("upstream must be called");
        // pdk-unit 1.10 exposes request-phase properties (violation) on the forwarded request.
        assert!(upstream_req.violation().is_some(), "deprecated call must be reported as violation");
    }

    // T03 — heads-up injects _meta into a buffered MCP JSON response
    #[test]
    fn t03_heads_up_mcp_meta() {
        let up = r#"{"jsonrpc":"2.0","id":42,"result":{"content":[{"type":"text","text":"ok"}]}}"#;
        let mut t = UnitTestBuilder::default()
            .with_config(heads_up())
            .with_backend(upstream_json(up))
            .with_entrypoint(crate::configure);
        let r = t.request(mcp("tools/call"));
        let v = body_json(&r);
        assert_eq!(v["result"]["_meta"]["ai.p4a/deprecation"]["status"], "deprecated");
        assert_eq!(v["result"]["content"][0]["text"], "ok", "original result untouched");
    }

    // T04 — SSE responses are never buffered or rewritten (headers only)
    #[test]
    fn t04_heads_up_sse_headers_only() {
        let sse = "data: {\"x\":1}\n\n";
        let mut t = UnitTestBuilder::default()
            .with_config(heads_up())
            .with_backend(Rc::new(TraceBackend::new(
                UnitHttpResponse::new(200).with_header("content-type", "text/event-stream").with_body(sse),
            )))
            .with_entrypoint(crate::configure);
        let r = t.request(UnitHttpRequest::post().with_path("/v1/chat/completions").with_body(r#"{"stream":true}"#));
        assert_eq!(r.body(), sse.as_bytes());
        assert!(r.header("sunset").is_some());
    }

    // T05 — tombstone: MCP tools/call answered locally as LLM-visible tool error, upstream NOT called
    #[test]
    fn t05_tombstone_mcp_tool_result() {
        let backend = upstream_json("{}");
        let mut t = UnitTestBuilder::default()
            .with_config(tombstone())
            .with_backend(Rc::clone(&backend))
            .with_entrypoint(crate::configure);
        let r = t.request(mcp("tools/call"));
        assert_eq!(r.status_code(), 200);
        let v = body_json(&r);
        assert_eq!(v["id"], 42);
        assert_eq!(v["result"]["isError"], true);
        assert_eq!(r.header("cache-control").unwrap(), "no-store");
        assert!(backend.next().is_none(), "upstream must NOT be called in tombstone");
    }

    // T06 — tombstone: MCP tools/list exposes only get_deprecation_info
    #[test]
    fn t06_tombstone_mcp_tools_list() {
        let mut t = UnitTestBuilder::default().with_config(tombstone()).with_entrypoint(crate::configure);
        let v = body_json(&t.request(mcp("tools/list")));
        assert_eq!(v["result"]["tools"][0]["name"], "get_deprecation_info");
    }

    // T07 — tombstone: MCP notification gets 202 with empty body
    #[test]
    fn t07_tombstone_mcp_notification() {
        let mut t = UnitTestBuilder::default().with_config(tombstone()).with_entrypoint(crate::configure);
        let r = t.request(
            UnitHttpRequest::post().with_path("/mcp").with_header("content-type", "application/json")
                .with_header("mcp-protocol-version", "2025-06-18")
                .with_body(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
        );
        assert_eq!(r.status_code(), 202);
        assert!(r.body().is_empty());
    }

    // T08 — tombstone: A2A v1 JSON-RPC error with ErrorInfo AGENT_SUNSET
    #[test]
    fn t08_tombstone_a2a_v1() {
        let mut t = UnitTestBuilder::default().with_config(tombstone()).with_entrypoint(crate::configure);
        let r = t.request(
            UnitHttpRequest::post().with_path("/").with_header("content-type", "application/json")
                .with_header("a2a-version", "1.0")
                .with_body(r#"{"jsonrpc":"2.0","id":"r1","method":"SendMessage","params":{}}"#),
        );
        let v = body_json(&r);
        assert_eq!(v["error"]["data"][0]["reason"], "AGENT_SUNSET");
        assert_eq!(v["id"], "r1");
    }

    // T09 — tombstone: LLM route returns 410 OpenAI-compatible error
    #[test]
    fn t09_tombstone_llm() {
        let mut t = UnitTestBuilder::default().with_config(tombstone()).with_entrypoint(crate::configure);
        let r = t.request(
            UnitHttpRequest::post().with_path("/v1/chat/completions").with_header("content-type", "application/json")
                .with_body(r#"{"model":"gpt-x","messages":[]}"#),
        );
        assert_eq!(r.status_code(), 410);
        assert_eq!(body_json(&r)["error"]["code"], "asset_sunset");
    }

    // T10 — tombstone: plain REST returns RFC 9457 problem+json
    #[test]
    fn t10_tombstone_rest_problem_json() {
        let mut t = UnitTestBuilder::default()
            .with_config(cfg(PAST, PAST2, FUTURE, json!({"contactEmail": "ai-platform@example.com"}))).with_entrypoint(crate::configure);
        let r = t.request(UnitHttpRequest::get().with_path("/orders/1"));
        assert_eq!(r.status_code(), 410);
        assert_eq!(r.header("content-type").unwrap(), "application/problem+json");
        assert!(body_json(&r).get("contact").is_none(), "anonymous caller must not see contact");
    }

    // T11 — invalid timeline before sunset: fail open
    #[test]
    fn t11_invalid_config_fails_open() {
        let backend = upstream_json("{}");
        let mut t = UnitTestBuilder::default()
            .with_config(cfg("not-a-date", FUTURE, FUTURE2, json!({})))
            .with_backend(Rc::clone(&backend))
            .with_entrypoint(crate::configure);
        assert_eq!(t.request(UnitHttpRequest::get().with_path("/")).status_code(), 200);
        assert!(backend.next().is_some());
    }

    // T12 — invalid config AFTER sunset: fail closed (cannot re-open a retired asset by breaking config)
    #[test]
    fn t12_invalid_config_after_sunset_fails_closed() {
        let backend = upstream_json("{}");
        let mut t = UnitTestBuilder::default()
            .with_config(cfg("not-a-date", PAST2, FUTURE, json!({})))
            .with_backend(Rc::clone(&backend))
            .with_entrypoint(crate::configure);
        assert_eq!(t.request(UnitHttpRequest::get().with_path("/")).status_code(), 410);
        assert!(backend.next().is_none());
    }

    // T13 — spoofed identity header does NOT grant an exemption
    #[test]
    fn t13_raw_client_id_header_is_not_identity() {
        let mut t = UnitTestBuilder::default()
            .with_config(cfg(PAST, PAST2, FUTURE, json!({"exemptions":[{"clientId":"payroll","until":FUTURE}]})))
            .with_entrypoint(crate::configure);
        let r = t.request(UnitHttpRequest::get().with_path("/").with_header("client_id", "payroll")
            .with_header("x-client-id", "payroll"));
        assert_eq!(r.status_code(), 410, "exemptions only via the PDK Authentication object");
    }

    // T14 — local mode (no control plane) still works
    #[test]
    fn t14_local_mode() {
        let mut t = UnitTestBuilder::default()
            .local_mode()
            .with_config(tombstone())
            .with_entrypoint(crate::configure);
        assert_eq!(t.request(UnitHttpRequest::get().with_path("/")).status_code(), 410);
    }

    // T15 — CR/LF in config never reaches a header
    #[test]
    fn t15_header_injection_from_config() {
        let mut t = UnitTestBuilder::default()
            .with_config(cfg(PAST, FUTURE, FUTURE2, json!({"message":"bye\r\nset-cookie: pwn=1"})))
            .with_backend(upstream_json("{}"))
            .with_entrypoint(crate::configure);
        let r = t.request(UnitHttpRequest::get().with_path("/"));
        assert!(r.header("set-cookie").is_none());
        assert!(!r.header("x-p4a-sunset-notice").unwrap().contains('\n'));
    }

    // T20 — heads-up: successor-version Link only for identified callers (disclosure: standard)
    #[test]
    fn t20_successor_link_only_for_identified_callers() {
        use pdk::authentication::AuthenticationData;
        let conf = cfg(PAST, FUTURE, FUTURE2, json!({"successorUrl": "https://gw.example.com/orders-v2"}));
        let mut anon = UnitTestBuilder::default().with_config(conf.clone()).with_backend(upstream_json("{}")).with_entrypoint(crate::configure);
        let r = anon.request(UnitHttpRequest::get().with_path("/orders"));
        let link = r.header("link").unwrap_or_default();
        assert!(link.contains("rel=\"deprecation\""));
        assert!(!link.contains("successor-version"), "anonymous caller must not see the successor link");

        let mut known = UnitTestBuilder::default().with_config(conf).with_backend(upstream_json("{}")).with_entrypoint(crate::configure);
        let auth = AuthenticationData { principal: None, client_id: Some("sales-bot".into()), client_name: Some("Sales Bot".into()), properties: Default::default() };
        let r = known.request(UnitHttpRequest::get().with_path("/orders").with_authentication_data(auth));
        assert!(r.header("link").unwrap_or_default().contains("successor-version"), "identified caller sees the successor link");
    }

    // T17 — forged A2A header on a REST path does not unlock task grace (auto mode)
    #[test]
    fn t17_forged_a2a_follow_up_no_grace() {
        let backend = upstream_json("{}");
        let mut t = UnitTestBuilder::default()
            .with_config(cfg(PAST, PAST2, FUTURE, json!({"taskGraceMinutes": 20160})))
            .with_backend(Rc::clone(&backend))
            .with_entrypoint(crate::configure);
        let r = t.request(UnitHttpRequest::get().with_path("/v1/tasks/123").with_header("a2a-version", "1.0"));
        assert_eq!(r.status_code(), 410);
        assert!(backend.next().is_none());
    }

    // T18 — a violation set by an earlier policy is not overwritten (needs FilterChainBuilder; manual check in local Flex)

    // T16 — JSON-RPC batch in tombstone answers every request id, skips notifications
    #[test]
    fn t16_tombstone_batch() {
        let mut t = UnitTestBuilder::default().with_config(tombstone()).with_entrypoint(crate::configure);
        let r = t.request(
            UnitHttpRequest::post().with_path("/mcp").with_header("content-type", "application/json")
                .with_header("mcp-protocol-version", "2025-06-18")
                .with_body(r#"[{"jsonrpc":"2.0","id":1,"method":"tools/list"},{"jsonrpc":"2.0","method":"notifications/x"},{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"a"}}]"#),
        );
        let v = body_json(&r);
        assert_eq!(v.as_array().unwrap().len(), 2);
    }
}
