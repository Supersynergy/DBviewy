//! Unit tests for [`openai`](openai). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use serde_json::json;

#[test]
fn flattens_tool_use_and_results() {
    let messages = vec![
        Message::user_text("how many rows?"),
        Message::assistant(vec![ContentBlock::ToolUse {
            id: "call_1".into(),
            name: "count_rows".into(),
            input: json!({"open_tab": "@active"}),
        }]),
        Message::tool_results(vec![ContentBlock::ToolResult {
            id: "call_1".into(),
            content: "{\"row_count\":42}".into(),
            is_error: false,
        }]),
    ];
    let wire = messages_to_wire("be helpful", &messages);
    // system, user, assistant(with tool_calls), tool.
    assert_eq!(wire[0]["role"], "system");
    assert_eq!(wire[1]["role"], "user");
    assert_eq!(wire[2]["role"], "assistant");
    assert_eq!(wire[2]["content"], Value::Null);
    assert_eq!(wire[2]["tool_calls"][0]["id"], "call_1");
    assert_eq!(wire[2]["tool_calls"][0]["function"]["name"], "count_rows");
    assert_eq!(wire[3]["role"], "tool");
    assert_eq!(wire[3]["tool_call_id"], "call_1");
}

#[test]
fn tool_schema_wraps_under_function() {
    let cfg = ProviderConfig {
        model: "gpt-4o".into(),
        base_url: None,
        api_key: "k".into(),
        temperature: Some(0.5),
        max_tokens: Some(100),
        reasoning: None,
        verbosity: None,
        pro_mode: false,
    };
    let tools = vec![ToolDef {
        name: "schema".into(),
        description: "desc".into(),
        input_schema: json!({"type": "object", "properties": {}}),
    }];
    let body = build_body(
        &cfg,
        "sys",
        &[Message::user_text("hi")],
        &tools,
        "max_completion_tokens",
    );
    assert_eq!(body["stream"], true);
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["function"]["name"], "schema");
    assert_eq!(body["tool_choice"], "auto");
    // The chosen token field carries the cap; the legacy field is absent.
    assert_eq!(body["max_completion_tokens"], 100);
    assert!(body.get("max_tokens").is_none());
}

#[test]
fn unlimited_tokens_omits_the_field() {
    let cfg = ProviderConfig {
        model: "gpt-4o".into(),
        base_url: None,
        api_key: "k".into(),
        temperature: Some(0.5),
        max_tokens: None,
        reasoning: None,
        verbosity: None,
        pro_mode: false,
    };
    let body = build_body(&cfg, "sys", &[Message::user_text("hi")], &[], "max_tokens");
    assert!(body.get("max_tokens").is_none());
    assert!(body.get("max_completion_tokens").is_none());
}

#[test]
fn reasoning_sets_the_effort_field() {
    let cfg = ProviderConfig {
        model: "gpt-5.4".into(),
        base_url: None,
        api_key: "k".into(),
        temperature: Some(0.0),
        max_tokens: Some(1000),
        reasoning: Some("high".into()),
        verbosity: None,
        pro_mode: false,
    };
    let body = build_body(&cfg, "sys", &[], &[], "max_completion_tokens");
    assert_eq!(body["reasoning_effort"], json!("high"));
}

#[test]
fn blank_reasoning_omits_the_effort_field() {
    // Empty is "no thinking", not "effort = empty string", which the API would
    // reject.
    for value in [None, Some(String::new()), Some("   ".to_string())] {
        let cfg = ProviderConfig {
            model: "gpt-5.4".into(),
            base_url: None,
            api_key: "k".into(),
            temperature: Some(0.0),
            max_tokens: Some(1000),
            reasoning: value,
            verbosity: None,
            pro_mode: false,
        };
        let body = build_body(&cfg, "sys", &[], &[], "max_completion_tokens");
        assert!(body.get("reasoning_effort").is_none());
    }
}

#[test]
fn completion_stream_reports_provider_errors_and_empty_token_limited_turns() {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    for (chunk, expected) in [
        (
            json!({"error":{"message":"no free capacity"}}),
            "no free capacity",
        ),
        (
            json!({"model":"test/free","choices":[{"delta":{},"finish_reason":"length"}]}),
            "test/free returned no answer (finish_reason: length)",
        ),
    ] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut input = BufReader::new(&conn);
            let mut content_length = None;
            for line in input.by_ref().lines() {
                let line = line.unwrap();
                if line.is_empty() {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    content_length = value.trim().parse::<usize>().ok();
                }
            }
            let mut bytes = vec![0; content_length.unwrap()];
            input.read_exact(&mut bytes).unwrap();
            let response = format!("data: {chunk}\n\ndata: [DONE]\n\n");
            write!(conn, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{response}", response.len()).unwrap();
        });
        let error = run_openai(
            &endpoint,
            &[],
            &json!({}),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap_err();
        server.join().unwrap();
        assert_eq!(error, expected);
    }
}
