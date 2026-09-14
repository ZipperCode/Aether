use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_stream::stream;
use axum::body::{Body, Bytes};
use axum::http::header;
use axum::routing::post;
use axum::{Json, Router};
use futures_util::{Stream, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{
    build_sse_body_stream, SseEventBoundary, SSE_CONTROL_FILTER_MAX_BUFFER_BYTES,
    SSE_KEEPALIVE_BYTES,
};

/// Consume the actual output stream, with a bound that also catches accidental full-event buffering.
async fn next_chunk<S, E>(body: &mut S) -> Bytes
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Debug,
{
    tokio::time::timeout(Duration::from_secs(2), body.next())
        .await
        .expect("next body chunk should arrive")
        .expect("body should remain open")
        .expect("body chunk should succeed")
}

fn sse_event(payload: &Value, newline: &str) -> String {
    format!(
        "event: {}{newline}data: {payload}{newline}{newline}",
        payload["type"].as_str().expect("fixture event type")
    )
}

/// Decode the final client bytes rather than the audit snapshot taken before keepalive insertion.
fn sse_json_events(bytes: &[u8]) -> Vec<Value> {
    std::str::from_utf8(bytes)
        .expect("complete SSE output should be UTF-8")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split("\n\n")
        .filter_map(|record| {
            let data = record
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .collect::<Vec<_>>()
                .join("\n");
            (!data.is_empty()).then(|| {
                serde_json::from_str(&data).expect("every client-visible data event must be JSON")
            })
        })
        .collect()
}

#[test]
fn sse_body_stream_boundary_distinguishes_crlf_and_nonempty_lines() {
    let mut boundary = SseEventBoundary::default();
    assert!(!boundary.event_open);
    for (chunk, event_open) in [
        (b"data: {}\r".as_slice(), true),
        (b"\n", true),
        (b" \t\r\n", true),
        (b"\r", false),
        (b"\n", false),
        (b"data: {}\n\npartial", true),
        (b"\n\n", false),
    ] {
        boundary.observe(chunk);
        assert_eq!(boundary.event_open, event_open, "after {chunk:?}");
    }
}

/// Every pause below crosses several timer ticks while a JSON record or its separator is unfinished.
#[tokio::test]
async fn sse_body_stream_keepalive_waits_for_prefetched_and_live_event_boundaries() {
    let arguments = json!({"path": "C:\\临时\\a\"b\n.rs"});
    let message_start = json!({"type": "message_start", "message": {"id": "msg_tools"}});
    let tool_delta = json!({
        "type": "content_block_delta",
        "index": 1,
        "delta": {"type": "input_json_delta", "partial_json": arguments.to_string()}
    });
    for newline in ["\n", "\r\n", "\r"] {
        for filter_control_blocks in [false, true] {
            for prefetched in [false, true] {
                let record = sse_event(&tool_delta, newline);
                let escape_split = record.find(r"\\").expect("escaped tool argument") + 1;
                let utf8_split = record.find('临').expect("UTF-8 tool argument") + 1;
                let line_end = record.len() - 2 * newline.len();
                let mut first = sse_event(&message_start, newline).into_bytes();
                first.extend_from_slice(&record.as_bytes()[..escape_split]);
                let mut fragments = vec![
                    Bytes::from(first),
                    Bytes::copy_from_slice(&record.as_bytes()[escape_split..utf8_split]),
                    Bytes::copy_from_slice(&record.as_bytes()[utf8_split..line_end]),
                ];
                if newline == "\r\n" {
                    fragments.extend([
                        Bytes::from_static(b"\r"),
                        Bytes::from_static(b"\n"),
                        Bytes::from_static(b"\r\n"),
                    ]);
                } else {
                    fragments.extend([Bytes::from(newline), Bytes::from(newline)]);
                }
                let (tx, rx) = mpsc::channel::<Result<Bytes, io::Error>>(1);
                let mut body = Box::pin(build_sse_body_stream(
                    if prefetched {
                        vec![fragments[0].clone()]
                    } else {
                        Vec::new()
                    },
                    rx,
                    filter_control_blocks,
                    true,
                    true,
                    Duration::from_millis(5),
                ));
                let mut wire = Vec::new();
                if !prefetched {
                    assert_eq!(next_chunk(&mut body).await.as_ref(), SSE_KEEPALIVE_BYTES);
                }
                for (index, fragment) in fragments.iter().enumerate() {
                    if index != 0 || !prefetched {
                        tx.send(Ok(fragment.clone()))
                            .await
                            .expect("fragment should send");
                    }
                    let output = next_chunk(&mut body).await;
                    assert_eq!(
                        &output, fragment,
                        "incomplete events must forward progressively"
                    );
                    wire.extend_from_slice(&output);
                    if index + 1 != fragments.len() {
                        assert!(
                            tokio::time::timeout(Duration::from_millis(20), body.next())
                                .await
                                .is_err(),
                            "no heartbeat inside a partial event: newline={newline:?}, filter={filter_control_blocks}, prefetched={prefetched}, fragment={index}"
                        );
                    }
                }
                assert_eq!(next_chunk(&mut body).await.as_ref(), SSE_KEEPALIVE_BYTES);
                let events = sse_json_events(&wire);
                assert_eq!(events, vec![message_start.clone(), tool_delta.clone()]);
                assert_eq!(
                    serde_json::from_str::<Value>(
                        events[1]["delta"]["partial_json"].as_str().unwrap()
                    )
                    .unwrap(),
                    arguments
                );

                let stop = sse_event(&json!({"type": "message_stop"}), newline);
                tx.send(Ok(Bytes::from(format!(
                    "{stop}data: {{\"unexpected\":true}}\n\n"
                ))))
                .await
                .expect("terminal event should send");
                assert_eq!(next_chunk(&mut body).await.as_ref(), stop.as_bytes());
                assert!(tokio::time::timeout(Duration::from_secs(2), body.next())
                    .await
                    .expect("message_stop must close the body while the sender is alive")
                    .is_none());
                assert!(tx.is_closed());
            }
        }
    }
}

/// Clearing the filter's size-limited buffer does not end the event already visible to the client.
#[tokio::test]
async fn sse_body_stream_keepalive_waits_after_control_filter_overflow() {
    let prefix = Bytes::from(format!(
        "data: {{\"padding\":\"{}",
        "x".repeat(SSE_CONTROL_FILTER_MAX_BUFFER_BYTES)
    ));
    let (tx, rx) = mpsc::channel::<Result<Bytes, io::Error>>(1);
    let mut body = Box::pin(build_sse_body_stream(
        vec![prefix.clone()],
        rx,
        true,
        true,
        false,
        Duration::from_millis(5),
    ));
    assert_eq!(next_chunk(&mut body).await, prefix);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), body.next())
            .await
            .is_err(),
        "filter overflow must not allow a heartbeat inside emitted JSON"
    );
    let suffix = Bytes::from_static(b"\",\ndata: \"ok\":true}\n\n");
    tx.send(Ok(suffix.clone()))
        .await
        .expect("suffix should send");
    assert_eq!(next_chunk(&mut body).await, suffix);
    assert_eq!(next_chunk(&mut body).await.as_ref(), SSE_KEEPALIVE_BYTES);
    let events = sse_json_events(&[prefix.as_ref(), suffix.as_ref()].concat());
    assert_eq!(
        events[0]["padding"].as_str().unwrap().len(),
        SSE_CONTROL_FILTER_MAX_BUFFER_BYTES
    );
    assert_eq!(events[0]["ok"], true);
}

/// Two loopback HTTP hops exercise the final wrapper bytes and the next request's tool-result IDs.
#[tokio::test]
async fn sse_body_stream_http_preserves_native_anthropic_tool_roundtrip() {
    let expected_content = vec![
        json!({"type": "thinking", "thinking": "检查目录，再读取文件。", "signature": "opaque-synthetic-signature=="}),
        json!({"type": "tool_use", "id": "toolu_list", "name": "list_files", "input": {"path": "C:\\临时", "filter": "\"*.rs\"\n"}}),
        json!({"type": "tool_use", "id": "toolu_read", "name": "read_file", "input": {"path": "C:\\另一个\\文件.rs"}}),
    ];
    let events = vec![
        json!({"type": "message_start", "message": {"id": "msg_tools", "role": "assistant", "content": [], "model": "synthetic-glm"}}),
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": ""}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": expected_content[0]["thinking"]}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "signature_delta", "signature": expected_content[0]["signature"]}}),
        json!({"type": "content_block_stop", "index": 0}),
        json!({"type": "content_block_start", "index": 1, "content_block": {"type": "tool_use", "id": "toolu_list", "name": "list_files", "input": {}}}),
        json!({"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta", "partial_json": expected_content[1]["input"].to_string()}}),
        json!({"type": "content_block_stop", "index": 1}),
        json!({"type": "content_block_start", "index": 2, "content_block": {"type": "tool_use", "id": "toolu_read", "name": "read_file", "input": {}}}),
        json!({"type": "content_block_delta", "index": 2, "delta": {"type": "input_json_delta", "partial_json": expected_content[2]["input"].to_string()}}),
        json!({"type": "content_block_stop", "index": 2}),
        json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 20}}),
        json!({"type": "message_stop"}),
    ];
    let expected_wire = events
        .iter()
        .map(|event| sse_event(event, "\r\n"))
        .collect::<String>();
    let first_split = expected_wire.find('临').unwrap() + 1;
    let second_split = expected_wire.find('另').unwrap() + 1;
    let fragments = [
        Bytes::copy_from_slice(&expected_wire.as_bytes()[..first_split]),
        Bytes::copy_from_slice(&expected_wire.as_bytes()[first_split..second_split]),
    ];
    let mut final_fragment = expected_wire.as_bytes()[second_split..].to_vec();
    final_fragment.extend_from_slice(b"data: {\"unexpected_after_stop\":true}\r\n\r\n");
    let (upstream_tx, upstream_rx) = mpsc::channel::<Bytes>(1);
    let upstream_rx = Arc::new(Mutex::new(Some(upstream_rx)));
    let requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let requests_for_route = Arc::clone(&requests);
    let upstream_listener = crate::test_support::bind_loopback_listener()
        .await
        .expect("mock upstream should bind");
    let upstream_addr = upstream_listener.local_addr().unwrap();
    let upstream_app = Router::new().route(
        "/v1/messages",
        post(move |Json(request): Json<Value>| {
            let requests = Arc::clone(&requests_for_route);
            let upstream_rx = Arc::clone(&upstream_rx);
            async move {
                requests.lock().unwrap().push(request);
                let rx = upstream_rx.lock().unwrap().take();
                let body = if let Some(mut rx) = rx {
                    Body::from_stream(stream! {
                        while let Some(chunk) = rx.recv().await {
                            yield Ok::<_, io::Error>(chunk);
                        }
                    })
                } else {
                    Body::from(sse_event(&json!({"type": "message_stop"}), "\n"))
                };
                ([(header::CONTENT_TYPE, "text/event-stream")], body)
            }
        }),
    );
    let upstream_server = tokio::spawn(async move {
        axum::serve(upstream_listener, upstream_app)
            .await
            .expect("mock upstream should serve");
    });

    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let client_for_route = client.clone();
    let relay_listener = crate::test_support::bind_loopback_listener()
        .await
        .expect("test relay should bind");
    let relay_addr = relay_listener.local_addr().unwrap();
    let relay_app = Router::new().route(
        "/v1/messages",
        post(move |Json(request): Json<Value>| {
            let client = client_for_route.clone();
            async move {
                let response = client
                    .post(format!("http://{upstream_addr}/v1/messages"))
                    .json(&request)
                    .send()
                    .await
                    .expect("mock upstream request should succeed");
                let (tx, rx) = mpsc::channel(1);
                tokio::spawn(async move {
                    let mut upstream = response.bytes_stream();
                    while let Some(chunk) = upstream.next().await {
                        if tx.send(chunk.map_err(io::Error::other)).await.is_err() {
                            break;
                        }
                    }
                });
                let body = Body::from_stream(build_sse_body_stream(
                    Vec::new(),
                    rx,
                    true,
                    true,
                    true,
                    Duration::from_millis(5),
                ));
                ([(header::CONTENT_TYPE, "text/event-stream")], body)
            }
        }),
    );
    let relay_server = tokio::spawn(async move {
        axum::serve(relay_listener, relay_app)
            .await
            .expect("test relay should serve");
    });

    let initial_message = json!({"role": "user", "content": "请查看并读取文件"});
    let response = client
        .post(format!("http://{relay_addr}/v1/messages"))
        .json(&json!({"model": "synthetic-glm", "stream": true, "messages": [initial_message.clone()]}))
        .send()
        .await
        .expect("client request should succeed");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    let mut body = response.bytes_stream();
    let mut wire = Vec::new();
    for fragment in &fragments {
        upstream_tx.send(fragment.clone()).await.unwrap();
        // HTTP may split or coalesce writes; compare accumulated bytes, never transport chunk counts.
        tokio::time::timeout(Duration::from_secs(2), async {
            while !wire.ends_with(fragment.as_ref()) {
                wire.extend_from_slice(&next_chunk(&mut body).await);
            }
        })
        .await
        .expect("partial tool event must reach the client before its remaining bytes");
        assert!(
            tokio::time::timeout(Duration::from_millis(20), body.next())
                .await
                .is_err(),
            "a heartbeat must not split either tool's JSON on the HTTP wire"
        );
    }
    upstream_tx.send(Bytes::from(final_fragment)).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(chunk) = body.next().await {
            wire.extend_from_slice(&chunk.expect("HTTP body should succeed"));
        }
    })
    .await
    .expect("message_stop should close HTTP body with upstream still open");
    drop(upstream_tx);
    let received_events = sse_json_events(&wire);
    assert_eq!(received_events, events);
    assert_eq!(
        std::str::from_utf8(&wire)
            .unwrap()
            .replace(std::str::from_utf8(SSE_KEEPALIVE_BYTES).unwrap(), ""),
        expected_wire
    );

    let mut assistant_content = vec![json!({
        "type": "thinking",
        "thinking": received_events.iter().filter_map(|event| event["delta"]["thinking"].as_str()).collect::<String>(),
        "signature": received_events.iter().filter_map(|event| event["delta"]["signature"].as_str()).collect::<String>()
    })];
    for start in received_events.iter().filter(|event| {
        event["type"] == "content_block_start" && event["content_block"]["type"] == "tool_use"
    }) {
        let arguments = received_events
            .iter()
            .filter(|event| event["index"] == start["index"])
            .filter_map(|event| event["delta"]["partial_json"].as_str())
            .collect::<String>();
        let mut tool = start["content_block"].clone();
        tool["input"] = serde_json::from_str(&arguments).expect("tool arguments should reassemble");
        assistant_content.push(tool);
    }
    assert_eq!(assistant_content, expected_content);
    let results = json!([
        {"type": "tool_result", "tool_use_id": assistant_content[1]["id"], "content": "listed files"},
        {"type": "tool_result", "tool_use_id": assistant_content[2]["id"], "content": "file contents"}
    ]);
    let followup = json!({
        "model": "synthetic-glm", "stream": true,
        "messages": [
            initial_message,
            {"role": "assistant", "content": assistant_content},
            {"role": "user", "content": results}
        ]
    });
    let completed = tokio::time::timeout(Duration::from_secs(2), async {
        client
            .post(format!("http://{relay_addr}/v1/messages"))
            .json(&followup)
            .send()
            .await
            .expect("tool results request should succeed")
            .bytes()
            .await
            .expect("tool results response should complete")
    })
    .await
    .expect("tool results round should complete");
    assert_eq!(
        sse_json_events(&completed),
        vec![json!({"type": "message_stop"})]
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1], followup);
    assert_eq!(
        requests[1]["messages"][2]["content"][0]["tool_use_id"],
        "toolu_list"
    );
    assert_eq!(
        requests[1]["messages"][2]["content"][1]["tool_use_id"],
        "toolu_read"
    );
    upstream_server.abort();
    relay_server.abort();
}
