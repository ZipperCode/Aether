use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use aether_contracts::{StreamFrame, StreamFramePayload, StreamFrameType};
use aether_crypto::DEVELOPMENT_ENCRYPTION_KEY;
use aether_data::repository::usage::InMemoryUsageReadRepository;
use aether_data_contracts::repository::usage::UsageReadRepository;
use aether_usage_runtime::UsageRuntimeConfig;
use async_stream::stream;
use axum::body::{to_bytes, Bytes};
use axum::http::StatusCode;
use base64::Engine as _;
use futures_util::StreamExt as _;
use serde_json::{json, Value};

use super::tests::{
    native_anthropic_stream_plan, ndjson_frame, provider_catalog_for_plan, test_decision,
};
use super::{
    execute_stream_from_frame_stream, maybe_build_provider_private_stream_normalizer,
    maybe_build_stream_response_rewriter, normalize_provider_private_report_context,
    SSE_KEEPALIVE_BYTES,
};
use crate::stage_metrics::RequestStageTrace;
use crate::AppState;

/// 经真实网关交接与 Body 消费验证字节，同时等待后台用量结算完成。
async fn forward_prefetch_fragments(chunks: Vec<Vec<u8>>, mut context: Value) -> Bytes {
    let request_id = format!("prefetch-handoff-{}", uuid::Uuid::new_v4());
    let mut plan = native_anthropic_stream_plan(&request_id);
    plan.provider_api_format = context["provider_api_format"].as_str().unwrap().to_string();
    plan.client_api_format = context["client_api_format"].as_str().unwrap().to_string();
    let plan_kind = match plan.client_api_format.as_str() {
        "openai:responses" => "openai_responses_stream",
        "gemini:generate_content" => "gemini_chat_stream",
        _ => "openai_chat_stream",
    };
    context["request_id"] = json!(request_id);
    context["candidate_id"] = json!(plan.candidate_id);
    context["candidate_index"] = json!(0);
    context["retry_index"] = json!(0);
    let usage_repository = Arc::new(InMemoryUsageReadRepository::default());
    let state = AppState::new()
        .unwrap()
        .with_data_state_for_tests(
            crate::data::GatewayDataState::with_usage_repository_for_tests(Arc::clone(
                &usage_repository,
            ))
            .with_provider_catalog_reader(Arc::new(provider_catalog_for_plan(&plan, None)))
            .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY),
        )
        .with_usage_runtime_for_tests(UsageRuntimeConfig {
            enabled: true,
            ..UsageRuntimeConfig::default()
        });
    let frames = stream! {
        yield Ok::<Bytes, std::io::Error>(ndjson_frame(StreamFrame {
            frame_type: StreamFrameType::Headers,
            payload: StreamFramePayload::Headers {
                status_code: 200,
                headers: BTreeMap::from([("content-type".into(), "text/event-stream".into())]),
                response_observation: None,
            },
        }));
        for chunk in chunks {
            // 传输分片允许拆开 UTF-8 字符，不能先转换成字符串修复测试输入。
            yield Ok(ndjson_frame(StreamFrame {
                frame_type: StreamFrameType::Data,
                payload: StreamFramePayload::Data {
                    chunk_b64: Some(base64::engine::general_purpose::STANDARD.encode(chunk)),
                    text: None,
                },
            }));
        }
        yield Ok(ndjson_frame(StreamFrame::eof()));
    }
    .boxed();
    let response = execute_stream_from_frame_stream(
        &state,
        plan,
        "trace-prefetch-handoff",
        &test_decision(),
        plan_kind,
        Some(format!("{plan_kind}_success")),
        Some(context),
        crate::clock::current_unix_ms(),
        Instant::now(),
        RequestStageTrace::from_env(),
        false,
        frames,
        None,
    )
    .await
    .expect("valid upstream SSE should execute")
    .expect("valid upstream SSE should commit");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("client body should finish");
    let usage = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(usage) = usage_repository
                .find_by_request_id(&request_id)
                .await
                .unwrap()
            {
                if usage.status != "pending" && usage.status != "streaming" {
                    break usage;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("terminal usage should settle");
    assert_eq!(usage.status, "completed", "{:?}", usage.error_message);
    assert_eq!((usage.input_tokens, usage.output_tokens), (11, 7));
    body
}

fn responses_context() -> Value {
    let context = json!({
        "provider_api_format": "openai:responses",
        "client_api_format": "openai:responses",
        "needs_conversion": true,
    });
    assert!(maybe_build_stream_response_rewriter(Some(&context)).is_some());
    context
}

/// 开场事件携带长工具描述；终态补全字段由真实 Responses 兼容重写器生成。
fn responses_events(description_bytes: usize) -> Vec<Value> {
    vec![
        json!({"type": "response.created", "sequence_number": 0, "response": {
            "id": "resp-prefetch", "status": "in_progress", "model": "gpt-test",
            "tools": [{"type": "function", "name": "inspect", "description":
                format!("{}边界🦀\\\"end", "tool-description ".repeat(description_bytes / 17))}],
            "future_field": {"opaque": "preserve-created"}
        }}),
        json!({"type": "response.in_progress", "sequence_number": 1,
            "response": {"id": "resp-prefetch", "future_field": [1, "unchanged"]}}),
        json!({"type": "response.output_text.delta", "sequence_number": 2,
            "delta": "finished", "future_field": {"opaque": true}}),
        json!({"type": "response.completed", "sequence_number": 3, "response": {
            "id": "resp-prefetch", "status": "completed", "created_at": 123,
            "output": [], "usage": {"input_tokens": 11, "output_tokens": 7, "total_tokens": 18},
            "future_field": {"opaque": "preserve-terminal"}
        }}),
    ]
}

fn encode_events(events: &[Value]) -> Vec<u8> {
    events
        .iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect::<String>()
        .into_bytes()
}

/// 同时检查每个 JSON 的全部字段、记录次序和终态，无损坏或重复输出。
fn assert_responses_events(body: &[u8], mut expected: Vec<Value>, case: &str) {
    let terminal = expected.last_mut().unwrap();
    terminal["response"]["completed_at"] = json!(123);
    terminal["response"]["output_text"] = json!("");
    let actual = std::str::from_utf8(body)
        .expect("output must retain UTF-8")
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|data| {
            serde_json::from_str::<Value>(data).expect("every SSE data line must remain JSON")
        })
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), expected.len(), "{case}: event count");
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        assert!(
            actual == expected,
            "{case}: event {index} changed: expected {} JSON bytes, got {}",
            expected.to_string().len(),
            actual.to_string().len()
        );
    }
    assert!(
        body == encode_events(&expected),
        "{case}: framing/order changed"
    );
}

#[tokio::test]
async fn responses_prefetch_handoff_preserves_full_fragments() {
    for size in [20_000, 18_000, 75_000] {
        let events = responses_events(size);
        let wire = encode_events(&events);
        for chunk_size in [5_000, 6_000, 16_384, 16_383, 16_385, wire.len()] {
            let body = forward_prefetch_fragments(
                wire.chunks(chunk_size).map(<[u8]>::to_vec).collect(),
                responses_context(),
            )
            .await;
            assert_responses_events(
                &body,
                events.clone(),
                &format!("size={size}, chunk={chunk_size}"),
            );
        }
    }
}

#[tokio::test]
async fn responses_prefetch_handoff_preserves_partial_records_and_split_utf8() {
    let mut events = responses_events(20_000);
    // 完整语义事件提前提交时，同一个分片中的下一条半记录仍必须恢复。
    events.insert(
        0,
        json!({"type": "response.output_text.delta", "delta": "early"}),
    );
    let wire = encode_events(&events);
    for split in [200, 18_000] {
        let body = forward_prefetch_fragments(
            vec![wire[..split].to_vec(), wire[split..].to_vec()],
            responses_context(),
        )
        .await;
        assert_responses_events(
            &body,
            events.clone(),
            &format!("semantic-prefix, split={split}"),
        );
    }

    let events = responses_events(20_000);
    let wire = encode_events(&events);
    let character = wire
        .windows("边".len())
        .position(|bytes| bytes == "边".as_bytes())
        .unwrap();
    let split = character + 1;
    assert!(std::str::from_utf8(&wire[..split]).is_err());
    let body = forward_prefetch_fragments(
        vec![
            wire[..split].to_vec(),
            wire[split..split + 1].to_vec(),
            wire[split + 1..].to_vec(),
        ],
        responses_context(),
    )
    .await;
    assert_responses_events(&body, events, "split UTF-8 at handoff");
}

#[tokio::test]
async fn private_normalizer_prefetch_handoff_preserves_full_fragments() {
    let context = json!({
        "provider_api_format": "gemini:generate_content",
        "client_api_format": "gemini:generate_content",
        "has_envelope": true,
        "envelope_name": "gemini_cli:v1internal",
    });
    assert!(maybe_build_provider_private_stream_normalizer(Some(&context)).is_some());
    let normalized = normalize_provider_private_report_context(Some(&context)).unwrap();
    assert!(maybe_build_stream_response_rewriter(Some(&normalized)).is_none());
    let expected = [
        json!({"candidates": [{"content": {"role": "model", "parts": [{"text": "长描述🦀".repeat(5_000)}]}}],
            "future_field": {"opaque": "retained"}}),
        json!({"candidates": [{"content": {"role": "model", "parts": []}, "finishReason": "STOP"}],
            "usageMetadata": {"promptTokenCount": 11, "candidatesTokenCount": 7, "totalTokenCount": 18}}),
    ];
    let wire = expected
        .iter()
        .map(|event| format!("data: {}\n\n", json!({"response": event})))
        .collect::<String>();
    let expected_wire = expected
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect::<String>();
    for chunk_size in [5_000, 6_000, 16_384, 16_385] {
        let body = forward_prefetch_fragments(
            wire.as_bytes()
                .chunks(chunk_size)
                .map(<[u8]>::to_vec)
                .collect(),
            context.clone(),
        )
        .await;
        // 原生 Gemini 在首条完整记录前发送一次保活；业务字节仍须逐字保留。
        let business_body = body
            .strip_prefix(SSE_KEEPALIVE_BYTES)
            .expect("Gemini should emit its initial keepalive before the incomplete record");
        assert!(
            business_body == expected_wire.as_bytes(),
            "private normalizer lost bytes at chunk={chunk_size}"
        );
    }
}

#[tokio::test]
async fn passthrough_prefetch_handoff_preserves_original_bytes() {
    let context = json!({"provider_api_format": "openai:chat", "client_api_format": "openai:chat"});
    assert!(maybe_build_provider_private_stream_normalizer(Some(&context)).is_none());
    assert!(maybe_build_stream_response_rewriter(Some(&context)).is_none());
    let wire = format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({"id": "chat-prefetch", "choices": [{"index": 0, "delta": {"content": "opaque words ".repeat(6_250)}}], "future_field": true}),
        json!({"id": "chat-prefetch", "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}], "usage": {"prompt_tokens": 11, "completion_tokens": 7, "total_tokens": 18}})
    );
    for chunk_size in [5_000, 6_000, 16_384, 16_385] {
        let body = forward_prefetch_fragments(
            wire.as_bytes()
                .chunks(chunk_size)
                .map(<[u8]>::to_vec)
                .collect(),
            context.clone(),
        )
        .await;
        assert!(
            body.as_ref() == wire.as_bytes(),
            "passthrough changed bytes at chunk={chunk_size}"
        );
    }
}
