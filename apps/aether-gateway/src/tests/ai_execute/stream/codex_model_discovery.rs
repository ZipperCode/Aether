use std::convert::Infallible;
use std::future::Future;
use std::io::Read as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aether_crypto::DEVELOPMENT_ENCRYPTION_KEY;
use aether_data::repository::{
    auth::{InMemoryAuthApiKeySnapshotRepository, StoredAuthApiKeySnapshot},
    candidate_selection::InMemoryMinimalCandidateSelectionReadRepository,
    candidates::InMemoryRequestCandidateRepository,
    global_models::InMemoryGlobalModelReadRepository,
    provider_catalog::InMemoryProviderCatalogReadRepository,
    routing_profiles::InMemoryRoutingGroupRepository,
    usage::InMemoryUsageReadRepository,
};
use aether_data_contracts::repository::{
    candidate_selection::{
        StoredMinimalCandidateRoutingFacts, StoredMinimalCandidateSelectionRow,
        StoredProviderModelMapping,
    },
    candidates::{RequestCandidateReadRepository, RequestCandidateStatus},
    global_models::{StoredAdminGlobalModel, StoredAdminProviderModel, StoredModelEndpointBinding},
    provider_catalog::{
        ProviderCatalogReadRepository, StoredProviderCatalogEndpoint, StoredProviderCatalogKey,
        StoredProviderCatalogProvider,
    },
    routing_profiles::{CreateRoutingGroupRecord, RoutingGroupWriteRepository},
};
use axum::body::{to_bytes, Body, Bytes};
use axum::extract::ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, CONTENT_ENCODING};
use axum::http::{HeaderMap, Response, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

use super::{
    build_router_with_state, start_server, strip_sse_keepalive_comments, AppState,
    UsageRuntimeConfig, LOCAL_EXECUTION_RUNTIME_MISS_REASON_HEADER, TRACE_ID_HEADER,
};
use crate::ai_serving::extract_pool_sticky_session_token;
use crate::constants::{
    GATEWAY_HEADER, TRUSTED_ADMIN_SESSION_ID_HEADER, TRUSTED_ADMIN_USER_ID_HEADER,
    TRUSTED_ADMIN_USER_ROLE_HEADER,
};
use crate::handlers::shared::provider_pool::{
    admin_provider_pool_config_from_config_value, read_provider_pool_sticky_bound_key_id,
    record_admin_provider_pool_success, AdminProviderPoolConfig,
};
use crate::model_fetch::{perform_model_fetch_for_key, ModelFetchRunSummary};

const BASIC_MODEL: &str = "gpt-basic-test";
const ADVANCED_MODEL: &str = "gpt-6.1-sol";
const ALIAS_GLOBAL_MODEL: &str = "sol-pro-alias";
const DISCOVERY_NAMESPACE: &str = "codex_model_discovery";

const FREE_OAUTH_TOKEN: &str = "codex-free-oauth-token-synth";
const FREE_ACCOUNT_ID: &str = "acct-codex-free-synth";
const PAID_OAUTH_TOKEN: &str = "codex-paid-oauth-token-synth";
const PAID_ACCOUNT_ID: &str = "acct-codex-paid-synth";

const DEFAULT_CLIENT_API_KEY: &str = "sk-codex-discovery-client";

#[derive(Debug, Clone)]
enum UpstreamCatalogReply {
    Models { etag: String, slugs: Vec<String> },
    Error { status: StatusCode, message: String },
}

impl UpstreamCatalogReply {
    fn models(etag: &str, slugs: &[&str]) -> Self {
        Self::Models {
            etag: etag.to_string(),
            slugs: slugs.iter().map(|slug| (*slug).to_string()).collect(),
        }
    }

    fn server_error(message: &str) -> Self {
        Self::Error {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.to_string(),
        }
    }
}

#[derive(Debug)]
struct UpstreamMutableState {
    free_catalog: UpstreamCatalogReply,
    paid_catalog: UpstreamCatalogReply,
    free_supported_models: Vec<String>,
    paid_supported_models: Vec<String>,
}

#[derive(Default)]
struct UpstreamTelemetry {
    free_catalog_hits: AtomicUsize,
    paid_catalog_hits: AtomicUsize,
    unauthenticated_hits: AtomicUsize,
    total_generation_hits: AtomicUsize,
    free_generation_hits: AtomicUsize,
    paid_generation_hits: AtomicUsize,
    free_ws_handshakes: AtomicUsize,
    paid_ws_handshakes: AtomicUsize,
    free_generation_models: Mutex<Vec<String>>,
    paid_generation_models: Mutex<Vec<String>>,
    free_ws_models: Mutex<Vec<String>>,
    paid_ws_models: Mutex<Vec<String>>,
}

struct UpstreamAppState {
    mutable: Mutex<UpstreamMutableState>,
    telemetry: UpstreamTelemetry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UpstreamCountsSnapshot {
    free_catalog: usize,
    paid_catalog: usize,
    unauthenticated: usize,
    total_gen: usize,
    free_gen: usize,
    paid_gen: usize,
    free_ws_handshakes: usize,
    paid_ws_handshakes: usize,
    free_models: Vec<String>,
    paid_models: Vec<String>,
    free_ws_models: Vec<String>,
    paid_ws_models: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AuthenticatedUpstreamAccount {
    Free,
    Paid,
}

fn identify_upstream_account(headers: &HeaderMap) -> Option<AuthenticatedUpstreamAccount> {
    let bearer = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)?;
    let account_id = headers
        .get("chatgpt-account-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)?;

    if bearer == FREE_OAUTH_TOKEN && account_id == FREE_ACCOUNT_ID {
        Some(AuthenticatedUpstreamAccount::Free)
    } else if bearer == PAID_OAUTH_TOKEN && account_id == PAID_ACCOUNT_ID {
        Some(AuthenticatedUpstreamAccount::Paid)
    } else {
        None
    }
}

fn codex_model_card_json(slug: &str) -> Value {
    json!({
        "id": slug,
        "slug": slug,
        "display_name": format!("Codex {slug}"),
        "description": "Synthetic Codex test model card",
        "default_reasoning_level": "medium",
        "supported_reasoning_levels": [
            {"effort": "low", "description": "Low"},
            {"effort": "medium", "description": "Medium"},
            {"effort": "high", "description": "High"}
        ],
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "supports_search_tool": true,
        "priority": 1,
        "availability_nux": null,
        "upgrade": null,
        "base_instructions": "Follow synthetic test instructions.",
        "model_messages": null,
        "available_in_plans": ["free", "plus", "pro"],
        "support_verbosity": true,
        "default_verbosity": "low",
        "apply_patch_tool_type": "freeform",
        "truncation_policy": {"mode": "tokens", "limit": 10000},
        "supports_parallel_tool_calls": true,
        "experimental_supported_tools": [],
        "minimal_client_version": "0.144.0"
    })
}

fn build_codex_sse_body(model: &str, response_id: &str) -> Vec<Bytes> {
    let created = format!(
        "event: response.created\ndata: {}\n\n",
        json!({
            "type": "response.created",
            "response": {
                "id": response_id,
                "object": "response",
                "model": model,
                "status": "in_progress",
                "output": []
            }
        })
    );
    let in_progress = format!(
        "event: response.in_progress\ndata: {}\n\n",
        json!({
            "type": "response.in_progress",
            "response": {
                "id": response_id,
                "status": "in_progress"
            }
        })
    );
    let item_added = format!(
        "event: response.output_item.added\ndata: {}\n\n",
        json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": {
                "id": "msg-codex-discovery",
                "type": "message",
                "role": "assistant",
                "content": []
            }
        })
    );
    let part_added = format!(
        "event: response.content_part.added\ndata: {}\n\n",
        json!({
            "type": "response.content_part.added",
            "item_id": "msg-codex-discovery",
            "output_index": 0,
            "content_index": 0,
            "part": {
                "type": "output_text",
                "text": "",
                "annotations": []
            }
        })
    );
    let delta = format!(
        "event: response.output_text.delta\ndata: {}\n\n",
        json!({
            "type": "response.output_text.delta",
            "item_id": "msg-codex-discovery",
            "output_index": 0,
            "content_index": 0,
            "delta": "OK"
        })
    );
    let completed = format!(
        "event: response.completed\ndata: {}\n\n",
        json!({
            "type": "response.completed",
            "response": {
                "id": response_id,
                "object": "response",
                "model": model,
                "status": "completed",
                "created_at": 123,
                "completed_at": 123,
                "output_text": "OK",
                "output": [{
                    "id": "msg-codex-discovery",
                    "type": "message",
                    "role": "assistant",
                    "status": "completed",
                    "content": [{
                        "type": "output_text",
                        "text": "OK",
                        "annotations": []
                    }]
                }],
                "usage": {
                    "input_tokens": 3,
                    "output_tokens": 1,
                    "total_tokens": 4
                }
            }
        })
    );
    vec![
        created,
        in_progress,
        item_added,
        part_added,
        delta,
        completed,
    ]
    .into_iter()
    .map(Bytes::from)
    .collect()
}

fn unsupported_chatgpt_model_response(model: &str) -> Response<Body> {
    let body = json!({
        "error": {
            "message": format!(
                "The '{model}' model is not supported when using Codex with a ChatGPT account."
            ),
            "type": "invalid_request_error",
            "code": "model_not_supported"
        }
    });
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("unsupported model response should build")
}

fn unauthorized_upstream_response() -> Response<Body> {
    let body = json!({
        "error": {
            "message": "invalid synthetic codex oauth credentials",
            "type": "invalid_request_error",
            "code": "invalid_api_key"
        }
    });
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("unauthorized response should build")
}

async fn handle_upstream_models(
    State(state): State<Arc<UpstreamAppState>>,
    headers: HeaderMap,
) -> Response<Body> {
    let Some(account) = identify_upstream_account(&headers) else {
        state
            .telemetry
            .unauthenticated_hits
            .fetch_add(1, Ordering::SeqCst);
        return unauthorized_upstream_response();
    };
    let reply = {
        let guard = state.mutable.lock().expect("upstream state mutex");
        match account {
            AuthenticatedUpstreamAccount::Free => {
                state
                    .telemetry
                    .free_catalog_hits
                    .fetch_add(1, Ordering::SeqCst);
                guard.free_catalog.clone()
            }
            AuthenticatedUpstreamAccount::Paid => {
                state
                    .telemetry
                    .paid_catalog_hits
                    .fetch_add(1, Ordering::SeqCst);
                guard.paid_catalog.clone()
            }
        }
    };
    match reply {
        UpstreamCatalogReply::Models { etag, slugs } => {
            let cards: Vec<Value> = slugs
                .iter()
                .map(|slug| codex_model_card_json(slug))
                .collect();
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .header("etag", etag)
                .body(Body::from(json!({ "models": cards }).to_string()))
                .expect("catalog response should build")
        }
        UpstreamCatalogReply::Error { status, message } => Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "error": {
                        "message": message,
                        "type": "server_error"
                    }
                })
                .to_string(),
            ))
            .expect("catalog error response should build"),
    }
}

async fn handle_upstream_http_responses(
    State(state): State<Arc<UpstreamAppState>>,
    headers: HeaderMap,
    request: Request,
) -> Response<Body> {
    let seq = state
        .telemetry
        .total_generation_hits
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    let Some(account) = identify_upstream_account(&headers) else {
        state
            .telemetry
            .unauthenticated_hits
            .fetch_add(1, Ordering::SeqCst);
        return unauthorized_upstream_response();
    };
    let raw_body = to_bytes(request.into_body(), 128 * 1024)
        .await
        .expect("upstream responses body should read");
    let content_encoding = headers
        .get(CONTENT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .unwrap_or_default();
    let decoded_body = match content_encoding {
        "" | "identity" => raw_body.to_vec(),
        "zstd" => zstd::stream::decode_all(std::io::Cursor::new(raw_body.as_ref()))
            .expect("zstd upstream request body should decode"),
        "gzip" => {
            let mut decoder = flate2::read::GzDecoder::new(raw_body.as_ref());
            let mut out = Vec::new();
            decoder
                .read_to_end(&mut out)
                .expect("gzip upstream request body should decode");
            out
        }
        other => panic!("unexpected upstream request content-encoding: {other}"),
    };
    let model = serde_json::from_slice::<Value>(&decoded_body)
        .ok()
        .and_then(|payload| {
            payload
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();

    let supported = {
        let guard = state.mutable.lock().expect("upstream state mutex");
        match account {
            AuthenticatedUpstreamAccount::Free => {
                state
                    .telemetry
                    .free_generation_hits
                    .fetch_add(1, Ordering::SeqCst);
                state
                    .telemetry
                    .free_generation_models
                    .lock()
                    .expect("free models mutex")
                    .push(model.clone());
                guard
                    .free_supported_models
                    .iter()
                    .any(|item| item == &model)
            }
            AuthenticatedUpstreamAccount::Paid => {
                state
                    .telemetry
                    .paid_generation_hits
                    .fetch_add(1, Ordering::SeqCst);
                state
                    .telemetry
                    .paid_generation_models
                    .lock()
                    .expect("paid models mutex")
                    .push(model.clone());
                guard
                    .paid_supported_models
                    .iter()
                    .any(|item| item == &model)
            }
        }
    };

    if !supported {
        return unsupported_chatgpt_model_response(&model);
    }

    let chunks = build_codex_sse_body(&model, &format!("resp-codex-http-{seq}"));
    let stream = async_stream::stream! {
        for chunk in chunks {
            yield Ok::<_, Infallible>(chunk);
        }
    };
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .body(Body::from_stream(stream))
        .expect("SSE response should build")
}

async fn handle_upstream_ws_responses(
    State(state): State<Arc<UpstreamAppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response<Body> {
    let Some(account) = identify_upstream_account(&headers) else {
        state
            .telemetry
            .unauthenticated_hits
            .fetch_add(1, Ordering::SeqCst);
        return unauthorized_upstream_response();
    };
    match account {
        AuthenticatedUpstreamAccount::Free => {
            state
                .telemetry
                .free_ws_handshakes
                .fetch_add(1, Ordering::SeqCst);
        }
        AuthenticatedUpstreamAccount::Paid => {
            state
                .telemetry
                .paid_ws_handshakes
                .fetch_add(1, Ordering::SeqCst);
        }
    }
    ws.on_upgrade(move |socket| serve_upstream_ws_socket(socket, state, account))
        .into_response()
}

async fn serve_upstream_ws_socket(
    mut socket: WebSocket,
    state: Arc<UpstreamAppState>,
    account: AuthenticatedUpstreamAccount,
) {
    let mut turn_counter = 0usize;
    while let Some(Ok(frame)) = socket.recv().await {
        let text = match frame {
            AxumWsMessage::Text(text) => text,
            AxumWsMessage::Ping(payload) => {
                let _ = socket.send(AxumWsMessage::Pong(payload)).await;
                continue;
            }
            AxumWsMessage::Close(_) => break,
            _ => continue,
        };
        let Ok(event) = serde_json::from_str::<Value>(text.as_str()) else {
            break;
        };
        if event.get("type").and_then(Value::as_str) != Some("response.create") {
            continue;
        }
        turn_counter += 1;
        state
            .telemetry
            .total_generation_hits
            .fetch_add(1, Ordering::SeqCst);
        let model = event
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let supported = {
            let guard = state.mutable.lock().expect("upstream state mutex");
            match account {
                AuthenticatedUpstreamAccount::Free => {
                    state
                        .telemetry
                        .free_generation_hits
                        .fetch_add(1, Ordering::SeqCst);
                    state
                        .telemetry
                        .free_ws_models
                        .lock()
                        .expect("free ws models mutex")
                        .push(model.clone());
                    guard
                        .free_supported_models
                        .iter()
                        .any(|item| item == &model)
                }
                AuthenticatedUpstreamAccount::Paid => {
                    state
                        .telemetry
                        .paid_generation_hits
                        .fetch_add(1, Ordering::SeqCst);
                    state
                        .telemetry
                        .paid_ws_models
                        .lock()
                        .expect("paid ws models mutex")
                        .push(model.clone());
                    guard
                        .paid_supported_models
                        .iter()
                        .any(|item| item == &model)
                }
            }
        };

        if !supported {
            let error_event = json!({
                "type": "error",
                "status": 400,
                "error": {
                    "type": "invalid_request_error",
                    "code": "model_not_supported",
                    "message": format!(
                        "The '{model}' model is not supported when using Codex with a ChatGPT account."
                    )
                }
            });
            let _ = socket
                .send(AxumWsMessage::Text(error_event.to_string().into()))
                .await;
            continue;
        }

        let account_tag = match account {
            AuthenticatedUpstreamAccount::Free => "free",
            AuthenticatedUpstreamAccount::Paid => "paid",
        };
        let response_id = format!("resp-ws-{account_tag}-{turn_counter}");
        for payload in [
            json!({
                "type": "response.created",
                "response": {
                    "id": response_id,
                    "object": "response",
                    "model": model,
                    "status": "in_progress",
                    "output": []
                }
            }),
            json!({
                "type": "response.output_text.delta",
                "item_id": "msg-ws-discovery",
                "output_index": 0,
                "content_index": 0,
                "delta": "OK"
            }),
            json!({
                "type": "response.completed",
                "response": {
                    "id": response_id,
                    "object": "response",
                    "model": model,
                    "status": "completed",
                    "created_at": 123,
                    "completed_at": 123,
                    "output_text": "OK",
                    "output": [{
                        "id": "msg-ws-discovery",
                        "type": "message",
                        "role": "assistant",
                        "status": "completed",
                        "content": [{
                            "type": "output_text",
                            "text": "OK",
                            "annotations": []
                        }]
                    }],
                    "usage": {
                        "input_tokens": 3,
                        "output_tokens": 1,
                        "total_tokens": 4
                    }
                }
            }),
        ] {
            if socket
                .send(AxumWsMessage::Text(payload.to_string().into()))
                .await
                .is_err()
            {
                return;
            }
        }
    }
}

#[derive(Clone)]
struct AccountSeedSpec {
    key_id: String,
    key_name: String,
    oauth_token: &'static str,
    account_id: &'static str,
    internal_priority: i32,
    auto_fetch_models: bool,
    allowed_models: Option<Vec<String>>,
    locked_models: Option<Vec<String>>,
    upstream_metadata: Option<Value>,
}

impl AccountSeedSpec {
    fn free(label: &str) -> Self {
        Self {
            key_id: format!("key-codex-free-{label}"),
            key_name: "free".to_string(),
            oauth_token: FREE_OAUTH_TOKEN,
            account_id: FREE_ACCOUNT_ID,
            internal_priority: 0,
            auto_fetch_models: true,
            allowed_models: None,
            locked_models: None,
            upstream_metadata: None,
        }
    }

    fn paid(label: &str) -> Self {
        Self {
            key_id: format!("key-codex-paid-{label}"),
            key_name: "paid".to_string(),
            oauth_token: PAID_OAUTH_TOKEN,
            account_id: PAID_ACCOUNT_ID,
            internal_priority: 1,
            auto_fetch_models: true,
            allowed_models: None,
            locked_models: None,
            upstream_metadata: None,
        }
    }
}

struct FixtureBuilder {
    label: String,
    pool_enabled: bool,
    sticky_cache_affinity: bool,
    responses_websocket_enabled: bool,
    include_alias_model: bool,
    client_api_key: String,
    endpoint_body_rules: Option<Value>,
    free_account: AccountSeedSpec,
    paid_account: AccountSeedSpec,
    free_catalog: UpstreamCatalogReply,
    paid_catalog: UpstreamCatalogReply,
    free_supported_models: Vec<String>,
    paid_supported_models: Vec<String>,
}

impl FixtureBuilder {
    fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            pool_enabled: true,
            sticky_cache_affinity: false,
            responses_websocket_enabled: false,
            include_alias_model: false,
            client_api_key: DEFAULT_CLIENT_API_KEY.to_string(),
            endpoint_body_rules: None,
            free_account: AccountSeedSpec::free(label),
            paid_account: AccountSeedSpec::paid(label),
            free_catalog: UpstreamCatalogReply::models("etag-free-v1", &[BASIC_MODEL]),
            paid_catalog: UpstreamCatalogReply::models(
                "etag-paid-v1",
                &[BASIC_MODEL, ADVANCED_MODEL],
            ),
            free_supported_models: vec![BASIC_MODEL.to_string()],
            paid_supported_models: vec![BASIC_MODEL.to_string(), ADVANCED_MODEL.to_string()],
        }
    }

    fn build_provider_config(&self) -> Option<Value> {
        let mut root = serde_json::Map::new();
        if self.pool_enabled {
            let pool_advanced = if self.sticky_cache_affinity {
                json!({
                    "sticky_session_ttl_seconds": 3600,
                    "scheduling_presets": [
                        {"preset": "cache_affinity", "enabled": true}
                    ]
                })
            } else {
                json!({
                    "scheduling_presets": [
                        {"preset": "single_account", "enabled": true}
                    ]
                })
            };
            root.insert("pool_advanced".to_string(), pool_advanced);
        }
        if self.responses_websocket_enabled {
            root.insert("responses_websocket".to_string(), json!({"enabled": true}));
        }
        (!root.is_empty()).then_some(Value::Object(root))
    }

    async fn build(self) -> CodexDiscoveryHarness {
        let provider_config = self.build_provider_config();
        let upstream_state = Arc::new(UpstreamAppState {
            mutable: Mutex::new(UpstreamMutableState {
                free_catalog: self.free_catalog,
                paid_catalog: self.paid_catalog,
                free_supported_models: self.free_supported_models,
                paid_supported_models: self.paid_supported_models,
            }),
            telemetry: UpstreamTelemetry::default(),
        });
        let upstream_router = Router::new()
            .route("/backend-api/codex/models", get(handle_upstream_models))
            .route(
                "/backend-api/codex/responses",
                get(handle_upstream_ws_responses).post(handle_upstream_http_responses),
            )
            .with_state(Arc::clone(&upstream_state));
        let (upstream_url, upstream_server) = start_server(upstream_router).await;

        let mut bootstrap = AppState::new()
            .expect("gateway bootstrap state should build")
            .with_data_state_for_tests(
                crate::data::GatewayDataState::disabled()
                    .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY),
            );
        bootstrap.upstream_target_admission =
            Arc::new(crate::upstream_admission::UpstreamTargetAdmission::new(
                Some(16),
                Duration::from_secs(1),
            ));

        let provider_id = format!("provider-codex-discovery-{}", self.label);
        let endpoint_id = format!("endpoint-codex-responses-{}", self.label);
        let pool_config = provider_config
            .as_ref()
            .filter(|_| self.pool_enabled)
            .and_then(|cfg| admin_provider_pool_config_from_config_value(Some(cfg)));

        let provider = StoredProviderCatalogProvider::new(
            provider_id.clone(),
            "codex".to_string(),
            Some("https://chatgpt.com".to_string()),
            "codex".to_string(),
        )
        .expect("codex provider should build")
        .with_transport_fields(
            true,
            false,
            true,
            None,
            Some(1),
            None,
            Some(20.0),
            None,
            provider_config,
        );

        let base_url = format!("{}/backend-api/codex", upstream_url.trim_end_matches('/'));
        let endpoint = StoredProviderCatalogEndpoint::new(
            endpoint_id.clone(),
            provider_id.clone(),
            "openai:responses".to_string(),
            Some("openai".to_string()),
            Some("responses".to_string()),
            true,
        )
        .expect("codex responses endpoint should build")
        .with_transport_fields(
            base_url,
            None,
            self.endpoint_body_rules,
            Some(1),
            None,
            None,
            None,
            None,
        )
        .expect("codex endpoint transport should build");

        let free_key = build_codex_oauth_catalog_key(&bootstrap, &provider_id, &self.free_account);
        let paid_key = build_codex_oauth_catalog_key(&bootstrap, &provider_id, &self.paid_account);

        let mut model_specs = vec![
            (
                format!("model-basic-{}", self.label),
                format!("global-basic-{}", self.label),
                BASIC_MODEL,
                BASIC_MODEL,
            ),
            (
                format!("model-advanced-{}", self.label),
                format!("global-advanced-{}", self.label),
                ADVANCED_MODEL,
                ADVANCED_MODEL,
            ),
        ];
        if self.include_alias_model {
            model_specs.push((
                format!("model-alias-{}", self.label),
                format!("global-alias-{}", self.label),
                ALIAS_GLOBAL_MODEL,
                ADVANCED_MODEL,
            ));
        }

        let mut selection_rows = Vec::new();
        let mut admin_global_models = Vec::new();
        let mut admin_provider_models = Vec::new();
        let mut endpoint_bindings = Vec::new();

        for (model_id, global_model_id, global_name, provider_model_name) in &model_specs {
            for account in [&self.free_account, &self.paid_account] {
                selection_rows.push(build_codex_selection_row(
                    &provider_id,
                    &endpoint_id,
                    model_id,
                    global_model_id,
                    global_name,
                    provider_model_name,
                    account,
                    self.pool_enabled,
                ));
            }
            admin_global_models.push(
                StoredAdminGlobalModel::new(
                    global_model_id.clone(),
                    (*global_name).to_string(),
                    (*global_name).to_string(),
                    true,
                    None,
                    None,
                    None,
                    Some(json!({ "model_mappings": [provider_model_name] })),
                    0,
                    0,
                    0,
                    Some(1_711_000_000),
                    Some(1_711_000_000),
                )
                .expect("global model should build"),
            );
            admin_provider_models.push(
                StoredAdminProviderModel::new(
                    model_id.clone(),
                    provider_id.clone(),
                    global_model_id.clone(),
                    (*provider_model_name).to_string(),
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
                    Some((*global_name).to_string()),
                    Some((*global_name).to_string()),
                    None,
                    None,
                    None,
                    Some(json!({ "model_mappings": [provider_model_name] })),
                )
                .expect("provider model should build"),
            );
            endpoint_bindings.push(
                StoredModelEndpointBinding::new(
                    model_id.clone(),
                    endpoint_id.clone(),
                    "discovered".to_string(),
                    true,
                    None,
                    None,
                )
                .expect("model endpoint binding should build"),
            );
        }

        let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
            vec![provider],
            vec![endpoint],
            vec![free_key, paid_key],
        ));
        let global_model_repository = Arc::new(
            InMemoryGlobalModelReadRepository::seed(Vec::new())
                .with_admin_global_models(admin_global_models)
                .with_admin_provider_models(admin_provider_models)
                .with_model_endpoint_bindings(endpoint_bindings)
                .with_endpoint_provider_ids([(endpoint_id.as_str(), provider_id.as_str())]),
        );

        let allowed_client_models: Vec<&str> = if self.include_alias_model {
            vec![BASIC_MODEL, ADVANCED_MODEL, ALIAS_GLOBAL_MODEL]
        } else {
            vec![BASIC_MODEL, ADVANCED_MODEL]
        };
        let client_snapshot = StoredAuthApiKeySnapshot::new(
            format!("user-codex-discovery-{}", self.label),
            "alice".to_string(),
            None,
            "user".to_string(),
            "local".to_string(),
            true,
            false,
            Some(json!(["codex"])),
            Some(json!(["openai:responses"])),
            Some(json!(allowed_client_models)),
            format!("client-key-codex-discovery-{}", self.label),
            None,
            true,
            false,
            false,
            None,
            None,
            Some(4_102_444_800),
            Some(json!(["codex"])),
            Some(json!(["openai:responses"])),
            Some(json!(allowed_client_models)),
        )
        .expect("client auth snapshot should build");
        let auth_repository = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![(
            Some(format!(
                "{:x}",
                Sha256::digest(self.client_api_key.as_bytes())
            )),
            client_snapshot,
        )]));
        let candidate_repository = Arc::new(InMemoryRequestCandidateRepository::default());
        let routing_repository = Arc::new(InMemoryRoutingGroupRepository::default());
        routing_repository
            .create_routing_group(CreateRoutingGroupRecord {
                id: format!("routing-codex-discovery-{}", self.label),
                name: format!("routing-codex-discovery-{}", self.label),
                description: None,
                enabled: true,
                is_system_default: true,
                sort_order: 0,
                config_json: json!({
                    "default_policy": {
                        "priority_mode": "provider",
                        "scheduling_mode": "fixed_order",
                        "sticky_key_attempts": 1
                    }
                }),
                version: 1,
                created_at: 1,
                updated_at: 1,
                published_at: Some(1),
            })
            .await
            .expect("routing group should create");

        let gateway_state = bootstrap
            .with_data_state_for_tests(
                crate::data::GatewayDataState::with_auth_candidate_selection_provider_catalog_request_candidates_and_usage_for_tests(
                    auth_repository,
                    Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(selection_rows)),
                    Arc::clone(&provider_catalog_repository),
                    Arc::clone(&candidate_repository),
                    Arc::new(InMemoryUsageReadRepository::default()),
                    DEVELOPMENT_ENCRYPTION_KEY,
                )
                .with_global_model_repository_for_tests(global_model_repository)
                .with_routing_group_repository_for_tests(routing_repository)
                .with_system_config_values_for_tests([("request_record_level".into(), json!("full"))]),
            )
            .with_usage_runtime_for_tests(UsageRuntimeConfig {
                enabled: true,
                ..Default::default()
            });

        let (gateway_url, gateway_server) =
            start_server(build_router_with_state(gateway_state.clone())).await;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("HTTP client should build");

        CodexDiscoveryHarness {
            provider_id,
            endpoint_id,
            free_key_id: self.free_account.key_id,
            paid_key_id: self.paid_account.key_id,
            client_api_key: self.client_api_key,
            gateway_url,
            client,
            gateway_state,
            provider_catalog_repository,
            candidate_repository,
            pool_config,
            upstream_state,
            servers: vec![gateway_server, upstream_server],
        }
    }
}

fn build_codex_oauth_catalog_key(
    state: &AppState,
    provider_id: &str,
    spec: &AccountSeedSpec,
) -> StoredProviderCatalogKey {
    let auth_config_json = json!({
        "provider_type": "codex",
        "account_id": spec.account_id,
        "access_token": spec.oauth_token,
        "expires_at": 4_102_444_800_u64
    })
    .to_string();
    let mut key = StoredProviderCatalogKey::new(
        spec.key_id.clone(),
        provider_id.to_string(),
        spec.key_name.clone(),
        "oauth".to_string(),
        None,
        true,
    )
    .expect("codex oauth key should build")
    .with_transport_fields(
        Some(json!(["openai:responses"])),
        state
            .seal_provider_catalog_key_api_key(provider_id, &spec.key_id, spec.oauth_token)
            .expect("codex oauth access token should seal"),
        Some(
            state
                .seal_provider_catalog_key_auth_config(provider_id, &spec.key_id, &auth_config_json)
                .expect("codex oauth auth_config should seal"),
        ),
        None,
        None,
        spec.allowed_models.as_ref().map(|models| json!(models)),
        None,
        None,
        None,
    )
    .expect("codex oauth key transport should build");
    key.internal_priority = spec.internal_priority;
    key.auto_fetch_models = spec.auto_fetch_models;
    key.locked_models = spec.locked_models.as_ref().map(|models| json!(models));
    key.upstream_metadata = spec.upstream_metadata.clone();
    key
}

#[allow(clippy::too_many_arguments)]
fn build_codex_selection_row(
    provider_id: &str,
    endpoint_id: &str,
    model_id: &str,
    global_model_id: &str,
    global_model_name: &str,
    provider_model_name: &str,
    account: &AccountSeedSpec,
    pool_enabled: bool,
) -> StoredMinimalCandidateSelectionRow {
    StoredMinimalCandidateSelectionRow {
        provider_id: provider_id.to_string(),
        provider_name: "codex".to_string(),
        provider_type: "codex".to_string(),
        provider_priority: 1,
        provider_is_active: true,
        endpoint_id: endpoint_id.to_string(),
        endpoint_api_format: "openai:responses".to_string(),
        endpoint_api_family: Some("openai".to_string()),
        endpoint_kind: Some("responses".to_string()),
        endpoint_is_active: true,
        key_id: account.key_id.clone(),
        key_name: account.key_name.clone(),
        key_auth_type: "oauth".to_string(),
        key_is_active: true,
        key_api_formats: Some(vec!["openai:responses".to_string()]),
        key_allowed_models: account.allowed_models.clone(),
        key_capabilities: None,
        key_internal_priority: account.internal_priority,
        key_global_priority_by_format: None,
        routing_facts: if pool_enabled {
            StoredMinimalCandidateRoutingFacts {
                provider_pool_enabled: true,
                ..Default::default()
            }
        } else {
            Default::default()
        },
        model_id: model_id.to_string(),
        global_model_id: global_model_id.to_string(),
        global_model_name: global_model_name.to_string(),
        global_model_mappings: None,
        global_model_supports_streaming: Some(true),
        model_provider_model_name: provider_model_name.to_string(),
        model_provider_model_mappings: Some(vec![StoredProviderModelMapping {
            name: provider_model_name.to_string(),
            priority: 1,
            api_formats: Some(vec!["openai:responses".to_string()]),
            endpoint_ids: None,
            operations: None,
        }]),
        model_supports_streaming: Some(true),
        model_is_active: true,
        model_is_available: true,
    }
}

struct CodexDiscoveryHarness {
    provider_id: String,
    endpoint_id: String,
    free_key_id: String,
    paid_key_id: String,
    client_api_key: String,
    gateway_url: String,
    client: reqwest::Client,
    gateway_state: AppState,
    provider_catalog_repository: Arc<InMemoryProviderCatalogReadRepository>,
    candidate_repository: Arc<InMemoryRequestCandidateRepository>,
    pool_config: Option<AdminProviderPoolConfig>,
    upstream_state: Arc<UpstreamAppState>,
    servers: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for CodexDiscoveryHarness {
    fn drop(&mut self) {
        for server in &self.servers {
            server.abort();
        }
    }
}

impl CodexDiscoveryHarness {
    fn counts(&self) -> UpstreamCountsSnapshot {
        let telemetry = &self.upstream_state.telemetry;
        UpstreamCountsSnapshot {
            free_catalog: telemetry.free_catalog_hits.load(Ordering::SeqCst),
            paid_catalog: telemetry.paid_catalog_hits.load(Ordering::SeqCst),
            unauthenticated: telemetry.unauthenticated_hits.load(Ordering::SeqCst),
            total_gen: telemetry.total_generation_hits.load(Ordering::SeqCst),
            free_gen: telemetry.free_generation_hits.load(Ordering::SeqCst),
            paid_gen: telemetry.paid_generation_hits.load(Ordering::SeqCst),
            free_ws_handshakes: telemetry.free_ws_handshakes.load(Ordering::SeqCst),
            paid_ws_handshakes: telemetry.paid_ws_handshakes.load(Ordering::SeqCst),
            free_models: telemetry
                .free_generation_models
                .lock()
                .expect("free models mutex")
                .clone(),
            paid_models: telemetry
                .paid_generation_models
                .lock()
                .expect("paid models mutex")
                .clone(),
            free_ws_models: telemetry
                .free_ws_models
                .lock()
                .expect("free ws models mutex")
                .clone(),
            paid_ws_models: telemetry
                .paid_ws_models
                .lock()
                .expect("paid ws models mutex")
                .clone(),
        }
    }

    fn set_free_catalog(&self, reply: UpstreamCatalogReply) {
        self.upstream_state
            .mutable
            .lock()
            .expect("upstream state mutex")
            .free_catalog = reply;
    }

    fn set_paid_catalog(&self, reply: UpstreamCatalogReply) {
        self.upstream_state
            .mutable
            .lock()
            .expect("upstream state mutex")
            .paid_catalog = reply;
    }

    async fn fetch_key_models(&self, key_id: &str) -> ModelFetchRunSummary {
        perform_model_fetch_for_key(&self.gateway_state, &self.provider_id, key_id)
            .await
            .expect("per-key model fetch should complete")
    }

    async fn fetch_both_keys(&self) {
        let free_summary = self.fetch_key_models(&self.free_key_id).await;
        assert_eq!(
            free_summary.succeeded, 1,
            "free key model fetch must succeed"
        );
        let paid_summary = self.fetch_key_models(&self.paid_key_id).await;
        assert_eq!(
            paid_summary.succeeded, 1,
            "paid key model fetch must succeed"
        );
    }

    async fn load_stored_key(&self, key_id: &str) -> StoredProviderCatalogKey {
        self.provider_catalog_repository
            .list_keys_by_ids(&[key_id.to_string()])
            .await
            .expect("stored keys should load")
            .into_iter()
            .next()
            .expect("stored key should exist")
    }

    async fn admin_force_refresh_key_models(&self, key_id: &str) -> (StatusCode, Value) {
        let response = self
            .client
            .post(format!(
                "{}/api/admin/provider-query/models",
                self.gateway_url
            ))
            .header(GATEWAY_HEADER, "rust-phase3b")
            .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-codex-discovery")
            .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
            .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-codex-discovery")
            .json(&json!({
                "provider_id": self.provider_id,
                "api_key_id": key_id,
                "force_refresh": true
            }))
            .send()
            .await
            .expect("admin provider-query/models request should complete");
        let status = response.status();
        let body = response
            .json::<Value>()
            .await
            .expect("admin provider-query/models JSON should parse");
        (status, body)
    }

    async fn admin_test_fixed_key_model(&self, key_id: &str, model: &str) -> (StatusCode, Value) {
        let response = self
            .client
            .post(format!(
                "{}/api/admin/provider-query/test-model",
                self.gateway_url
            ))
            .header(GATEWAY_HEADER, "rust-phase3b")
            .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-codex-discovery")
            .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
            .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-codex-discovery")
            .json(&json!({
                "provider_id": self.provider_id,
                "endpoint_id": self.endpoint_id,
                "api_format": "openai:responses",
                "api_key_id": key_id,
                "model": model
            }))
            .send()
            .await
            .expect("admin test-model request should complete");
        let status = response.status();
        let body = response
            .json::<Value>()
            .await
            .expect("admin test-model JSON should parse");
        (status, body)
    }

    async fn post_responses(
        &self,
        trace_id: &str,
        body: &Value,
    ) -> (StatusCode, String, Option<String>) {
        let response = self
            .client
            .post(format!("{}/v1/responses", self.gateway_url))
            .bearer_auth(&self.client_api_key)
            .header(TRACE_ID_HEADER, trace_id)
            .json(body)
            .send()
            .await
            .expect("gateway /v1/responses request should complete");
        read_bounded_response(response).await
    }
}

async fn read_bounded_response(
    mut response: reqwest::Response,
) -> (StatusCode, String, Option<String>) {
    let status = response.status();
    let miss_reason = response
        .headers()
        .get(LOCAL_EXECUTION_RUNTIME_MISS_REASON_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .expect("response body chunk should read")
    {
        assert!(bytes.len() + chunk.len() <= 128 * 1024);
        bytes.extend_from_slice(&chunk);
    }
    (
        status,
        String::from_utf8(bytes).expect("UTF-8 response body"),
        miss_reason,
    )
}

async fn read_ws_turn_until_terminal(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Vec<Value> {
    let mut events = Vec::new();
    loop {
        let next = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("gateway WebSocket should deliver turn frame before timeout");
        let Some(frame_result) = next else {
            break;
        };
        let frame = frame_result.expect("gateway WebSocket frame should decode");
        match frame {
            TungsteniteMessage::Text(text) => {
                let event: Value = serde_json::from_str(text.as_ref())
                    .expect("WebSocket text frame should be JSON");
                let is_terminal = matches!(
                    event.get("type").and_then(Value::as_str),
                    Some("response.completed" | "response.failed" | "error")
                );
                events.push(event);
                if is_terminal {
                    break;
                }
            }
            TungsteniteMessage::Ping(payload) => {
                socket
                    .send(TungsteniteMessage::Pong(payload))
                    .await
                    .expect("client pong should send");
            }
            TungsteniteMessage::Close(_) => break,
            _ => {}
        }
    }
    events
}

fn run_async_test_on_large_stack<F>(name: &'static str, future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    let handle = std::thread::Builder::new()
        .name(name.to_string())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("large-stack Codex discovery test runtime should build")
                .block_on(future);
        })
        .expect("large-stack Codex discovery test thread should spawn");
    if let Err(payload) = handle.join() {
        std::panic::resume_unwind(payload);
    }
}

/// 管理端显式强刷在 `auto_fetch_models=false` 时按各自真实 OAuth 认证写入独立发现证据，且不篡改用户开关。
#[test]
fn codex_model_discovery_admin_force_refresh_establishes_account_evidence_without_auto_fetch() {
    run_async_test_on_large_stack(
        "codex_model_discovery_admin_force_refresh_establishes_account_evidence_without_auto_fetch",
        codex_model_discovery_admin_force_refresh_establishes_account_evidence_without_auto_fetch_impl(),
    );
}

async fn codex_model_discovery_admin_force_refresh_establishes_account_evidence_without_auto_fetch_impl(
) {
    let mut builder = FixtureBuilder::new("admin-force-no-auto");
    builder.free_account.auto_fetch_models = false;
    builder.paid_account.auto_fetch_models = false;
    let harness = builder.build().await;

    let (free_status, free_payload) = harness
        .admin_force_refresh_key_models(&harness.free_key_id)
        .await;
    assert_eq!(free_status, StatusCode::OK, "{free_payload}");
    assert_eq!(free_payload["success"], json!(true), "{free_payload}");

    let (paid_status, paid_payload) = harness
        .admin_force_refresh_key_models(&harness.paid_key_id)
        .await;
    assert_eq!(paid_status, StatusCode::OK, "{paid_payload}");
    assert_eq!(paid_payload["success"], json!(true), "{paid_payload}");

    let counts = harness.counts();
    assert_eq!(counts.free_catalog, 1);
    assert_eq!(counts.paid_catalog, 1);
    assert_eq!(counts.unauthenticated, 0);

    let stored_free = harness.load_stored_key(&harness.free_key_id).await;
    let stored_paid = harness.load_stored_key(&harness.paid_key_id).await;
    assert!(!stored_free.auto_fetch_models);
    assert!(!stored_paid.auto_fetch_models);
    assert_eq!(
        stored_free
            .upstream_metadata
            .as_ref()
            .and_then(|meta| meta.get(DISCOVERY_NAMESPACE)),
        Some(&json!({ "model_ids": [BASIC_MODEL] }))
    );
    assert_eq!(
        stored_paid
            .upstream_metadata
            .as_ref()
            .and_then(|meta| meta.get(DISCOVERY_NAMESPACE)),
        Some(&json!({ "model_ids": [BASIC_MODEL, ADVANCED_MODEL] }))
    );
}

/// Verification 1：free 优先并作为 PoolGroup 代表时，真实抓取后请求高级模型只触达 paid，free 零上游发送。
#[test]
fn codex_model_discovery_pool_free_representative_routes_advanced_to_paid_only() {
    run_async_test_on_large_stack(
        "codex_model_discovery_pool_free_representative_routes_advanced_to_paid_only",
        codex_model_discovery_pool_free_representative_routes_advanced_to_paid_only_impl(),
    );
}

async fn codex_model_discovery_pool_free_representative_routes_advanced_to_paid_only_impl() {
    let harness = FixtureBuilder::new("pool-rep").build().await;
    harness.fetch_both_keys().await;

    let stored_free = harness.load_stored_key(&harness.free_key_id).await;
    let stored_paid = harness.load_stored_key(&harness.paid_key_id).await;
    assert_eq!(
        stored_free
            .upstream_metadata
            .as_ref()
            .and_then(|meta| meta.get(DISCOVERY_NAMESPACE)),
        Some(&json!({ "model_ids": [BASIC_MODEL] }))
    );
    assert_eq!(
        stored_paid
            .upstream_metadata
            .as_ref()
            .and_then(|meta| meta.get(DISCOVERY_NAMESPACE)),
        Some(&json!({ "model_ids": [BASIC_MODEL, ADVANCED_MODEL] }))
    );

    let trace_id = "trace-codex-discovery-pool-rep";
    let (status, raw_body, _) = harness
        .post_responses(
            trace_id,
            &json!({
                "model": ADVANCED_MODEL,
                "input": "Reply OK",
                "stream": true
            }),
        )
        .await;
    let clean_body = strip_sse_keepalive_comments(&raw_body);
    assert_eq!(status, StatusCode::OK, "{clean_body}");
    assert!(
        clean_body.contains("event: response.completed"),
        "{clean_body}"
    );

    let counts = harness.counts();
    assert_eq!(counts.free_catalog, 1);
    assert_eq!(counts.paid_catalog, 1);
    assert_eq!(
        counts.free_gen, 0,
        "free account must receive zero upstream sends"
    );
    assert_eq!(
        counts.paid_gen, 1,
        "paid account must serve the advanced model"
    );
    assert_eq!(counts.paid_models, vec![ADVANCED_MODEL.to_string()]);

    let candidates = harness
        .candidate_repository
        .list_by_request_id(trace_id)
        .await
        .expect("request candidates should load");
    assert!(
        !candidates.iter().any(|candidate| {
            candidate.key_id.as_deref() == Some(harness.free_key_id.as_str())
                && matches!(
                    candidate.status,
                    RequestCandidateStatus::Failed | RequestCandidateStatus::Success
                )
        }),
        "free representative must be skipped rather than attempted: {candidates:#?}"
    );
}

/// Verification 2：free 的 `locked_models`、显式白名单或客户端别名映射均不能绕过缺失的远端发现目录。
#[test]
fn codex_model_discovery_locked_models_and_alias_whitelist_cannot_override_missing_remote_model() {
    run_async_test_on_large_stack(
        "codex_model_discovery_locked_models_and_alias_whitelist_cannot_override_missing_remote_model",
        codex_model_discovery_locked_models_and_alias_whitelist_cannot_override_missing_remote_model_impl(),
    );
}

async fn codex_model_discovery_locked_models_and_alias_whitelist_cannot_override_missing_remote_model_impl(
) {
    let mut builder = FixtureBuilder::new("locked-and-alias");
    builder.include_alias_model = true;
    builder.free_account.allowed_models = Some(vec![
        BASIC_MODEL.to_string(),
        ADVANCED_MODEL.to_string(),
        ALIAS_GLOBAL_MODEL.to_string(),
    ]);
    builder.free_account.locked_models = Some(vec![
        ADVANCED_MODEL.to_string(),
        ALIAS_GLOBAL_MODEL.to_string(),
    ]);
    let harness = builder.build().await;
    harness.fetch_both_keys().await;

    let stored_free = harness.load_stored_key(&harness.free_key_id).await;
    let free_whitelist = stored_free
        .allowed_models
        .as_ref()
        .and_then(Value::as_array)
        .expect("free whitelist should persist");
    assert!(
        free_whitelist.contains(&json!(ADVANCED_MODEL)),
        "locked_models must still populate configuration allowed_models: {free_whitelist:?}"
    );
    assert!(
        free_whitelist.contains(&json!(ALIAS_GLOBAL_MODEL)),
        "locked alias must still populate configuration allowed_models: {free_whitelist:?}"
    );

    let (direct_status, direct_body, _) = harness
        .post_responses(
            "trace-codex-discovery-locked-direct",
            &json!({
                "model": ADVANCED_MODEL,
                "input": "Reply OK",
                "stream": true
            }),
        )
        .await;
    assert_eq!(direct_status, StatusCode::OK, "{direct_body}");

    let (alias_status, alias_body, _) = harness
        .post_responses(
            "trace-codex-discovery-locked-alias",
            &json!({
                "model": ALIAS_GLOBAL_MODEL,
                "input": "Reply OK",
                "stream": true
            }),
        )
        .await;
    assert_eq!(alias_status, StatusCode::OK, "{alias_body}");

    let counts = harness.counts();
    assert_eq!(
        counts.free_gen, 0,
        "locked_models and client alias in whitelist must never authorize free account upstream send"
    );
    assert_eq!(counts.paid_gen, 2);
    assert_eq!(
        counts.paid_models,
        vec![ADVANCED_MODEL.to_string(), ADVANCED_MODEL.to_string()]
    );
}

/// Verification 3：free 正常服务基础模型且不被整号禁用；建立 free sticky 会话后请求高级模型必须转到 paid。
#[test]
fn codex_model_discovery_preserves_free_basic_service_and_reroutes_sticky_session_for_advanced() {
    run_async_test_on_large_stack(
        "codex_model_discovery_preserves_free_basic_service_and_reroutes_sticky_session_for_advanced",
        codex_model_discovery_preserves_free_basic_service_and_reroutes_sticky_session_for_advanced_impl(),
    );
}

async fn codex_model_discovery_preserves_free_basic_service_and_reroutes_sticky_session_for_advanced_impl(
) {
    let mut builder = FixtureBuilder::new("sticky-reroute");
    builder.sticky_cache_affinity = true;
    builder.free_account.allowed_models =
        Some(vec![BASIC_MODEL.to_string(), ADVANCED_MODEL.to_string()]);
    builder.free_account.locked_models = Some(vec![ADVANCED_MODEL.to_string()]);
    let harness = builder.build().await;
    harness.fetch_both_keys().await;

    let session_key = "codex-sticky-session-alpha";
    let basic_payload = json!({
        "model": BASIC_MODEL,
        "input": "Reply OK",
        "prompt_cache_key": session_key,
        "stream": true
    });
    let sticky_token = extract_pool_sticky_session_token(&basic_payload)
        .expect("prompt_cache_key must produce a pool sticky session token");

    let (basic_status, basic_body, _) = harness
        .post_responses("trace-codex-discovery-basic-1", &basic_payload)
        .await;
    assert_eq!(basic_status, StatusCode::OK, "{basic_body}");
    assert_eq!(harness.counts().free_gen, 1);
    assert_eq!(harness.counts().paid_gen, 0);
    assert_eq!(harness.counts().free_models, vec![BASIC_MODEL.to_string()]);

    let pool_config = harness
        .pool_config
        .as_ref()
        .expect("pool config must exist");
    record_admin_provider_pool_success(
        harness.gateway_state.runtime_state.as_ref(),
        &harness.provider_id,
        &harness.free_key_id,
        pool_config,
        Some(sticky_token.as_str()),
        4,
        Some(5),
    )
    .await;
    let bound_sticky = read_provider_pool_sticky_bound_key_id(
        harness.gateway_state.runtime_state.as_ref(),
        &harness.provider_id,
        pool_config,
        Some(sticky_token.as_str()),
    )
    .await;
    assert_eq!(
        bound_sticky.as_deref(),
        Some(harness.free_key_id.as_str()),
        "sticky session must explicitly bind to free before advanced request"
    );

    let (adv_status, adv_body, _) = harness
        .post_responses(
            "trace-codex-discovery-sticky-adv",
            &json!({
                "model": ADVANCED_MODEL,
                "input": "Reply OK",
                "prompt_cache_key": session_key,
                "stream": true
            }),
        )
        .await;
    assert_eq!(adv_status, StatusCode::OK, "{adv_body}");
    let after_adv = harness.counts();
    assert_eq!(
        after_adv.free_gen, 1,
        "sticky free account must not receive advanced model request"
    );
    assert_eq!(after_adv.paid_gen, 1);
    assert_eq!(after_adv.paid_models, vec![ADVANCED_MODEL.to_string()]);

    let stored_free = harness.load_stored_key(&harness.free_key_id).await;
    assert!(stored_free.is_active, "free account must remain active");
    assert!(
        stored_free.oauth_invalid_at_unix_secs.is_none(),
        "missing model capability must never mark OAuth invalid"
    );
}

/// Verification 4：仅有旧 `codex_models` / `allowed_models` 或首次抓取 500 时严格拒绝（503 + 零发送 + `key_model_disabled`），强刷 paid 后免重启恢复 200。
#[test]
fn codex_model_discovery_denies_missing_or_failed_initial_catalog_and_recovers_on_admin_force_refresh(
) {
    run_async_test_on_large_stack(
        "codex_model_discovery_denies_missing_or_failed_initial_catalog_and_recovers_on_admin_force_refresh",
        codex_model_discovery_denies_missing_or_failed_initial_catalog_and_recovers_on_admin_force_refresh_impl(),
    );
}

async fn codex_model_discovery_denies_missing_or_failed_initial_catalog_and_recovers_on_admin_force_refresh_impl(
) {
    let legacy_only_metadata = json!({
        "codex_models": {
            "cards": {
                BASIC_MODEL: codex_model_card_json(BASIC_MODEL),
                ADVANCED_MODEL: codex_model_card_json(ADVANCED_MODEL)
            }
        }
    });
    let mut builder = FixtureBuilder::new("missing-and-recover");
    builder.free_account.auto_fetch_models = true;
    builder.free_account.allowed_models =
        Some(vec![BASIC_MODEL.to_string(), ADVANCED_MODEL.to_string()]);
    builder.free_account.upstream_metadata = Some(legacy_only_metadata.clone());
    builder.paid_account.auto_fetch_models = false;
    builder.paid_account.allowed_models =
        Some(vec![BASIC_MODEL.to_string(), ADVANCED_MODEL.to_string()]);
    builder.paid_account.upstream_metadata = Some(legacy_only_metadata);
    builder.free_catalog = UpstreamCatalogReply::server_error("initial codex catalog 500");
    builder.paid_catalog = UpstreamCatalogReply::server_error("initial codex catalog 500");
    let harness = builder.build().await;

    let missing_trace = "trace-codex-discovery-missing-evidence";
    let (missing_status, missing_body, missing_miss_reason) = harness
        .post_responses(
            missing_trace,
            &json!({
                "model": ADVANCED_MODEL,
                "input": "Reply OK",
                "stream": true
            }),
        )
        .await;
    assert_eq!(
        missing_status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{missing_body}"
    );
    assert_ne!(
        missing_miss_reason.as_deref(),
        Some("execution_runtime_candidates_exhausted")
    );
    assert_eq!(
        harness.counts().total_gen,
        0,
        "legacy metadata must send nothing"
    );
    let missing_candidates = harness
        .candidate_repository
        .list_by_request_id(missing_trace)
        .await
        .expect("candidates should load");
    let missing_diag = format!("{missing_candidates:?}");
    assert!(
        missing_diag.contains("key_model_disabled"),
        "missing evidence must record key_model_disabled: {missing_diag}"
    );

    let free_summary = harness.fetch_key_models(&harness.free_key_id).await;
    assert_eq!(free_summary.failed, 1);
    let _ = harness
        .admin_force_refresh_key_models(&harness.paid_key_id)
        .await;

    let failed_trace = "trace-codex-discovery-initial-500";
    let (failed_status, failed_body, _) = harness
        .post_responses(
            failed_trace,
            &json!({
                "model": ADVANCED_MODEL,
                "input": "Reply OK",
                "stream": true
            }),
        )
        .await;
    assert_eq!(
        failed_status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{failed_body}"
    );
    assert_eq!(
        harness.counts().total_gen,
        0,
        "initial 500 and preset fallback must not synthesize discovery evidence"
    );
    let failed_candidates = harness
        .candidate_repository
        .list_by_request_id(failed_trace)
        .await
        .expect("candidates should load");
    assert!(format!("{failed_candidates:?}").contains("key_model_disabled"));

    harness.set_paid_catalog(UpstreamCatalogReply::models(
        "etag-paid-recovered",
        &[BASIC_MODEL, ADVANCED_MODEL],
    ));
    let (refresh_status, refresh_payload) = harness
        .admin_force_refresh_key_models(&harness.paid_key_id)
        .await;
    assert_eq!(refresh_status, StatusCode::OK, "{refresh_payload}");
    assert_eq!(refresh_payload["success"], json!(true), "{refresh_payload}");

    let (recovered_status, recovered_body, _) = harness
        .post_responses(
            "trace-codex-discovery-recovered",
            &json!({
                "model": ADVANCED_MODEL,
                "input": "Reply OK",
                "stream": true
            }),
        )
        .await;
    assert_eq!(recovered_status, StatusCode::OK, "{recovered_body}");
    let counts = harness.counts();
    assert_eq!(counts.free_gen, 0);
    assert_eq!(counts.paid_gen, 1);
    assert_eq!(counts.paid_models, vec![ADVANCED_MODEL.to_string()]);
    assert!(
        !harness
            .load_stored_key(&harness.paid_key_id)
            .await
            .auto_fetch_models
    );
}

/// Verification 5：成功抓取后远端 500 保留 LKG；成功目录移除高级模型后完整替换并立即失效路由缓存（503 + 零发送）；重新返回后恢复 200。
#[test]
fn codex_model_discovery_preserves_lkg_on_refresh_500_and_invalidates_on_model_removal_and_readd() {
    run_async_test_on_large_stack(
        "codex_model_discovery_preserves_lkg_on_refresh_500_and_invalidates_on_model_removal_and_readd",
        codex_model_discovery_preserves_lkg_on_refresh_500_and_invalidates_on_model_removal_and_readd_impl(),
    );
}

async fn codex_model_discovery_preserves_lkg_on_refresh_500_and_invalidates_on_model_removal_and_readd_impl(
) {
    let mut builder = FixtureBuilder::new("lkg-lifecycle");
    builder.paid_account.allowed_models =
        Some(vec![BASIC_MODEL.to_string(), ADVANCED_MODEL.to_string()]);
    builder.paid_account.locked_models = Some(vec![ADVANCED_MODEL.to_string()]);
    let harness = builder.build().await;
    harness.fetch_both_keys().await;

    let (first_status, first_body, _) = harness
        .post_responses(
            "trace-codex-discovery-lifecycle-1",
            &json!({"model": ADVANCED_MODEL, "input": "Reply OK", "stream": true}),
        )
        .await;
    assert_eq!(first_status, StatusCode::OK, "{first_body}");
    assert_eq!(harness.counts().paid_gen, 1);

    harness.set_paid_catalog(UpstreamCatalogReply::server_error("transient catalog 500"));
    let failed_refresh = harness.fetch_key_models(&harness.paid_key_id).await;
    assert_eq!(failed_refresh.failed, 1);

    let (lkg_status, lkg_body, _) = harness
        .post_responses(
            "trace-codex-discovery-lifecycle-lkg",
            &json!({"model": ADVANCED_MODEL, "input": "Reply OK", "stream": true}),
        )
        .await;
    assert_eq!(lkg_status, StatusCode::OK, "{lkg_body}");
    assert_eq!(
        harness.counts().paid_gen,
        2,
        "LKG catalog must remain eligible"
    );
    assert_eq!(harness.counts().free_gen, 0);

    harness.set_paid_catalog(UpstreamCatalogReply::models(
        "etag-paid-downgraded",
        &[BASIC_MODEL],
    ));
    let downgraded_refresh = harness.fetch_key_models(&harness.paid_key_id).await;
    assert_eq!(downgraded_refresh.succeeded, 1);

    let removed_trace = "trace-codex-discovery-lifecycle-removed";
    let (removed_status, removed_body, _) = harness
        .post_responses(
            removed_trace,
            &json!({"model": ADVANCED_MODEL, "input": "Reply OK", "stream": true}),
        )
        .await;
    assert_eq!(
        removed_status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{removed_body}"
    );
    assert_eq!(
        harness.counts().paid_gen,
        2,
        "removed model must not be unioned with stale LKG or cached route"
    );
    assert_eq!(harness.counts().free_gen, 0);
    let removed_candidates = harness
        .candidate_repository
        .list_by_request_id(removed_trace)
        .await
        .expect("candidates should load");
    assert!(format!("{removed_candidates:?}").contains("key_model_disabled"));

    harness.set_paid_catalog(UpstreamCatalogReply::models(
        "etag-paid-restored",
        &[BASIC_MODEL, ADVANCED_MODEL],
    ));
    let restored_refresh = harness.fetch_key_models(&harness.paid_key_id).await;
    assert_eq!(restored_refresh.succeeded, 1);

    let (restored_status, restored_body, _) = harness
        .post_responses(
            "trace-codex-discovery-lifecycle-restored",
            &json!({"model": ADVANCED_MODEL, "input": "Reply OK", "stream": true}),
        )
        .await;
    assert_eq!(restored_status, StatusCode::OK, "{restored_body}");
    assert_eq!(harness.counts().paid_gen, 3);
    assert_eq!(harness.counts().free_gen, 0);
}

/// Verification 6：关闭 `pool_advanced` 的普通候选在同步、SSE 与 Responses WebSocket 每轮独立 `response.create` 上均不得向 free 发送高级模型。
#[test]
fn codex_model_discovery_ordinary_candidates_enforce_sync_sse_and_per_turn_websocket_isolation() {
    run_async_test_on_large_stack(
        "codex_model_discovery_ordinary_candidates_enforce_sync_sse_and_per_turn_websocket_isolation",
        codex_model_discovery_ordinary_candidates_enforce_sync_sse_and_per_turn_websocket_isolation_impl(),
    );
}

async fn codex_model_discovery_ordinary_candidates_enforce_sync_sse_and_per_turn_websocket_isolation_impl(
) {
    let mut builder = FixtureBuilder::new("ordinary-sync-sse-ws");
    builder.pool_enabled = false;
    builder.responses_websocket_enabled = true;
    builder.free_account.allowed_models =
        Some(vec![BASIC_MODEL.to_string(), ADVANCED_MODEL.to_string()]);
    builder.free_account.locked_models = Some(vec![ADVANCED_MODEL.to_string()]);
    let harness = builder.build().await;
    harness.fetch_both_keys().await;

    let (sync_status, sync_body, _) = harness
        .post_responses(
            "trace-codex-discovery-ordinary-sync",
            &json!({
                "model": ADVANCED_MODEL,
                "input": "Reply OK",
                "stream": false
            }),
        )
        .await;
    assert_eq!(sync_status, StatusCode::OK, "{sync_body}");
    let sync_json: Value = serde_json::from_str(&sync_body).expect("sync response should be JSON");
    assert_eq!(sync_json["status"], json!("completed"), "{sync_json}");
    assert_eq!(harness.counts().free_gen, 0, "ordinary sync must skip free");
    assert_eq!(harness.counts().paid_gen, 1);

    let (sse_status, sse_body, _) = harness
        .post_responses(
            "trace-codex-discovery-ordinary-sse",
            &json!({
                "model": ADVANCED_MODEL,
                "input": "Reply OK",
                "stream": true
            }),
        )
        .await;
    let clean_sse = strip_sse_keepalive_comments(&sse_body);
    assert_eq!(sse_status, StatusCode::OK, "{clean_sse}");
    assert!(
        clean_sse.contains("event: response.completed"),
        "{clean_sse}"
    );
    assert_eq!(harness.counts().free_gen, 0, "ordinary SSE must skip free");
    assert_eq!(harness.counts().paid_gen, 2);

    let ws_url = format!(
        "{}/v1/responses",
        harness.gateway_url.replacen("http://", "ws://", 1)
    );
    let mut ws_request = ws_url
        .into_client_request()
        .expect("WebSocket client request should build");
    ws_request.headers_mut().insert(
        AUTHORIZATION,
        format!("Bearer {}", harness.client_api_key)
            .parse()
            .expect("Bearer header should parse"),
    );
    let (mut ws_stream, ws_response) = tokio_tungstenite::connect_async(ws_request)
        .await
        .expect("gateway /v1/responses WebSocket should connect");
    assert_eq!(ws_response.status(), StatusCode::SWITCHING_PROTOCOLS);

    ws_stream
        .send(TungsteniteMessage::Text(
            json!({
                "type": "response.create",
                "model": BASIC_MODEL,
                "input": [{
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Reply OK"}]
                }]
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("first WebSocket response.create should send");
    let first_turn_events = read_ws_turn_until_terminal(&mut ws_stream).await;
    assert_eq!(
        first_turn_events
            .last()
            .and_then(|event| event.get("type"))
            .and_then(Value::as_str),
        Some("response.completed"),
        "first basic WebSocket turn must complete on free: {first_turn_events:?}"
    );
    assert_eq!(
        harness.counts().free_ws_models,
        vec![BASIC_MODEL.to_string()]
    );
    assert!(harness.counts().paid_ws_models.is_empty());

    ws_stream
        .send(TungsteniteMessage::Text(
            json!({
                "type": "response.create",
                "model": ADVANCED_MODEL,
                "input": [{
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Reply OK"}]
                }]
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("second WebSocket response.create should send");
    let second_turn_events = read_ws_turn_until_terminal(&mut ws_stream).await;
    let counts_after_ws = harness.counts();
    assert_eq!(counts_after_ws.free_ws_handshakes, 1);
    assert_eq!(
        counts_after_ws.free_ws_models,
        vec![BASIC_MODEL.to_string()],
        "second WebSocket turn must never send advanced model to free: {second_turn_events:?}"
    );
    let second_terminal = second_turn_events
        .last()
        .expect("second WebSocket turn must produce a terminal event");
    match second_terminal.get("type").and_then(Value::as_str) {
        Some("response.completed") => {
            assert_eq!(counts_after_ws.paid_ws_handshakes, 1);
            assert_eq!(
                counts_after_ws.paid_ws_models,
                vec![ADVANCED_MODEL.to_string()],
                "rebound independent WebSocket turn must execute on paid"
            );
        }
        Some("error") => {
            let error_code = second_terminal["error"]["code"]
                .as_str()
                .unwrap_or_default();
            assert_eq!(
                error_code, "responses_continuation_model_change_unsupported",
                "only established session model-binding rejection is permitted, got {second_terminal}"
            );
            assert_eq!(counts_after_ws.paid_ws_handshakes, 0);
            assert!(counts_after_ws.paid_ws_models.is_empty());
        }
        other => panic!(
            "unexpected terminal event for second WebSocket turn: {other:?}, events={second_turn_events:?}"
        ),
    }
    let _ = ws_stream.close(None).await;
}

/// Verification 7a：管理端固定账号文本测试对 free 返回 `skipped` / `key_model_disabled` 且零上游发送，固定 paid 成功。
#[test]
fn codex_model_discovery_admin_fixed_key_test_skips_free_and_executes_paid() {
    run_async_test_on_large_stack(
        "codex_model_discovery_admin_fixed_key_test_skips_free_and_executes_paid",
        codex_model_discovery_admin_fixed_key_test_skips_free_and_executes_paid_impl(),
    );
}

async fn codex_model_discovery_admin_fixed_key_test_skips_free_and_executes_paid_impl() {
    let mut builder = FixtureBuilder::new("admin-fixed-test");
    builder.include_alias_model = true;
    builder.free_account.allowed_models = Some(vec![
        BASIC_MODEL.to_string(),
        ADVANCED_MODEL.to_string(),
        ALIAS_GLOBAL_MODEL.to_string(),
    ]);
    builder.free_account.locked_models = Some(vec![
        ADVANCED_MODEL.to_string(),
        ALIAS_GLOBAL_MODEL.to_string(),
    ]);
    let harness = builder.build().await;
    harness.fetch_both_keys().await;

    for requested_model in [ADVANCED_MODEL, ALIAS_GLOBAL_MODEL] {
        let (free_status, free_payload) = harness
            .admin_test_fixed_key_model(&harness.free_key_id, requested_model)
            .await;
        assert_eq!(free_status, StatusCode::OK, "{free_payload}");
        assert_eq!(free_payload["success"], json!(false), "{free_payload}");
        assert_eq!(free_payload["total_attempts"], json!(0), "{free_payload}");
        assert_eq!(
            free_payload["attempts"][0]["effective_model"],
            json!(ADVANCED_MODEL),
            "{free_payload}"
        );
        assert_eq!(
            free_payload["attempts"][0]["status"],
            json!("skipped"),
            "{free_payload}"
        );
        assert_eq!(
            free_payload["attempts"][0]["skip_reason"],
            json!("key_model_disabled"),
            "{free_payload}"
        );
    }
    assert_eq!(harness.counts().free_gen, 0);
    assert_eq!(harness.counts().paid_gen, 0);

    let (paid_status, paid_payload) = harness
        .admin_test_fixed_key_model(&harness.paid_key_id, ALIAS_GLOBAL_MODEL)
        .await;
    assert_eq!(paid_status, StatusCode::OK, "{paid_payload}");
    assert_eq!(paid_payload["success"], json!(true), "{paid_payload}");
    assert_eq!(
        paid_payload["attempts"][0]["effective_model"],
        json!(ADVANCED_MODEL),
        "{paid_payload}"
    );
    assert_eq!(
        paid_payload["attempts"][0]["status"],
        json!("success"),
        "{paid_payload}"
    );
    let counts = harness.counts();
    assert_eq!(counts.free_gen, 0);
    assert_eq!(counts.paid_gen, 1);
    assert_eq!(counts.paid_models, vec![ADVANCED_MODEL.to_string()]);
}

/// Verification 7a-body-rules：请求初始模型为 free 已发现的 `gpt-basic-test`，但 Endpoint `body_rules` 将最终发送模型改写为 `gpt-6.1-sol` 时，固定 free 必须被 `key_model_disabled` 跳过且零上游发送，固定 paid 正常成功。
#[test]
fn codex_model_discovery_admin_body_rules_model_rewrite_enforces_final_provider_model() {
    run_async_test_on_large_stack(
        "codex_model_discovery_admin_body_rules_model_rewrite_enforces_final_provider_model",
        codex_model_discovery_admin_body_rules_model_rewrite_enforces_final_provider_model_impl(),
    );
}

async fn codex_model_discovery_admin_body_rules_model_rewrite_enforces_final_provider_model_impl() {
    let mut builder = FixtureBuilder::new("admin-body-rules-rewrite");
    builder.endpoint_body_rules = Some(json!([
        {
            "action": "set",
            "path": "model",
            "value": ADVANCED_MODEL
        }
    ]));
    let harness = builder.build().await;
    harness.fetch_both_keys().await;

    let (free_status, free_payload) = harness
        .admin_test_fixed_key_model(&harness.free_key_id, BASIC_MODEL)
        .await;
    assert_eq!(free_status, StatusCode::OK, "{free_payload}");
    assert_eq!(free_payload["success"], json!(false), "{free_payload}");
    assert_eq!(free_payload["total_attempts"], json!(0), "{free_payload}");
    assert_eq!(
        free_payload["attempts"][0]["status"],
        json!("skipped"),
        "{free_payload}"
    );
    assert_eq!(
        free_payload["attempts"][0]["skip_reason"],
        json!("key_model_disabled"),
        "{free_payload}"
    );
    assert_eq!(
        harness.counts().free_gen,
        0,
        "body_rules rewrite to advanced model must not leak upstream on free"
    );
    assert_eq!(harness.counts().paid_gen, 0);

    let (paid_status, paid_payload) = harness
        .admin_test_fixed_key_model(&harness.paid_key_id, BASIC_MODEL)
        .await;
    assert_eq!(paid_status, StatusCode::OK, "{paid_payload}");
    assert_eq!(paid_payload["success"], json!(true), "{paid_payload}");
    assert_eq!(
        paid_payload["attempts"][0]["status"],
        json!("success"),
        "{paid_payload}"
    );
    assert_eq!(
        paid_payload["attempts"][0]["request_body"]["model"],
        json!(ADVANCED_MODEL),
        "{paid_payload}"
    );
    let counts = harness.counts();
    assert_eq!(counts.free_gen, 0);
    assert_eq!(counts.paid_gen, 1);
    assert_eq!(counts.paid_models, vec![ADVANCED_MODEL.to_string()]);
}

/// Verification 7b：公开版本化 `/v1/models?client_version=...` 在新账号无证据时仍能引导返回卡片，且不写库冒充账号级发现授权。
#[test]
fn codex_model_discovery_versioned_models_bootstrap_succeeds_without_writing_discovery_evidence() {
    run_async_test_on_large_stack(
        "codex_model_discovery_versioned_models_bootstrap_succeeds_without_writing_discovery_evidence",
        codex_model_discovery_versioned_models_bootstrap_succeeds_without_writing_discovery_evidence_impl(),
    );
}

async fn codex_model_discovery_versioned_models_bootstrap_succeeds_without_writing_discovery_evidence_impl(
) {
    let mut builder = FixtureBuilder::new("versioned-bootstrap");
    builder.pool_enabled = false;
    builder.free_account.auto_fetch_models = false;
    builder.free_account.allowed_models = Some(vec![BASIC_MODEL.to_string()]);
    builder.paid_account.auto_fetch_models = false;
    builder.paid_account.allowed_models =
        Some(vec![BASIC_MODEL.to_string(), ADVANCED_MODEL.to_string()]);
    let harness = builder.build().await;

    let response = harness
        .client
        .get(format!(
            "{}/v1/models?client_version=0.159.3",
            harness.gateway_url
        ))
        .bearer_auth(&harness.client_api_key)
        .send()
        .await
        .expect("versioned /v1/models request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let catalog_payload: Value = response
        .json()
        .await
        .expect("versioned /v1/models JSON should parse");
    let slugs: Vec<&str> = catalog_payload["models"]
        .as_array()
        .expect("models array should exist")
        .iter()
        .filter_map(|card| card.get("slug").and_then(Value::as_str))
        .collect();
    assert!(slugs.contains(&BASIC_MODEL), "slugs={slugs:?}");
    assert!(slugs.contains(&ADVANCED_MODEL), "slugs={slugs:?}");

    for key_id in [&harness.free_key_id, &harness.paid_key_id] {
        let stored_key = harness.load_stored_key(key_id).await;
        assert!(
            stored_key
                .upstream_metadata
                .as_ref()
                .and_then(|meta| meta.get(DISCOVERY_NAMESPACE))
                .is_none(),
            "public /v1/models must not persist codex_model_discovery on key {key_id}"
        );
    }

    let (gen_status, gen_body, _) = harness
        .post_responses(
            "trace-codex-discovery-after-public-models",
            &json!({
                "model": BASIC_MODEL,
                "input": "Reply OK",
                "stream": true
            }),
        )
        .await;
    assert_eq!(gen_status, StatusCode::SERVICE_UNAVAILABLE, "{gen_body}");
    assert_eq!(
        harness.counts().total_gen,
        0,
        "public versioned /v1/models display must not authorize text generation without per-key discovery"
    );
}
