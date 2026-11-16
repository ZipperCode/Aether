use super::*;
use aether_crypto::{encrypt_python_fernet_plaintext, DEVELOPMENT_ENCRYPTION_KEY};
use aether_data::repository::auth::InMemoryAuthApiKeySnapshotRepository;
use aether_data::repository::candidate_selection::InMemoryMinimalCandidateSelectionReadRepository;
use aether_data::repository::candidates::InMemoryRequestCandidateRepository;
use aether_data::repository::provider_catalog::InMemoryProviderCatalogReadRepository;
use aether_data_contracts::repository::auth::StoredAuthApiKeySnapshot;
use aether_data_contracts::repository::candidate_selection::{
    StoredMinimalCandidateSelectionRow, StoredProviderModelMapping,
};
use aether_data_contracts::repository::candidates::{
    RequestCandidateReadRepository, RequestCandidateStatus,
};
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use sha2::{Digest, Sha256};

#[test]
fn gateway_audio_transcription_uses_antigravity_lifecycle_and_rejects_unsupported_input() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(audio_transcription_impl());
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn audio_transcription_impl() {
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
            Some(serde_json::json!(["openai", "google"])),
            Some(serde_json::json!(["openai:audio:transcriptions"])),
            Some(serde_json::json!(["audio-client-model"])),
            api_key_id.to_string(),
            Some("default".to_string()),
            true,
            false,
            false,
            Some(60),
            Some(5),
            Some(4_102_444_800_i64),
            Some(serde_json::json!(["openai", "google"])),
            Some(serde_json::json!(["openai:audio:transcriptions"])),
            Some(serde_json::json!(["audio-client-model"])),
        )
        .expect("auth snapshot should build")
    }

    fn sample_candidate_row() -> StoredMinimalCandidateSelectionRow {
        StoredMinimalCandidateSelectionRow {
            provider_id: "provider-gemini-image-bridge-1".to_string(),
            provider_name: "google".to_string(),
            provider_type: "google".to_string(),
            provider_priority: 10,
            provider_is_active: true,
            endpoint_id: "endpoint-gemini-image-bridge-1".to_string(),
            endpoint_api_format: "gemini:generate_content".to_string(),
            endpoint_api_family: Some("gemini".to_string()),
            endpoint_kind: Some("chat".to_string()),
            endpoint_is_active: true,
            key_id: "key-gemini-image-bridge-1".to_string(),
            key_name: "prod".to_string(),
            key_auth_type: "api_key".to_string(),
            key_is_active: true,
            key_api_formats: Some(vec!["gemini:generate_content".to_string()]),
            key_allowed_models: None,
            key_capabilities: None,
            key_internal_priority: 5,
            key_global_priority_by_format: Some(serde_json::json!({
                "gemini:generate_content": 1
            })),
            routing_facts: Default::default(),
            model_id: "model-gemini-image-bridge-1".to_string(),
            global_model_id: "global-model-gemini-image-bridge-1".to_string(),
            global_model_name: "audio-client-model".to_string(),
            global_model_mappings: None,
            global_model_supports_streaming: Some(true),
            model_provider_model_name: "audio-account-model".to_string(),
            model_provider_model_mappings: Some(vec![StoredProviderModelMapping {
                name: "audio-account-model".to_string(),
                priority: 1,
                api_formats: Some(vec!["gemini:generate_content".to_string()]),
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
            "provider-gemini-image-bridge-1".to_string(),
            "google".to_string(),
            Some("https://generativelanguage.googleapis.com".to_string()),
            "google".to_string(),
        )
        .expect("provider should build")
        .with_transport_fields(
            true,
            false,
            true,
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
            "endpoint-gemini-image-bridge-1".to_string(),
            "provider-gemini-image-bridge-1".to_string(),
            "gemini:generate_content".to_string(),
            Some("gemini".to_string()),
            Some("chat".to_string()),
            true,
        )
        .expect("endpoint should build")
        .with_transport_fields(
            "https://generativelanguage.googleapis.com".to_string(),
            None,
            Some(serde_json::json!([
                {"action":"drop","path":"model"}
            ])),
            Some(2),
            None,
            None,
            None,
            None,
        )
        .expect("endpoint transport should build")
    }

    fn sample_provider_catalog_key() -> StoredProviderCatalogKey {
        StoredProviderCatalogKey::new(
            "key-gemini-image-bridge-1".to_string(),
            "provider-gemini-image-bridge-1".to_string(),
            "prod".to_string(),
            "api_key".to_string(),
            None,
            true,
        )
        .expect("key should build")
        .with_transport_fields(
            Some(serde_json::json!(["gemini:generate_content"])),
            encrypt_python_fernet_plaintext(DEVELOPMENT_ENCRYPTION_KEY, "sk-upstream-gemini-image")
                .expect("api key should encrypt"),
            None,
            None,
            Some(serde_json::json!({"gemini:generate_content": 1})),
            None,
            None,
            None,
            None,
        )
        .expect("key transport should build")
    }

    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for antigravity in [true, false] {
        let calls_runtime = Arc::clone(&calls);
        let execution_runtime = Router::new().route("/v1/execute/sync", any(move |Json(payload): Json<serde_json::Value>| {
            let calls = Arc::clone(&calls_runtime);
            async move {
                // 成功请求会异步刷新账号配额；该发现调用不属于转录执行次数。
                if payload["url"].as_str().is_some_and(|url| url.ends_with("/v1internal:loadCodeAssist")) {
                    return Json(json!({"request_id":payload["request_id"],"status_code":200,"headers":{"content-type":"application/json"},"body":{"json_body":{"cloudaicompanionProject":"audio-project"}}}));
                }
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                assert!(antigravity, "non-Antigravity provider must never receive audio");
                assert_eq!(payload["url"], "https://daily-cloudcode-pa.googleapis.com/v1internal:generateContent");
                let body = &payload["body"]["json_body"];
                assert_eq!(body["model"], "audio-account-model");
                assert_eq!(body["project"], "audio-project");
                assert_eq!(body["request"]["contents"][0]["parts"][1]["inlineData"], json!({"mimeType":"audio/wav","data":"aGVsbG8="}));
                assert!(body["request"]["contents"][0]["parts"][0]["text"].as_str().unwrap().contains("meeting"));
                Json(json!({"request_id":payload["request_id"],"status_code":200,"headers":{"content-type":"application/json"},
                    "body":{"json_body":{"response":{"candidates":[{"content":{"parts":[{"text":"hello world"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":2}}}},"report_context":payload["report_context"]}))
            }
        }));
        let mut row = sample_candidate_row();
        let mut provider = sample_provider_catalog_provider();
        let mut endpoint = sample_provider_catalog_endpoint();
        let mut key = sample_provider_catalog_key();
        if antigravity {
            row.provider_type = "antigravity".into();
            row.key_auth_type = "oauth".into();
            provider.provider_type = "antigravity".into();
            endpoint.base_url = "https://daily-cloudcode-pa.googleapis.com".into();
            endpoint.body_rules = None;
            key.auth_type = "oauth".into();
            key.encrypted_auth_config = Some(encrypt_python_fernet_plaintext(DEVELOPMENT_ENCRYPTION_KEY,
                &json!({"provider_type":"antigravity","project_id":"audio-project","access_token_import_temporary":true,"headers":{"Authorization":"Bearer audio-token"}}).to_string()).unwrap());
        }
        let client_key = "sk-audio-client";
        let auth = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![(
            Some(hash_api_key(client_key)),
            sample_auth_snapshot("audio-key", "audio-user"),
        )]));
        let candidates = Arc::new(InMemoryRequestCandidateRepository::default());
        let (runtime_url, runtime_handle) = start_server(execution_runtime).await;
        let state = build_state_with_execution_runtime_override(runtime_url).with_data_state_for_tests(
            crate::data::GatewayDataState::with_auth_candidate_selection_provider_catalog_and_request_candidate_repository_for_tests(
                auth, Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(vec![row])),
                Arc::new(InMemoryProviderCatalogReadRepository::seed(vec![provider],vec![endpoint],vec![key])),
                Arc::clone(&candidates), DEVELOPMENT_ENCRYPTION_KEY));
        let (url, handle) = start_server(build_router_with_state(state)).await;
        let form = |format: &str, model: &str| {
            format!("--b\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{model}\r\n--b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"input.wav\"\r\nContent-Type: audio/wav\r\n\r\nhello\r\n--b\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\nmeeting\r\n--b\r\nContent-Disposition: form-data; name=\"response_format\"\r\n\r\n{format}\r\n--b--\r\n")
        };
        let before = calls.load(std::sync::atomic::Ordering::SeqCst);
        for format in ["json", "text"] {
            let response = reqwest::Client::new()
                .post(format!("{url}/v1/audio/transcriptions"))
                .bearer_auth(client_key)
                .header(TRACE_ID_HEADER, format!("trace-audio-{format}"))
                .header("content-type", "multipart/form-data; boundary=b")
                .body(form(format, "audio-client-model"))
                .send()
                .await
                .unwrap();
            let status = response.status();
            let content_type = response
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            let body = response.text().await.unwrap();
            if antigravity {
                if status != StatusCode::OK {
                    let records = candidates
                        .list_by_request_id(&format!("trace-audio-{format}"))
                        .await
                        .unwrap();
                    panic!("audio status={status} body={body} candidates={records:?}");
                }
                if format == "text" {
                    assert!(content_type.starts_with("text/plain"));
                    assert_eq!(body, "hello world");
                } else {
                    assert_eq!(
                        serde_json::from_str::<serde_json::Value>(&body).unwrap()["text"],
                        "hello world"
                    );
                }
                let mut stored = Vec::new();
                for _ in 0..50 {
                    stored = candidates
                        .list_by_request_id(&format!("trace-audio-{format}"))
                        .await
                        .unwrap();
                    if stored.len() == 1 && stored[0].status == RequestCandidateStatus::Success {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                assert_eq!(stored.len(), 1);
                assert_eq!(stored[0].status, RequestCandidateStatus::Success);
            } else {
                assert!(!status.is_success(), "{body}");
            }
        }
        if antigravity {
            for body in [
                form("verbose_json", "audio-client-model"),
                "--b--\r\n".to_string(),
            ] {
                let response = reqwest::Client::new()
                    .post(format!("{url}/v1/audio/transcriptions"))
                    .bearer_auth(client_key)
                    .header("content-type", "multipart/form-data; boundary=b")
                    .body(body)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    StatusCode::BAD_REQUEST,
                    "{}",
                    response.text().await.unwrap()
                );
            }
            let response = reqwest::Client::new()
                .post(format!("{url}/v1/audio/transcriptions"))
                .bearer_auth(client_key)
                .header("content-type", "multipart/form-data; boundary=b")
                .body(form("json", "unfetched-model"))
                .send()
                .await
                .unwrap();
            assert!(!response.status().is_success());
        }
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst) - before,
            if antigravity { 2 } else { 0 }
        );
        handle.abort();
        runtime_handle.abort();
    }
}
