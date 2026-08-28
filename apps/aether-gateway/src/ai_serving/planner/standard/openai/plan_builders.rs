use aether_ai_serving::{
    resolve_ai_passthrough_sync_request_body, AiRequestGzipPolicy, OriginalRequestPayload,
};
use aether_contracts::RequestBody;

#[path = "plan_builders/stream.rs"]
mod stream;
#[path = "plan_builders/sync.rs"]
mod sync;

/// 在 OpenAI 候选完成全部 JSON 变换后选择执行体：优先复用决策已验证的精确字节，
/// 否则仅在格式等价、最终 JSON 未变化且无需重新编码时从本地请求扩展恢复原始字节。
fn resolve_openai_plan_request_body(
    parts: &http::request::Parts,
    provider_request_body: serde_json::Value,
    provider_request_body_base64: Option<String>,
    client_api_format: &str,
    provider_api_format: &str,
    content_encoding: Option<&str>,
    request_gzip: Option<&AiRequestGzipPolicy>,
) -> RequestBody {
    let provider_request_body_base64 = provider_request_body_base64
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            if !crate::ai_serving::api_format_alias_matches(client_api_format, provider_api_format)
                || content_encoding.is_some_and(|value| !value.trim().is_empty())
                || request_gzip.is_some_and(|policy| policy.enabled != Some(false))
            {
                return None;
            }

            parts
                .extensions
                .get::<OriginalRequestPayload>()?
                .body_bytes_base64_if_unchanged(&provider_request_body)
        });

    resolve_ai_passthrough_sync_request_body(
        Some(provider_request_body),
        provider_request_body_base64,
    )
}

pub(crate) use self::stream::{
    build_openai_chat_stream_plan_from_decision, build_openai_responses_stream_plan_from_decision,
};
pub(crate) use self::sync::{
    build_openai_chat_sync_plan_from_decision, build_openai_responses_sync_plan_from_decision,
};

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use serde_json::{json, Value};

    use super::{
        build_openai_chat_stream_plan_from_decision, build_openai_chat_sync_plan_from_decision,
        build_openai_responses_stream_plan_from_decision,
        build_openai_responses_sync_plan_from_decision, resolve_openai_plan_request_body,
    };
    use crate::AiExecutionDecision;
    use aether_ai_serving::{AiRequestGzipPolicy, OriginalRequestPayload};

    const PREVALIDATED_BODY_BASE64: &str = "eyAiZXhhY3QiOiB0cnVlIH0=";

    /// 构造只包含 OpenAI plan builder 必需字段的决策，避免测试复制生产决策器细节。
    fn decision_with_body(
        action: &str,
        api_format: &str,
        body: Value,
        body_base64: Option<&str>,
    ) -> AiExecutionDecision {
        serde_json::from_value(json!({
            "action": action,
            "decision_kind": format!("openai_test_{action}"),
            "request_id": format!("req_{action}"),
            "candidate_id": format!("cand_{action}"),
            "provider_id": "provider_test",
            "endpoint_id": "endpoint_test",
            "key_id": "key_test",
            "upstream_url": "https://example.com/v1/test",
            "auth_header": "authorization",
            "auth_value": "Bearer test",
            "provider_api_format": api_format,
            "client_api_format": api_format,
            "provider_request_headers": {"content-type": "application/json"},
            "provider_request_body": body,
            "provider_request_body_base64": body_base64,
            "content_type": "application/json",
            "upstream_is_stream": action == "stream",
            "report_context": {}
        }))
        .expect("decision should deserialize")
    }

    /// 构造带指定 URI 的请求 parts，供四条 builder 路径共享。
    fn request_parts(uri: &str) -> http::request::Parts {
        http::Request::builder()
            .uri(uri)
            .body(())
            .expect("request should build")
            .into_parts()
            .0
    }

    /// 验证 Chat/Responses 的同步与流式 builder 都优先保留决策已提供的精确字节，
    /// 其中 Responses 同步分支同时覆盖 Compact 计划。
    #[test]
    fn openai_sync_and_stream_builders_prefer_prevalidated_exact_body() {
        let chat_sync = build_openai_chat_sync_plan_from_decision(
            &request_parts("http://localhost/v1/chat/completions"),
            &json!({}),
            decision_with_body(
                "sync",
                "openai:chat",
                json!({"model": "gpt-5", "messages": [], "stream": false}),
                Some(PREVALIDATED_BODY_BASE64),
            ),
        )
        .expect("chat sync plan should build")
        .expect("chat sync plan should exist");
        let responses_sync = build_openai_responses_sync_plan_from_decision(
            &request_parts("http://localhost/v1/responses"),
            &json!({}),
            decision_with_body(
                "sync",
                "openai:responses:compact",
                json!({"model": "gpt-5", "input": [], "stream": false}),
                Some(PREVALIDATED_BODY_BASE64),
            ),
            true,
        )
        .expect("responses sync plan should build")
        .expect("responses sync plan should exist");
        let chat_stream = build_openai_chat_stream_plan_from_decision(
            &request_parts("http://localhost/v1/chat/completions"),
            &json!({}),
            decision_with_body(
                "stream",
                "openai:chat",
                json!({"model": "gpt-5", "messages": [], "stream": true}),
                Some(PREVALIDATED_BODY_BASE64),
            ),
        )
        .expect("chat stream plan should build")
        .expect("chat stream plan should exist");
        let responses_stream = build_openai_responses_stream_plan_from_decision(
            &request_parts("http://localhost/v1/responses"),
            &json!({}),
            decision_with_body(
                "stream",
                "openai:responses",
                json!({"model": "gpt-5", "input": [], "stream": true}),
                Some(PREVALIDATED_BODY_BASE64),
            ),
            false,
        )
        .expect("responses stream plan should build")
        .expect("responses stream plan should exist");

        assert_eq!(
            responses_sync.plan.provider_api_format,
            "openai:responses:compact"
        );

        for body in [
            &chat_sync.plan.body,
            &responses_sync.plan.body,
            &chat_stream.plan.body,
            &responses_stream.plan.body,
        ] {
            assert!(body.json_body.is_none());
            assert_eq!(
                body.body_bytes_b64.as_deref(),
                Some(PREVALIDATED_BODY_BASE64)
            );
        }
    }

    /// 验证本地原始字节只在最终 JSON、格式和编码策略均允许时启用，任一门禁失败即回退 JSON。
    #[test]
    fn local_exact_body_requires_unchanged_same_format_unencoded_json() {
        let raw = br#"{ "model": "gpt-5", "messages": [] }"#;
        let parsed: Value = serde_json::from_slice(raw).expect("request should parse");
        let mut parts = request_parts("http://localhost/v1/chat/completions");
        parts
            .extensions
            .insert(OriginalRequestPayload::from_parsed_json(
                parsed.clone(),
                raw,
            ));

        let exact = resolve_openai_plan_request_body(
            &parts,
            parsed.clone(),
            None,
            "openai",
            "openai:chat",
            None,
            None,
        );
        assert!(exact.json_body.is_none());
        assert_eq!(
            exact.body_bytes_b64,
            Some(base64::engine::general_purpose::STANDARD.encode(raw))
        );

        let gzip = AiRequestGzipPolicy {
            enabled: Some(true),
            min_bytes: Some(0),
        };
        let fallbacks = [
            resolve_openai_plan_request_body(
                &parts,
                json!({"model": "gpt-5.1", "messages": []}),
                None,
                "openai:chat",
                "openai:chat",
                None,
                None,
            ),
            resolve_openai_plan_request_body(
                &parts,
                parsed.clone(),
                None,
                "claude:messages",
                "openai:chat",
                None,
                None,
            ),
            resolve_openai_plan_request_body(
                &parts,
                parsed.clone(),
                None,
                "openai:chat",
                "openai:chat",
                Some("zstd"),
                None,
            ),
            resolve_openai_plan_request_body(
                &parts,
                parsed.clone(),
                None,
                "openai:chat",
                "openai:chat",
                None,
                Some(&gzip),
            ),
            resolve_openai_plan_request_body(
                &request_parts("http://localhost/v1/chat/completions"),
                parsed,
                None,
                "openai:chat",
                "openai:chat",
                None,
                None,
            ),
        ];

        for fallback in fallbacks {
            assert!(fallback.json_body.is_some());
            assert!(fallback.body_bytes_b64.is_none());
        }
    }
}
