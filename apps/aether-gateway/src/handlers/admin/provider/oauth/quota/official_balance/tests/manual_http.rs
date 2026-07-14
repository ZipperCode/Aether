use super::*;
use aether_crypto::{encrypt_python_fernet_plaintext, DEVELOPMENT_ENCRYPTION_KEY};
use aether_data::repository::provider_catalog::InMemoryProviderCatalogReadRepository;
use axum::{
    body::{to_bytes, Body},
    extract::Request,
    middleware::{self, Next},
    response::Response,
    routing::post,
    Json, Router,
};
use http::StatusCode;
use std::{sync::Mutex, time::Duration};
use tokio::sync::oneshot;

use crate::{
    data::GatewayDataState,
    handlers::admin::request::AdminAppState,
    tests::{build_router_with_state, build_state_with_execution_runtime_override, start_server},
};

const MANUAL_QA_PORT: u16 = 19085;

#[tokio::test]
#[ignore = "manual bound-router curl harness"]
async fn manual_bound_router_exposes_multi_key_official_quota_payload() {
    // Given
    let execution_runtime = Router::new().route(
        "/v1/execute/sync",
        post(|request: Request| async move {
            let plan: aether_contracts::ExecutionPlan = serde_json::from_slice(
                &to_bytes(request.into_body(), usize::MAX)
                    .await
                    .expect("execution plan body"),
            )
            .expect("execution plan");
            let result = if plan.key_id == "key-error" {
                aether_contracts::ExecutionResult {
                    request_id: plan.request_id,
                    candidate_id: None,
                    status_code: 429,
                    headers: BTreeMap::from([("retry-after".into(), "60".into())]),
                    body: Some(ResponseBody {
                        json_body: Some(json!({"error":{"message":"Bearer must-not-leak\r\nX-Api-Key: hidden"}})),
                        body_bytes_b64: None,
                    }),
                    telemetry: None,
                    error: None,
                }
            } else {
                aether_contracts::ExecutionResult {
                    request_id: plan.request_id,
                    candidate_id: None,
                    status_code: 200,
                    headers: BTreeMap::new(),
                    body: Some(ResponseBody {
                        json_body: Some(json!({
                            "is_available": true,
                            "balance_infos": [{
                                "currency": "CNY",
                                "total_balance": "12.34",
                                "granted_balance": "10.00",
                                "topped_up_balance": "2.34"
                            }]
                        })),
                        body_bytes_b64: None,
                    }),
                    telemetry: None,
                    error: None,
                }
            };
            (StatusCode::OK, Json(result))
        }),
    );
    let (execution_runtime_url, execution_runtime_handle) = start_server(execution_runtime).await;
    let provider = provider();
    let endpoint = official_endpoint();
    let keys = vec![
        provider_key("key-success", "Success", None),
        provider_key("key-error", "Error", None),
        provider_key("key-backoff", "Backoff", Some(backoff_snapshot())),
    ];
    let repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![provider.clone()],
        vec![endpoint.clone()],
        keys.clone(),
    ));
    let (completed_tx, completed_rx) = oneshot::channel();
    let completed = Arc::new(Mutex::new(Some(completed_tx)));
    let state = build_state_with_execution_runtime_override(execution_runtime_url)
        .with_data_state_for_tests(
            GatewayDataState::with_provider_catalog_repository_for_tests(repository)
                .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY),
        );
    let qa_state = state.clone();
    let gateway = build_router_with_state(state)
        .route(
            "/__qa/background-refresh",
            post(move || {
                let state = qa_state.clone();
                let provider = provider.clone();
                let endpoint = endpoint.clone();
                let keys = keys.clone();
                async move {
                    let payload = super::super::refresh_official_balance_provider_quota_locally(
                        &AdminAppState::new(&state),
                        &provider,
                        &endpoint,
                        keys,
                        None,
                        QuotaRefreshSource::PoolBackground,
                    )
                    .await
                    .expect("background refresh")
                    .expect("official quota payload");
                    Json(payload)
                }
            }),
        )
        .layer(middleware::from_fn(move |request, next| {
            notify_after_manual_request(request, next, Arc::clone(&completed))
        }));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", MANUAL_QA_PORT))
        .await
        .expect("manual QA port");
    let gateway_handle = tokio::spawn(async move {
        axum::serve(
            listener,
            gateway.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .expect("manual gateway");
    });
    eprintln!("MANUAL_QUOTA_URL=http://127.0.0.1:{MANUAL_QA_PORT}/__qa/background-refresh");

    // When
    let completed = tokio::time::timeout(Duration::from_secs(180), completed_rx).await;

    // Then
    gateway_handle.abort();
    execution_runtime_handle.abort();
    assert!(completed.is_ok(), "manual curl did not arrive");
}

async fn notify_after_manual_request(
    request: Request,
    next: Next,
    completed: Arc<Mutex<Option<oneshot::Sender<()>>>>,
) -> Response {
    let notify = request.uri().path() == "/__qa/background-refresh";
    let response = next.run(request).await;
    if notify {
        if let Some(sender) = completed.lock().expect("completion lock").take() {
            let _ = sender.send(());
        }
    }
    response
}

fn provider() -> aether_data_contracts::repository::provider_catalog::StoredProviderCatalogProvider
{
    aether_data_contracts::repository::provider_catalog::StoredProviderCatalogProvider::new(
        "provider-deepseek".into(),
        "DeepSeek".into(),
        Some("https://api.deepseek.com".into()),
        "deepseek".into(),
    )
    .expect("provider")
}

fn official_endpoint() -> StoredProviderCatalogEndpoint {
    endpoint(EndpointFixture {
        id: "endpoint-deepseek",
        provider_id: "provider-deepseek",
        base_url: "https://api.deepseek.com",
        active: true,
    })
}

fn provider_key(id: &str, name: &str, quota: Option<Value>) -> StoredProviderCatalogKey {
    let encrypted = encrypt_python_fernet_plaintext(DEVELOPMENT_ENCRYPTION_KEY, "qa-secret")
        .expect("encrypted key");
    let mut key = StoredProviderCatalogKey::new(
        id.into(),
        "provider-deepseek".into(),
        name.into(),
        "api_key".into(),
        None,
        true,
    )
    .expect("provider key")
    .with_transport_fields(
        Some(json!(["openai:chat"])),
        encrypted,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("key transport");
    key.status_snapshot = quota.map(|quota| json!({"quota": quota}));
    key
}

fn backoff_snapshot() -> Value {
    let mut snapshot = ProviderQuotaSnapshotContract::balance("deepseek", Vec::new());
    snapshot.refresh_state = ProviderQuotaRefreshState {
        last_attempt_at: Some(100),
        last_success_at: Some(90),
        error: Some("http_rate_limited: quota upstream rate limited the request".into()),
        next_eligible_at: Some(u64::MAX),
        failure_count: Some(1),
    };
    serde_json::to_value(snapshot).expect("backoff snapshot")
}
