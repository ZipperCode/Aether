use std::sync::{Arc, Mutex};

use aether_data::repository::global_models::InMemoryGlobalModelReadRepository;
use aether_data::repository::provider_catalog::InMemoryProviderCatalogReadRepository;
use aether_data::repository::quota::InMemoryProviderQuotaRepository;
use aether_data_contracts::repository::{
    global_models::{AdminProviderModelListQuery, GlobalModelReadRepository},
    quota::StoredProviderQuotaSnapshot,
};
use axum::body::Body;
use axum::routing::any;
use axum::{extract::Request, Router};
use http::StatusCode;
use serde_json::json;

use super::super::super::{
    build_router_with_state, sample_admin_global_model, sample_admin_provider_model,
    sample_endpoint, sample_key, sample_provider, start_server, AppState,
};
use crate::constants::{
    GATEWAY_HEADER, TRUSTED_ADMIN_SESSION_ID_HEADER, TRUSTED_ADMIN_USER_ID_HEADER,
    TRUSTED_ADMIN_USER_ROLE_HEADER,
};
use crate::data::GatewayDataState;

#[tokio::test]
async fn gateway_handles_admin_provider_models_locally_with_trusted_admin_principal() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let upstream_hits_clone = Arc::clone(&upstream_hits);
    let upstream = Router::new().route(
        "/api/admin/providers/provider-openai/models",
        any(move |_request: Request| {
            let upstream_hits_inner = Arc::clone(&upstream_hits_clone);
            async move {
                *upstream_hits_inner.lock().expect("mutex should lock") += 1;
                (StatusCode::OK, Body::from("unexpected upstream hit"))
            }
        }),
    );

    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-openai", "openai", 10)],
        Vec::new(),
        Vec::new(),
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new()).with_admin_provider_models(vec![
            sample_admin_provider_model(
                "model-openai-gpt5",
                "provider-openai",
                "global-gpt-5",
                "gpt-5-upstream",
            ),
        ]),
    );
    let quota_repository = Arc::new(InMemoryProviderQuotaRepository::seed(vec![]));

    let (upstream_url, upstream_handle) = start_server(upstream).await;
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_global_model_and_quota_readers_for_tests(
                    provider_catalog_repository,
                    global_model_repository,
                    quota_repository,
                ),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .get(format!(
            "{gateway_url}/api/admin/providers/provider-openai/models?skip=0&limit=20"
        ))
        .header(crate::constants::GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    let items = payload.as_array().expect("payload should be an array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "model-openai-gpt5");
    assert_eq!(items[0]["provider_id"], "provider-openai");
    assert_eq!(items[0]["global_model_id"], "global-gpt-5");
    assert_eq!(items[0]["provider_model_name"], "gpt-5-upstream");
    assert_eq!(items[0]["global_model_name"], "gpt-5");
    assert_eq!(items[0]["global_model_display_name"], "GPT 5");
    assert_eq!(items[0]["effective_input_price"], 3.0);
    assert_eq!(items[0]["effective_output_price"], 15.0);
    assert_eq!(items[0]["effective_supports_streaming"], true);
    assert!(items[0]["model_test_capabilities"]["openai:image"].is_null());
    assert_eq!(items[0]["created_at"], "2024-03-21T05:46:40Z");
    assert_eq!(items[0]["updated_at"], "2024-03-21T05:48:20Z");
    assert_eq!(*upstream_hits.lock().expect("mutex should lock"), 0);

    gateway_handle.abort();
    upstream_handle.abort();
}
#[tokio::test]
async fn gateway_handles_admin_provider_model_detail_locally_with_trusted_admin_principal() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let upstream_hits_clone = Arc::clone(&upstream_hits);
    let upstream = Router::new().route(
        "/api/admin/providers/provider-openai/models/model-openai-gpt5",
        any(move |_request: Request| {
            let upstream_hits_inner = Arc::clone(&upstream_hits_clone);
            async move {
                *upstream_hits_inner.lock().expect("mutex should lock") += 1;
                (StatusCode::OK, Body::from("unexpected upstream hit"))
            }
        }),
    );

    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-openai", "openai", 10)],
        Vec::new(),
        Vec::new(),
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new()).with_admin_provider_models(vec![
            sample_admin_provider_model(
                "model-openai-gpt5",
                "provider-openai",
                "global-gpt-5",
                "gpt-5-upstream",
            ),
        ]),
    );
    let quota_repository = Arc::new(InMemoryProviderQuotaRepository::seed(vec![]));

    let (upstream_url, upstream_handle) = start_server(upstream).await;
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_global_model_and_quota_readers_for_tests(
                    provider_catalog_repository,
                    global_model_repository,
                    quota_repository,
                ),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .get(format!(
            "{gateway_url}/api/admin/providers/provider-openai/models/model-openai-gpt5"
        ))
        .header(crate::constants::GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(payload["id"], "model-openai-gpt5");
    assert_eq!(
        payload["provider_model_mappings"],
        json!([{"name": "gpt-5-upstream-alias", "priority": 1}])
    );
    assert_eq!(
        payload["effective_config"],
        json!({
            "billing": {"currency": "USD", "mode": "local"},
            "provider_hint": "gpt-5-upstream",
            "streaming": true,
            "vision": false
        })
    );
    assert_eq!(payload["effective_supports_vision"], true);
    assert_eq!(payload["effective_supports_image_generation"], false);
    assert_eq!(*upstream_hits.lock().expect("mutex should lock"), 0);

    gateway_handle.abort();
    upstream_handle.abort();
}
#[tokio::test]
async fn gateway_creates_admin_provider_model_locally_with_trusted_admin_principal() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let upstream_hits_clone = Arc::clone(&upstream_hits);
    let upstream = Router::new().route(
        "/api/admin/providers/provider-openai/models",
        any(move |_request: Request| {
            let upstream_hits_inner = Arc::clone(&upstream_hits_clone);
            async move {
                *upstream_hits_inner.lock().expect("mutex should lock") += 1;
                (StatusCode::OK, Body::from("unexpected upstream hit"))
            }
        }),
    );

    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-openai", "openai", 10)],
        vec![sample_endpoint(
            "endpoint-openai-embedding",
            "provider-openai",
            "openai:embedding",
            "https://api.openai.example",
        )],
        Vec::new(),
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![sample_admin_global_model(
                "global-gpt-5",
                "gpt-5",
                "GPT 5",
            )])
            .with_endpoint_provider_ids([("endpoint-openai-embedding", "provider-openai")]),
    );

    let (upstream_url, upstream_handle) = start_server(upstream).await;
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(
                    provider_catalog_repository,
                )
                .with_global_model_repository_for_tests(global_model_repository.clone()),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .post(format!(
            "{gateway_url}/api/admin/providers/provider-openai/models"
        ))
        .header(crate::constants::GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({
            "provider_model_name": "gpt-5-upstream",
            "global_model_id": "global-gpt-5",
            "supports_vision": true,
            "provider_model_mappings": [{"name": "text-embedding-3-small", "priority": 1, "api_formats": ["openai:embedding"]}],
            "config": {"provider_hint": "gpt-5-upstream", "api_formats": ["openai:embedding"], "model_type": "embedding"}
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(payload["provider_id"], "provider-openai");
    assert_eq!(payload["global_model_id"], "global-gpt-5");
    assert_eq!(payload["provider_model_name"], "gpt-5-upstream");
    assert_eq!(payload["effective_supports_vision"], true);
    assert_eq!(payload["effective_supports_embedding"], true);
    assert_eq!(
        payload["provider_model_mappings"][0]["api_formats"],
        json!(["openai:embedding"])
    );
    assert_eq!(*upstream_hits.lock().expect("mutex should lock"), 0);

    let created = global_model_repository
        .list_admin_provider_models(&AdminProviderModelListQuery {
            provider_id: "provider-openai".to_string(),
            is_active: None,
            offset: 0,
            limit: 20,
        })
        .await
        .expect("models should read");
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].provider_model_name, "gpt-5-upstream");
    assert_eq!(
        created[0]
            .config
            .as_ref()
            .and_then(|value| value.get("api_formats")),
        Some(&json!(["openai:embedding"]))
    );

    gateway_handle.abort();
    upstream_handle.abort();
}

#[tokio::test]
async fn gateway_uses_global_model_regex_to_infer_created_model_endpoint_binding() {
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-mixed", "custom", 10)],
        vec![
            sample_endpoint(
                "endpoint-openai-chat",
                "provider-mixed",
                "openai:chat",
                "https://openai.example",
            ),
            sample_endpoint(
                "endpoint-claude-messages",
                "provider-mixed",
                "claude:messages",
                "https://claude.example",
            ),
        ],
        vec![sample_key(
            "key-mixed",
            "provider-mixed",
            "claude:messages",
            "sk-test",
        )],
    ));
    let mut global_model =
        sample_admin_global_model("global-claude-opus", "claude-opus", "Claude Opus");
    global_model.config = Some(json!({ "model_mappings": ["claude-opus-.*"] }));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![global_model])
            .with_endpoint_provider_ids([
                ("endpoint-openai-chat", "provider-mixed"),
                ("endpoint-claude-messages", "provider-mixed"),
            ]),
    );
    let state = AppState::new()
        .expect("gateway should build")
        .with_data_state_for_tests(
            GatewayDataState::with_provider_catalog_reader_for_tests(provider_catalog_repository)
                .with_global_model_repository_for_tests(global_model_repository.clone()),
        );
    state
        .runtime_kv_setex(
            "upstream_models:provider-mixed:key-mixed",
            &serde_json::to_string(&vec![json!({
                "id": "claude-opus-4-6",
                "endpoint_ids": ["endpoint-claude-messages"]
            })])
            .expect("model cache should serialize"),
            60,
        )
        .await
        .expect("model cache should seed");
    let (gateway_url, gateway_handle) = start_server(build_router_with_state(state)).await;

    let response = reqwest::Client::new()
        .post(format!(
            "{gateway_url}/api/admin/providers/provider-mixed/models"
        ))
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({
            "provider_model_name": "claude-opus",
            "global_model_id": "global-claude-opus"
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    let model_id = payload["id"].as_str().expect("model id should exist");
    let bindings = global_model_repository
        .list_model_endpoint_bindings(&[model_id.to_string()])
        .await
        .expect("bindings should read");
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].endpoint_id, "endpoint-claude-messages");
    assert_eq!(bindings[0].source, "discovered");

    gateway_handle.abort();
}

#[tokio::test]
async fn gateway_updates_and_deletes_admin_provider_model_locally_with_trusted_admin_principal() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let upstream_hits_clone = Arc::clone(&upstream_hits);
    let upstream = Router::new().route(
        "/api/admin/providers/provider-openai/models/model-openai-gpt5",
        any(move |_request: Request| {
            let upstream_hits_inner = Arc::clone(&upstream_hits_clone);
            async move {
                *upstream_hits_inner.lock().expect("mutex should lock") += 1;
                (StatusCode::OK, Body::from("unexpected upstream hit"))
            }
        }),
    );

    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-openai", "openai", 10)],
        vec![sample_endpoint(
            "endpoint-openai-embedding",
            "provider-openai",
            "openai:embedding",
            "https://api.openai.example",
        )],
        Vec::new(),
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![
                sample_admin_global_model("global-gpt-5", "gpt-5", "GPT 5"),
                sample_admin_global_model("global-gpt-5-mini", "gpt-5-mini", "GPT 5 mini"),
            ])
            .with_admin_provider_models(vec![sample_admin_provider_model(
                "model-openai-gpt5",
                "provider-openai",
                "global-gpt-5",
                "gpt-5-upstream",
            )])
            .with_endpoint_provider_ids([("endpoint-openai-embedding", "provider-openai")]),
    );

    let (upstream_url, upstream_handle) = start_server(upstream).await;
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(
                    provider_catalog_repository,
                )
                .with_global_model_repository_for_tests(global_model_repository.clone()),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let update_response = reqwest::Client::new()
        .patch(format!(
            "{gateway_url}/api/admin/providers/provider-openai/models/model-openai-gpt5"
        ))
        .header(crate::constants::GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({
            "provider_model_name": "gpt-5-mini-upstream",
            "global_model_id": "global-gpt-5-mini",
            "provider_model_mappings": [{"name": "text-embedding-3-small", "priority": 1, "api_formats": ["openai:embedding"]}],
            "supports_streaming": false,
            "is_available": false,
            "config": {"api_formats": ["openai:embedding"], "model_type": "embedding"}
        }))
        .send()
        .await
        .expect("request should succeed");
    assert_eq!(update_response.status(), StatusCode::OK);
    let update_payload: serde_json::Value = update_response
        .json()
        .await
        .expect("json body should parse");
    assert_eq!(update_payload["provider_model_name"], "gpt-5-mini-upstream");
    assert_eq!(update_payload["global_model_id"], "global-gpt-5-mini");
    assert_eq!(update_payload["is_available"], false);
    assert_eq!(update_payload["effective_supports_embedding"], true);

    let delete_response = reqwest::Client::new()
        .delete(format!(
            "{gateway_url}/api/admin/providers/provider-openai/models/model-openai-gpt5"
        ))
        .header(crate::constants::GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .send()
        .await
        .expect("request should succeed");
    assert_eq!(delete_response.status(), StatusCode::OK);
    let delete_payload: serde_json::Value = delete_response
        .json()
        .await
        .expect("json body should parse");
    assert_eq!(
        delete_payload["message"],
        "Model 'gpt-5-mini-upstream' deleted successfully"
    );
    assert_eq!(*upstream_hits.lock().expect("mutex should lock"), 0);

    let models = global_model_repository
        .list_admin_provider_models(&AdminProviderModelListQuery {
            provider_id: "provider-openai".to_string(),
            is_active: None,
            offset: 0,
            limit: 20,
        })
        .await
        .expect("models should read");
    assert!(models.is_empty());

    gateway_handle.abort();
    upstream_handle.abort();
}

#[tokio::test]
async fn gateway_updates_only_provider_model_endpoint_bindings() {
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-openai", "openai", 10)],
        vec![sample_endpoint(
            "endpoint-openai-chat",
            "provider-openai",
            "openai:chat",
            "https://api.openai.example",
        )],
        Vec::new(),
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![sample_admin_global_model(
                "global-gpt-5",
                "gpt-5",
                "GPT 5",
            )])
            .with_admin_provider_models(vec![sample_admin_provider_model(
                "model-openai-gpt5",
                "provider-openai",
                "global-gpt-5",
                "gpt-5-upstream",
            )])
            .with_endpoint_provider_ids([("endpoint-openai-chat", "provider-openai")]),
    );
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(
                    provider_catalog_repository,
                )
                .with_global_model_repository_for_tests(global_model_repository.clone()),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .patch(format!(
            "{gateway_url}/api/admin/providers/provider-openai/models/model-openai-gpt5"
        ))
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({
            "endpoint_bindings": [{
                "endpoint_id": "endpoint-openai-chat",
                "is_active": true
            }]
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(payload["provider_model_name"], "gpt-5-upstream");
    assert_eq!(payload["global_model_id"], "global-gpt-5");
    let bindings = global_model_repository
        .list_model_endpoint_bindings(&["model-openai-gpt5".to_string()])
        .await
        .expect("bindings should read");
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].endpoint_id, "endpoint-openai-chat");
    assert_eq!(bindings[0].source, "manual");
    assert!(bindings[0].is_active);

    gateway_handle.abort();
}

#[tokio::test]
async fn gateway_rejects_duplicate_provider_model_endpoint_bindings_as_bad_request() {
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-openai", "openai", 10)],
        vec![sample_endpoint(
            "endpoint-openai-chat",
            "provider-openai",
            "openai:chat",
            "https://api.openai.example",
        )],
        Vec::new(),
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![sample_admin_global_model(
                "global-gpt-5",
                "gpt-5",
                "GPT 5",
            )])
            .with_admin_provider_models(vec![sample_admin_provider_model(
                "model-openai-gpt5",
                "provider-openai",
                "global-gpt-5",
                "gpt-5-upstream",
            )])
            .with_endpoint_provider_ids([("endpoint-openai-chat", "provider-openai")]),
    );
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(
                    provider_catalog_repository,
                )
                .with_global_model_repository_for_tests(global_model_repository.clone()),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .patch(format!(
            "{gateway_url}/api/admin/providers/provider-openai/models/model-openai-gpt5"
        ))
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({
            "endpoint_bindings": [
                {"endpoint_id": "endpoint-openai-chat", "is_active": true},
                {"endpoint_id": " endpoint-openai-chat ", "is_active": false}
            ]
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(
        payload["detail"],
        "Endpoint endpoint-openai-chat 在绑定列表中重复"
    );
    assert!(global_model_repository
        .list_model_endpoint_bindings(&["model-openai-gpt5".to_string()])
        .await
        .expect("bindings should read")
        .is_empty());

    let empty_response = reqwest::Client::new()
        .patch(format!(
            "{gateway_url}/api/admin/providers/provider-openai/models/model-openai-gpt5"
        ))
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({ "endpoint_bindings": [] }))
        .send()
        .await
        .expect("request should succeed");
    assert_eq!(empty_response.status(), StatusCode::BAD_REQUEST);
    let empty_payload: serde_json::Value =
        empty_response.json().await.expect("json body should parse");
    assert_eq!(empty_payload["detail"], "至少提供一个 Endpoint 绑定");

    gateway_handle.abort();
}

#[tokio::test]
async fn gateway_rejects_cross_provider_model_endpoint_binding_without_mutating_model() {
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![
            sample_provider("provider-openai", "openai", 10),
            sample_provider("provider-alt", "alt", 20),
        ],
        vec![
            sample_endpoint(
                "endpoint-openai-chat",
                "provider-openai",
                "openai:chat",
                "https://api.openai.example",
            ),
            sample_endpoint(
                "endpoint-alt-chat",
                "provider-alt",
                "openai:chat",
                "https://api.alt.example",
            ),
        ],
        Vec::new(),
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![
                sample_admin_global_model("global-gpt-5", "gpt-5", "GPT 5"),
                sample_admin_global_model("global-gpt-5-mini", "gpt-5-mini", "GPT 5 mini"),
            ])
            .with_admin_provider_models(vec![sample_admin_provider_model(
                "model-openai-gpt5",
                "provider-openai",
                "global-gpt-5",
                "gpt-5-upstream",
            )]),
    );
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(
                    provider_catalog_repository,
                )
                .with_global_model_repository_for_tests(global_model_repository.clone()),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .patch(format!(
            "{gateway_url}/api/admin/providers/provider-openai/models/model-openai-gpt5"
        ))
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({
            "provider_model_name": "must-not-persist",
            "global_model_id": "global-gpt-5-mini",
            "endpoint_bindings": [{
                "endpoint_id": "endpoint-alt-chat",
                "is_active": true
            }]
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(
        payload["detail"],
        "Endpoint endpoint-alt-chat 不属于当前 Provider"
    );
    let stored = global_model_repository
        .get_admin_provider_model("provider-openai", "model-openai-gpt5")
        .await
        .expect("model should read")
        .expect("model should remain");
    assert_eq!(stored.provider_model_name, "gpt-5-upstream");
    assert_eq!(stored.global_model_id, "global-gpt-5");
    assert!(global_model_repository
        .list_model_endpoint_bindings(&["model-openai-gpt5".to_string()])
        .await
        .expect("bindings should read")
        .is_empty());

    gateway_handle.abort();
}

#[tokio::test]
async fn gateway_rebuilds_automatic_endpoint_bindings_when_model_mapping_changes() {
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-mixed", "custom", 10)],
        vec![
            sample_endpoint(
                "endpoint-openai-chat",
                "provider-mixed",
                "openai:chat",
                "https://openai.example",
            ),
            sample_endpoint(
                "endpoint-claude-messages",
                "provider-mixed",
                "claude:messages",
                "https://claude.example",
            ),
        ],
        Vec::new(),
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![sample_admin_global_model(
                "global-claude-opus",
                "claude-opus",
                "Claude Opus",
            )])
            .with_admin_provider_models(vec![sample_admin_provider_model(
                "model-claude-opus",
                "provider-mixed",
                "global-claude-opus",
                "claude-opus-upstream",
            )])
            .with_model_endpoint_bindings(vec![
                aether_data_contracts::repository::global_models::StoredModelEndpointBinding::new(
                    "model-claude-opus".to_string(),
                    "endpoint-openai-chat".to_string(),
                    "discovered".to_string(),
                    true,
                    Some(1),
                    Some(1),
                )
                .expect("old automatic binding should build"),
            ])
            .with_endpoint_provider_ids([
                ("endpoint-openai-chat", "provider-mixed"),
                ("endpoint-claude-messages", "provider-mixed"),
            ]),
    );
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(
                    provider_catalog_repository,
                )
                .with_global_model_repository_for_tests(global_model_repository.clone()),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .patch(format!(
            "{gateway_url}/api/admin/providers/provider-mixed/models/model-claude-opus"
        ))
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({
            "provider_model_mappings": [{
                "name": "claude-opus-upstream",
                "priority": 1,
                "endpoint_ids": ["endpoint-claude-messages"]
            }]
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let bindings = global_model_repository
        .list_model_endpoint_bindings(&["model-claude-opus".to_string()])
        .await
        .expect("bindings should read");
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].endpoint_id, "endpoint-claude-messages");
    assert_eq!(bindings[0].source, "mapping");
    assert!(bindings[0].is_active);

    gateway_handle.abort();
}

#[tokio::test]
async fn gateway_rebuilds_automatic_endpoint_bindings_when_provider_model_name_changes() {
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-mixed", "custom", 10)],
        vec![
            sample_endpoint(
                "endpoint-old",
                "provider-mixed",
                "openai:chat",
                "https://old.example",
            ),
            sample_endpoint(
                "endpoint-new",
                "provider-mixed",
                "claude:messages",
                "https://new.example",
            ),
        ],
        vec![sample_key(
            "key-mixed",
            "provider-mixed",
            "claude:messages",
            "sk-test",
        )],
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![sample_admin_global_model(
                "global-claude-opus",
                "claude-opus",
                "Claude Opus",
            )])
            .with_admin_provider_models(vec![sample_admin_provider_model(
                "model-claude-opus",
                "provider-mixed",
                "global-claude-opus",
                "old-upstream-name",
            )])
            .with_model_endpoint_bindings(vec![
                aether_data_contracts::repository::global_models::StoredModelEndpointBinding::new(
                    "model-claude-opus".to_string(),
                    "endpoint-old".to_string(),
                    "discovered".to_string(),
                    true,
                    Some(1),
                    Some(1),
                )
                .expect("old automatic binding should build"),
            ])
            .with_endpoint_provider_ids([
                ("endpoint-old", "provider-mixed"),
                ("endpoint-new", "provider-mixed"),
            ]),
    );
    let state = AppState::new()
        .expect("gateway should build")
        .with_data_state_for_tests(
            GatewayDataState::with_provider_catalog_reader_for_tests(provider_catalog_repository)
                .with_global_model_repository_for_tests(global_model_repository.clone()),
        );
    state
        .runtime_kv_setex(
            "upstream_models:provider-mixed:key-mixed",
            &serde_json::to_string(&vec![json!({
                "id": "new-upstream-name",
                "endpoint_ids": ["endpoint-new"]
            })])
            .expect("model cache should serialize"),
            60,
        )
        .await
        .expect("model cache should seed");
    let (gateway_url, gateway_handle) = start_server(build_router_with_state(state)).await;

    let response = reqwest::Client::new()
        .patch(format!(
            "{gateway_url}/api/admin/providers/provider-mixed/models/model-claude-opus"
        ))
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({ "provider_model_name": "new-upstream-name" }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let bindings = global_model_repository
        .list_model_endpoint_bindings(&["model-claude-opus".to_string()])
        .await
        .expect("bindings should read");
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].endpoint_id, "endpoint-new");
    assert_eq!(bindings[0].source, "discovered");

    gateway_handle.abort();
}

#[tokio::test]
async fn gateway_uses_explicit_bindings_when_renaming_multi_endpoint_model() {
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-mixed", "custom", 10)],
        vec![
            sample_endpoint(
                "endpoint-old",
                "provider-mixed",
                "openai:chat",
                "https://old.example",
            ),
            sample_endpoint(
                "endpoint-new",
                "provider-mixed",
                "claude:messages",
                "https://new.example",
            ),
        ],
        Vec::new(),
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![sample_admin_global_model(
                "global-claude-opus",
                "claude-opus",
                "Claude Opus",
            )])
            .with_admin_provider_models(vec![sample_admin_provider_model(
                "model-claude-opus",
                "provider-mixed",
                "global-claude-opus",
                "old-upstream-name",
            )])
            .with_model_endpoint_bindings(vec![
                aether_data_contracts::repository::global_models::StoredModelEndpointBinding::new(
                    "model-claude-opus".to_string(),
                    "endpoint-old".to_string(),
                    "discovered".to_string(),
                    true,
                    Some(1),
                    Some(1),
                )
                .expect("old automatic binding should build"),
            ])
            .with_endpoint_provider_ids([
                ("endpoint-old", "provider-mixed"),
                ("endpoint-new", "provider-mixed"),
            ]),
    );
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(
                    provider_catalog_repository,
                )
                .with_global_model_repository_for_tests(global_model_repository.clone()),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .patch(format!(
            "{gateway_url}/api/admin/providers/provider-mixed/models/model-claude-opus"
        ))
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({
            "provider_model_name": "renamed-with-manual-route",
            "endpoint_bindings": [{
                "endpoint_id": "endpoint-new",
                "is_active": true
            }]
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let bindings = global_model_repository
        .list_model_endpoint_bindings(&["model-claude-opus".to_string()])
        .await
        .expect("bindings should read");
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].endpoint_id, "endpoint-new");
    assert_eq!(bindings[0].source, "manual");
    assert!(bindings[0].is_active);

    gateway_handle.abort();
}

#[tokio::test]
async fn gateway_rejects_multi_endpoint_model_without_endpoint_evidence() {
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-mixed", "custom", 10)],
        vec![
            sample_endpoint(
                "endpoint-openai-chat",
                "provider-mixed",
                "openai:chat",
                "https://openai.example",
            ),
            sample_endpoint(
                "endpoint-claude-messages",
                "provider-mixed",
                "claude:messages",
                "https://claude.example",
            ),
        ],
        Vec::new(),
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new()).with_admin_global_models(vec![
            sample_admin_global_model("global-claude-opus", "claude-opus", "Claude Opus"),
        ]),
    );
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(
                    provider_catalog_repository,
                )
                .with_global_model_repository_for_tests(global_model_repository.clone()),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .post(format!(
            "{gateway_url}/api/admin/providers/provider-mixed/models"
        ))
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({
            "provider_model_name": "claude-opus-upstream",
            "global_model_id": "global-claude-opus"
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(
        payload["detail"],
        "模型 claude-opus-upstream 无法推断 Endpoint，请明确选择至少一个 Endpoint"
    );
    assert!(global_model_repository
        .list_admin_provider_models(&AdminProviderModelListQuery {
            provider_id: "provider-mixed".to_string(),
            is_active: None,
            offset: 0,
            limit: 20,
        })
        .await
        .expect("models should read")
        .is_empty());

    gateway_handle.abort();
}

#[tokio::test]
async fn gateway_creates_multi_endpoint_model_with_explicit_endpoint_binding() {
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-mixed", "custom", 10)],
        vec![
            sample_endpoint(
                "endpoint-openai-chat",
                "provider-mixed",
                "openai:chat",
                "https://openai.example",
            ),
            sample_endpoint(
                "endpoint-claude-messages",
                "provider-mixed",
                "claude:messages",
                "https://claude.example",
            ),
        ],
        Vec::new(),
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![sample_admin_global_model(
                "global-claude-opus",
                "claude-opus",
                "Claude Opus",
            )])
            .with_endpoint_provider_ids([
                ("endpoint-openai-chat", "provider-mixed"),
                ("endpoint-claude-messages", "provider-mixed"),
            ]),
    );
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(
                    provider_catalog_repository,
                )
                .with_global_model_repository_for_tests(global_model_repository.clone()),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .post(format!(
            "{gateway_url}/api/admin/providers/provider-mixed/models"
        ))
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({
            "provider_model_name": "claude-opus-upstream",
            "global_model_id": "global-claude-opus",
            "endpoint_ids": ["endpoint-claude-messages"]
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    let model_id = payload["id"].as_str().expect("model id should exist");
    let bindings = global_model_repository
        .list_model_endpoint_bindings(&[model_id.to_string()])
        .await
        .expect("bindings should read");
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].endpoint_id, "endpoint-claude-messages");
    assert_eq!(bindings[0].source, "manual");

    gateway_handle.abort();
}

#[tokio::test]
async fn gateway_batch_creates_admin_provider_models_locally_with_trusted_admin_principal() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let upstream_hits_clone = Arc::clone(&upstream_hits);
    let upstream = Router::new().route(
        "/api/admin/providers/provider-openai/models/batch",
        any(move |_request: Request| {
            let upstream_hits_inner = Arc::clone(&upstream_hits_clone);
            async move {
                *upstream_hits_inner.lock().expect("mutex should lock") += 1;
                (StatusCode::OK, Body::from("unexpected upstream hit"))
            }
        }),
    );

    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-openai", "openai", 10)],
        vec![sample_endpoint(
            "endpoint-openai-chat",
            "provider-openai",
            "openai:chat",
            "https://api.openai.example",
        )],
        Vec::new(),
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![
                sample_admin_global_model("global-gpt-5", "gpt-5", "GPT 5"),
                sample_admin_global_model("global-gpt-4.1", "gpt-4.1", "GPT 4.1"),
            ])
            .with_endpoint_provider_ids([("endpoint-openai-chat", "provider-openai")]),
    );

    let (upstream_url, upstream_handle) = start_server(upstream).await;
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(
                    provider_catalog_repository,
                )
                .with_global_model_repository_for_tests(global_model_repository.clone()),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .post(format!(
            "{gateway_url}/api/admin/providers/provider-openai/models/batch"
        ))
        .header(crate::constants::GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!([
            {"provider_model_name": "gpt-5-upstream", "global_model_id": "global-gpt-5"},
            {"provider_model_name": "gpt-4.1-upstream", "global_model_id": "global-gpt-4.1"}
        ]))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(payload.as_array().expect("payload array").len(), 2);
    assert_eq!(*upstream_hits.lock().expect("mutex should lock"), 0);

    gateway_handle.abort();
    upstream_handle.abort();
}
#[tokio::test]
async fn gateway_handles_admin_provider_available_source_models_locally_with_trusted_admin_principal(
) {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let upstream_hits_clone = Arc::clone(&upstream_hits);
    let upstream = Router::new().route(
        "/api/admin/providers/provider-openai/available-source-models",
        any(move |_request: Request| {
            let upstream_hits_inner = Arc::clone(&upstream_hits_clone);
            async move {
                *upstream_hits_inner.lock().expect("mutex should lock") += 1;
                (StatusCode::OK, Body::from("unexpected upstream hit"))
            }
        }),
    );

    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-openai", "openai", 10)],
        Vec::new(),
        Vec::new(),
    ));
    let mut global_model = sample_admin_global_model(
        "global-gpt-5",
        "text-embedding-3-small",
        "Text Embedding 3 Small",
    );
    global_model.supported_capabilities = Some(json!(["embedding"]));
    global_model.config = Some(json!({"api_formats": ["openai:embedding"]}));
    let mut primary_model = sample_admin_provider_model(
        "model-openai-gpt5",
        "provider-openai",
        "global-gpt-5",
        "text-embedding-3-small",
    );
    primary_model.global_model_name = Some("text-embedding-3-small".to_string());
    primary_model.global_model_display_name = Some("Text Embedding 3 Small".to_string());
    primary_model.global_model_supported_capabilities = Some(json!(["embedding"]));
    primary_model.global_model_config = Some(json!({"api_formats": ["openai:embedding"]}));
    let mut alternate_model = sample_admin_provider_model(
        "model-openai-gpt5-b",
        "provider-openai",
        "global-gpt-5",
        "gpt-5-alt",
    );
    alternate_model.global_model_name = Some("text-embedding-3-small".to_string());
    alternate_model.global_model_display_name = Some("Text Embedding 3 Small".to_string());
    alternate_model.global_model_supported_capabilities = Some(json!(["embedding"]));
    alternate_model.global_model_config = Some(json!({"api_formats": ["openai:embedding"]}));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![global_model])
            .with_admin_provider_models(vec![primary_model, alternate_model]),
    );

    let (upstream_url, upstream_handle) = start_server(upstream).await;
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(
                    provider_catalog_repository,
                )
                .with_global_model_repository_for_tests(global_model_repository),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .get(format!(
            "{gateway_url}/api/admin/providers/provider-openai/available-source-models"
        ))
        .header(crate::constants::GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert_eq!(payload["total"], 1);
    assert_eq!(
        payload["models"][0]["global_model_name"],
        "text-embedding-3-small"
    );
    assert_eq!(
        payload["models"][0]["capabilities"]["supports_embedding"],
        true
    );
    assert_eq!(*upstream_hits.lock().expect("mutex should lock"), 0);

    gateway_handle.abort();
    upstream_handle.abort();
}
#[tokio::test]
async fn gateway_assigns_and_imports_admin_provider_models_locally_with_trusted_admin_principal() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let upstream_hits_clone = Arc::clone(&upstream_hits);
    let upstream = Router::new()
        .route(
            "/api/admin/providers/provider-openai/assign-global-models",
            any({
                let upstream_hits_inner = Arc::clone(&upstream_hits_clone);
                move |_request: Request| {
                    let upstream_hits_inner = Arc::clone(&upstream_hits_inner);
                    async move {
                        *upstream_hits_inner.lock().expect("mutex should lock") += 1;
                        (StatusCode::OK, Body::from("unexpected upstream hit"))
                    }
                }
            }),
        )
        .route(
            "/api/admin/providers/provider-openai/import-from-upstream",
            any(move |_request: Request| {
                let upstream_hits_inner = Arc::clone(&upstream_hits);
                async move {
                    *upstream_hits_inner.lock().expect("mutex should lock") += 1;
                    (StatusCode::OK, Body::from("unexpected upstream hit"))
                }
            }),
        );

    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-openai", "openai", 10)],
        vec![sample_endpoint(
            "endpoint-openai-chat",
            "provider-openai",
            "openai:chat",
            "https://api.openai.example",
        )],
        Vec::new(),
    ));
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models(vec![
                sample_admin_global_model("global-gpt-5", "gpt-5", "GPT 5"),
                sample_admin_global_model("global-gpt-4.1", "gpt-4.1", "GPT 4.1"),
            ])
            .with_admin_provider_models(vec![sample_admin_provider_model(
                "model-openai-gpt5",
                "provider-openai",
                "global-gpt-5",
                "gpt-5-upstream",
            )]),
    );

    let (upstream_url, upstream_handle) = start_server(upstream).await;
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(
                    provider_catalog_repository,
                )
                .with_global_model_repository_for_tests(global_model_repository.clone()),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let assign_response = reqwest::Client::new()
        .post(format!(
            "{gateway_url}/api/admin/providers/provider-openai/assign-global-models"
        ))
        .header(crate::constants::GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({"global_model_ids": ["global-gpt-5", "global-gpt-4.1"]}))
        .send()
        .await
        .expect("request should succeed");
    assert_eq!(assign_response.status(), StatusCode::OK);
    let assign_payload: serde_json::Value = assign_response
        .json()
        .await
        .expect("json body should parse");
    assert_eq!(
        assign_payload["success"]
            .as_array()
            .expect("success array")
            .len(),
        1
    );
    assert_eq!(
        assign_payload["errors"]
            .as_array()
            .expect("errors array")
            .len(),
        1
    );

    let legacy_import_response = reqwest::Client::new()
        .post(format!(
            "{gateway_url}/api/admin/providers/provider-openai/import-from-upstream"
        ))
        .header(crate::constants::GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({
            "model_ids": ["gpt-5-upstream", "legacy-only-model"],
            "price_per_request": 0.1
        }))
        .send()
        .await
        .expect("legacy import request should succeed");
    assert_eq!(legacy_import_response.status(), StatusCode::OK);
    let legacy_import_payload: serde_json::Value = legacy_import_response
        .json()
        .await
        .expect("legacy import body should parse");
    assert_eq!(
        legacy_import_payload["success"]
            .as_array()
            .expect("legacy success array")
            .len(),
        2
    );
    let legacy_provider_model_id = legacy_import_payload["success"][1]["provider_model_id"]
        .as_str()
        .expect("legacy provider model id")
        .to_string();
    let legacy_bindings = global_model_repository
        .list_model_endpoint_bindings(&[
            "model-openai-gpt5".to_string(),
            legacy_provider_model_id.clone(),
        ])
        .await
        .expect("legacy import bindings should read");
    assert!(legacy_bindings.iter().any(|binding| {
        binding.model_id == "model-openai-gpt5"
            && binding.endpoint_id == "endpoint-openai-chat"
            && binding.source == "migration"
            && binding.is_active
    }));
    assert!(legacy_bindings.iter().any(|binding| {
        binding.model_id == legacy_provider_model_id
            && binding.endpoint_id == "endpoint-openai-chat"
            && binding.source == "discovered"
            && binding.is_active
    }));

    let import_response = reqwest::Client::new()
        .post(format!(
            "{gateway_url}/api/admin/providers/provider-openai/import-from-upstream"
        ))
        .header(crate::constants::GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({
            "model_ids": ["gpt-5-upstream", "brand-new-model"],
            "models": [
                {
                    "id": "gpt-5-upstream",
                    "api_formats": ["openai:chat"],
                    "endpoint_ids": ["endpoint-openai-chat"]
                },
                {
                    "id": "brand-new-model",
                    "api_formats": ["openai:chat"],
                    "endpoint_ids": ["endpoint-openai-chat"]
                }
            ],
            "price_per_request": 0.1
        }))
        .send()
        .await
        .expect("request should succeed");
    assert_eq!(import_response.status(), StatusCode::OK);
    let import_payload: serde_json::Value = import_response
        .json()
        .await
        .expect("json body should parse");
    assert_eq!(
        import_payload["success"]
            .as_array()
            .expect("success array")
            .len(),
        2
    );
    assert_eq!(import_payload["success"][1]["created_global_model"], true);
    let imported_provider_model_id = import_payload["success"][1]["provider_model_id"]
        .as_str()
        .expect("provider model id")
        .to_string();
    let bindings = global_model_repository
        .list_model_endpoint_bindings(&[
            "model-openai-gpt5".to_string(),
            imported_provider_model_id.clone(),
        ])
        .await
        .expect("imported bindings should read");
    assert!(bindings.iter().any(|binding| {
        binding.model_id == "model-openai-gpt5"
            && binding.endpoint_id == "endpoint-openai-chat"
            && binding.is_active
    }));
    assert!(bindings.iter().any(|binding| {
        binding.model_id == imported_provider_model_id
            && binding.endpoint_id == "endpoint-openai-chat"
            && binding.is_active
    }));
    assert_eq!(*upstream_hits_clone.lock().expect("mutex should lock"), 0);

    gateway_handle.abort();
    upstream_handle.abort();
}

#[tokio::test]
async fn gateway_import_failure_removes_new_unreferenced_global_model() {
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider("provider-openai", "openai", 10)],
        vec![sample_endpoint(
            "endpoint-openai-chat",
            "provider-openai",
            "openai:chat",
            "https://api.openai.example",
        )],
        Vec::new(),
    ));
    // Provider Catalog 能看到 Endpoint，但持久化测试替身显式拒绝它，模拟原子创建失败。
    let global_model_repository = Arc::new(
        InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_endpoint_provider_ids(std::iter::empty::<(&str, &str)>()),
    );
    let gateway = build_router_with_state(
        AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(
                    provider_catalog_repository,
                )
                .with_global_model_repository_for_tests(global_model_repository.clone()),
            ),
    );
    let (gateway_url, gateway_handle) = start_server(gateway).await;

    let response = reqwest::Client::new()
        .post(format!(
            "{gateway_url}/api/admin/providers/provider-openai/import-from-upstream"
        ))
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
        .json(&json!({
            "model_ids": ["orphan-prone-model"],
            "models": [{
                "id": "orphan-prone-model",
                "api_formats": ["openai:chat"],
                "endpoint_ids": ["endpoint-openai-chat"]
            }]
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = response.json().await.expect("json body should parse");
    assert!(payload["success"]
        .as_array()
        .expect("success array")
        .is_empty());
    assert_eq!(payload["errors"].as_array().expect("errors array").len(), 1);
    assert!(global_model_repository
        .get_admin_global_model_by_name("orphan-prone-model")
        .await
        .expect("global model should read")
        .is_none());

    gateway_handle.abort();
}
