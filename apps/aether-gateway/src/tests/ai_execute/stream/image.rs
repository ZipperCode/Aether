use super::{
    any, build_router_with_state, build_state_with_execution_runtime_override, json, start_server,
    to_bytes, Arc, Body, Json, Mutex, Request, Router, StatusCode, TRACE_ID_HEADER,
};
use aether_crypto::{encrypt_python_fernet_plaintext, DEVELOPMENT_ENCRYPTION_KEY};
use aether_data::repository::auth::{
    InMemoryAuthApiKeySnapshotRepository, StoredAuthApiKeySnapshot,
};
use aether_data::repository::candidate_selection::InMemoryMinimalCandidateSelectionReadRepository;
use aether_data::repository::candidates::InMemoryRequestCandidateRepository;
use aether_data::repository::provider_catalog::InMemoryProviderCatalogReadRepository;
use aether_data_contracts::repository::candidate_selection::{
    StoredMinimalCandidateSelectionRow, StoredProviderModelMapping,
};
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use sha2::{Digest, Sha256};

const STREAM_IMAGE_TEST_STACK_BYTES: usize = 16 * 1024 * 1024;

fn run_stream_image_test<F, Fut>(test_name: &'static str, make_future: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    let handle = std::thread::Builder::new()
        .name(test_name.to_string())
        .stack_size(STREAM_IMAGE_TEST_STACK_BYTES)
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime should build");
            runtime.block_on(make_future());
        })
        .expect("stream image test thread should spawn");

    if let Err(payload) = handle.join() {
        std::panic::resume_unwind(payload);
    }
}

#[test]
fn gateway_executes_codex_image_stream_via_local_decision_gate_after_oauth_refresh() {
    run_stream_image_test(
        "gateway_executes_codex_image_stream_via_local_decision_gate_after_oauth_refresh",
        gateway_executes_codex_image_stream_via_local_decision_gate_after_oauth_refresh_impl,
    );
}

async fn gateway_executes_codex_image_stream_via_local_decision_gate_after_oauth_refresh_impl() {
    #[derive(Debug, Clone)]
    struct SeenExecutionRuntimeStreamRequest {
        trace_id: String,
        url: String,
        authorization: String,
        headers: serde_json::Value,
        body: serde_json::Value,
        plan_stream: bool,
    }

    #[derive(Debug, Clone)]
    struct SeenRefreshRequest {
        content_type: String,
        body: String,
    }

    fn hash_api_key(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn sample_auth_snapshot(api_key_id: &str, user_id: &str) -> StoredAuthApiKeySnapshot {
        StoredAuthApiKeySnapshot::new(
            user_id.to_string(),
            "alice".to_string(),
            Some("alice@example.com".to_string()),
            "user".to_string(),
            "local".to_string(),
            true,
            false,
            Some(serde_json::json!(["openai", "codex"])),
            Some(serde_json::json!(["openai:image"])),
            Some(serde_json::json!(["gpt-image-2"])),
            api_key_id.to_string(),
            Some("default".to_string()),
            true,
            false,
            false,
            Some(60),
            Some(5),
            Some(4_102_444_800_i64),
            Some(serde_json::json!(["openai", "codex"])),
            Some(serde_json::json!(["openai:image"])),
            Some(serde_json::json!(["gpt-image-2"])),
        )
        .expect("auth snapshot should build")
    }

    fn sample_candidate_row() -> StoredMinimalCandidateSelectionRow {
        StoredMinimalCandidateSelectionRow {
            provider_id: "provider-codex-image-stream-local-1".to_string(),
            provider_name: "codex".to_string(),
            provider_type: "codex".to_string(),
            provider_priority: 10,
            provider_is_active: true,
            endpoint_id: "endpoint-codex-image-stream-local-1".to_string(),
            endpoint_api_format: "openai:image".to_string(),
            endpoint_api_family: Some("openai".to_string()),
            endpoint_kind: Some("image".to_string()),
            endpoint_is_active: true,
            key_id: "key-codex-image-stream-local-1".to_string(),
            key_name: "oauth".to_string(),
            key_auth_type: "oauth".to_string(),
            key_is_active: true,
            key_api_formats: Some(vec!["openai:image".to_string()]),
            key_allowed_models: None,
            key_capabilities: None,
            key_internal_priority: 5,
            key_global_priority_by_format: Some(serde_json::json!({"openai:image": 1})),
            routing_facts: Default::default(),
            model_id: "model-codex-image-stream-local-1".to_string(),
            global_model_id: "global-model-codex-image-stream-local-1".to_string(),
            global_model_name: "gpt-image-2".to_string(),
            global_model_mappings: None,
            global_model_supports_streaming: Some(true),
            model_provider_model_name: "gpt-image-2".to_string(),
            model_provider_model_mappings: Some(vec![StoredProviderModelMapping {
                name: "gpt-image-2".to_string(),
                priority: 1,
                api_formats: Some(vec!["openai:image".to_string()]),
                endpoint_ids: None,
                operations: None,
            }]),
            model_supports_streaming: Some(true),
            model_is_active: true,
            model_is_available: true,
        }
    }

    fn sample_provider_catalog_provider() -> StoredProviderCatalogProvider {
        StoredProviderCatalogProvider::new(
            "provider-codex-image-stream-local-1".to_string(),
            "codex".to_string(),
            Some("https://chatgpt.com".to_string()),
            "codex".to_string(),
        )
        .expect("provider should build")
        .with_transport_fields(
            true,
            false,
            false,
            None,
            Some(2),
            None,
            Some(20.0),
            None,
            None,
        )
    }

    fn sample_provider_catalog_endpoint() -> StoredProviderCatalogEndpoint {
        StoredProviderCatalogEndpoint::new(
            "endpoint-codex-image-stream-local-1".to_string(),
            "provider-codex-image-stream-local-1".to_string(),
            "openai:image".to_string(),
            Some("openai".to_string()),
            Some("image".to_string()),
            true,
        )
        .expect("endpoint should build")
        .with_transport_fields(
            "https://chatgpt.com/backend-api/codex".to_string(),
            None,
            None,
            Some(2),
            None,
            None,
            None,
            None,
        )
        .expect("endpoint transport should build")
    }

    fn sample_provider_catalog_key() -> StoredProviderCatalogKey {
        let encrypted_auth_config = encrypt_python_fernet_plaintext(
            DEVELOPMENT_ENCRYPTION_KEY,
            r#"{"provider_type":"codex","refresh_token":"rt-codex-image-stream-local-123"}"#,
        )
        .expect("auth config should encrypt");
        StoredProviderCatalogKey::new(
            "key-codex-image-stream-local-1".to_string(),
            "provider-codex-image-stream-local-1".to_string(),
            "oauth".to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("key should build")
        .with_transport_fields(
            Some(serde_json::json!(["openai:image"])),
            encrypt_python_fernet_plaintext(DEVELOPMENT_ENCRYPTION_KEY, "__placeholder__")
                .expect("placeholder api key should encrypt"),
            Some(encrypted_auth_config),
            None,
            Some(serde_json::json!({"openai:image": 1})),
            None,
            None,
            None,
            None,
        )
        .expect("key transport should build")
    }

    let seen_execution_runtime = Arc::new(Mutex::new(None::<SeenExecutionRuntimeStreamRequest>));
    let seen_execution_runtime_clone = Arc::clone(&seen_execution_runtime);
    let seen_refresh = Arc::new(Mutex::new(None::<SeenRefreshRequest>));
    let seen_refresh_clone = Arc::clone(&seen_refresh);

    let refresh = Router::new().route(
        "/oauth/token",
        any(move |request: Request| {
            let seen_refresh_inner = Arc::clone(&seen_refresh_clone);
            async move {
                let (parts, body) = request.into_parts();
                let raw_body = to_bytes(body, usize::MAX).await.expect("body should read");
                *seen_refresh_inner.lock().expect("mutex should lock") = Some(SeenRefreshRequest {
                    content_type: parts
                        .headers
                        .get(http::header::CONTENT_TYPE)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .to_string(),
                    body: String::from_utf8(raw_body.to_vec())
                        .expect("refresh body should be utf8"),
                });
                Json(json!({
                    "access_token": "refreshed-codex-image-stream-access-token",
                    "refresh_token": "rt-codex-image-stream-local-456",
                    "token_type": "Bearer",
                    "expires_in": 3600
                }))
            }
        }),
    );

    let execution_runtime = Router::new().route(
        "/v1/execute/stream",
        any(move |request: Request| {
            let seen_execution_runtime_inner = Arc::clone(&seen_execution_runtime_clone);
            async move {
                let (parts, body) = request.into_parts();
                let raw_body = to_bytes(body, usize::MAX).await.expect("body should read");
                let payload: serde_json::Value = serde_json::from_slice(&raw_body)
                    .expect("execution runtime payload should parse");
                *seen_execution_runtime_inner
                    .lock()
                    .expect("mutex should lock") = Some(SeenExecutionRuntimeStreamRequest {
                    trace_id: parts
                        .headers
                        .get(TRACE_ID_HEADER)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .to_string(),
                    url: payload
                        .get("url")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    authorization: payload
                        .get("headers")
                        .and_then(|value| value.get("authorization"))
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    headers: payload.get("headers").cloned().unwrap_or_default(),
                    body: payload
                        .get("body")
                        .and_then(|value| value.get("json_body"))
                        .cloned()
                        .unwrap_or_default(),
                    plan_stream: payload
                        .get("stream")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false),
                });
                let frames = concat!(
                    "{\"type\":\"headers\",\"payload\":{\"kind\":\"headers\",\"status_code\":200,\"headers\":{\"content-type\":\"application/json\"}}}\n",
                    "{\"type\":\"data\",\"payload\":{\"kind\":\"data\",\"text\":\"{\\\"created\\\":1776991097,\\\"data\\\":[{\\\"b64_json\\\":\\\"aGVsbG8=\\\",\\\"revised_prompt\\\":\\\"水墨视觉海报\\\"}],\\\"usage\\\":{\\\"input_tokens\\\":11,\\\"output_tokens\\\":22,\\\"total_tokens\\\":33}}\"}}\n",
                    "{\"type\":\"telemetry\",\"payload\":{\"kind\":\"telemetry\",\"telemetry\":{\"elapsed_ms\":41}}}\n",
                    "{\"type\":\"eof\",\"payload\":{\"kind\":\"eof\"}}\n"
                );
                let mut response = http::Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from(frames))
                    .expect("response should build");
                response.headers_mut().insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/x-ndjson"),
                );
                response
            }
        }),
    );

    let client_api_key = "sk-client-codex-image-stream-local";
    let auth_repository = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![(
        Some(hash_api_key(client_api_key)),
        sample_auth_snapshot(
            "key-codex-image-stream-client-123",
            "user-codex-image-stream-client-123",
        ),
    )]));
    let candidate_selection_repository =
        Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(vec![
            sample_candidate_row(),
        ]));
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider_catalog_provider()],
        vec![sample_provider_catalog_endpoint()],
        vec![sample_provider_catalog_key()],
    ));

    let (refresh_url, refresh_handle) = start_server(refresh).await;
    let (execution_runtime_url, execution_runtime_handle) = start_server(execution_runtime).await;
    let oauth_refresh =
        crate::provider_transport::LocalOAuthRefreshCoordinator::with_adapters_for_tests(vec![
            Arc::new(
                crate::provider_transport::oauth_refresh::GenericOAuthRefreshAdapter::default()
                    .with_token_url_for_tests("codex", format!("{refresh_url}/oauth/token")),
            ),
        ]);
    let gateway_state = build_state_with_execution_runtime_override(execution_runtime_url.clone())
        .with_data_state_for_tests(
            crate::data::GatewayDataState::with_auth_candidate_selection_provider_catalog_and_request_candidate_repository_for_tests(
                auth_repository,
                candidate_selection_repository,
                provider_catalog_repository,
                Arc::new(InMemoryRequestCandidateRepository::default()),
                DEVELOPMENT_ENCRYPTION_KEY,
            ),
        )
        .with_oauth_refresh_coordinator_for_tests(oauth_refresh);
    let gateway = build_router_with_state(gateway_state);
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .post(format!("{gateway_url}/v1/images/generations"))
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {client_api_key}"),
        )
        .header(TRACE_ID_HEADER, "trace-codex-image-stream-local-123")
        .header(
            http::header::USER_AGENT,
            "codex_cli_rs/0.250.0",
        )
        .body(
            "{\"model\":\"gpt-image-2\",\"prompt\":\"生成一张水墨视觉海报\",\"background\":\"auto\",\"quality\":\"auto\",\"size\":\"auto\",\"stream\":true,\"response_format\":\"b64_json\"}",
        )
        .send()
        .await
        .expect("request should succeed");

    let response_status = response.status();
    let response_content_type = response
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let response_text = response.text().await.expect("body should read");
    assert_eq!(response_status, StatusCode::OK, "{response_text}");
    assert_eq!(
        response_content_type.as_deref(),
        Some("text/event-stream"),
        "{response_text}"
    );
    assert!(response_text.contains("event: image_generation.completed"));
    assert!(response_text.contains("\"type\":\"image_generation.completed\""));
    assert!(response_text.contains("\"b64_json\":\"aGVsbG8=\""));
    assert!(response_text.contains("\"total_tokens\":33"));
    assert!(!response_text.contains("response.completed"));

    let seen_refresh_request = seen_refresh
        .lock()
        .expect("mutex should lock")
        .clone()
        .expect("refresh request should be captured");
    assert_eq!(
        seen_refresh_request.content_type,
        "application/x-www-form-urlencoded"
    );
    assert!(seen_refresh_request
        .body
        .contains("refresh_token=rt-codex-image-stream-local-123"));

    let seen_execution_runtime_request = seen_execution_runtime
        .lock()
        .expect("mutex should lock")
        .clone()
        .expect("execution runtime stream should be captured");
    assert_eq!(
        seen_execution_runtime_request.trace_id,
        "trace-codex-image-stream-local-123"
    );
    assert_eq!(
        seen_execution_runtime_request.url,
        "https://chatgpt.com/backend-api/codex/images/generations"
    );
    assert_eq!(
        seen_execution_runtime_request.authorization,
        "Bearer refreshed-codex-image-stream-access-token"
    );
    assert_eq!(
        seen_execution_runtime_request.body,
        json!({
            "prompt": "生成一张水墨视觉海报",
            "background": "auto",
            "model": "gpt-image-2",
            "quality": "auto",
            "size": "auto"
        })
    );
    // 来路 codex_cli_rs 新版本经透传保留到上游计划，不再被内置回退版本覆盖。
    assert_eq!(
        seen_execution_runtime_request.headers["user-agent"],
        "codex_cli_rs/0.250.0"
    );
    assert_eq!(
        seen_execution_runtime_request.headers["originator"],
        "codex_cli_rs"
    );
    for header in ["x-client-request-id", "session-id", "thread-id"] {
        assert!(seen_execution_runtime_request.headers.get(header).is_none());
    }
    assert!(!seen_execution_runtime_request.plan_stream);

    gateway_handle.abort();
    execution_runtime_handle.abort();
    refresh_handle.abort();
}

#[test]
fn gateway_executes_codex_image_stream_with_key_model_allowlist_via_real_local_upstream() {
    run_stream_image_test(
        "gateway_executes_codex_image_stream_with_key_model_allowlist_via_real_local_upstream",
        gateway_executes_codex_image_stream_with_key_model_allowlist_via_real_local_upstream_impl,
    );
}

// 流式真实链路：不给执行 runtime 传 override（空字符串被过滤为 None），网关以进程内
// 直连执行器真实 HTTP 访问本机图片上游；客户端 stream=true，codex 图片上游返回同步
// JSON，由网关桥接为 image SSE。Key.allowed_models 为刷新补全后的持久化状态
// [gpt-5.4-mini, gpt-image-2]，请求必须穿过真实模型准入门。
async fn gateway_executes_codex_image_stream_with_key_model_allowlist_via_real_local_upstream_impl()
{
    #[derive(Debug, Clone)]
    struct SeenUpstreamImageRequest {
        method: String,
        path: String,
        authorization: String,
        user_agent: String,
        originator: String,
        body: serde_json::Value,
    }

    #[derive(Debug, Clone)]
    struct SeenRefreshRequest {
        content_type: String,
        body: String,
    }

    fn hash_api_key(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn sample_auth_snapshot(api_key_id: &str, user_id: &str) -> StoredAuthApiKeySnapshot {
        StoredAuthApiKeySnapshot::new(
            user_id.to_string(),
            "alice".to_string(),
            Some("alice@example.com".to_string()),
            "user".to_string(),
            "local".to_string(),
            true,
            false,
            Some(serde_json::json!(["openai", "codex"])),
            Some(serde_json::json!(["openai:image"])),
            Some(serde_json::json!(["gpt-image-2"])),
            api_key_id.to_string(),
            Some("default".to_string()),
            true,
            false,
            false,
            Some(60),
            Some(5),
            Some(4_102_444_800_i64),
            Some(serde_json::json!(["openai", "codex"])),
            Some(serde_json::json!(["openai:image"])),
            Some(serde_json::json!(["gpt-image-2"])),
        )
        .expect("auth snapshot should build")
    }

    fn sample_candidate_row() -> StoredMinimalCandidateSelectionRow {
        StoredMinimalCandidateSelectionRow {
            provider_id: "provider-codex-image-stream-allowlist-local-1".to_string(),
            provider_name: "codex".to_string(),
            provider_type: "codex".to_string(),
            provider_priority: 10,
            provider_is_active: true,
            endpoint_id: "endpoint-codex-image-stream-allowlist-local-1".to_string(),
            endpoint_api_format: "openai:image".to_string(),
            endpoint_api_family: Some("openai".to_string()),
            endpoint_kind: Some("image".to_string()),
            endpoint_is_active: true,
            key_id: "key-codex-image-stream-allowlist-local-1".to_string(),
            key_name: "oauth".to_string(),
            key_auth_type: "oauth".to_string(),
            key_is_active: true,
            key_api_formats: Some(vec!["openai:image".to_string()]),
            // 发现刷新补全后的持久化白名单：文本模型保留，gpt-image-2 被补进。
            key_allowed_models: Some(vec!["gpt-5.4-mini".to_string(), "gpt-image-2".to_string()]),
            key_capabilities: None,
            key_internal_priority: 5,
            key_global_priority_by_format: Some(serde_json::json!({"openai:image": 1})),
            routing_facts: Default::default(),
            model_id: "model-codex-image-stream-allowlist-local-1".to_string(),
            global_model_id: "global-model-codex-image-stream-allowlist-local-1".to_string(),
            global_model_name: "gpt-image-2".to_string(),
            global_model_mappings: None,
            global_model_supports_streaming: Some(true),
            model_provider_model_name: "gpt-image-2".to_string(),
            model_provider_model_mappings: Some(vec![StoredProviderModelMapping {
                name: "gpt-image-2".to_string(),
                priority: 1,
                api_formats: Some(vec!["openai:image".to_string()]),
                endpoint_ids: None,
                operations: None,
            }]),
            model_supports_streaming: Some(true),
            model_is_active: true,
            model_is_available: true,
        }
    }

    fn sample_provider_catalog_provider() -> StoredProviderCatalogProvider {
        StoredProviderCatalogProvider::new(
            "provider-codex-image-stream-allowlist-local-1".to_string(),
            "codex".to_string(),
            Some("https://chatgpt.com".to_string()),
            "codex".to_string(),
        )
        .expect("provider should build")
        .with_transport_fields(
            true,
            false,
            false,
            None,
            Some(2),
            None,
            Some(20.0),
            None,
            None,
        )
    }

    fn sample_provider_catalog_endpoint(base_url: &str) -> StoredProviderCatalogEndpoint {
        StoredProviderCatalogEndpoint::new(
            "endpoint-codex-image-stream-allowlist-local-1".to_string(),
            "provider-codex-image-stream-allowlist-local-1".to_string(),
            "openai:image".to_string(),
            Some("openai".to_string()),
            Some("image".to_string()),
            true,
        )
        .expect("endpoint should build")
        .with_transport_fields(
            format!("{base_url}/backend-api/codex"),
            None,
            None,
            Some(2),
            None,
            None,
            None,
            None,
        )
        .expect("endpoint transport should build")
    }

    fn sample_provider_catalog_key() -> StoredProviderCatalogKey {
        let encrypted_auth_config = encrypt_python_fernet_plaintext(
            DEVELOPMENT_ENCRYPTION_KEY,
            r#"{"provider_type":"codex","refresh_token":"rt-codex-image-stream-allowlist-local-123"}"#,
        )
        .expect("auth config should encrypt");
        StoredProviderCatalogKey::new(
            "key-codex-image-stream-allowlist-local-1".to_string(),
            "provider-codex-image-stream-allowlist-local-1".to_string(),
            "oauth".to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("key should build")
        .with_transport_fields(
            Some(serde_json::json!(["openai:image"])),
            encrypt_python_fernet_plaintext(DEVELOPMENT_ENCRYPTION_KEY, "__placeholder__")
                .expect("placeholder api key should encrypt"),
            Some(encrypted_auth_config),
            None,
            Some(serde_json::json!({"openai:image": 1})),
            // 持久化在 Key 上的同一份白名单，供执行期 transport 快照读取。
            Some(serde_json::json!(["gpt-5.4-mini", "gpt-image-2"])),
            None,
            None,
            None,
        )
        .expect("key transport should build")
    }

    let seen_upstream_requests = Arc::new(Mutex::new(Vec::<SeenUpstreamImageRequest>::new()));
    let seen_upstream_requests_clone = Arc::clone(&seen_upstream_requests);
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let upstream_hits_clone = Arc::clone(&upstream_hits);
    let seen_refresh = Arc::new(Mutex::new(None::<SeenRefreshRequest>));
    let seen_refresh_clone = Arc::clone(&seen_refresh);
    let refresh_hits = Arc::new(Mutex::new(0usize));
    let refresh_hits_clone = Arc::clone(&refresh_hits);

    let refresh = Router::new().route(
        "/oauth/token",
        any(move |request: Request| {
            let seen_refresh_inner = Arc::clone(&seen_refresh_clone);
            let refresh_hits_inner = Arc::clone(&refresh_hits_clone);
            async move {
                *refresh_hits_inner.lock().expect("mutex should lock") += 1;
                let (parts, body) = request.into_parts();
                let raw_body = to_bytes(body, usize::MAX).await.expect("body should read");
                *seen_refresh_inner.lock().expect("mutex should lock") = Some(SeenRefreshRequest {
                    content_type: parts
                        .headers
                        .get(http::header::CONTENT_TYPE)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .to_string(),
                    body: String::from_utf8(raw_body.to_vec())
                        .expect("refresh body should be utf8"),
                });
                Json(json!({
                    "access_token": "refreshed-codex-image-stream-allowlist-access-token",
                    "refresh_token": "rt-codex-image-stream-allowlist-local-456",
                    "token_type": "Bearer",
                    "expires_in": 3600
                }))
            }
        }),
    );

    let upstream = Router::new().route(
        "/backend-api/codex/images/generations",
        any(move |request: Request| {
            let seen_upstream_inner = Arc::clone(&seen_upstream_requests_clone);
            let upstream_hits_inner = Arc::clone(&upstream_hits_clone);
            async move {
                *upstream_hits_inner.lock().expect("mutex should lock") += 1;
                let (parts, body) = request.into_parts();
                let raw_body = to_bytes(body, usize::MAX).await.expect("body should read");
                let payload: serde_json::Value =
                    serde_json::from_slice(&raw_body).expect("upstream image payload should parse");
                seen_upstream_inner.lock().expect("mutex should lock").push(
                    SeenUpstreamImageRequest {
                        method: parts.method.to_string(),
                        path: parts.uri.path().to_string(),
                        authorization: parts
                            .headers
                            .get(http::header::AUTHORIZATION)
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or_default()
                            .to_string(),
                        user_agent: parts
                            .headers
                            .get(http::header::USER_AGENT)
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or_default()
                            .to_string(),
                        originator: parts
                            .headers
                            .get("originator")
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or_default()
                            .to_string(),
                        body: payload,
                    },
                );
                let upstream_json = json!({
                    "created": 1776972205_u64,
                    "data": [{
                        "b64_json": "c3RyZWFtLXJlYWwtdXBzdHJlYW0taW1hZ2U=",
                        "revised_prompt": "流式真实上游图片"
                    }],
                    "usage": {"input_tokens": 11, "output_tokens": 22, "total_tokens": 33}
                });
                let encoded = serde_json::to_vec(&upstream_json).expect("json should encode");
                let chunks = encoded
                    .chunks(64)
                    .map(axum::body::Bytes::copy_from_slice)
                    .map(|chunk| Ok::<_, std::convert::Infallible>(chunk))
                    .collect::<Vec<_>>();
                let stream = futures_util::stream::iter(chunks);
                let mut response = axum::response::Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from_stream(stream))
                    .expect("response should build");
                response.headers_mut().insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/json"),
                );
                response
            }
        }),
    );

    let client_api_key = "sk-client-codex-image-stream-allowlist-local";
    let auth_repository = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![(
        Some(hash_api_key(client_api_key)),
        sample_auth_snapshot(
            "key-codex-image-stream-allowlist-client-123",
            "user-codex-image-stream-allowlist-client-123",
        ),
    )]));
    let candidate_selection_repository =
        Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(vec![
            sample_candidate_row(),
        ]));

    let (refresh_url, refresh_handle) = start_server(refresh).await;
    let (upstream_url, upstream_handle) = start_server(upstream).await;
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider_catalog_provider()],
        vec![sample_provider_catalog_endpoint(&upstream_url)],
        vec![sample_provider_catalog_key()],
    ));

    let oauth_refresh =
        crate::provider_transport::LocalOAuthRefreshCoordinator::with_adapters_for_tests(vec![
            Arc::new(
                crate::provider_transport::oauth_refresh::GenericOAuthRefreshAdapter::default()
                    .with_token_url_for_tests("codex", format!("{refresh_url}/oauth/token")),
            ),
        ]);
    // 空 override：网关使用进程内直连执行器，真实 HTTP 访问本机图片上游。
    let gateway_state = build_state_with_execution_runtime_override("")
        .with_data_state_for_tests(
            crate::data::GatewayDataState::with_auth_candidate_selection_provider_catalog_and_request_candidate_repository_for_tests(
                auth_repository,
                candidate_selection_repository,
                provider_catalog_repository,
                Arc::new(InMemoryRequestCandidateRepository::default()),
                DEVELOPMENT_ENCRYPTION_KEY,
            ),
        )
        .with_oauth_refresh_coordinator_for_tests(oauth_refresh);
    let gateway = build_router_with_state(gateway_state);
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .post(format!("{gateway_url}/v1/images/generations"))
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {client_api_key}"),
        )
        .header(
            TRACE_ID_HEADER,
            "trace-codex-image-stream-allowlist-local-123",
        )
        .body(
            "{\"model\":\"gpt-image-2\",\"prompt\":\"生成一张水墨视觉海报\",\"background\":\"auto\",\"quality\":\"high\",\"size\":\"1024x1024\",\"n\":1,\"stream\":true,\"response_format\":\"b64_json\"}",
        )
        .send()
        .await
        .expect("request should succeed");

    let response_status = response.status();
    let response_content_type = response
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    // 内部桥接标记是网关私有信号，不得外泄到客户端响应头。
    let response_had_bridge_marker_header = response
        .headers()
        .contains_key("x-aether-bridged-client-sse");
    let response_text = response.text().await.expect("body should read");
    assert_eq!(response_status, StatusCode::OK, "{response_text}");
    assert_eq!(
        response_content_type.as_deref(),
        Some("text/event-stream"),
        "{response_text}"
    );
    assert!(
        !response_had_bridge_marker_header,
        "internal bridge marker must not leak to the client response"
    );
    assert!(
        response_text.contains("event: image_generation.completed"),
        "{response_text}"
    );
    assert!(response_text.contains("\"type\":\"image_generation.completed\""));
    assert!(
        response_text.contains("\"b64_json\":\"c3RyZWFtLXJlYWwtdXBzdHJlYW0taW1hZ2U=\""),
        "{response_text}"
    );
    assert!(response_text.contains("\"total_tokens\":33"));
    assert!(!response_text.contains("response.completed"));

    let seen_refresh_request = seen_refresh
        .lock()
        .expect("mutex should lock")
        .clone()
        .expect("refresh request should be captured");
    assert_eq!(
        seen_refresh_request.content_type,
        "application/x-www-form-urlencoded"
    );
    assert!(seen_refresh_request
        .body
        .contains("refresh_token=rt-codex-image-stream-allowlist-local-123"));
    assert_eq!(*refresh_hits.lock().expect("mutex should lock"), 1);

    let upstream_requests = seen_upstream_requests
        .lock()
        .expect("mutex should lock")
        .clone();
    assert_eq!(*upstream_hits.lock().expect("mutex should lock"), 1);
    assert_eq!(upstream_requests.len(), 1);
    let seen_upstream_request = &upstream_requests[0];
    assert_eq!(seen_upstream_request.method, "POST");
    assert_eq!(
        seen_upstream_request.path,
        "/backend-api/codex/images/generations"
    );
    assert_eq!(
        seen_upstream_request.authorization,
        "Bearer refreshed-codex-image-stream-allowlist-access-token"
    );
    assert_eq!(
        seen_upstream_request.user_agent,
        aether_ai_formats::CODEX_CLIENT_USER_AGENT
    );
    assert_eq!(seen_upstream_request.originator, "codex_cli_rs");
    assert_eq!(
        seen_upstream_request.body,
        json!({
            "prompt": "生成一张水墨视觉海报",
            "background": "auto",
            "model": "gpt-image-2",
            "n": 1,
            "quality": "high",
            "size": "1024x1024"
        })
    );

    gateway_handle.abort();
    upstream_handle.abort();
    refresh_handle.abort();
}

#[test]
fn gateway_bridges_codex_image_sync_json_to_streaming_image_sse() {
    run_stream_image_test(
        "gateway_bridges_codex_image_sync_json_to_streaming_image_sse",
        gateway_bridges_codex_image_sync_json_to_streaming_image_sse_impl,
    );
}

async fn gateway_bridges_codex_image_sync_json_to_streaming_image_sse_impl() {
    #[derive(Debug, Clone)]
    struct SeenExecutionRuntimeStreamRequest {
        trace_id: String,
        request_stream: bool,
        plan_stream: bool,
    }

    fn hash_api_key(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn sample_auth_snapshot(api_key_id: &str, user_id: &str) -> StoredAuthApiKeySnapshot {
        StoredAuthApiKeySnapshot::new(
            user_id.to_string(),
            "alice".to_string(),
            Some("alice@example.com".to_string()),
            "user".to_string(),
            "local".to_string(),
            true,
            false,
            Some(serde_json::json!(["openai", "codex"])),
            Some(serde_json::json!(["openai:image"])),
            Some(serde_json::json!(["gpt-image-2"])),
            api_key_id.to_string(),
            Some("default".to_string()),
            true,
            false,
            false,
            Some(60),
            Some(5),
            Some(4_102_444_800_i64),
            Some(serde_json::json!(["openai", "codex"])),
            Some(serde_json::json!(["openai:image"])),
            Some(serde_json::json!(["gpt-image-2"])),
        )
        .expect("auth snapshot should build")
    }

    fn sample_candidate_row() -> StoredMinimalCandidateSelectionRow {
        StoredMinimalCandidateSelectionRow {
            provider_id: "provider-codex-image-stream-local-1".to_string(),
            provider_name: "codex".to_string(),
            provider_type: "codex".to_string(),
            provider_priority: 10,
            provider_is_active: true,
            endpoint_id: "endpoint-codex-image-stream-local-1".to_string(),
            endpoint_api_format: "openai:image".to_string(),
            endpoint_api_family: Some("openai".to_string()),
            endpoint_kind: Some("image".to_string()),
            endpoint_is_active: true,
            key_id: "key-codex-image-stream-local-1".to_string(),
            key_name: "oauth".to_string(),
            key_auth_type: "oauth".to_string(),
            key_is_active: true,
            key_api_formats: Some(vec!["openai:image".to_string()]),
            key_allowed_models: None,
            key_capabilities: None,
            key_internal_priority: 5,
            key_global_priority_by_format: Some(serde_json::json!({"openai:image": 1})),
            routing_facts: Default::default(),
            model_id: "model-codex-image-stream-local-1".to_string(),
            global_model_id: "global-model-codex-image-stream-local-1".to_string(),
            global_model_name: "gpt-image-2".to_string(),
            global_model_mappings: None,
            global_model_supports_streaming: Some(true),
            model_provider_model_name: "gpt-image-2".to_string(),
            model_provider_model_mappings: Some(vec![StoredProviderModelMapping {
                name: "gpt-image-2".to_string(),
                priority: 1,
                api_formats: Some(vec!["openai:image".to_string()]),
                endpoint_ids: None,
                operations: None,
            }]),
            model_supports_streaming: Some(true),
            model_is_active: true,
            model_is_available: true,
        }
    }

    fn sample_provider_catalog_provider() -> StoredProviderCatalogProvider {
        StoredProviderCatalogProvider::new(
            "provider-codex-image-stream-local-1".to_string(),
            "codex".to_string(),
            Some("https://chatgpt.com".to_string()),
            "codex".to_string(),
        )
        .expect("provider should build")
        .with_transport_fields(
            true,
            false,
            false,
            None,
            Some(2),
            None,
            Some(20.0),
            None,
            None,
        )
    }

    fn sample_provider_catalog_endpoint() -> StoredProviderCatalogEndpoint {
        StoredProviderCatalogEndpoint::new(
            "endpoint-codex-image-stream-local-1".to_string(),
            "provider-codex-image-stream-local-1".to_string(),
            "openai:image".to_string(),
            Some("openai".to_string()),
            Some("image".to_string()),
            true,
        )
        .expect("endpoint should build")
        .with_transport_fields(
            "https://chatgpt.com/backend-api/codex".to_string(),
            None,
            None,
            Some(2),
            None,
            None,
            None,
            None,
        )
        .expect("endpoint transport should build")
    }

    fn sample_provider_catalog_key() -> StoredProviderCatalogKey {
        let encrypted_auth_config = encrypt_python_fernet_plaintext(
            DEVELOPMENT_ENCRYPTION_KEY,
            r#"{"provider_type":"codex","refresh_token":"rt-codex-image-stream-local-123"}"#,
        )
        .expect("auth config should encrypt");
        StoredProviderCatalogKey::new(
            "key-codex-image-stream-local-1".to_string(),
            "provider-codex-image-stream-local-1".to_string(),
            "oauth".to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("key should build")
        .with_transport_fields(
            Some(serde_json::json!(["openai:image"])),
            encrypt_python_fernet_plaintext(DEVELOPMENT_ENCRYPTION_KEY, "__placeholder__")
                .expect("placeholder api key should encrypt"),
            Some(encrypted_auth_config),
            None,
            Some(serde_json::json!({"openai:image": 1})),
            None,
            None,
            None,
            None,
        )
        .expect("key transport should build")
    }

    let seen_execution_runtime = Arc::new(Mutex::new(None::<SeenExecutionRuntimeStreamRequest>));
    let seen_execution_runtime_clone = Arc::clone(&seen_execution_runtime);
    let refresh = Router::new().route(
        "/oauth/token",
        any(move |_request: Request| async move {
            Json(json!({
                "access_token": "refreshed-codex-image-stream-access-token",
                "refresh_token": "rt-codex-image-stream-local-456",
                "token_type": "Bearer",
                "expires_in": 3600
            }))
        }),
    );

    let execution_runtime = Router::new().route(
        "/v1/execute/stream",
        any(move |request: Request| {
            let seen_execution_runtime_inner = Arc::clone(&seen_execution_runtime_clone);
            async move {
                let (parts, body) = request.into_parts();
                let raw_body = to_bytes(body, usize::MAX).await.expect("body should read");
                let payload: serde_json::Value = serde_json::from_slice(&raw_body)
                    .expect("execution runtime payload should parse");
                *seen_execution_runtime_inner
                    .lock()
                    .expect("mutex should lock") = Some(SeenExecutionRuntimeStreamRequest {
                    trace_id: parts
                        .headers
                        .get(TRACE_ID_HEADER)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .to_string(),
                    request_stream: payload
                        .get("body")
                        .and_then(|value| value.get("json_body"))
                        .and_then(|value| value.get("stream"))
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false),
                    plan_stream: payload
                        .get("stream")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false),
                });
                let frames = concat!(
                    "{\"type\":\"headers\",\"payload\":{\"kind\":\"headers\",\"status_code\":200,\"headers\":{\"content-type\":\"application/json\"}}}\n",
                    "{\"type\":\"data\",\"payload\":{\"kind\":\"data\",\"text\":\"{\\\"created\\\":1776991097,\\\"data\\\":[{\\\"b64_json\\\":\\\"aGVsbG8=\\\"}],\\\"usage\\\":{\\\"total_tokens\\\":100,\\\"input_tokens\\\":50,\\\"output_tokens\\\":50,\\\"input_tokens_details\\\":{\\\"text_tokens\\\":10,\\\"image_tokens\\\":40}}}\"}}\n",
                    "{\"type\":\"telemetry\",\"payload\":{\"kind\":\"telemetry\",\"telemetry\":{\"elapsed_ms\":41}}}\n",
                    "{\"type\":\"eof\",\"payload\":{\"kind\":\"eof\"}}\n"
                );
                let mut response = http::Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from(frames))
                    .expect("response should build");
                response.headers_mut().insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/x-ndjson"),
                );
                response
            }
        }),
    );

    let client_api_key = "sk-client-codex-image-stream-local";
    let auth_repository = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![(
        Some(hash_api_key(client_api_key)),
        sample_auth_snapshot(
            "key-codex-image-stream-client-123",
            "user-codex-image-stream-client-123",
        ),
    )]));
    let candidate_selection_repository =
        Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(vec![
            sample_candidate_row(),
        ]));
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider_catalog_provider()],
        vec![sample_provider_catalog_endpoint()],
        vec![sample_provider_catalog_key()],
    ));

    let (refresh_url, refresh_handle) = start_server(refresh).await;
    let (execution_runtime_url, execution_runtime_handle) = start_server(execution_runtime).await;
    let oauth_refresh =
        crate::provider_transport::LocalOAuthRefreshCoordinator::with_adapters_for_tests(vec![
            Arc::new(
                crate::provider_transport::oauth_refresh::GenericOAuthRefreshAdapter::default()
                    .with_token_url_for_tests("codex", format!("{refresh_url}/oauth/token")),
            ),
        ]);
    let gateway_state = build_state_with_execution_runtime_override(execution_runtime_url.clone())
        .with_data_state_for_tests(
            crate::data::GatewayDataState::with_auth_candidate_selection_provider_catalog_and_request_candidate_repository_for_tests(
                auth_repository,
                candidate_selection_repository,
                provider_catalog_repository,
                Arc::new(InMemoryRequestCandidateRepository::default()),
                DEVELOPMENT_ENCRYPTION_KEY,
            ),
        )
        .with_oauth_refresh_coordinator_for_tests(oauth_refresh);
    let gateway = build_router_with_state(gateway_state);
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .post(format!("{gateway_url}/v1/images/generations"))
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {client_api_key}"),
        )
        .header(TRACE_ID_HEADER, "trace-codex-image-stream-json-123")
        .body("{\"model\":\"gpt-image-2\",\"prompt\":\"生成一张海报\",\"stream\":true}")
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let response_text = response.text().await.expect("body should read");
    assert!(response_text.contains("event: image_generation.completed"));
    assert!(response_text.contains("\"type\":\"image_generation.completed\""));
    assert!(response_text.contains("\"b64_json\":\"aGVsbG8=\""));
    assert!(response_text.contains("\"total_tokens\":100"));
    assert!(!response_text.trim_start().starts_with('{'));
    assert!(!response_text.contains("\"created\": 1776991097"));

    let seen_execution_runtime_request = seen_execution_runtime
        .lock()
        .expect("mutex should lock")
        .clone()
        .expect("execution runtime stream should be captured");
    assert_eq!(
        seen_execution_runtime_request.trace_id,
        "trace-codex-image-stream-json-123"
    );
    assert!(!seen_execution_runtime_request.request_stream);
    assert!(!seen_execution_runtime_request.plan_stream);

    gateway_handle.abort();
    execution_runtime_handle.abort();
    refresh_handle.abort();
}

#[derive(Debug, Clone)]
struct SeenImageBridgeExecutionPlan {
    trace_id: String,
    client_api_format: String,
    provider_api_format: String,
    url: String,
    plan_stream: bool,
    auth_header: String,
    body_json: serde_json::Value,
}

fn image_bridge_hash_api_key(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn image_bridge_auth_snapshot(api_key_id: &str, user_id: &str) -> StoredAuthApiKeySnapshot {
    StoredAuthApiKeySnapshot::new(
        user_id.to_string(),
        "alice".to_string(),
        Some("alice@example.com".to_string()),
        "user".to_string(),
        "local".to_string(),
        true,
        false,
        None,
        Some(serde_json::json!([
            "openai:chat",
            "openai:responses",
            "openai:image"
        ])),
        Some(serde_json::json!(["gpt-image-2"])),
        api_key_id.to_string(),
        Some("default".to_string()),
        true,
        false,
        false,
        Some(60),
        Some(5),
        Some(4_102_444_800_i64),
        None,
        Some(serde_json::json!([
            "openai:chat",
            "openai:responses",
            "openai:image"
        ])),
        Some(serde_json::json!(["gpt-image-2"])),
    )
    .expect("auth snapshot should build")
}

fn image_bridge_candidate_row(
    prefix: &str,
    provider_name: &str,
    provider_type: &str,
) -> StoredMinimalCandidateSelectionRow {
    let key_auth_type = if provider_type == "chatgpt_web" {
        "bearer"
    } else {
        "api_key"
    };
    StoredMinimalCandidateSelectionRow {
        provider_id: format!("provider-{prefix}"),
        provider_name: provider_name.to_string(),
        provider_type: provider_type.to_string(),
        provider_priority: 10,
        provider_is_active: true,
        endpoint_id: format!("endpoint-{prefix}"),
        endpoint_api_format: "openai:image".to_string(),
        endpoint_api_family: Some("openai".to_string()),
        endpoint_kind: Some("image".to_string()),
        endpoint_is_active: true,
        key_id: format!("key-{prefix}"),
        key_name: "prod".to_string(),
        key_auth_type: key_auth_type.to_string(),
        key_is_active: true,
        key_api_formats: Some(vec!["openai:image".to_string()]),
        key_allowed_models: None,
        key_capabilities: None,
        key_internal_priority: 5,
        key_global_priority_by_format: Some(serde_json::json!({"openai:image": 1})),
        routing_facts: Default::default(),
        model_id: format!("model-{prefix}"),
        global_model_id: format!("global-model-{prefix}"),
        global_model_name: "gpt-image-2".to_string(),
        global_model_mappings: None,
        global_model_supports_streaming: Some(false),
        model_provider_model_name: "gpt-image-2".to_string(),
        model_provider_model_mappings: Some(vec![StoredProviderModelMapping {
            name: "gpt-image-2".to_string(),
            priority: 1,
            api_formats: Some(vec!["openai:image".to_string()]),
            endpoint_ids: None,
            operations: None,
        }]),
        model_supports_streaming: Some(false),
        model_is_active: true,
        model_is_available: true,
    }
}

fn image_bridge_provider_catalog_provider(
    prefix: &str,
    provider_name: &str,
    provider_type: &str,
    base_url: &str,
) -> StoredProviderCatalogProvider {
    StoredProviderCatalogProvider::new(
        format!("provider-{prefix}"),
        provider_name.to_string(),
        Some(base_url.to_string()),
        provider_type.to_string(),
    )
    .expect("provider should build")
    .with_transport_fields(
        true,
        false,
        false,
        None,
        Some(2),
        None,
        Some(20.0),
        None,
        None,
    )
}

fn image_bridge_provider_catalog_endpoint(
    prefix: &str,
    base_url: &str,
) -> StoredProviderCatalogEndpoint {
    StoredProviderCatalogEndpoint::new(
        format!("endpoint-{prefix}"),
        format!("provider-{prefix}"),
        "openai:image".to_string(),
        Some("openai".to_string()),
        Some("image".to_string()),
        true,
    )
    .expect("endpoint should build")
    .with_transport_fields(
        base_url.to_string(),
        None,
        None,
        Some(2),
        None,
        None,
        None,
        None,
    )
    .expect("endpoint transport should build")
}

fn image_bridge_provider_catalog_key(
    prefix: &str,
    provider_type: &str,
) -> StoredProviderCatalogKey {
    let auth_type = if provider_type == "chatgpt_web" {
        "bearer"
    } else {
        "api_key"
    };
    StoredProviderCatalogKey::new(
        format!("key-{prefix}"),
        format!("provider-{prefix}"),
        "prod".to_string(),
        auth_type.to_string(),
        None,
        true,
    )
    .expect("key should build")
    .with_transport_fields(
        Some(serde_json::json!(["openai:image"])),
        encrypt_python_fernet_plaintext(DEVELOPMENT_ENCRYPTION_KEY, "sk-upstream-image-bridge")
            .expect("api key should encrypt"),
        None,
        None,
        Some(serde_json::json!({"openai:image": 1})),
        None,
        None,
        None,
        None,
    )
    .expect("key transport should build")
}

async fn start_image_bridge_gateway(
    prefix: &str,
    provider_name: &str,
    provider_type: &str,
    base_url: &str,
    execution_runtime_url: String,
) -> (
    String,
    tokio::task::JoinHandle<()>,
    String,
    Arc<InMemoryRequestCandidateRepository>,
) {
    let client_api_key = format!("sk-client-{prefix}");
    let auth_repository = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![(
        Some(image_bridge_hash_api_key(&client_api_key)),
        image_bridge_auth_snapshot(&format!("api-key-{prefix}"), &format!("user-{prefix}")),
    )]));
    let candidate_selection_repository =
        Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(vec![
            image_bridge_candidate_row(prefix, provider_name, provider_type),
        ]));
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![image_bridge_provider_catalog_provider(
            prefix,
            provider_name,
            provider_type,
            base_url,
        )],
        vec![image_bridge_provider_catalog_endpoint(prefix, base_url)],
        vec![image_bridge_provider_catalog_key(prefix, provider_type)],
    ));
    let request_candidate_repository = Arc::new(InMemoryRequestCandidateRepository::default());
    let gateway_state = build_state_with_execution_runtime_override(execution_runtime_url)
        .with_data_state_for_tests(
            crate::data::GatewayDataState::with_auth_candidate_selection_provider_catalog_and_request_candidate_repository_for_tests(
                auth_repository,
                candidate_selection_repository,
                provider_catalog_repository,
                Arc::clone(&request_candidate_repository),
                DEVELOPMENT_ENCRYPTION_KEY,
            ),
        );
    let gateway = build_router_with_state(gateway_state);
    let (gateway_url, gateway_handle) = start_server(gateway).await;
    (
        gateway_url,
        gateway_handle,
        client_api_key,
        request_candidate_repository,
    )
}

fn capture_image_bridge_execution_plan(
    parts: http::request::Parts,
    payload: serde_json::Value,
) -> SeenImageBridgeExecutionPlan {
    SeenImageBridgeExecutionPlan {
        trace_id: parts
            .headers
            .get(TRACE_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string(),
        client_api_format: payload
            .get("client_api_format")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        provider_api_format: payload
            .get("provider_api_format")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        url: payload
            .get("url")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        plan_stream: payload
            .get("stream")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        auth_header: payload
            .get("headers")
            .and_then(|value| value.get("authorization"))
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        body_json: payload
            .get("body")
            .and_then(|value| value.get("json_body"))
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    }
}

fn image_bridge_execution_runtime(
    seen_execution_plan: Arc<Mutex<Option<SeenImageBridgeExecutionPlan>>>,
) -> Router {
    Router::new().route(
        "/v1/execute/stream",
        any(move |request: Request| {
            let seen_execution_plan_inner = Arc::clone(&seen_execution_plan);
            async move {
                let (parts, body) = request.into_parts();
                let raw_body = to_bytes(body, usize::MAX).await.expect("body should read");
                let payload: serde_json::Value = serde_json::from_slice(&raw_body)
                    .expect("execution runtime payload should parse");
                *seen_execution_plan_inner.lock().expect("mutex should lock") =
                    Some(capture_image_bridge_execution_plan(parts, payload));
                let frames = concat!(
                    "{\"type\":\"headers\",\"payload\":{\"kind\":\"headers\",\"status_code\":200,\"headers\":{\"content-type\":\"text/event-stream\"}}}\n",
                    "{\"type\":\"data\",\"payload\":{\"kind\":\"data\",\"text\":\"event: response.output_item.done\\ndata: {\\\"type\\\":\\\"response.output_item.done\\\",\\\"output_index\\\":0,\\\"item\\\":{\\\"id\\\":\\\"ig_bridge_123\\\",\\\"type\\\":\\\"image_generation_call\\\",\\\"result\\\":\\\"aGVsbG8=\\\",\\\"output_format\\\":\\\"png\\\"}}\\n\\n\"}}\n",
                    "{\"type\":\"data\",\"payload\":{\"kind\":\"data\",\"text\":\"event: response.completed\\ndata: {\\\"type\\\":\\\"response.completed\\\",\\\"response\\\":{\\\"id\\\":\\\"resp_bridge_123\\\",\\\"object\\\":\\\"response\\\",\\\"model\\\":\\\"gpt-image-2\\\",\\\"status\\\":\\\"completed\\\",\\\"output\\\":[]}}\\n\\n\"}}\n",
                    "{\"type\":\"eof\",\"payload\":{\"kind\":\"eof\"}}\n"
                );
                let mut response = http::Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from(frames))
                    .expect("response should build");
                response.headers_mut().insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/x-ndjson"),
                );
                response
            }
        }),
    )
}

#[test]
fn gateway_routes_openai_responses_stream_image_intent_to_openai_image_plan_without_streaming_support(
) {
    run_stream_image_test(
        "gateway_routes_openai_responses_stream_image_intent_to_openai_image_plan_without_streaming_support",
        gateway_routes_openai_responses_stream_image_intent_to_openai_image_plan_without_streaming_support_impl,
    );
}

async fn gateway_routes_openai_responses_stream_image_intent_to_openai_image_plan_without_streaming_support_impl(
) {
    let seen_execution_plan = Arc::new(Mutex::new(None::<SeenImageBridgeExecutionPlan>));
    let execution_runtime = image_bridge_execution_runtime(Arc::clone(&seen_execution_plan));
    let (execution_runtime_url, execution_runtime_handle) = start_server(execution_runtime).await;
    let (gateway_url, gateway_handle, client_api_key, _request_candidate_repository) =
        start_image_bridge_gateway(
            "responses-stream-image-bridge",
            "image-provider",
            "custom",
            "https://images.example.com/v1",
            execution_runtime_url,
        )
        .await;

    let response = reqwest::Client::new()
        .post(format!("{gateway_url}/v1/responses"))
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::AUTHORIZATION, format!("Bearer {client_api_key}"))
        .header(TRACE_ID_HEADER, "trace-responses-stream-image-bridge-123")
        .body(
            r#"{"model":"gpt-image-2","input":"Draw a mountain observatory","tools":[{"type":"image_generation","size":"1024x1024"}],"tool_choice":{"type":"image_generation"},"stream":true}"#,
        )
        .send()
        .await
        .expect("request should succeed");

    let status = response.status();
    let response_text = response.text().await.expect("body should read");
    assert_eq!(status, StatusCode::OK, "{response_text}");
    assert!(response_text.contains("response.output_item.done"));
    assert!(response_text.contains("image_generation_call"));

    let seen_plan = seen_execution_plan
        .lock()
        .expect("mutex should lock")
        .clone()
        .expect("execution plan should be captured");
    assert_eq!(
        seen_plan.trace_id,
        "trace-responses-stream-image-bridge-123"
    );
    assert_eq!(seen_plan.client_api_format, "openai:responses");
    assert_eq!(seen_plan.provider_api_format, "openai:image");
    assert_eq!(
        seen_plan.url,
        "https://images.example.com/v1/images/generations"
    );
    assert!(!seen_plan.plan_stream);
    assert_eq!(seen_plan.auth_header, "Bearer sk-upstream-image-bridge");
    assert_eq!(seen_plan.body_json["model"], "gpt-image-2");
    assert_eq!(seen_plan.body_json["prompt"], "Draw a mountain observatory");
    assert_eq!(seen_plan.body_json["size"], "1024x1024");
    assert!(seen_plan.body_json.get("stream").is_none());
    assert!(seen_plan.body_json.get("input").is_none());
    assert!(seen_plan.body_json.get("tools").is_none());

    gateway_handle.abort();
    execution_runtime_handle.abort();
}
