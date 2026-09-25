use std::sync::{Arc, Mutex};
use std::time::Duration;

use aether_contracts::ExecutionPlan;
use aether_crypto::{encrypt_python_fernet_plaintext, DEVELOPMENT_ENCRYPTION_KEY};
use aether_data::repository::global_models::InMemoryGlobalModelReadRepository;
use aether_data::repository::provider_catalog::InMemoryProviderCatalogReadRepository;
use aether_data_contracts::repository::global_models::{
    AdminProviderModelListQuery, GlobalModelReadRepository, StoredAdminGlobalModel,
    StoredAdminProviderModel,
};
use aether_data_contracts::repository::provider_catalog::{
    ProviderCatalogReadRepository, StoredProviderCatalogEndpoint, StoredProviderCatalogKey,
    StoredProviderCatalogProvider,
};
use axum::routing::any;
use axum::{extract::Request, Json, Router};
use serde_json::json;

use super::{perform_model_fetch_for_key, perform_model_fetch_once, ModelFetchRunSummary};
use crate::AppState;

async fn start_server(app: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = crate::test_support::bind_loopback_listener()
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("local addr should resolve");
    let handle = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .expect("server should run");
    });
    (format!("http://{addr}"), handle)
}

fn build_state_with_execution_runtime_override(
    execution_runtime_override_base_url: impl Into<String>,
) -> AppState {
    AppState::new()
        .expect("gateway should build")
        .with_execution_runtime_override_base_url(execution_runtime_override_base_url)
}

fn sample_provider(provider_id: &str) -> StoredProviderCatalogProvider {
    StoredProviderCatalogProvider::new(
        provider_id.to_string(),
        "openai".to_string(),
        Some("https://example.com".to_string()),
        "custom".to_string(),
    )
    .expect("provider should build")
    .with_transport_fields(true, false, true, None, None, None, None, None, None)
}

fn sample_endpoint(
    provider_id: &str,
    endpoint_id: &str,
    base_url: &str,
) -> StoredProviderCatalogEndpoint {
    StoredProviderCatalogEndpoint::new(
        endpoint_id.to_string(),
        provider_id.to_string(),
        "openai:chat".to_string(),
        Some("openai".to_string()),
        Some("chat".to_string()),
        true,
    )
    .expect("endpoint should build")
    .with_transport_fields(
        base_url.to_string(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("endpoint transport should build")
}

fn sample_key(provider_id: &str, key_id: &str) -> StoredProviderCatalogKey {
    let mut key = StoredProviderCatalogKey::new(
        key_id.to_string(),
        provider_id.to_string(),
        "primary".to_string(),
        "api_key".to_string(),
        None,
        true,
    )
    .expect("key should build")
    .with_transport_fields(
        Some(json!(["openai:chat"])),
        encrypt_python_fernet_plaintext(DEVELOPMENT_ENCRYPTION_KEY, "live-secret-api-key")
            .expect("api key should encrypt"),
        None,
        None,
        None,
        Some(json!(["gpt-4.1"])),
        None,
        None,
        None,
    )
    .expect("key transport should build");
    key.auto_fetch_models = true;
    key.locked_models = Some(json!(["locked-model"]));
    key.model_include_patterns = Some(json!(["gpt-*"]));
    key.model_exclude_patterns = Some(json!(["gpt-beta"]));
    key
}

fn sample_global_model(id: &str, name: &str, mappings: &[&str]) -> StoredAdminGlobalModel {
    StoredAdminGlobalModel::new(
        id.to_string(),
        name.to_string(),
        name.to_string(),
        true,
        None,
        None,
        None,
        Some(json!({ "model_mappings": mappings })),
        0,
        0,
        0,
        Some(1_711_000_000),
        Some(1_711_000_000),
    )
    .expect("global model should build")
}

fn sample_provider_model(
    id: &str,
    provider_id: &str,
    global_model_id: &str,
    provider_model_name: &str,
    global_model_name: &str,
    mappings: &[&str],
) -> StoredAdminProviderModel {
    StoredAdminProviderModel::new(
        id.to_string(),
        provider_id.to_string(),
        global_model_id.to_string(),
        provider_model_name.to_string(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        true,
        true,
        None,
        Some(1_711_000_000),
        Some(1_711_000_000),
        Some(global_model_name.to_string()),
        Some(global_model_name.to_string()),
        None,
        None,
        None,
        Some(json!({ "model_mappings": mappings })),
    )
    .expect("provider model should build")
}

struct TestEnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl Drop for TestEnvVarGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.as_deref() {
            std::env::set_var(self.key, previous);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

fn set_test_env_var(key: &'static str, value: &str) -> TestEnvVarGuard {
    let previous = std::env::var(key).ok();
    std::env::set_var(key, value);
    TestEnvVarGuard { key, previous }
}

#[tokio::test]
async fn gateway_model_fetch_updates_key_and_syncs_provider_model_whitelist_associations() {
    let seen_execution_runtime_plan = Arc::new(Mutex::new(None::<ExecutionPlan>));
    let seen_execution_runtime_plan_clone = Arc::clone(&seen_execution_runtime_plan);
    let execution_runtime = Router::new().route(
        "/v1/execute/sync",
        any(move |Json(plan): Json<ExecutionPlan>| {
            let seen_execution_runtime_plan = Arc::clone(&seen_execution_runtime_plan_clone);
            async move {
                *seen_execution_runtime_plan
                    .lock()
                    .expect("mutex should lock") = Some(plan);
                Json(json!({
                    "request_id": "req-model-fetch-key-1",
                    "status_code": 200,
                    "headers": {
                        "content-type": "application/json"
                    },
                    "body": {
                        "json_body": {
                            "data": [
                                {"id": "gpt-5"},
                                {"id": "gpt-beta"},
                                {"id": "other-model"}
                            ]
                        }
                    }
                }))
            }
        }),
    );
    let (execution_runtime_url, execution_runtime_handle) = start_server(execution_runtime).await;

    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-openai")],
        vec![sample_endpoint(
            "provider-openai",
            "endpoint-openai",
            "https://api.openai.example",
        )],
        vec![sample_key("provider-openai", "key-openai")],
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![
                sample_global_model("global-model-gpt5", "gpt-5", &[]),
                sample_global_model("global-model-gpt4", "gpt-4.1", &["gpt-4\\.1"]),
            ])
            .with_admin_provider_models(vec![sample_provider_model(
                "provider-model-gpt4",
                "provider-openai",
                "global-model-gpt4",
                "gpt-4.1",
                "gpt-4.1",
                &["gpt-4\\.1"],
            )])
            .with_endpoint_provider_ids([("endpoint-openai", "provider-openai")]),
    );
    let data_state = crate::data::GatewayDataState::disabled()
        .attach_provider_catalog_repository_for_tests(Arc::clone(&provider_catalog_repository))
        .with_global_model_repository_for_tests(Arc::clone(&global_model_repository))
        .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY);
    let state = build_state_with_execution_runtime_override(execution_runtime_url)
        .with_data_state_for_tests(data_state);

    let summary = perform_model_fetch_once(&state)
        .await
        .expect("model fetch should succeed");
    assert_eq!(
        summary,
        ModelFetchRunSummary {
            attempted: 1,
            succeeded: 1,
            failed: 0,
            skipped: 0,
        }
    );

    let seen_plan = seen_execution_runtime_plan
        .lock()
        .expect("mutex should lock")
        .clone()
        .expect("execution runtime plan should be captured");
    assert_eq!(seen_plan.method, "GET");
    assert_eq!(seen_plan.url, "https://api.openai.example/models");
    assert_eq!(
        seen_plan.headers.get("authorization").map(String::as_str),
        Some("Bearer live-secret-api-key")
    );

    let updated_key = provider_catalog_repository
        .list_keys_by_ids(&["key-openai".to_string()])
        .await
        .expect("keys should load")
        .into_iter()
        .next()
        .expect("updated key should exist");
    assert_eq!(
        updated_key.allowed_models,
        Some(json!(["gpt-5", "locked-model"]))
    );
    assert_eq!(updated_key.last_models_fetch_error, None);
    assert!(updated_key.last_models_fetch_at_unix_secs.is_some());

    let provider_models = global_model_repository
        .list_admin_provider_models(&AdminProviderModelListQuery {
            provider_id: "provider-openai".to_string(),
            is_active: None,
            offset: 0,
            limit: 10_000,
        })
        .await
        .expect("provider models should load");
    let mut provider_model_names = provider_models
        .iter()
        .map(|model| model.provider_model_name.as_str())
        .collect::<Vec<_>>();
    provider_model_names.sort_unstable();
    assert_eq!(provider_model_names, vec!["gpt-4.1", "gpt-5"]);
    assert!(provider_models
        .iter()
        .any(|model| model.provider_model_name == "gpt-4.1" && !model.is_available));
    assert!(provider_models
        .iter()
        .any(|model| model.provider_model_name == "gpt-5" && model.is_available));

    execution_runtime_handle.abort();
}

#[tokio::test]
async fn codex_preset_model_fetch_associates_the_api_supported_review_model() {
    let provider = StoredProviderCatalogProvider::new(
        "provider-codex".to_string(),
        "codex".to_string(),
        Some("https://chatgpt.com".to_string()),
        "codex".to_string(),
    )
    .expect("provider should build")
    .with_transport_fields(true, false, true, None, None, None, None, None, None);
    let mut key = sample_key("provider-codex", "key-codex");
    key.locked_models = None;
    key.model_include_patterns = None;
    key.model_exclude_patterns = None;
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![provider],
        Vec::new(),
        vec![key],
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new()).with_admin_global_models(vec![
            sample_global_model(
                "global-model-codex-auto-review",
                "codex-auto-review",
                &["codex-auto-review"],
            ),
        ]),
    );
    let data_state = crate::data::GatewayDataState::disabled()
        .attach_provider_catalog_repository_for_tests(Arc::clone(&provider_catalog_repository))
        .with_global_model_repository_for_tests(Arc::clone(&global_model_repository))
        .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY);
    let state = AppState::new()
        .expect("gateway should build")
        .with_data_state_for_tests(data_state);

    let summary = perform_model_fetch_once(&state)
        .await
        .expect("Codex preset model fetch should succeed");
    assert_eq!(
        summary,
        ModelFetchRunSummary {
            attempted: 1,
            succeeded: 1,
            failed: 0,
            skipped: 0,
        }
    );

    let updated_key = provider_catalog_repository
        .list_keys_by_ids(&["key-codex".to_string()])
        .await
        .expect("keys should load")
        .into_iter()
        .next()
        .expect("updated key should exist");
    assert!(updated_key
        .allowed_models
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .is_some_and(|models| models.iter().any(|model| model == "codex-auto-review")));
    assert!(updated_key
        .upstream_metadata
        .as_ref()
        .is_some_and(|metadata| {
            metadata["codex_models"]["cards"]["codex-auto-review"]["supported_in_api"] == true
        }));

    let provider_models = global_model_repository
        .list_admin_provider_models(&AdminProviderModelListQuery {
            provider_id: "provider-codex".to_string(),
            is_active: None,
            offset: 0,
            limit: 10_000,
        })
        .await
        .expect("provider models should load");
    assert!(provider_models.iter().any(|model| {
        model.provider_model_name == "codex-auto-review"
            && model.global_model_id == "global-model-codex-auto-review"
    }));
}

#[tokio::test]
async fn gateway_model_fetch_updates_key_and_syncs_provider_model_whitelist_associations_without_execution_runtime_override(
) {
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SeenUpstreamRequest {
        method: String,
        authorization: String,
    }

    let seen_upstream = Arc::new(Mutex::new(None::<SeenUpstreamRequest>));
    let seen_upstream_clone = Arc::clone(&seen_upstream);
    let upstream = Router::new().route(
        "/models",
        any(move |request: Request| {
            let seen_upstream_inner = Arc::clone(&seen_upstream_clone);
            async move {
                let authorization = request
                    .headers()
                    .get(http::header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                *seen_upstream_inner.lock().expect("mutex should lock") =
                    Some(SeenUpstreamRequest {
                        method: request.method().as_str().to_string(),
                        authorization,
                    });
                Json(json!({
                    "data": [
                        {"id": "gpt-5"},
                        {"id": "gpt-beta"},
                        {"id": "other-model"}
                    ]
                }))
            }
        }),
    );
    let (upstream_url, upstream_handle) = start_server(upstream).await;

    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-openai")],
        vec![sample_endpoint(
            "provider-openai",
            "endpoint-openai",
            &upstream_url,
        )],
        vec![sample_key("provider-openai", "key-openai")],
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![
                sample_global_model("global-model-gpt5", "gpt-5", &["gpt-5"]),
                sample_global_model("global-model-gpt4", "gpt-4.1", &["gpt-4\\.1"]),
            ])
            .with_admin_provider_models(vec![sample_provider_model(
                "provider-model-gpt4",
                "provider-openai",
                "global-model-gpt4",
                "gpt-4.1",
                "gpt-4.1",
                &["gpt-4\\.1"],
            )])
            .with_endpoint_provider_ids([("endpoint-openai", "provider-openai")]),
    );
    let data_state = crate::data::GatewayDataState::disabled()
        .attach_provider_catalog_repository_for_tests(Arc::clone(&provider_catalog_repository))
        .with_global_model_repository_for_tests(Arc::clone(&global_model_repository))
        .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY);
    let state = AppState::new()
        .expect("state should build")
        .with_data_state_for_tests(data_state);

    let summary = perform_model_fetch_once(&state)
        .await
        .expect("model fetch should succeed");
    assert_eq!(
        summary,
        ModelFetchRunSummary {
            attempted: 1,
            succeeded: 1,
            failed: 0,
            skipped: 0,
        }
    );

    assert_eq!(
        seen_upstream.lock().expect("mutex should lock").clone(),
        Some(SeenUpstreamRequest {
            method: "GET".to_string(),
            authorization: "Bearer live-secret-api-key".to_string(),
        })
    );

    let updated_key = provider_catalog_repository
        .list_keys_by_ids(&["key-openai".to_string()])
        .await
        .expect("keys should load")
        .into_iter()
        .next()
        .expect("updated key should exist");
    assert_eq!(
        updated_key.allowed_models,
        Some(json!(["gpt-5", "locked-model"]))
    );
    assert_eq!(updated_key.last_models_fetch_error, None);
    assert!(updated_key.last_models_fetch_at_unix_secs.is_some());

    upstream_handle.abort();
}

#[tokio::test]
async fn gateway_background_model_fetch_updates_key_and_syncs_provider_model_whitelist_associations(
) {
    let _startup_enabled = set_test_env_var("MODEL_FETCH_STARTUP_ENABLED", "true");
    let _startup_delay = set_test_env_var("MODEL_FETCH_STARTUP_DELAY_SECONDS", "0");

    let seen_execution_runtime_plan = Arc::new(Mutex::new(None::<ExecutionPlan>));
    let seen_execution_runtime_plan_clone = Arc::clone(&seen_execution_runtime_plan);
    let execution_runtime = Router::new().route(
        "/v1/execute/sync",
        any(move |Json(plan): Json<ExecutionPlan>| {
            let seen_execution_runtime_plan = Arc::clone(&seen_execution_runtime_plan_clone);
            async move {
                *seen_execution_runtime_plan
                    .lock()
                    .expect("mutex should lock") = Some(plan);
                Json(json!({
                    "request_id": "req-model-fetch-key-1",
                    "status_code": 200,
                    "headers": {
                        "content-type": "application/json"
                    },
                    "body": {
                        "json_body": {
                            "data": [
                                {"id": "gpt-5"},
                                {"id": "gpt-beta"},
                                {"id": "other-model"}
                            ]
                        }
                    }
                }))
            }
        }),
    );
    let (execution_runtime_url, execution_runtime_handle) = start_server(execution_runtime).await;

    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-openai")],
        vec![sample_endpoint(
            "provider-openai",
            "endpoint-openai",
            "https://api.openai.example",
        )],
        vec![sample_key("provider-openai", "key-openai")],
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![
                sample_global_model("global-model-gpt5", "gpt-5", &["gpt-5"]),
                sample_global_model("global-model-gpt4", "gpt-4.1", &["gpt-4\\.1"]),
            ])
            .with_admin_provider_models(vec![sample_provider_model(
                "provider-model-gpt4",
                "provider-openai",
                "global-model-gpt4",
                "gpt-4.1",
                "gpt-4.1",
                &["gpt-4\\.1"],
            )])
            .with_endpoint_provider_ids([("endpoint-openai", "provider-openai")]),
    );
    let data_state = crate::data::GatewayDataState::disabled()
        .attach_provider_catalog_repository_for_tests(Arc::clone(&provider_catalog_repository))
        .with_global_model_repository_for_tests(Arc::clone(&global_model_repository))
        .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY);
    let gateway_state = build_state_with_execution_runtime_override(execution_runtime_url)
        .with_data_state_for_tests(data_state);

    let background_tasks = gateway_state.spawn_background_tasks();
    assert!(
        !background_tasks.is_empty(),
        "model fetch worker should spawn"
    );

    let updated_key = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let updated_key = provider_catalog_repository
                .list_keys_by_ids(&["key-openai".to_string()])
                .await
                .expect("keys should load")
                .into_iter()
                .next()
                .expect("updated key should exist");
            if updated_key.last_models_fetch_at_unix_secs.is_some() {
                break updated_key;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("background worker should fetch models on startup");

    assert_eq!(
        updated_key.allowed_models,
        Some(json!(["gpt-5", "locked-model"]))
    );
    assert_eq!(updated_key.last_models_fetch_error, None);

    let provider_models = global_model_repository
        .list_admin_provider_models(&AdminProviderModelListQuery {
            provider_id: "provider-openai".to_string(),
            is_active: None,
            offset: 0,
            limit: 10_000,
        })
        .await
        .expect("provider models should load");
    let mut provider_model_names = provider_models
        .iter()
        .map(|model| model.provider_model_name.as_str())
        .collect::<Vec<_>>();
    provider_model_names.sort_unstable();
    assert_eq!(provider_model_names, vec!["gpt-4.1", "gpt-5"]);
    assert!(provider_models
        .iter()
        .any(|model| model.provider_model_name == "gpt-4.1" && !model.is_available));
    assert!(provider_models
        .iter()
        .any(|model| model.provider_model_name == "gpt-5" && model.is_available));

    let seen_plan = seen_execution_runtime_plan
        .lock()
        .expect("mutex should lock")
        .clone()
        .expect("execution runtime plan should be captured");
    assert_eq!(seen_plan.url, "https://api.openai.example/models");

    background_tasks.shutdown().await;
    execution_runtime_handle.abort();
}

/// Codex 专用 Endpoint fixture：api_format 可为 openai:responses / openai:image。
fn codex_image_sample_endpoint(
    endpoint_id: &str,
    provider_id: &str,
    api_format: &str,
    base_url: &str,
) -> StoredProviderCatalogEndpoint {
    StoredProviderCatalogEndpoint::new(
        endpoint_id.to_string(),
        provider_id.to_string(),
        api_format.to_string(),
        None,
        None,
        true,
    )
    .expect("codex endpoint should build")
    .with_transport_fields(
        base_url.to_string(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("codex endpoint transport should build")
}

fn codex_image_sample_provider(provider_id: &str) -> StoredProviderCatalogProvider {
    StoredProviderCatalogProvider::new(
        provider_id.to_string(),
        "codex".to_string(),
        Some("https://chatgpt.com".to_string()),
        "codex".to_string(),
    )
    .expect("codex provider should build")
    .with_transport_fields(true, false, true, None, None, None, None, None, None)
}

/// Codex 专用 Key fixture：api_formats 与旧白名单显式指定。
fn codex_image_sample_key(
    key_id: &str,
    provider_id: &str,
    api_formats: &[&str],
    allowed_models: Option<Vec<&str>>,
) -> StoredProviderCatalogKey {
    let mut key = StoredProviderCatalogKey::new(
        key_id.to_string(),
        provider_id.to_string(),
        "primary".to_string(),
        "api_key".to_string(),
        None,
        true,
    )
    .expect("codex key should build")
    .with_transport_fields(
        Some(json!(api_formats)),
        encrypt_python_fernet_plaintext(DEVELOPMENT_ENCRYPTION_KEY, "live-secret-api-key")
            .expect("api key should encrypt"),
        None,
        None,
        None,
        allowed_models.map(|models| json!(models)),
        None,
        None,
        None,
    )
    .expect("codex key transport should build");
    key.auto_fetch_models = true;
    key
}

/// 断言 Provider 模型的 Endpoint 绑定集合与来源。
async fn assert_codex_image_model_bindings(
    global_model_repository: &Arc<InMemoryGlobalModelReadRepository>,
    model_id: &str,
    expected_endpoint_ids: &[&str],
) {
    let bindings = global_model_repository
        .list_model_endpoint_bindings(&[model_id.to_string()])
        .await
        .expect("bindings should load");
    assert_eq!(
        bindings
            .iter()
            .map(|binding| binding.endpoint_id.as_str())
            .collect::<Vec<_>>(),
        expected_endpoint_ids
    );
    assert!(bindings
        .iter()
        .all(|binding| binding.source == "discovered"));
}

#[tokio::test]
/// 文本目录强制刷新必须修复旧白名单（缺 gpt-image-2），恢复既有 image ProviderModel
/// 可用性并绑定图片 Endpoint；原生 codex_models 元数据不得写入本地补全卡。
async fn codex_image_model_fetch_repairs_legacy_whitelist_and_restores_image_model_availability() {
    let upstream = Router::new().route(
        "/backend-api/codex/models",
        any(|| async {
            Json(json!({
                "models": [
                    {
                        "slug": "gpt-5.4-codex",
                        "family": "gpt-5.4",
                        "display_name": "GPT-5.4 Codex"
                    }
                ]
            }))
        }),
    );
    let (upstream_url, upstream_handle) = start_server(upstream).await;
    let base_url = format!("{}/backend-api/codex", upstream_url.trim_end_matches('/'));
    let provider = codex_image_sample_provider("provider-codex-image");
    let endpoints = vec![
        codex_image_sample_endpoint(
            "endpoint-codex-responses",
            "provider-codex-image",
            "openai:responses",
            &base_url,
        ),
        codex_image_sample_endpoint(
            "endpoint-codex-image",
            "provider-codex-image",
            "openai:image",
            &base_url,
        ),
    ];
    // 旧白名单：上次文本目录刷新写入的结果，缺 gpt-image-2。
    let key = codex_image_sample_key(
        "key-codex-image",
        "provider-codex-image",
        &["openai:responses", "openai:image"],
        Some(vec!["gpt-5.4-codex"]),
    );
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![provider],
        endpoints,
        vec![key],
    ));
    let mut legacy_image_model = sample_provider_model(
        "provider-model-image",
        "provider-codex-image",
        "global-model-image",
        "gpt-image-2",
        "gpt-image-2",
        &["gpt-image-2"],
    );
    // 上一次 availability reconcile 因旧白名单缺失把图片模型标记为不可用。
    legacy_image_model.is_available = false;
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![
                sample_global_model("global-model-image", "gpt-image-2", &["gpt-image-2"]),
                sample_global_model("global-model-codex", "gpt-5.4-codex", &["gpt-5.4-codex"]),
            ])
            .with_admin_provider_models(vec![legacy_image_model])
            .with_endpoint_provider_ids([
                ("endpoint-codex-responses", "provider-codex-image"),
                ("endpoint-codex-image", "provider-codex-image"),
            ]),
    );
    let data_state = crate::data::GatewayDataState::disabled()
        .attach_provider_catalog_repository_for_tests(Arc::clone(&provider_catalog_repository))
        .with_global_model_repository_for_tests(Arc::clone(&global_model_repository))
        .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY);
    let state = AppState::new()
        .expect("gateway should build")
        .with_data_state_for_tests(data_state);

    let summary = perform_model_fetch_for_key(&state, "provider-codex-image", "key-codex-image")
        .await
        .expect("codex model fetch should succeed");
    assert_eq!(
        summary,
        ModelFetchRunSummary {
            attempted: 1,
            succeeded: 1,
            failed: 0,
            skipped: 0,
        }
    );

    let updated_key = provider_catalog_repository
        .list_keys_by_ids(&["key-codex-image".to_string()])
        .await
        .expect("keys should load")
        .into_iter()
        .next()
        .expect("updated key should exist");
    // 强制刷新修复旧白名单：文本目录模型与补全的 gpt-image-2 同时写入。
    assert_eq!(
        updated_key.allowed_models,
        Some(json!(["gpt-5.4-codex", "gpt-image-2"]))
    );
    // 原生 codex_models 元数据不写入本地补全卡，但保留上游文本卡。
    let catalog_cards = updated_key
        .upstream_metadata
        .as_ref()
        .and_then(|metadata| metadata["codex_models"]["cards"].as_object())
        .expect("codex model catalog metadata should persist");
    assert!(catalog_cards.contains_key("gpt-5.4-codex"));
    assert!(!catalog_cards.contains_key("gpt-image-2"));

    let provider_models = global_model_repository
        .list_admin_provider_models(&AdminProviderModelListQuery {
            provider_id: "provider-codex-image".to_string(),
            is_active: None,
            offset: 0,
            limit: 10_000,
        })
        .await
        .expect("provider models should load");
    let image_model = provider_models
        .iter()
        .find(|model| model.provider_model_name == "gpt-image-2")
        .expect("image provider model should exist");
    assert!(
        image_model.is_available,
        "per-key force refresh must reconcile and restore image model availability"
    );
    assert_codex_image_model_bindings(
        &global_model_repository,
        &image_model.id,
        &["endpoint-codex-image"],
    )
    .await;

    upstream_handle.abort();
}

#[tokio::test]
/// Key 格式不允许 openai:image 或 Provider 无图片 Endpoint 时不得扩权：
/// 白名单不出现 gpt-image-2，既有 image ProviderModel 保持不可用。
async fn codex_image_model_fetch_without_image_permission_or_endpoint_does_not_expand_whitelist() {
    async fn run_scenario(
        include_image_endpoint: bool,
        key_api_formats: &[&str],
        exclude_patterns: &[&str],
    ) -> (Option<serde_json::Value>, bool) {
        let upstream = Router::new().route(
            "/backend-api/codex/models",
            any(|| async {
                Json(json!({
                    "models": [
                        {
                            "slug": "gpt-5.4-codex",
                            "family": "gpt-5.4",
                            "display_name": "GPT-5.4 Codex"
                        }
                    ]
                }))
            }),
        );
        let (upstream_url, upstream_handle) = start_server(upstream).await;
        let base_url = format!("{}/backend-api/codex", upstream_url.trim_end_matches('/'));
        let provider = codex_image_sample_provider("provider-codex-image");
        let mut endpoints = vec![codex_image_sample_endpoint(
            "endpoint-codex-responses",
            "provider-codex-image",
            "openai:responses",
            &base_url,
        )];
        if include_image_endpoint {
            endpoints.push(codex_image_sample_endpoint(
                "endpoint-codex-image",
                "provider-codex-image",
                "openai:image",
                &base_url,
            ));
        }
        let mut key = codex_image_sample_key(
            "key-codex-image",
            "provider-codex-image",
            key_api_formats,
            Some(vec!["gpt-5.4-codex"]),
        );
        if !exclude_patterns.is_empty() {
            key.model_exclude_patterns = Some(json!(exclude_patterns));
        }
        let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
            vec![provider],
            endpoints,
            vec![key],
        ));
        let mut legacy_image_model = sample_provider_model(
            "provider-model-image",
            "provider-codex-image",
            "global-model-image",
            "gpt-image-2",
            "gpt-image-2",
            &["gpt-image-2"],
        );
        legacy_image_model.is_available = false;
        let mut endpoint_provider_ids = vec![("endpoint-codex-responses", "provider-codex-image")];
        if include_image_endpoint {
            endpoint_provider_ids.push(("endpoint-codex-image", "provider-codex-image"));
        }
        let global_model_repository = Arc::new(
            InMemoryGlobalModelReadRepository::seed(Vec::new())
                .with_admin_global_models(vec![sample_global_model(
                    "global-model-image",
                    "gpt-image-2",
                    &["gpt-image-2"],
                )])
                .with_admin_provider_models(vec![legacy_image_model])
                .with_endpoint_provider_ids(endpoint_provider_ids),
        );
        let data_state = crate::data::GatewayDataState::disabled()
            .attach_provider_catalog_repository_for_tests(Arc::clone(&provider_catalog_repository))
            .with_global_model_repository_for_tests(Arc::clone(&global_model_repository))
            .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY);
        let state = AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(data_state);

        perform_model_fetch_for_key(&state, "provider-codex-image", "key-codex-image")
            .await
            .expect("codex model fetch should succeed");
        upstream_handle.abort();

        let updated_key = provider_catalog_repository
            .list_keys_by_ids(&["key-codex-image".to_string()])
            .await
            .expect("keys should load")
            .into_iter()
            .next()
            .expect("updated key should exist");
        let provider_models = global_model_repository
            .list_admin_provider_models(&AdminProviderModelListQuery {
                provider_id: "provider-codex-image".to_string(),
                is_active: None,
                offset: 0,
                limit: 10_000,
            })
            .await
            .expect("provider models should load");
        let image_available = provider_models
            .iter()
            .find(|model| model.provider_model_name == "gpt-image-2")
            .expect("image provider model should exist")
            .is_available;
        (updated_key.allowed_models, image_available)
    }

    // Key 不允许 openai:image：即使存在 active 图片 Endpoint 也不补全、不恢复可用。
    let (allowed_models, image_available) = run_scenario(true, &["openai:responses"], &[]).await;
    assert_eq!(allowed_models, Some(json!(["gpt-5.4-codex"])));
    assert!(!image_available);

    // Provider 无图片 Endpoint：Key 格式允许 openai:image 也不补全。
    let (allowed_models, image_available) =
        run_scenario(false, &["openai:responses", "openai:image"], &[]).await;
    assert_eq!(allowed_models, Some(json!(["gpt-5.4-codex"])));
    assert!(!image_available);

    // 双 Endpoint 双格式允许图片，但 exclude 模式过滤补全 ID：白名单不含 image、
    // 既有 image ProviderModel 保持不可用。
    let (allowed_models, image_available) = run_scenario(
        true,
        &["openai:responses", "openai:image"],
        &["gpt-image-*"],
    )
    .await;
    assert_eq!(allowed_models, Some(json!(["gpt-5.4-codex"])));
    assert!(!image_available);
}
