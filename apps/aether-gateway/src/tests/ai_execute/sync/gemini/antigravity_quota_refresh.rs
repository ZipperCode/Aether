use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use aether_admin::provider::quota::parse_antigravity_usage_response;
use aether_data::repository::pool_scores::InMemoryPoolMemberScoreRepository;
use aether_data_contracts::repository::candidate_selection::StoredMinimalCandidateRoutingFacts;
use aether_data_contracts::repository::provider_catalog::{
    ProviderCatalogKeySchedulingStateCasUpdate, ProviderCatalogReadRepository,
};
use base64::Engine as _;

use super::*;
use crate::ai_serving::extract_pool_sticky_session_token;
use crate::constants::{
    GATEWAY_HEADER, TRUSTED_ADMIN_SESSION_ID_HEADER, TRUSTED_ADMIN_USER_ID_HEADER,
    TRUSTED_ADMIN_USER_ROLE_HEADER,
};
use crate::handlers::shared::provider_pool::{
    admin_provider_pool_config_from_config_value, read_provider_pool_sticky_bound_key_id,
};
use crate::handlers::shared::sync_provider_key_quota_status_snapshot;

#[derive(Debug, Clone)]
struct AccountQuotaMockPolicy {
    fetch_models_status: u16,
    target_remaining_fraction: f64,
    sibling_remaining_fraction: f64,
}

#[derive(Debug, Clone)]
struct QuotaRuntimeMockState {
    reset_rfc3339: String,
    account_a: AccountQuotaMockPolicy,
    account_b: AccountQuotaMockPolicy,
}

fn format_rfc3339_secs(unix_secs: u64) -> String {
    chrono::DateTime::from_timestamp(unix_secs as i64, 0)
        .expect("unix timestamp should convert to UTC datetime")
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs()
}

fn hash_client_api_key(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn build_client_auth_snapshot(api_key_id: &str, user_id: &str) -> StoredAuthApiKeySnapshot {
    StoredAuthApiKeySnapshot::new(
        user_id.to_string(),
        "alice".to_string(),
        None,
        "user".to_string(),
        "local".to_string(),
        true,
        false,
        None,
        None,
        None,
        api_key_id.to_string(),
        None,
        true,
        false,
        false,
        None,
        None,
        Some(4_102_444_800),
        None,
        None,
        None,
    )
    .expect("auth snapshot should build")
}

fn build_seeded_antigravity_quota_state(
    project_id: &str,
    target_remaining_fraction: f64,
    sibling_remaining_fraction: f64,
    updated_at_unix_secs: u64,
    observed_at_unix_secs: u64,
    reset_rfc3339: &str,
) -> (serde_json::Value, Option<serde_json::Value>) {
    let usage_body = json!({
        "models": {
            "claude-sonnet-4-5": {
                "displayName": "Claude Sonnet 4.5",
                "quotaInfo": {
                    "remainingFraction": target_remaining_fraction,
                    "resetTime": reset_rfc3339
                }
            },
            "claude-sonnet-4-6": {
                "displayName": "Claude Sonnet 4.6",
                "quotaInfo": {
                    "remainingFraction": sibling_remaining_fraction,
                    "resetTime": reset_rfc3339
                }
            }
        }
    });
    let mut antigravity_bucket =
        parse_antigravity_usage_response(&usage_body, updated_at_unix_secs)
            .expect("antigravity usage fixture should parse");
    let bucket_obj = antigravity_bucket
        .as_object_mut()
        .expect("antigravity metadata bucket should be an object");
    bucket_obj.insert("project_id".to_string(), json!(project_id));
    bucket_obj.insert("subscription_tier".to_string(), json!("pro"));
    bucket_obj.insert("paid_tier_id".to_string(), json!("g1-pro-tier"));
    bucket_obj.insert("current_tier_id".to_string(), json!("g1-pro-tier"));
    bucket_obj.insert("account_info_status".to_string(), json!("success"));
    bucket_obj.insert(
        "account_info_checked_at".to_string(),
        json!(observed_at_unix_secs),
    );
    bucket_obj.insert(
        "account_info_updated_at".to_string(),
        json!(observed_at_unix_secs),
    );

    let upstream_metadata = json!({ "antigravity": antigravity_bucket });
    let status_snapshot = sync_provider_key_quota_status_snapshot(
        None,
        "antigravity",
        Some(&upstream_metadata),
        "refresh_api",
    );
    (upstream_metadata, status_snapshot)
}

struct AccountFixtureSpec<'a> {
    key_id: &'a str,
    key_name: &'a str,
    priority: i32,
    access_token: &'a str,
    refresh_token: &'a str,
    project_id: &'a str,
    session_id: &'a str,
    initial_target_fraction: f64,
}

fn build_antigravity_account_key(
    seed_state: &crate::AppState,
    provider_id: &str,
    spec: &AccountFixtureSpec<'_>,
    allowed_models: &[String],
    now_unix_secs: u64,
    reset_rfc3339: &str,
) -> StoredProviderCatalogKey {
    let auth_config_plaintext = json!({
        "provider_type": "antigravity",
        "project_id": spec.project_id,
        "client_version": "1.18.4",
        "session_id": spec.session_id,
        "refresh_token": spec.refresh_token,
    })
    .to_string();
    let encrypted_api_key = seed_state
        .seal_provider_catalog_key_api_key(provider_id, spec.key_id, spec.access_token)
        .expect("account access token should seal");
    let encrypted_auth_config = seed_state
        .seal_provider_catalog_key_auth_config(provider_id, spec.key_id, &auth_config_plaintext)
        .expect("account auth config should seal");

    let mut key = StoredProviderCatalogKey::new(
        spec.key_id.to_string(),
        provider_id.to_string(),
        spec.key_name.to_string(),
        "oauth".to_string(),
        None,
        true,
    )
    .expect("catalog key should build")
    .with_transport_fields(
        Some(json!(["gemini:generate_content"])),
        encrypted_api_key,
        Some(encrypted_auth_config),
        None,
        Some(json!({ "gemini:generate_content": spec.priority })),
        Some(json!(allowed_models)),
        Some(4_102_444_800),
        None,
        None,
    )
    .expect("catalog key transport fields should build");
    key.internal_priority = spec.priority;

    let (upstream_metadata, status_snapshot) = build_seeded_antigravity_quota_state(
        spec.project_id,
        spec.initial_target_fraction,
        0.8,
        now_unix_secs.saturating_sub(60),
        now_unix_secs,
        reset_rfc3339,
    );
    key.upstream_metadata = Some(upstream_metadata);
    key.status_snapshot = status_snapshot;
    key
}

fn build_antigravity_candidate_row(
    provider_id: &str,
    endpoint_id: &str,
    model_id: &str,
    spec: &AccountFixtureSpec<'_>,
    allowed_models: Vec<String>,
    pool_enabled: bool,
) -> StoredMinimalCandidateSelectionRow {
    StoredMinimalCandidateSelectionRow {
        provider_id: provider_id.to_string(),
        provider_name: "antigravity".to_string(),
        provider_type: "antigravity".to_string(),
        provider_priority: 10,
        provider_is_active: true,
        endpoint_id: endpoint_id.to_string(),
        endpoint_api_format: "gemini:generate_content".to_string(),
        endpoint_api_family: Some("gemini".to_string()),
        endpoint_kind: Some("cli".to_string()),
        endpoint_is_active: true,
        key_id: spec.key_id.to_string(),
        key_name: spec.key_name.to_string(),
        key_auth_type: "oauth".to_string(),
        key_is_active: true,
        key_api_formats: Some(vec!["gemini:generate_content".to_string()]),
        key_allowed_models: Some(allowed_models),
        key_capabilities: None,
        key_internal_priority: spec.priority,
        key_global_priority_by_format: Some(json!({ "gemini:generate_content": spec.priority })),
        routing_facts: if pool_enabled {
            StoredMinimalCandidateRoutingFacts {
                provider_pool_enabled: true,
                ..Default::default()
            }
        } else {
            Default::default()
        },
        model_id: model_id.to_string(),
        global_model_id: format!("global-{model_id}"),
        global_model_name: "gemini-cli".to_string(),
        global_model_mappings: None,
        global_model_supports_streaming: Some(true),
        model_provider_model_name: "claude-sonnet-4-5".to_string(),
        model_provider_model_mappings: Some(vec![StoredProviderModelMapping {
            name: "claude-sonnet-4-5".to_string(),
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

#[test]
fn gateway_antigravity_quota_refresh_restores_serving() {
    run_gemini_cli_sync_test(
        "gateway_antigravity_quota_refresh_restores_serving",
        gateway_antigravity_quota_refresh_restores_serving_impl,
    );
}

async fn gateway_antigravity_quota_refresh_restores_serving_impl() {
    run_antigravity_quota_refresh_serving_mode(false).await;
    run_antigravity_quota_refresh_serving_mode(true).await;
}

async fn run_antigravity_quota_refresh_serving_mode(pool_enabled: bool) {
    let mode_label = if pool_enabled { "pool" } else { "ordinary" };
    let provider_id = format!("provider-antigravity-quota-{mode_label}");
    let endpoint_id = format!("endpoint-antigravity-quota-{mode_label}");
    let model_id = format!("model-antigravity-quota-{mode_label}");
    let key_a_id = format!("key-antigravity-quota-a-{mode_label}");
    let key_b_id = format!("key-antigravity-quota-b-{mode_label}");

    let token_a = "ya29.fake-antigravity-quota-token-a";
    let token_b = "ya29.fake-antigravity-quota-token-b";
    let bearer_a = format!("Bearer {token_a}");
    let bearer_b = format!("Bearer {token_b}");
    let project_a = "project-antigravity-quota-a";
    let project_b = "project-antigravity-quota-b";
    let refresh_token_a = "rt-fake-antigravity-quota-a";
    let refresh_token_b = "rt-fake-antigravity-quota-b";

    let now_secs = current_unix_secs();
    let fixed_reset_rfc3339 = format_rfc3339_secs(now_secs + 3600);

    let gen_a = Arc::new(AtomicUsize::new(0));
    let gen_b = Arc::new(AtomicUsize::new(0));
    let probe_a = Arc::new(AtomicUsize::new(0));
    let probe_b = Arc::new(AtomicUsize::new(0));

    let mock_state = Arc::new(Mutex::new(QuotaRuntimeMockState {
        reset_rfc3339: fixed_reset_rfc3339.clone(),
        account_a: AccountQuotaMockPolicy {
            fetch_models_status: 200,
            target_remaining_fraction: 0.5,
            sibling_remaining_fraction: 0.8,
        },
        account_b: AccountQuotaMockPolicy {
            fetch_models_status: 200,
            target_remaining_fraction: 0.8,
            sibling_remaining_fraction: 0.8,
        },
    }));

    let refresh_router = Router::new().route(
        "/oauth/token",
        any(move |request: Request| async move {
            let raw_body = to_bytes(request.into_body(), usize::MAX)
                .await
                .expect("oauth token request body should read");
            let body_text =
                String::from_utf8(raw_body.to_vec()).expect("oauth token body should be utf8");
            let (access_token, refresh_token) = if body_text.contains(refresh_token_a) {
                (token_a, refresh_token_a)
            } else if body_text.contains(refresh_token_b) {
                (token_b, refresh_token_b)
            } else {
                panic!("unexpected oauth refresh body: {body_text}");
            };
            Json(json!({
                "access_token": access_token,
                "refresh_token": refresh_token,
                "token_type": "Bearer",
                "expires_in": 3600
            }))
        }),
    );

    let gen_a_runtime = gen_a.clone();
    let gen_b_runtime = gen_b.clone();
    let probe_a_runtime = probe_a.clone();
    let probe_b_runtime = probe_b.clone();
    let mock_state_runtime = mock_state.clone();
    let bearer_a_expected = bearer_a;
    let bearer_b_expected = bearer_b;

    let execution_runtime = Router::new().route(
        "/v1/execute/sync",
        any(move |request: Request| {
            let gen_a_inner = gen_a_runtime.clone();
            let gen_b_inner = gen_b_runtime.clone();
            let probe_a_inner = probe_a_runtime.clone();
            let probe_b_inner = probe_b_runtime.clone();
            let mock_state_inner = mock_state_runtime.clone();
            let bearer_a_expected = bearer_a_expected.clone();
            let bearer_b_expected = bearer_b_expected.clone();
            async move {
                let raw_body = to_bytes(request.into_body(), usize::MAX)
                    .await
                    .expect("execution plan body should read");
                let plan: aether_contracts::ExecutionPlan =
                    serde_json::from_slice(&raw_body).expect("execution plan should parse");
                let authorization = plan
                    .headers
                    .get("authorization")
                    .cloned()
                    .unwrap_or_default();
                let is_account_a = if authorization == bearer_a_expected {
                    true
                } else if authorization == bearer_b_expected {
                    false
                } else {
                    panic!(
                        "unexpected execution authorization '{}' for format '{}'",
                        authorization, plan.provider_api_format
                    );
                };
                let expected_project = if is_account_a { project_a } else { project_b };

                match plan.provider_api_format.as_str() {
                    "gemini:generate_content" => {
                        assert_eq!(
                            plan.url,
                            "https://daily-cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse"
                        );
                        let body_json = plan
                            .body
                            .json_body
                            .as_ref()
                            .expect("generation request should carry json_body");
                        assert_eq!(
                            body_json.get("model").and_then(serde_json::Value::as_str),
                            Some("claude-sonnet-4-5")
                        );
                        assert_eq!(
                            body_json.get("project").and_then(serde_json::Value::as_str),
                            Some(expected_project)
                        );
                        assert_eq!(
                            body_json
                                .pointer("/request/generationConfig/temperature")
                                .and_then(serde_json::Value::as_f64),
                            Some(0.2)
                        );

                        if is_account_a {
                            gen_a_inner.fetch_add(1, Ordering::SeqCst);
                        } else {
                            gen_b_inner.fetch_add(1, Ordering::SeqCst);
                        }

                        let sse_frame = "data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello Antigravity CLI\"}],\"role\":\"model\"},\"finishReason\":\"STOP\",\"index\":0}],\"modelVersion\":\"claude-sonnet-4-5\",\"usageMetadata\":{\"promptTokenCount\":2,\"candidatesTokenCount\":3,\"totalTokenCount\":5}},\"responseId\":\"resp_antigravity_cli_quota_sync\"}\n\n";
                        let result = aether_contracts::ExecutionResult {
                            request_id: plan.request_id,
                            candidate_id: plan.candidate_id,
                            status_code: 200,
                            headers: BTreeMap::from([(
                                "content-type".to_string(),
                                "text/event-stream".to_string(),
                            )]),
                            response_observation: None,
                            body: Some(aether_contracts::ResponseBody {
                                json_body: None,
                                body_bytes_b64: Some(
                                    base64::engine::general_purpose::STANDARD.encode(sse_frame),
                                ),
                            }),
                            telemetry: None,
                            error: None,
                        };
                        (StatusCode::OK, Json(result))
                    }
                    "antigravity:load_code_assist" => {
                        if is_account_a {
                            probe_a_inner.fetch_add(1, Ordering::SeqCst);
                        } else {
                            probe_b_inner.fetch_add(1, Ordering::SeqCst);
                        }
                        let result = aether_contracts::ExecutionResult {
                            request_id: plan.request_id,
                            candidate_id: plan.candidate_id,
                            status_code: 200,
                            headers: BTreeMap::new(),
                            response_observation: None,
                            body: Some(aether_contracts::ResponseBody {
                                json_body: Some(json!({
                                    "cloudaicompanionProject": { "id": expected_project },
                                    "paidTier": { "id": "g1-pro-tier" },
                                    "currentTier": { "id": "g1-pro-tier" }
                                })),
                                body_bytes_b64: None,
                            }),
                            telemetry: None,
                            error: None,
                        };
                        (StatusCode::OK, Json(result))
                    }
                    "antigravity:fetch_available_models" => {
                        if is_account_a {
                            probe_a_inner.fetch_add(1, Ordering::SeqCst);
                        } else {
                            probe_b_inner.fetch_add(1, Ordering::SeqCst);
                        }
                        let (status_code, json_body) = {
                            let guard = mock_state_inner.lock().expect("mock state should lock");
                            let policy = if is_account_a {
                                &guard.account_a
                            } else {
                                &guard.account_b
                            };
                            if policy.fetch_models_status == 200 {
                                (
                                    200,
                                    json!({
                                        "models": {
                                            "claude-sonnet-4-5": {
                                                "displayName": "Claude Sonnet 4.5",
                                                "quotaInfo": {
                                                    "remainingFraction": policy.target_remaining_fraction,
                                                    "resetTime": guard.reset_rfc3339
                                                }
                                            },
                                            "claude-sonnet-4-6": {
                                                "displayName": "Claude Sonnet 4.6",
                                                "quotaInfo": {
                                                    "remainingFraction": policy.sibling_remaining_fraction,
                                                    "resetTime": guard.reset_rfc3339
                                                }
                                            }
                                        }
                                    }),
                                )
                            } else {
                                (
                                    policy.fetch_models_status,
                                    json!({
                                        "error": {
                                            "code": policy.fetch_models_status,
                                            "message": "upstream fetchAvailableModels failure"
                                        }
                                    }),
                                )
                            }
                        };
                        let result = aether_contracts::ExecutionResult {
                            request_id: plan.request_id,
                            candidate_id: plan.candidate_id,
                            status_code,
                            headers: BTreeMap::new(),
                            response_observation: None,
                            body: Some(aether_contracts::ResponseBody {
                                json_body: Some(json_body),
                                body_bytes_b64: None,
                            }),
                            telemetry: None,
                            error: None,
                        };
                        (StatusCode::OK, Json(result))
                    }
                    "antigravity:retrieve_user_quota_summary" => {
                        if is_account_a {
                            probe_a_inner.fetch_add(1, Ordering::SeqCst);
                        } else {
                            probe_b_inner.fetch_add(1, Ordering::SeqCst);
                        }
                        let reset_rfc3339 = mock_state_inner
                            .lock()
                            .expect("mock state should lock")
                            .reset_rfc3339
                            .clone();
                        let result = aether_contracts::ExecutionResult {
                            request_id: plan.request_id,
                            candidate_id: plan.candidate_id,
                            status_code: 200,
                            headers: BTreeMap::new(),
                            response_observation: None,
                            body: Some(aether_contracts::ResponseBody {
                                json_body: Some(json!({
                                    "groups": [{
                                        "displayName": "Claude and GPT models",
                                        "description": "Shared display group",
                                        "buckets": [{
                                            "bucketId": "3p-5h",
                                            "window": "5h",
                                            "remainingFraction": 0.8,
                                            "resetTime": reset_rfc3339,
                                            "displayName": "5 hour"
                                        }]
                                    }]
                                })),
                                body_bytes_b64: None,
                            }),
                            telemetry: None,
                            error: None,
                        };
                        (StatusCode::OK, Json(result))
                    }
                    unexpected => panic!("unexpected execution format: {unexpected}"),
                }
            }
        }),
    );

    let (refresh_url, refresh_handle) = start_server(refresh_router).await;
    let (execution_runtime_url, execution_runtime_handle) = start_server(execution_runtime).await;

    let seed_state = crate::AppState::new()
        .expect("seed state should build")
        .with_data_state_for_tests(
            crate::data::GatewayDataState::disabled()
                .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY),
        );

    let provider_config = pool_enabled.then(|| {
        json!({
            "pool_advanced": {
                "ignore_exhausted_accounts": true,
                "scheduling_presets": [
                    { "preset": "cache_affinity", "enabled": true },
                    { "preset": "priority_first", "enabled": true }
                ],
                "score_top_n": 64,
                "score_fallback_scan_limit": 64
            }
        })
    });
    let pool_config = pool_enabled.then(|| {
        admin_provider_pool_config_from_config_value(provider_config.as_ref())
            .expect("pool config should parse")
    });
    let provider = StoredProviderCatalogProvider::new(
        provider_id.clone(),
        "antigravity".to_string(),
        Some("https://example.com".to_string()),
        "antigravity".to_string(),
    )
    .expect("provider should build")
    .with_routing_fields(10)
    .with_transport_fields(
        true,
        false,
        false,
        None,
        Some(1),
        None,
        Some(20.0),
        None,
        provider_config,
    );
    let endpoint = StoredProviderCatalogEndpoint::new(
        endpoint_id.clone(),
        provider_id.clone(),
        "gemini:generate_content".to_string(),
        Some("gemini".to_string()),
        Some("cli".to_string()),
        true,
    )
    .expect("endpoint should build")
    .with_health_score(1.0)
    .with_transport_fields(
        "https://daily-cloudcode-pa.googleapis.com".to_string(),
        None,
        None,
        Some(1),
        None,
        None,
        None,
        None,
    )
    .expect("endpoint transport should build");

    let shared_models = vec![
        "claude-sonnet-4-5".to_string(),
        "claude-sonnet-4-6".to_string(),
    ];
    let account_a_spec = AccountFixtureSpec {
        key_id: &key_a_id,
        key_name: "account-a",
        priority: 1,
        access_token: token_a,
        refresh_token: refresh_token_a,
        project_id: project_a,
        session_id: "sess-antigravity-quota-a",
        initial_target_fraction: 0.5,
    };
    let account_b_spec = AccountFixtureSpec {
        key_id: &key_b_id,
        key_name: "account-b",
        priority: 2,
        access_token: token_b,
        refresh_token: refresh_token_b,
        project_id: project_b,
        session_id: "sess-antigravity-quota-b",
        initial_target_fraction: 0.8,
    };
    let key_a = build_antigravity_account_key(
        &seed_state,
        &provider_id,
        &account_a_spec,
        &shared_models,
        now_secs,
        &fixed_reset_rfc3339,
    );
    let key_b = build_antigravity_account_key(
        &seed_state,
        &provider_id,
        &account_b_spec,
        &shared_models,
        now_secs,
        &fixed_reset_rfc3339,
    );

    let candidate_rows = vec![
        build_antigravity_candidate_row(
            &provider_id,
            &endpoint_id,
            &model_id,
            &account_a_spec,
            shared_models.clone(),
            pool_enabled,
        ),
        build_antigravity_candidate_row(
            &provider_id,
            &endpoint_id,
            &model_id,
            &account_b_spec,
            shared_models,
            pool_enabled,
        ),
    ];

    let client_api_key = format!("client-antigravity-quota-{mode_label}");
    let auth_repository = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![(
        Some(hash_client_api_key(&client_api_key)),
        build_client_auth_snapshot(
            &format!("api-key-antigravity-quota-{mode_label}"),
            &format!("user-antigravity-quota-{mode_label}"),
        ),
    )]));
    let candidate_selection_repository = Arc::new(
        InMemoryMinimalCandidateSelectionReadRepository::seed(candidate_rows),
    );
    let request_candidate_repository = Arc::new(InMemoryRequestCandidateRepository::default());
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![provider],
        vec![endpoint],
        vec![key_a, key_b],
    ));
    let pool_score_repository = Arc::new(InMemoryPoolMemberScoreRepository::default());

    let oauth_refresh =
        crate::provider_transport::LocalOAuthRefreshCoordinator::with_adapters_for_tests(vec![
            Arc::new(
                crate::provider_transport::oauth_refresh::GenericOAuthRefreshAdapter::default()
                    .with_token_url_for_tests("antigravity", format!("{refresh_url}/oauth/token"))
                    .with_oauth_credentials_for_tests(
                        "antigravity",
                        "test-antigravity-client-id",
                        "test-antigravity-client-secret",
                    ),
            ),
        ]);

    let gateway_state = build_state_with_execution_runtime_override(execution_runtime_url)
        .with_data_state_for_tests(
            crate::data::GatewayDataState::with_auth_candidate_selection_provider_catalog_and_request_candidate_repository_for_tests(
                auth_repository,
                candidate_selection_repository,
                provider_catalog_repository.clone(),
                request_candidate_repository,
                DEVELOPMENT_ENCRYPTION_KEY,
            )
            .with_pool_score_repository_for_tests(pool_score_repository)
            .with_system_default_routing_group_for_tests(),
        )
        .with_oauth_refresh_coordinator_for_tests(oauth_refresh);

    let (gateway_url, gateway_handle) =
        start_server(build_router_with_state(gateway_state.clone())).await;

    let client = reqwest::Client::new();
    let client_session_id = format!("client-session-antigravity-quota-{mode_label}");
    let generate_body_json = json!({
        "contents": [],
        "generationConfig": { "temperature": 0.2 },
        "session_id": client_session_id,
    });
    let sticky_session_token = extract_pool_sticky_session_token(&generate_body_json);
    assert_eq!(
        sticky_session_token.as_deref(),
        Some(client_session_id.as_str()),
        "{mode_label}: request body should carry extractable Pool sticky session token"
    );
    let generate_body = generate_body_json.to_string();

    let read_pool_sticky_binding = || {
        let gateway_state = gateway_state.clone();
        let provider_id = provider_id.clone();
        let pool_config = pool_config.clone();
        let sticky_session_token = sticky_session_token.clone();
        async move {
            let pool_config = pool_config.as_ref()?;
            read_provider_pool_sticky_bound_key_id(
                gateway_state.runtime_state.as_ref(),
                &provider_id,
                pool_config,
                sticky_session_token.as_deref(),
            )
            .await
        }
    };
    assert_eq!(
        read_pool_sticky_binding().await,
        None,
        "{mode_label}: Pool sticky session should start unbound before warmup"
    );

    let send_generate = |step: &'static str| {
        let client = client.clone();
        let url = format!("{gateway_url}/v1beta/models/gemini-cli:generateContent");
        let api_key = client_api_key.clone();
        let session_id = client_session_id.clone();
        let body = generate_body.clone();
        let trace_id = format!("trace-antigravity-quota-{mode_label}-{step}");
        async move {
            client
                .post(url)
                .header(http::header::CONTENT_TYPE, "application/json")
                .header("user-agent", "GeminiCLI/1.0")
                .header("x-goog-api-key", api_key)
                .header("session-id", session_id)
                .header(TRACE_ID_HEADER, trace_id)
                .body(body)
                .send()
                .await
                .expect("gemini generateContent HTTP request should complete")
        }
    };

    let assert_valid_gemini_response = |response: reqwest::Response, step: &'static str| async move {
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{mode_label}/{step}: expected HTTP 200"
        );
        let body: serde_json::Value = response
            .json()
            .await
            .expect("gemini response JSON should parse");
        assert_eq!(
            body,
            json!({
                "responseId": "resp_antigravity_cli_quota_sync",
                "_v1internal_response_id": "resp_antigravity_cli_quota_sync",
                "candidates": [{
                    "content": {
                        "parts": [{ "text": "Hello Antigravity CLI" }],
                        "role": "model"
                    },
                    "finishReason": "STOP",
                    "index": 0
                }],
                "modelVersion": "claude-sonnet-4-5",
                "usageMetadata": {
                    "promptTokenCount": 2,
                    "candidatesTokenCount": 3,
                    "totalTokenCount": 5
                }
            }),
            "{mode_label}/{step}: final Gemini response mismatch"
        );
    };

    let send_refresh_quota = |key_ids_body: Option<serde_json::Value>| {
        let client = client.clone();
        let url =
            format!("{gateway_url}/api/admin/endpoints/providers/{provider_id}/refresh-quota");
        async move {
            let mut request = client
                .post(url)
                .header(GATEWAY_HEADER, "rust-phase3b")
                .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-quota-test")
                .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
                .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "admin-session-quota-test");
            if let Some(body) = key_ids_body {
                request = request.json(&body);
            }
            let response = request
                .send()
                .await
                .expect("admin refresh-quota HTTP request should complete");
            assert_eq!(response.status(), StatusCode::OK);
            response
                .json::<serde_json::Value>()
                .await
                .expect("refresh-quota payload should parse")
        }
    };

    let snapshot_counts = || {
        (
            gen_a.load(Ordering::SeqCst),
            gen_b.load(Ordering::SeqCst),
            probe_a.load(Ordering::SeqCst),
            probe_b.load(Ordering::SeqCst),
        )
    };

    // Step 1: Warmup request selects preferred account A (initial A=0.5, B=0.8) and warms caches.
    assert_valid_gemini_response(send_generate("step1-warmup").await, "step1-warmup").await;
    let c1 = snapshot_counts();
    assert_eq!(
        (c1.0, c1.1),
        (1, 0),
        "{mode_label}/step1: only A should serve"
    );
    if pool_enabled {
        assert_eq!(
            read_pool_sticky_binding().await.as_deref(),
            Some(key_a_id.as_str()),
            "{mode_label}/step1: warmup should bind Pool sticky session to A"
        );
    }
    println!(
        "[antigravity-quota-http][{mode_label}] step1_warmup gen_a={} gen_b={} probe_a={} probe_b={}",
        c1.0, c1.1, c1.2, c1.3
    );

    // Step 2: Refresh A target=0 (sibling=0.8), B target=0.8, reset=now+3600 -> only B serves.
    {
        let mut guard = mock_state.lock().expect("mock state should lock");
        guard.reset_rfc3339 = format_rfc3339_secs(current_unix_secs() + 3600);
        guard.account_a = AccountQuotaMockPolicy {
            fetch_models_status: 200,
            target_remaining_fraction: 0.0,
            sibling_remaining_fraction: 0.8,
        };
        guard.account_b = AccountQuotaMockPolicy {
            fetch_models_status: 200,
            target_remaining_fraction: 0.8,
            sibling_remaining_fraction: 0.8,
        };
    }
    let refresh_step2 = send_refresh_quota(None).await;
    assert_eq!(refresh_step2["success"], json!(2));
    assert_eq!(refresh_step2["failed"], json!(0));
    assert_eq!(refresh_step2["total"], json!(2));
    assert!(
        probe_a.load(Ordering::SeqCst) > c1.2 && probe_b.load(Ordering::SeqCst) > c1.3,
        "{mode_label}/step2: quota refresh must increment probe counters"
    );
    assert_eq!(
        (gen_a.load(Ordering::SeqCst), gen_b.load(Ordering::SeqCst)),
        (1, 0),
        "{mode_label}/step2: quota refresh must not increment generation counters"
    );

    assert_valid_gemini_response(send_generate("step2-failover-b").await, "step2-failover-b").await;
    let c2 = snapshot_counts();
    assert_eq!(
        (c2.0, c2.1),
        (1, 1),
        "{mode_label}/step2: exhausted A must be skipped despite sibling=0.8 and only B serves"
    );
    if pool_enabled {
        assert_eq!(
            read_pool_sticky_binding().await.as_deref(),
            Some(key_b_id.as_str()),
            "{mode_label}/step2: failover should rebind Pool sticky session to B"
        );
    }
    println!(
        "[antigravity-quota-http][{mode_label}] step2_a_exhausted gen_a={} gen_b={} probe_a={} probe_b={}",
        c2.0, c2.1, c2.2, c2.3
    );

    // Step 3: Refresh A target=0.5, B target=0.0 -> A immediately recovers without TTL wait or restart.
    {
        let mut guard = mock_state.lock().expect("mock state should lock");
        guard.reset_rfc3339 = format_rfc3339_secs(current_unix_secs() + 3600);
        guard.account_a = AccountQuotaMockPolicy {
            fetch_models_status: 200,
            target_remaining_fraction: 0.5,
            sibling_remaining_fraction: 0.8,
        };
        guard.account_b = AccountQuotaMockPolicy {
            fetch_models_status: 200,
            target_remaining_fraction: 0.0,
            sibling_remaining_fraction: 0.8,
        };
    }
    let refresh_step3 = send_refresh_quota(None).await;
    assert_eq!(refresh_step3["success"], json!(2));
    assert_eq!(refresh_step3["failed"], json!(0));

    assert_valid_gemini_response(send_generate("step3-recover-a").await, "step3-recover-a").await;
    let c3 = snapshot_counts();
    assert_eq!(
        (c3.0, c3.1),
        (2, 1),
        "{mode_label}/step3: recovered A must serve (+1) and exhausted B must not increase"
    );
    if pool_enabled {
        assert_eq!(
            read_pool_sticky_binding().await.as_deref(),
            Some(key_a_id.as_str()),
            "{mode_label}/step3: recovered sole-eligible A should rebind Pool sticky session to A"
        );
    }
    println!(
        "[antigravity-quota-http][{mode_label}] step3_a_recovered gen_a={} gen_b={} probe_a={} probe_b={}",
        c3.0, c3.1, c3.2, c3.3
    );

    // Step 4a: fetchAvailableModels=502 with loadCodeAssist=200 must not restore old exhausted accounts.
    // Exhaust A=0.0 while already-exhausted B gets loadCodeAssist=200 + fetchAvailableModels=502,
    // then also verify total 502 across both accounts.
    {
        let mut guard = mock_state.lock().expect("mock state should lock");
        guard.reset_rfc3339 = format_rfc3339_secs(current_unix_secs() + 3600);
        guard.account_a = AccountQuotaMockPolicy {
            fetch_models_status: 200,
            target_remaining_fraction: 0.0,
            sibling_remaining_fraction: 0.8,
        };
        guard.account_b = AccountQuotaMockPolicy {
            fetch_models_status: 502,
            target_remaining_fraction: 0.8,
            sibling_remaining_fraction: 0.8,
        };
    }
    let refresh_partial_502 = send_refresh_quota(None).await;
    assert_eq!(refresh_partial_502["success"], json!(1));
    assert_eq!(refresh_partial_502["failed"], json!(1));
    let b_result = refresh_partial_502["results"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["key_id"] == key_b_id))
        .expect("key B result should exist");
    assert_eq!(b_result["status"], json!("error"));
    assert_eq!(
        b_result["message"],
        json!("fetchAvailableModels 返回状态码 502")
    );

    let failed_probe_resp_1 = send_generate("step4a-partial-502").await;
    assert_eq!(
        failed_probe_resp_1.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        (gen_a.load(Ordering::SeqCst), gen_b.load(Ordering::SeqCst)),
        (2, 1),
        "{mode_label}/step4a: old exhausted B must not recover when fetchAvailableModels returns 502"
    );

    {
        let mut guard = mock_state.lock().expect("mock state should lock");
        guard.account_a.fetch_models_status = 502;
        guard.account_b.fetch_models_status = 502;
    }
    let refresh_all_502 = send_refresh_quota(None).await;
    assert_eq!(refresh_all_502["success"], json!(0));
    assert_eq!(refresh_all_502["failed"], json!(2));
    assert!(refresh_all_502["results"]
        .as_array()
        .expect("results array should exist")
        .iter()
        .all(|item| item["status"] == json!("error")));

    let failed_probe_resp_2 = send_generate("step4a-all-502").await;
    assert_eq!(
        failed_probe_resp_2.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let c4a = snapshot_counts();
    assert_eq!(
        (c4a.0, c4a.1),
        (2, 1),
        "{mode_label}/step4a: loadCodeAssist 200 + fetchAvailableModels 502 must keep accounts blocked"
    );
    println!(
        "[antigravity-quota-http][{mode_label}] step4a_fetch_502 gen_a={} gen_b={} probe_a={} probe_b={}",
        c4a.0, c4a.1, c4a.2, c4a.3
    );

    // Step 4b (1): Independent runtime manual scheduling block on A persists across positive quota refresh.
    let stored_a = provider_catalog_repository
        .list_keys_by_ids(std::slice::from_ref(&key_a_id))
        .await
        .expect("catalog key A should read")
        .into_iter()
        .next()
        .expect("catalog key A should exist");
    let cas_applied = gateway_state
        .compare_and_update_provider_catalog_key_scheduling_state(
            &ProviderCatalogKeySchedulingStateCasUpdate {
                key_id: key_a_id.clone(),
                expected_encrypted_api_key: stored_a.encrypted_api_key,
                expected_encrypted_auth_config: stored_a.encrypted_auth_config,
                expected_auth_type: stored_a.auth_type,
                expected_scheduling: None,
                scheduling: Some(json!({
                    "code": "quota_exhausted",
                    "blocked": true,
                    "requires_manual_recovery": true,
                    "source": "runtime_quota_exhaustion",
                    "confidence": "confirmed",
                    "confirmation_count": 1
                })),
                updated_at_unix_secs: Some(current_unix_secs()),
            },
        )
        .await
        .expect("manual scheduling block CAS should succeed");
    assert!(
        cas_applied,
        "{mode_label}/step4b: scheduling CAS should apply"
    );

    {
        let mut guard = mock_state.lock().expect("mock state should lock");
        guard.reset_rfc3339 = format_rfc3339_secs(current_unix_secs() + 3600);
        guard.account_a = AccountQuotaMockPolicy {
            fetch_models_status: 200,
            target_remaining_fraction: 0.8,
            sibling_remaining_fraction: 0.8,
        };
        guard.account_b = AccountQuotaMockPolicy {
            fetch_models_status: 200,
            target_remaining_fraction: 0.8,
            sibling_remaining_fraction: 0.8,
        };
    }
    let refresh_manual_ban = send_refresh_quota(None).await;
    assert_eq!(refresh_manual_ban["success"], json!(2));
    assert_eq!(refresh_manual_ban["failed"], json!(0));

    assert_valid_gemini_response(
        send_generate("step4b-manual-ban").await,
        "step4b-manual-ban",
    )
    .await;
    let c4b_ban = snapshot_counts();
    assert_eq!(
        (c4b_ban.0, c4b_ban.1),
        (2, 2),
        "{mode_label}/step4b: manually banned A must not serve even after positive quota refresh"
    );
    if pool_enabled {
        assert_eq!(
            read_pool_sticky_binding().await.as_deref(),
            Some(key_b_id.as_str()),
            "{mode_label}/step4b: manual ban on sticky A should fail over and rebind Pool sticky session to B"
        );
    }
    println!(
        "[antigravity-quota-http][{mode_label}] step4b_manual_ban gen_a={} gen_b={} probe_a={} probe_b={}",
        c4b_ban.0, c4b_ban.1, c4b_ban.2, c4b_ban.3
    );

    let clear_ban_resp = client
        .post(format!(
            "{gateway_url}/api/admin/endpoints/keys/{key_a_id}/clear-quota-exhausted"
        ))
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-quota-test")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "admin-session-quota-test")
        .send()
        .await
        .expect("clear-quota-exhausted request should complete");
    assert_eq!(clear_ban_resp.status(), StatusCode::OK);
    let clear_ban_json: serde_json::Value = clear_ban_resp
        .json()
        .await
        .expect("clear-quota-exhausted payload should parse");
    assert_eq!(clear_ban_json["cleared"], json!(true));

    // Step 4b (2): Manually disabled A (is_active=false) still cannot serve after positive quota refresh.
    let disable_a_resp = client
        .put(format!("{gateway_url}/api/admin/endpoints/keys/{key_a_id}"))
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-quota-test")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "admin-session-quota-test")
        .json(&json!({ "is_active": false }))
        .send()
        .await
        .expect("disable key A request should complete");
    assert_eq!(disable_a_resp.status(), StatusCode::OK);

    {
        let mut guard = mock_state.lock().expect("mock state should lock");
        guard.reset_rfc3339 = format_rfc3339_secs(current_unix_secs() + 3600);
        guard.account_a = AccountQuotaMockPolicy {
            fetch_models_status: 200,
            target_remaining_fraction: 0.8,
            sibling_remaining_fraction: 0.8,
        };
        guard.account_b = AccountQuotaMockPolicy {
            fetch_models_status: 200,
            target_remaining_fraction: 0.0,
            sibling_remaining_fraction: 0.8,
        };
    }
    let refresh_disabled =
        send_refresh_quota(Some(json!({ "key_ids": [key_a_id, key_b_id] }))).await;
    assert_eq!(refresh_disabled["success"], json!(2));
    assert_eq!(refresh_disabled["failed"], json!(0));

    let disabled_resp = send_generate("step4b-disabled-a").await;
    assert_eq!(disabled_resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let c4b_disabled = snapshot_counts();
    assert_eq!(
        (c4b_disabled.0, c4b_disabled.1),
        (2, 2),
        "{mode_label}/step4b: manually disabled A must not serve even as the only positive-quota key"
    );
    println!(
        "[antigravity-quota-http][{mode_label}] step4b_disabled gen_a={} gen_b={} probe_a={} probe_b={}",
        c4b_disabled.0, c4b_disabled.1, c4b_disabled.2, c4b_disabled.3
    );

    let enable_a_resp = client
        .put(format!("{gateway_url}/api/admin/endpoints/keys/{key_a_id}"))
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-quota-test")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "admin-session-quota-test")
        .json(&json!({ "is_active": true }))
        .send()
        .await
        .expect("re-enable key A request should complete");
    assert_eq!(enable_a_resp.status(), StatusCode::OK);
    assert_valid_gemini_response(
        send_generate("step4b-reenabled-a").await,
        "step4b-reenabled-a",
    )
    .await;
    let c4b_reenabled = snapshot_counts();
    assert_eq!(
        (c4b_reenabled.0, c4b_reenabled.1),
        (3, 2),
        "{mode_label}/step4b: re-enabled A with positive quota must serve again before all-exhausted check"
    );
    if pool_enabled {
        assert_eq!(
            read_pool_sticky_binding().await.as_deref(),
            Some(key_a_id.as_str()),
            "{mode_label}/step4b: re-enabled sole-eligible A should rebind Pool sticky session to A"
        );
    }
    // Step 4c: All active, unbanned accounts exhausted -> HTTP 503 with zero generation requests.
    {
        let mut guard = mock_state.lock().expect("mock state should lock");
        guard.reset_rfc3339 = format_rfc3339_secs(current_unix_secs() + 3600);
        guard.account_a = AccountQuotaMockPolicy {
            fetch_models_status: 200,
            target_remaining_fraction: 0.0,
            sibling_remaining_fraction: 0.8,
        };
        guard.account_b = AccountQuotaMockPolicy {
            fetch_models_status: 200,
            target_remaining_fraction: 0.0,
            sibling_remaining_fraction: 0.8,
        };
    }
    let refresh_all_exhausted = send_refresh_quota(None).await;
    assert_eq!(refresh_all_exhausted["success"], json!(2));
    assert_eq!(refresh_all_exhausted["failed"], json!(0));

    let all_exhausted_resp = send_generate("step4c-all-exhausted").await;
    assert_eq!(all_exhausted_resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let c4c = snapshot_counts();
    assert_eq!(
        (c4c.0, c4c.1),
        (3, 2),
        "{mode_label}/step4c: all exhausted accounts must return 503 with no generation requests"
    );
    println!(
        "[antigravity-quota-http][{mode_label}] step4c_all_exhausted_503 gen_a={} gen_b={} probe_a={} probe_b={}",
        c4c.0, c4c.1, c4c.2, c4c.3
    );

    gateway_handle.abort();
    execution_runtime_handle.abort();
    refresh_handle.abort();
}
