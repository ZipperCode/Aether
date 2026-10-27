use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use aether_admin::provider::quota::parse_antigravity_usage_response;
use aether_data::repository::pool_scores::InMemoryPoolMemberScoreRepository;
use aether_data_contracts::repository::candidate_selection::StoredMinimalCandidateRoutingFacts;
use aether_data_contracts::repository::pool_scores::{
    ListPoolMemberScoresQuery, PoolMemberHardState, PoolMemberIdentity,
    PoolMemberScoreWriteRepository, PoolScoreReadRepository, StoredPoolMemberScore,
};
use aether_data_contracts::repository::provider_catalog::{
    ProviderCatalogKeySchedulingStateCasUpdate, ProviderCatalogReadRepository,
};
use axum::response::Response;
use base64::Engine as _;

use super::*;
use crate::ai_serving::extract_pool_sticky_session_token;
use crate::constants::{
    GATEWAY_HEADER, TRUSTED_ADMIN_SESSION_ID_HEADER, TRUSTED_ADMIN_USER_ID_HEADER,
    TRUSTED_ADMIN_USER_ROLE_HEADER,
};
use crate::handlers::shared::provider_pool::{
    admin_provider_pool_config_from_config_value, read_admin_provider_pool_key_cooldown_reason,
    read_provider_pool_sticky_bound_key_id, AdminProviderPoolConfig,
};
use crate::handlers::shared::sync_provider_key_quota_status_snapshot;

/// 一个上游模型在账号目录/额度刷新响应中的最小观测形态。
///
/// `remaining_fraction` 为 `None` 时只表达“账号目录中存在该模型”，不会伪造额度数字；
/// 事故账号正是以这种占位条目暴露了不包含目标模型的权威目录。
#[derive(Debug, Clone, PartialEq)]
struct ModelQuotaFixture {
    display_name: String,
    remaining_fraction: Option<f64>,
}

impl ModelQuotaFixture {
    fn with_fraction(display_name: &str, remaining_fraction: f64) -> Self {
        Self {
            display_name: display_name.to_string(),
            remaining_fraction: Some(remaining_fraction),
        }
    }

    fn display_only(display_name: &str) -> Self {
        Self {
            display_name: display_name.to_string(),
            remaining_fraction: None,
        }
    }
}

#[derive(Debug, Clone)]
struct AccountMockPolicy {
    fetch_models_status: u16,
    models: BTreeMap<String, ModelQuotaFixture>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AntigravityUpstreamErrorKind {
    ModelQuota,
    AccountRateLimit,
}

#[derive(Debug, Clone, Copy)]
struct AntigravityUpstreamErrorFixture {
    retry_after_seconds: u64,
    kind: AntigravityUpstreamErrorKind,
}

#[derive(Debug, Clone)]
struct QuotaRuntimeMockState {
    reset_rfc3339: String,
    /// 所有已映射的上游模型名；执行计划只允许携带这里的模型。
    generation_models: BTreeSet<String>,
    generation_response_id: String,
    generation_text: String,
    account_a_subscription_tier: String,
    account_b_subscription_tier: String,
    account_a: AccountMockPolicy,
    account_b: AccountMockPolicy,
    /// 每个账号+上游模型触发的真实上游 429；`None` 表示正常成功。
    generation_errors: BTreeMap<(bool, String), AntigravityUpstreamErrorFixture>,
    /// 每个账号+上游模型收到的生成计划次数。
    generation_counts: BTreeMap<(bool, String), usize>,
}

fn antigravity_quota_error_body(model: &str, retry_after_seconds: u64) -> serde_json::Value {
    json!({
        "error": {
            "code": 429,
            "message": format!("Quota exhausted for model {model}."),
            "status": "RESOURCE_EXHAUSTED",
            "model": model,
            "model_id": model,
            "details": [
                {
                    "@type": "type.googleapis.com/google.rpc.ErrorInfo",
                    "reason": "QUOTA_EXHAUSTED",
                    "domain": "cloudcode-pa.googleapis.com",
                    "metadata": { "model": model }
                },
                {
                    "@type": "type.googleapis.com/google.rpc.RetryInfo",
                    "retryDelay": format!("{retry_after_seconds}s")
                }
            ]
        }
    })
}

fn antigravity_rate_limit_error_body(model: &str, retry_after_seconds: u64) -> serde_json::Value {
    json!({
        "error": {
            "code": 429,
            "message": "Rate limit exceeded: too many requests per minute.",
            "status": "RATE_LIMIT_EXCEEDED",
            "model": model,
            "details": [
                {
                    "@type": "type.googleapis.com/google.rpc.ErrorInfo",
                    "reason": "RATE_LIMIT_EXCEEDED",
                    "domain": "cloudcode-pa.googleapis.com",
                    "metadata": { "model": model }
                },
                {
                    "@type": "type.googleapis.com/google.rpc.RetryInfo",
                    "retryDelay": format!("{retry_after_seconds}s")
                }
            ]
        }
    })
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

fn claude_quota_models(
    target_remaining_fraction: f64,
    sibling_remaining_fraction: f64,
) -> BTreeMap<String, ModelQuotaFixture> {
    BTreeMap::from([
        (
            "claude-sonnet-4-5".to_string(),
            ModelQuotaFixture::with_fraction("Claude Sonnet 4.5", target_remaining_fraction),
        ),
        (
            "claude-sonnet-4-6".to_string(),
            ModelQuotaFixture::with_fraction("Claude Sonnet 4.6", sibling_remaining_fraction),
        ),
    ])
}

fn claude_quota_policy(
    fetch_models_status: u16,
    target_remaining_fraction: f64,
    sibling_remaining_fraction: f64,
) -> AccountMockPolicy {
    AccountMockPolicy {
        fetch_models_status,
        models: claude_quota_models(target_remaining_fraction, sibling_remaining_fraction),
    }
}

/// 上游 `fetchAvailableModels` 响应体：只回放账号自己的目录条目。
fn mock_antigravity_models_body(
    policy: &AccountMockPolicy,
    reset_rfc3339: &str,
) -> serde_json::Value {
    let models = policy
        .models
        .iter()
        .map(|(model_id, fixture)| {
            let mut entry = serde_json::Map::new();
            entry.insert("displayName".to_string(), json!(fixture.display_name));
            if let Some(remaining_fraction) = fixture.remaining_fraction {
                entry.insert(
                    "quotaInfo".to_string(),
                    json!({
                        "remainingFraction": remaining_fraction,
                        "resetTime": reset_rfc3339,
                    }),
                );
            }
            (model_id.clone(), serde_json::Value::Object(entry))
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    json!({ "models": models })
}

#[allow(clippy::too_many_arguments)]
fn build_seeded_antigravity_quota_state(
    project_id: &str,
    subscription_tier: &str,
    paid_tier_id: &str,
    models: &BTreeMap<String, ModelQuotaFixture>,
    updated_at_unix_secs: u64,
    observed_at_unix_secs: u64,
    reset_rfc3339: &str,
) -> (serde_json::Value, Option<serde_json::Value>) {
    let usage_body = mock_antigravity_models_body(
        &AccountMockPolicy {
            fetch_models_status: 200,
            models: models.clone(),
        },
        reset_rfc3339,
    );
    let mut antigravity_bucket =
        parse_antigravity_usage_response(&usage_body, updated_at_unix_secs)
            .expect("antigravity usage fixture should parse");
    let bucket_obj = antigravity_bucket
        .as_object_mut()
        .expect("antigravity metadata bucket should be an object");
    bucket_obj.insert("project_id".to_string(), json!(project_id));
    bucket_obj.insert("subscription_tier".to_string(), json!(subscription_tier));
    bucket_obj.insert("paid_tier_id".to_string(), json!(paid_tier_id));
    bucket_obj.insert("current_tier_id".to_string(), json!(paid_tier_id));
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

#[derive(Debug, Clone)]
struct AccountFixtureSpec {
    key_id: String,
    key_name: String,
    priority: i32,
    access_token: String,
    refresh_token: String,
    project_id: String,
    session_id: String,
    subscription_tier: String,
    allowed_models: Option<Vec<String>>,
    auto_fetch_models: bool,
    models: BTreeMap<String, ModelQuotaFixture>,
}

fn build_antigravity_account_key(
    seed_state: &crate::AppState,
    provider_id: &str,
    spec: &AccountFixtureSpec,
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
        .seal_provider_catalog_key_api_key(provider_id, &spec.key_id, &spec.access_token)
        .expect("account access token should seal");
    let encrypted_auth_config = seed_state
        .seal_provider_catalog_key_auth_config(provider_id, &spec.key_id, &auth_config_plaintext)
        .expect("account auth config should seal");

    let mut key = StoredProviderCatalogKey::new(
        spec.key_id.clone(),
        provider_id.to_string(),
        spec.key_name.clone(),
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
        spec.allowed_models.as_ref().map(|models| json!(models)),
        Some(4_102_444_800),
        None,
        None,
    )
    .expect("catalog key transport fields should build");
    key.internal_priority = spec.priority;
    key.auto_fetch_models = spec.auto_fetch_models;

    let paid_tier_id = format!("g1-{}-tier", spec.subscription_tier);
    let (upstream_metadata, status_snapshot) = build_seeded_antigravity_quota_state(
        &spec.project_id,
        &spec.subscription_tier,
        &paid_tier_id,
        &spec.models,
        now_unix_secs.saturating_sub(60),
        now_unix_secs,
        reset_rfc3339,
    );
    key.upstream_metadata = Some(upstream_metadata);
    key.status_snapshot = status_snapshot;
    key
}

#[allow(clippy::too_many_arguments)]
fn build_antigravity_candidate_row(
    provider_id: &str,
    endpoint_id: &str,
    model_id: &str,
    spec: &AccountFixtureSpec,
    global_model_name: &str,
    provider_model_name: &str,
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
        key_id: spec.key_id.clone(),
        key_name: spec.key_name.clone(),
        key_auth_type: "oauth".to_string(),
        key_is_active: true,
        key_api_formats: Some(vec!["gemini:generate_content".to_string()]),
        key_allowed_models: spec.allowed_models.clone(),
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
        global_model_name: global_model_name.to_string(),
        global_model_mappings: None,
        global_model_supports_streaming: Some(true),
        model_provider_model_name: provider_model_name.to_string(),
        model_provider_model_mappings: Some(vec![StoredProviderModelMapping {
            name: provider_model_name.to_string(),
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

/// 真实 localhost HTTP 网关 + 执行运行时夹具的配置。
struct AntigravityHttpConfig {
    mode_label: String,
    pool_enabled: bool,
    provider_id: String,
    endpoint_id: String,
    model_id: String,
    global_model_name: String,
    provider_model_name: String,
    secondary_model_id: Option<String>,
    secondary_global_model_name: Option<String>,
    secondary_provider_model_name: Option<String>,
    generation_response_id: String,
    generation_text: String,
    reset_rfc3339: String,
    key_a: AccountFixtureSpec,
    key_b: AccountFixtureSpec,
}

#[derive(Debug, Clone, Copy, Default)]
struct AntigravityHttpCounts {
    gen_a: usize,
    gen_b: usize,
    unsupported_a: usize,
    unsupported_b: usize,
    probe_a: usize,
    probe_b: usize,
}

/// 一次请求后账号级惩罚面：冷却、Pool 硬状态、健康、自适应与调度阻断。
#[derive(Debug, Clone, Default)]
struct AntigravityAccountPenaltySnapshot {
    cooldown_reason: Option<String>,
    pool_hard_states: Vec<PoolMemberHardState>,
    health_consecutive_failures: u64,
    adaptive_rpm_429_count: u32,
    adaptive_last_429_at_unix_secs: Option<u64>,
    adaptive_last_429_type: Option<String>,
    scheduling_code: Option<String>,
    oauth_invalid_at_unix_secs: Option<u64>,
}

struct AntigravityHttpHarness {
    provider_id: String,
    key_a_id: String,
    key_b_id: String,
    global_model_name: String,
    gateway_url: String,
    client: reqwest::Client,
    client_api_key: String,
    client_session_id: String,
    generate_body: String,
    stream_generate_body: String,
    sticky_session_token: Option<String>,
    mock_state: Arc<Mutex<QuotaRuntimeMockState>>,
    gen_a: Arc<AtomicUsize>,
    gen_b: Arc<AtomicUsize>,
    unsupported_a: Arc<AtomicUsize>,
    unsupported_b: Arc<AtomicUsize>,
    probe_a: Arc<AtomicUsize>,
    probe_b: Arc<AtomicUsize>,
    gateway_state: crate::AppState,
    provider_catalog_repository: Arc<InMemoryProviderCatalogReadRepository>,
    pool_score_repository: Arc<InMemoryPoolMemberScoreRepository>,
    pool_config: Option<AdminProviderPoolConfig>,
    servers: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for AntigravityHttpHarness {
    fn drop(&mut self) {
        for server in &self.servers {
            server.abort();
        }
    }
}

impl AntigravityHttpHarness {
    fn counts(&self) -> AntigravityHttpCounts {
        AntigravityHttpCounts {
            gen_a: self.gen_a.load(Ordering::SeqCst),
            gen_b: self.gen_b.load(Ordering::SeqCst),
            unsupported_a: self.unsupported_a.load(Ordering::SeqCst),
            unsupported_b: self.unsupported_b.load(Ordering::SeqCst),
            probe_a: self.probe_a.load(Ordering::SeqCst),
            probe_b: self.probe_b.load(Ordering::SeqCst),
        }
    }

    async fn send_generate(&self, step: &str) -> reqwest::Response {
        self.send_generate_model(&self.global_model_name, step)
            .await
    }

    async fn send_generate_model(&self, global_model_name: &str, step: &str) -> reqwest::Response {
        self.client
            .post(format!(
                "{}/v1beta/models/{}:generateContent",
                self.gateway_url, global_model_name
            ))
            .header(http::header::CONTENT_TYPE, "application/json")
            .header("user-agent", "GeminiCLI/1.0")
            .header("x-goog-api-key", self.client_api_key.as_str())
            .header("session-id", self.client_session_id.as_str())
            .header(
                TRACE_ID_HEADER,
                format!("trace-{}-{}", self.provider_id, step),
            )
            .body(self.generate_body.clone())
            .send()
            .await
            .expect("gemini generateContent HTTP request should complete")
    }

    async fn send_stream_generate(&self, step: &str) -> reqwest::Response {
        self.send_stream_generate_model(&self.global_model_name, step)
            .await
    }

    async fn send_stream_generate_model(
        &self,
        global_model_name: &str,
        step: &str,
    ) -> reqwest::Response {
        self.client
            .post(format!(
                "{}/v1beta/models/{}:streamGenerateContent?alt=sse",
                self.gateway_url, global_model_name
            ))
            .header(http::header::CONTENT_TYPE, "application/json")
            .header("user-agent", "GeminiCLI/1.0")
            .header("x-goog-api-key", self.client_api_key.as_str())
            .header("session-id", self.client_session_id.as_str())
            .header(
                TRACE_ID_HEADER,
                format!("trace-stream-{}-{}", self.provider_id, step),
            )
            .body(self.stream_generate_body.clone())
            .send()
            .await
            .expect("gemini streamGenerateContent HTTP request should complete")
    }

    async fn send_refresh_quota(
        &self,
        key_ids_body: Option<serde_json::Value>,
    ) -> serde_json::Value {
        let mut request = self
            .client
            .post(format!(
                "{}/api/admin/endpoints/providers/{}/refresh-quota",
                self.gateway_url, self.provider_id
            ))
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

    async fn send_admin_put(&self, path: &str, body: &serde_json::Value) -> reqwest::Response {
        self.client
            .put(format!("{}{}", self.gateway_url, path))
            .header(GATEWAY_HEADER, "rust-phase3b")
            .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-quota-test")
            .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
            .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "admin-session-quota-test")
            .json(body)
            .send()
            .await
            .expect("admin PUT HTTP request should complete")
    }

    async fn send_admin_post(&self, path: &str) -> reqwest::Response {
        self.client
            .post(format!("{}{}", self.gateway_url, path))
            .header(GATEWAY_HEADER, "rust-phase3b")
            .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-quota-test")
            .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
            .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "admin-session-quota-test")
            .send()
            .await
            .expect("admin POST HTTP request should complete")
    }

    async fn read_sticky(&self) -> Option<String> {
        let pool_config = self.pool_config.as_ref()?;
        read_provider_pool_sticky_bound_key_id(
            self.gateway_state.runtime_state.as_ref(),
            &self.provider_id,
            pool_config,
            self.sticky_session_token.as_deref(),
        )
        .await
    }

    async fn read_catalog_key(&self, key_id: &str) -> StoredProviderCatalogKey {
        self.provider_catalog_repository
            .list_keys_by_ids(&[key_id.to_string()])
            .await
            .expect("catalog key should read")
            .into_iter()
            .next()
            .expect("catalog key should exist")
    }

    fn generation_counts(&self) -> BTreeMap<(bool, String), usize> {
        self.mock_state
            .lock()
            .expect("mock state should lock")
            .generation_counts
            .clone()
    }

    fn model_generation_counts(
        &self,
        primary_model: &str,
        secondary_model: &str,
    ) -> (usize, usize, usize, usize) {
        let counts = self.generation_counts();
        let get = |is_account_a: bool, model: &str| {
            counts
                .get(&(is_account_a, model.to_string()))
                .copied()
                .unwrap_or(0)
        };
        (
            get(true, primary_model),
            get(true, secondary_model),
            get(false, primary_model),
            get(false, secondary_model),
        )
    }

    fn set_generation_quota_error(
        &self,
        is_account_a: bool,
        provider_model: &str,
        retry_after_seconds: u64,
    ) {
        self.mock_state
            .lock()
            .expect("mock state should lock")
            .generation_errors
            .insert(
                (is_account_a, provider_model.to_string()),
                AntigravityUpstreamErrorFixture {
                    retry_after_seconds,
                    kind: AntigravityUpstreamErrorKind::ModelQuota,
                },
            );
    }

    fn set_generation_rate_limit_error(
        &self,
        is_account_a: bool,
        provider_model: &str,
        retry_after_seconds: u64,
    ) {
        self.mock_state
            .lock()
            .expect("mock state should lock")
            .generation_errors
            .insert(
                (is_account_a, provider_model.to_string()),
                AntigravityUpstreamErrorFixture {
                    retry_after_seconds,
                    kind: AntigravityUpstreamErrorKind::AccountRateLimit,
                },
            );
    }

    fn clear_generation_quota_error(&self, is_account_a: bool, provider_model: &str) {
        self.mock_state
            .lock()
            .expect("mock state should lock")
            .generation_errors
            .remove(&(is_account_a, provider_model.to_string()));
    }

    fn set_account_models(&self, is_account_a: bool, models: BTreeMap<String, ModelQuotaFixture>) {
        let mut guard = self.mock_state.lock().expect("mock state should lock");
        let policy = if is_account_a {
            &mut guard.account_a
        } else {
            &mut guard.account_b
        };
        policy.models = models;
    }

    fn set_account_fetch_models_status(&self, is_account_a: bool, status: u16) {
        let mut guard = self.mock_state.lock().expect("mock state should lock");
        let policy = if is_account_a {
            &mut guard.account_a
        } else {
            &mut guard.account_b
        };
        policy.fetch_models_status = status;
    }

    fn set_quota_reset_rfc3339(&self, reset_rfc3339: String) {
        self.mock_state
            .lock()
            .expect("mock state should lock")
            .reset_rfc3339 = reset_rfc3339;
    }

    async fn materialize_pool_scores(&self) {
        let Some(pool_config) = self.pool_config.as_ref() else {
            return;
        };
        let now_unix_secs = current_unix_secs();
        for key_id in [&self.key_a_id, &self.key_b_id] {
            let key = self.read_catalog_key(key_id).await;
            let score = crate::ai_serving::build_provider_key_pool_score_upsert(
                &key,
                "antigravity",
                None,
                now_unix_secs,
                pool_config.score_rules.clone(),
            );
            self.pool_score_repository
                .upsert_pool_member_score(score)
                .await
                .expect("pool member score should materialize");
        }
    }

    async fn read_penalty_snapshot(&self, key_id: &str) -> AntigravityAccountPenaltySnapshot {
        let key = self.read_catalog_key(key_id).await;
        let cooldown_reason = read_admin_provider_pool_key_cooldown_reason(
            self.gateway_state.runtime_state.as_ref(),
            &self.provider_id,
            key_id,
        )
        .await
        .expect("pool cooldown reason should read");
        let pool_identity =
            PoolMemberIdentity::provider_api_key(self.provider_id.clone(), key_id.to_string());
        let pool_hard_states = self
            .pool_score_repository
            .list_pool_member_scores(&ListPoolMemberScoresQuery {
                pool_kind: pool_identity.pool_kind.clone(),
                pool_id: pool_identity.pool_id.clone(),
                capability: None,
                scope_kind: None,
                scope_id: None,
                hard_states: Vec::new(),
                probe_statuses: None,
                offset: 0,
                limit: 256,
            })
            .await
            .expect("pool member scores should read")
            .into_iter()
            .filter(|score: &StoredPoolMemberScore| score.member_id == key_id)
            .map(|score| score.hard_state)
            .collect();
        let health_consecutive_failures = key
            .health_by_format
            .as_ref()
            .and_then(|health| health.pointer("/gemini:generate_content/consecutive_failures"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let scheduling_code = key
            .status_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.pointer("/scheduling/code"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        AntigravityAccountPenaltySnapshot {
            cooldown_reason,
            pool_hard_states,
            health_consecutive_failures,
            adaptive_rpm_429_count: key.rpm_429_count.unwrap_or(0),
            adaptive_last_429_at_unix_secs: key.last_429_at_unix_secs,
            adaptive_last_429_type: key.last_429_type.clone(),
            scheduling_code,
            oauth_invalid_at_unix_secs: key.oauth_invalid_at_unix_secs,
        }
    }
}

fn gemini_sse_data_frames(body: &str) -> Vec<serde_json::Value> {
    body.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .filter(|payload| !payload.is_empty() && *payload != "[DONE]")
        .filter_map(|payload| serde_json::from_str::<serde_json::Value>(payload).ok())
        .collect()
}

fn print_antigravity_http_observation(
    mode_label: &str,
    step: &str,
    status: Option<StatusCode>,
    served_model: Option<&str>,
    sticky_before: Option<&str>,
    sticky_after: Option<&str>,
    counts: AntigravityHttpCounts,
) {
    println!(
        "[antigravity-account-model-http][{mode_label}] {step} status={} served_upstream_model={} gen_a={} gen_b={} unsupported_gen_a={} unsupported_gen_b={} probe_a={} probe_b={} sticky_before={} sticky_after={}",
        status.map_or_else(|| "none".to_string(), |status| status.as_u16().to_string()),
        served_model.unwrap_or("none"),
        counts.gen_a,
        counts.gen_b,
        counts.unsupported_a,
        counts.unsupported_b,
        counts.probe_a,
        counts.probe_b,
        sticky_before.unwrap_or("<unbound>"),
        sticky_after.unwrap_or("<unbound>"),
    );
}

fn assert_gemini_response_body(
    body: &serde_json::Value,
    mode_label: &str,
    step: &str,
    expected_model: &str,
    expected_response_id: &str,
    expected_text: &str,
) {
    assert_eq!(
        body,
        &json!({
            "responseId": expected_response_id,
            "_v1internal_response_id": expected_response_id,
            "candidates": [{
                "content": {
                    "parts": [{ "text": expected_text }],
                    "role": "model"
                },
                "finishReason": "STOP",
                "index": 0
            }],
            "modelVersion": expected_model,
            "usageMetadata": {
                "promptTokenCount": 2,
                "candidatesTokenCount": 3,
                "totalTokenCount": 5
            }
        }),
        "{mode_label}/{step}: final Gemini response mismatch"
    );
}

async fn assert_valid_gemini_response(
    response: reqwest::Response,
    mode_label: &str,
    step: &str,
    expected_model: &str,
    expected_response_id: &str,
    expected_text: &str,
) -> serde_json::Value {
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{mode_label}/{step}: expected HTTP 200"
    );
    let body: serde_json::Value = response
        .json()
        .await
        .expect("gemini response JSON should parse");
    assert_gemini_response_body(
        &body,
        mode_label,
        step,
        expected_model,
        expected_response_id,
        expected_text,
    );
    body
}

/// 两条执行运行时路径（sync/stream）共享的生成计划校验：真实计划 URL、映射后的
/// 上游模型、账号 project 与温度，并返回该账号目录是否支持目标模型。
fn antigravity_generation_plan_supported(
    plan: &aether_contracts::ExecutionPlan,
    is_account_a: bool,
    expected_project: &str,
    mock_state: &Mutex<QuotaRuntimeMockState>,
) -> (String, bool) {
    assert_eq!(
        plan.url,
        "https://daily-cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse"
    );
    let body_json = plan
        .body
        .json_body
        .as_ref()
        .expect("generation request should carry json_body");
    let (plan_model, supported) = {
        let guard = mock_state.lock().expect("mock state should lock");
        let policy = if is_account_a {
            &guard.account_a
        } else {
            &guard.account_b
        };
        let plan_model = body_json
            .get("model")
            .and_then(serde_json::Value::as_str)
            .expect("upstream generation must carry a mapped provider model")
            .to_string();
        assert!(
            guard.generation_models.contains(plan_model.as_str()),
            "upstream generation model {plan_model} must be a mapped provider model"
        );
        let supported = policy.models.contains_key(plan_model.as_str());
        (plan_model, supported)
    };
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
    (plan_model, supported)
}

async fn start_antigravity_http_harness(config: AntigravityHttpConfig) -> AntigravityHttpHarness {
    let AntigravityHttpConfig {
        mode_label,
        pool_enabled,
        provider_id,
        endpoint_id,
        model_id,
        global_model_name,
        provider_model_name,
        secondary_model_id,
        secondary_global_model_name,
        secondary_provider_model_name,
        generation_response_id,
        generation_text,
        reset_rfc3339,
        key_a,
        key_b,
    } = config;
    let key_a_id = key_a.key_id.clone();
    let key_b_id = key_b.key_id.clone();
    let mut generation_models = BTreeSet::from([provider_model_name.clone()]);
    if let Some(secondary_provider_model_name) = secondary_provider_model_name.as_ref() {
        generation_models.insert(secondary_provider_model_name.clone());
    }

    let token_a = key_a.access_token.clone();
    let token_b = key_b.access_token.clone();
    let refresh_token_a = key_a.refresh_token.clone();
    let refresh_token_b = key_b.refresh_token.clone();
    let project_a = key_a.project_id.clone();
    let project_b = key_b.project_id.clone();
    let bearer_a = format!("Bearer {token_a}");
    let bearer_b = format!("Bearer {token_b}");

    let now_secs = current_unix_secs();

    let gen_a = Arc::new(AtomicUsize::new(0));
    let gen_b = Arc::new(AtomicUsize::new(0));
    let unsupported_a = Arc::new(AtomicUsize::new(0));
    let unsupported_b = Arc::new(AtomicUsize::new(0));
    let probe_a = Arc::new(AtomicUsize::new(0));
    let probe_b = Arc::new(AtomicUsize::new(0));

    let mock_state = Arc::new(Mutex::new(QuotaRuntimeMockState {
        reset_rfc3339: reset_rfc3339.clone(),
        generation_models,
        generation_response_id,
        generation_text,
        account_a_subscription_tier: key_a.subscription_tier.clone(),
        account_b_subscription_tier: key_b.subscription_tier.clone(),
        account_a: AccountMockPolicy {
            fetch_models_status: 200,
            models: key_a.models.clone(),
        },
        account_b: AccountMockPolicy {
            fetch_models_status: 200,
            models: key_b.models.clone(),
        },
        generation_errors: BTreeMap::new(),
        generation_counts: BTreeMap::new(),
    }));

    let refresh_router = Router::new().route(
        "/oauth/token",
        any(move |request: Request| {
            let refresh_token_a = refresh_token_a.clone();
            let refresh_token_b = refresh_token_b.clone();
            let token_a = token_a.clone();
            let token_b = token_b.clone();
            async move {
                let raw_body = to_bytes(request.into_body(), usize::MAX)
                    .await
                    .expect("oauth token request body should read");
                let body_text =
                    String::from_utf8(raw_body.to_vec()).expect("oauth token body should be utf8");
                let (access_token, refresh_token) = if body_text.contains(refresh_token_a.as_str())
                {
                    (token_a, refresh_token_a)
                } else if body_text.contains(refresh_token_b.as_str()) {
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
            }
        }),
    );

    let gen_a_runtime = gen_a.clone();
    let gen_b_runtime = gen_b.clone();
    let unsupported_a_runtime = unsupported_a.clone();
    let unsupported_b_runtime = unsupported_b.clone();
    let probe_a_runtime = probe_a.clone();
    let probe_b_runtime = probe_b.clone();
    let mock_state_runtime = mock_state.clone();
    let bearer_a_expected = bearer_a.clone();
    let bearer_b_expected = bearer_b.clone();
    let project_a_expected = project_a.clone();
    let project_b_expected = project_b.clone();

    let execution_runtime = Router::new().route(
        "/v1/execute/sync",
        any(move |request: Request| {
            let gen_a_inner = gen_a_runtime.clone();
            let gen_b_inner = gen_b_runtime.clone();
            let unsupported_a_inner = unsupported_a_runtime.clone();
            let unsupported_b_inner = unsupported_b_runtime.clone();
            let probe_a_inner = probe_a_runtime.clone();
            let probe_b_inner = probe_b_runtime.clone();
            let mock_state_inner = mock_state_runtime.clone();
            let bearer_a_expected = bearer_a_expected.clone();
            let bearer_b_expected = bearer_b_expected.clone();
            let project_a_expected = project_a_expected.clone();
            let project_b_expected = project_b_expected.clone();
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
                let expected_project = if is_account_a {
                    &project_a_expected
                } else {
                    &project_b_expected
                };

                match plan.provider_api_format.as_str() {
                    "gemini:generate_content" => {
                        let (plan_model, supported) = antigravity_generation_plan_supported(
                            &plan,
                            is_account_a,
                            expected_project.as_str(),
                            mock_state_inner.as_ref(),
                        );
                        if !supported {
                            if is_account_a {
                                unsupported_a_inner.fetch_add(1, Ordering::SeqCst);
                            } else {
                                unsupported_b_inner.fetch_add(1, Ordering::SeqCst);
                            }
                            let result = aether_contracts::ExecutionResult {
                                request_id: plan.request_id,
                                candidate_id: plan.candidate_id,
                                status_code: 404,
                                headers: BTreeMap::from([(
                                    "content-type".to_string(),
                                    "application/json".to_string(),
                                )]),
                                response_observation: None,
                                body: Some(aether_contracts::ResponseBody {
                                    json_body: Some(json!({
                                        "error": {
                                            "code": 404,
                                            "message": "Requested entity was not found."
                                        }
                                    })),
                                    body_bytes_b64: None,
                                }),
                                telemetry: None,
                                error: None,
                            };
                            return (StatusCode::OK, Json(result));
                        }

                        if is_account_a {
                            gen_a_inner.fetch_add(1, Ordering::SeqCst);
                        } else {
                            gen_b_inner.fetch_add(1, Ordering::SeqCst);
                        }
                        let generation_error = {
                            let mut guard =
                                mock_state_inner.lock().expect("mock state should lock");
                            *guard
                                .generation_counts
                                .entry((is_account_a, plan_model.clone()))
                                .or_insert(0) += 1;
                            guard
                                .generation_errors
                                .get(&(is_account_a, plan_model.clone()))
                                .copied()
                        };
                        if let Some(generation_error) = generation_error {
                            let retry_after_seconds = generation_error.retry_after_seconds;
                            let error_body = match generation_error.kind {
                                AntigravityUpstreamErrorKind::ModelQuota => {
                                    antigravity_quota_error_body(&plan_model, retry_after_seconds)
                                }
                                AntigravityUpstreamErrorKind::AccountRateLimit => {
                                    antigravity_rate_limit_error_body(
                                        &plan_model,
                                        retry_after_seconds,
                                    )
                                }
                            };
                            let result = aether_contracts::ExecutionResult {
                                request_id: plan.request_id,
                                candidate_id: plan.candidate_id,
                                status_code: 429,
                                headers: BTreeMap::from([
                                    (
                                        "content-type".to_string(),
                                        "application/json".to_string(),
                                    ),
                                    (
                                        "retry-after".to_string(),
                                        retry_after_seconds.to_string(),
                                    ),
                                ]),
                                response_observation: None,
                                body: Some(aether_contracts::ResponseBody {
                                    json_body: Some(error_body),
                                    body_bytes_b64: None,
                                }),
                                telemetry: None,
                                error: None,
                            };
                            return (StatusCode::OK, Json(result));
                        }
                        let (response_id, text) = {
                            let guard = mock_state_inner.lock().expect("mock state should lock");
                            (
                                guard.generation_response_id.clone(),
                                guard.generation_text.clone(),
                            )
                        };
                        let model_version = plan_model;
                        let sse_frame = format!(
                            "data: {{\"response\":{{\"candidates\":[{{\"content\":{{\"parts\":[{{\"text\":\"{text}\"}}],\"role\":\"model\"}},\"finishReason\":\"STOP\",\"index\":0}}],\"modelVersion\":\"{model_version}\",\"usageMetadata\":{{\"promptTokenCount\":2,\"candidatesTokenCount\":3,\"totalTokenCount\":5}}}},\"responseId\":\"{response_id}\"}}\n\n"
                        );
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
                        let subscription_tier = {
                            let guard = mock_state_inner.lock().expect("mock state should lock");
                            if is_account_a {
                                guard.account_a_subscription_tier.clone()
                            } else {
                                guard.account_b_subscription_tier.clone()
                            }
                        };
                        let tier_id = format!("g1-{subscription_tier}-tier");
                        let result = aether_contracts::ExecutionResult {
                            request_id: plan.request_id,
                            candidate_id: plan.candidate_id,
                            status_code: 200,
                            headers: BTreeMap::new(),
                            response_observation: None,
                            body: Some(aether_contracts::ResponseBody {
                                json_body: Some(json!({
                                    "cloudaicompanionProject": { "id": expected_project },
                                    "paidTier": { "id": tier_id },
                                    "currentTier": { "id": tier_id }
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
                                    mock_antigravity_models_body(policy, &guard.reset_rfc3339),
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

    let gen_a_stream = gen_a.clone();
    let gen_b_stream = gen_b.clone();
    let unsupported_a_stream = unsupported_a.clone();
    let unsupported_b_stream = unsupported_b.clone();
    let mock_state_stream = mock_state.clone();
    let bearer_a_stream = bearer_a.clone();
    let bearer_b_stream = bearer_b.clone();
    let project_a_stream = project_a.clone();
    let project_b_stream = project_b.clone();

    // 客户端原生 SSE 走执行运行时 NDJSON 流协议：同一个生成计划校验与同一组观测计数。
    let stream_execution_runtime = Router::new().route(
        "/v1/execute/stream",
        any(move |request: Request| {
            let gen_a_inner = gen_a_stream.clone();
            let gen_b_inner = gen_b_stream.clone();
            let unsupported_a_inner = unsupported_a_stream.clone();
            let unsupported_b_inner = unsupported_b_stream.clone();
            let mock_state_inner = mock_state_stream.clone();
            let bearer_a_expected = bearer_a_stream.clone();
            let bearer_b_expected = bearer_b_stream.clone();
            let project_a_expected = project_a_stream.clone();
            let project_b_expected = project_b_stream.clone();
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
                let expected_project = if is_account_a {
                    &project_a_expected
                } else {
                    &project_b_expected
                };
                let (plan_model, supported) = antigravity_generation_plan_supported(
                    &plan,
                    is_account_a,
                    expected_project.as_str(),
                    mock_state_inner.as_ref(),
                );

                let mut frames = Vec::new();
                if !supported {
                    if is_account_a {
                        unsupported_a_inner.fetch_add(1, Ordering::SeqCst);
                    } else {
                        unsupported_b_inner.fetch_add(1, Ordering::SeqCst);
                    }
                    frames.push(json!({
                        "type": "headers",
                        "payload": {
                            "kind": "headers",
                            "status_code": 404,
                            "headers": { "content-type": "application/json" }
                        }
                    }));
                    frames.push(json!({
                        "type": "data",
                        "payload": {
                            "kind": "data",
                            "text": "{\"error\":{\"code\":404,\"message\":\"Requested entity was not found.\"}}"
                        }
                    }));
                } else {
                    if is_account_a {
                        gen_a_inner.fetch_add(1, Ordering::SeqCst);
                    } else {
                        gen_b_inner.fetch_add(1, Ordering::SeqCst);
                    }
                    let generation_error = {
                        let mut guard = mock_state_inner.lock().expect("mock state should lock");
                        *guard
                            .generation_counts
                            .entry((is_account_a, plan_model.clone()))
                            .or_insert(0) += 1;
                        guard
                            .generation_errors
                            .get(&(is_account_a, plan_model.clone()))
                            .copied()
                    };
                    if let Some(generation_error) = generation_error {
                        let retry_after_seconds = generation_error.retry_after_seconds;
                        let error_text = match generation_error.kind {
                            AntigravityUpstreamErrorKind::ModelQuota => {
                                antigravity_quota_error_body(&plan_model, retry_after_seconds)
                            }
                            AntigravityUpstreamErrorKind::AccountRateLimit => {
                                antigravity_rate_limit_error_body(
                                    &plan_model,
                                    retry_after_seconds,
                                )
                            }
                        }
                        .to_string();
                        frames.push(json!({
                            "type": "headers",
                            "payload": {
                                "kind": "headers",
                                "status_code": 429,
                                "headers": {
                                    "content-type": "application/json",
                                    "retry-after": retry_after_seconds.to_string()
                                }
                            }
                        }));
                        frames.push(json!({
                            "type": "data",
                            "payload": { "kind": "data", "text": error_text }
                        }));
                    } else {
                        let (response_id, text) = {
                            let guard =
                                mock_state_inner.lock().expect("mock state should lock");
                            (
                                guard.generation_response_id.clone(),
                                guard.generation_text.clone(),
                            )
                        };
                        let model_version = plan_model;
                        let sse_chunk = format!(
                            "data: {{\"response\":{{\"candidates\":[{{\"content\":{{\"parts\":[{{\"text\":\"{text}\"}}],\"role\":\"model\"}},\"finishReason\":\"STOP\",\"index\":0}}],\"modelVersion\":\"{model_version}\",\"usageMetadata\":{{\"promptTokenCount\":2,\"candidatesTokenCount\":3,\"totalTokenCount\":5}}}},\"responseId\":\"{response_id}\"}}\n\n"
                        );
                        frames.push(json!({
                            "type": "headers",
                            "payload": {
                                "kind": "headers",
                                "status_code": 200,
                                "headers": { "content-type": "text/event-stream" }
                            }
                        }));
                        frames.push(json!({
                            "type": "data",
                            "payload": { "kind": "data", "text": sse_chunk }
                        }));
                        frames.push(json!({
                            "type": "telemetry",
                            "payload": {
                                "kind": "telemetry",
                                "telemetry": { "upstream_bytes": sse_chunk.len() }
                            }
                        }));
                    }
                }
                frames.push(json!({ "type": "eof", "payload": { "kind": "eof" } }));

                let mut body = String::new();
                for frame in frames {
                    body.push_str(&frame.to_string());
                    body.push('\n');
                }
                Response::builder()
                    .status(StatusCode::OK)
                    .header(http::header::CONTENT_TYPE, "application/x-ndjson")
                    .body(Body::from(body))
                    .expect("stream execution runtime response should build")
            }
        }),
    );
    let execution_runtime = execution_runtime.merge(stream_execution_runtime);

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

    let key_a_record =
        build_antigravity_account_key(&seed_state, &provider_id, &key_a, now_secs, &reset_rfc3339);
    let key_b_record =
        build_antigravity_account_key(&seed_state, &provider_id, &key_b, now_secs, &reset_rfc3339);

    let mut candidate_rows = vec![
        build_antigravity_candidate_row(
            &provider_id,
            &endpoint_id,
            &model_id,
            &key_a,
            &global_model_name,
            &provider_model_name,
            pool_enabled,
        ),
        build_antigravity_candidate_row(
            &provider_id,
            &endpoint_id,
            &model_id,
            &key_b,
            &global_model_name,
            &provider_model_name,
            pool_enabled,
        ),
    ];
    if let (
        Some(secondary_model_id),
        Some(secondary_global_model_name),
        Some(secondary_provider_model_name),
    ) = (
        secondary_model_id.as_ref(),
        secondary_global_model_name.as_ref(),
        secondary_provider_model_name.as_ref(),
    ) {
        candidate_rows.push(build_antigravity_candidate_row(
            &provider_id,
            &endpoint_id,
            secondary_model_id,
            &key_a,
            secondary_global_model_name,
            secondary_provider_model_name,
            pool_enabled,
        ));
        candidate_rows.push(build_antigravity_candidate_row(
            &provider_id,
            &endpoint_id,
            secondary_model_id,
            &key_b,
            secondary_global_model_name,
            secondary_provider_model_name,
            pool_enabled,
        ));
    }

    let client_api_key = format!("client-{provider_id}");
    let auth_repository = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![(
        Some(hash_client_api_key(&client_api_key)),
        build_client_auth_snapshot(
            &format!("api-key-{provider_id}"),
            &format!("user-{provider_id}"),
        ),
    )]));
    let candidate_selection_repository = Arc::new(
        InMemoryMinimalCandidateSelectionReadRepository::seed(candidate_rows),
    );
    let request_candidate_repository = Arc::new(InMemoryRequestCandidateRepository::default());
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![provider],
        vec![endpoint],
        vec![key_a_record, key_b_record],
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
            .with_pool_score_repository_for_tests(pool_score_repository.clone())
            .with_system_default_routing_group_for_tests(),
        )
        .with_oauth_refresh_coordinator_for_tests(oauth_refresh);

    let (gateway_url, gateway_handle) =
        start_server(build_router_with_state(gateway_state.clone())).await;

    let client = reqwest::Client::new();
    let client_session_id = format!("client-session-{provider_id}");
    let generate_body_json = json!({
        "contents": [{ "role": "user", "parts": [{ "text": "ping" }] }],
        "generationConfig": { "temperature": 0.2 },
        "session_id": client_session_id,
    });
    let stream_generate_body_json = json!({
        "contents": [{ "role": "user", "parts": [{ "text": "ping" }] }],
        "generationConfig": { "temperature": 0.2 },
        "session_id": client_session_id,
    });
    let sticky_session_token = extract_pool_sticky_session_token(&generate_body_json);
    assert_eq!(
        sticky_session_token.as_deref(),
        Some(client_session_id.as_str()),
        "{mode_label}: request body should carry extractable Pool sticky session token"
    );

    AntigravityHttpHarness {
        provider_id,
        key_a_id,
        key_b_id,
        global_model_name,
        gateway_url,
        client,
        client_api_key,
        client_session_id,
        generate_body: generate_body_json.to_string(),
        stream_generate_body: stream_generate_body_json.to_string(),
        sticky_session_token,
        mock_state,
        gen_a,
        gen_b,
        unsupported_a,
        unsupported_b,
        probe_a,
        probe_b,
        gateway_state,
        provider_catalog_repository,
        pool_score_repository,
        pool_config,
        servers: vec![refresh_handle, execution_runtime_handle, gateway_handle],
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
    let shared_models = vec![
        "claude-sonnet-4-5".to_string(),
        "claude-sonnet-4-6".to_string(),
    ];
    let expected_model = "claude-sonnet-4-5";
    let expected_response_id = "resp_antigravity_cli_quota_sync";
    let expected_text = "Hello Antigravity CLI";

    let harness = start_antigravity_http_harness(AntigravityHttpConfig {
        mode_label: mode_label.to_string(),
        pool_enabled,
        provider_id: format!("provider-antigravity-quota-{mode_label}"),
        endpoint_id: format!("endpoint-antigravity-quota-{mode_label}"),
        model_id: format!("model-antigravity-quota-{mode_label}"),
        global_model_name: "gemini-cli".to_string(),
        provider_model_name: expected_model.to_string(),
        secondary_model_id: None,
        secondary_global_model_name: None,
        secondary_provider_model_name: None,
        generation_response_id: expected_response_id.to_string(),
        generation_text: expected_text.to_string(),
        reset_rfc3339: format_rfc3339_secs(current_unix_secs() + 3600),
        key_a: AccountFixtureSpec {
            key_id: format!("key-antigravity-quota-a-{mode_label}"),
            key_name: "account-a".to_string(),
            priority: 1,
            access_token: "ya29.fake-antigravity-quota-token-a".to_string(),
            refresh_token: "rt-fake-antigravity-quota-a".to_string(),
            project_id: "project-antigravity-quota-a".to_string(),
            session_id: "sess-antigravity-quota-a".to_string(),
            subscription_tier: "pro".to_string(),
            allowed_models: Some(shared_models.clone()),
            auto_fetch_models: false,
            models: claude_quota_models(0.5, 0.8),
        },
        key_b: AccountFixtureSpec {
            key_id: format!("key-antigravity-quota-b-{mode_label}"),
            key_name: "account-b".to_string(),
            priority: 2,
            access_token: "ya29.fake-antigravity-quota-token-b".to_string(),
            refresh_token: "rt-fake-antigravity-quota-b".to_string(),
            project_id: "project-antigravity-quota-b".to_string(),
            session_id: "sess-antigravity-quota-b".to_string(),
            subscription_tier: "pro".to_string(),
            allowed_models: Some(shared_models),
            auto_fetch_models: false,
            models: claude_quota_models(0.8, 0.8),
        },
    })
    .await;

    let key_a_id = harness.key_a_id.clone();
    let key_b_id = harness.key_b_id.clone();
    let snapshot_counts = || {
        let counts = harness.counts();
        (counts.gen_a, counts.gen_b, counts.probe_a, counts.probe_b)
    };

    assert_eq!(
        harness.read_sticky().await,
        None,
        "{mode_label}: Pool sticky session should start unbound before warmup"
    );

    // Step 1: Warmup request selects preferred account A (initial A=0.5, B=0.8) and warms caches.
    assert_valid_gemini_response(
        harness.send_generate("step1-warmup").await,
        mode_label,
        "step1-warmup",
        expected_model,
        expected_response_id,
        expected_text,
    )
    .await;
    let c1 = snapshot_counts();
    assert_eq!(
        (c1.0, c1.1),
        (1, 0),
        "{mode_label}/step1: only A should serve"
    );
    if pool_enabled {
        assert_eq!(
            harness.read_sticky().await.as_deref(),
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
        let mut guard = harness.mock_state.lock().expect("mock state should lock");
        guard.reset_rfc3339 = format_rfc3339_secs(current_unix_secs() + 3600);
        guard.account_a = claude_quota_policy(200, 0.0, 0.8);
        guard.account_b = claude_quota_policy(200, 0.8, 0.8);
    }
    let refresh_step2 = harness.send_refresh_quota(None).await;
    assert_eq!(refresh_step2["success"], json!(2));
    assert_eq!(refresh_step2["failed"], json!(0));
    assert_eq!(refresh_step2["total"], json!(2));
    {
        let c = snapshot_counts();
        assert!(
            c.2 > c1.2 && c.3 > c1.3,
            "{mode_label}/step2: quota refresh must increment probe counters"
        );
    }
    {
        let c = snapshot_counts();
        assert_eq!(
            (c.0, c.1),
            (1, 0),
            "{mode_label}/step2: quota refresh must not increment generation counters"
        );
    }

    assert_valid_gemini_response(
        harness.send_generate("step2-failover-b").await,
        mode_label,
        "step2-failover-b",
        expected_model,
        expected_response_id,
        expected_text,
    )
    .await;
    let c2 = snapshot_counts();
    assert_eq!(
        (c2.0, c2.1),
        (1, 1),
        "{mode_label}/step2: exhausted A must be skipped despite sibling=0.8 and only B serves"
    );
    if pool_enabled {
        assert_eq!(
            harness.read_sticky().await.as_deref(),
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
        let mut guard = harness.mock_state.lock().expect("mock state should lock");
        guard.reset_rfc3339 = format_rfc3339_secs(current_unix_secs() + 3600);
        guard.account_a = claude_quota_policy(200, 0.5, 0.8);
        guard.account_b = claude_quota_policy(200, 0.0, 0.8);
    }
    let refresh_step3 = harness.send_refresh_quota(None).await;
    assert_eq!(refresh_step3["success"], json!(2));
    assert_eq!(refresh_step3["failed"], json!(0));

    assert_valid_gemini_response(
        harness.send_generate("step3-recover-a").await,
        mode_label,
        "step3-recover-a",
        expected_model,
        expected_response_id,
        expected_text,
    )
    .await;
    let c3 = snapshot_counts();
    assert_eq!(
        (c3.0, c3.1),
        (2, 1),
        "{mode_label}/step3: recovered A must serve (+1) and exhausted B must not increase"
    );
    if pool_enabled {
        assert_eq!(
            harness.read_sticky().await.as_deref(),
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
        let mut guard = harness.mock_state.lock().expect("mock state should lock");
        guard.reset_rfc3339 = format_rfc3339_secs(current_unix_secs() + 3600);
        guard.account_a = claude_quota_policy(200, 0.0, 0.8);
        guard.account_b = claude_quota_policy(502, 0.8, 0.8);
    }
    let refresh_partial_502 = harness.send_refresh_quota(None).await;
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

    let failed_probe_resp_1 = harness.send_generate("step4a-partial-502").await;
    assert_eq!(
        failed_probe_resp_1.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    {
        let c = snapshot_counts();
        assert_eq!(
            (c.0, c.1),
            (2, 1),
            "{mode_label}/step4a: old exhausted B must not recover when fetchAvailableModels returns 502"
        );
    }

    {
        let mut guard = harness.mock_state.lock().expect("mock state should lock");
        guard.account_a.fetch_models_status = 502;
        guard.account_b.fetch_models_status = 502;
    }
    let refresh_all_502 = harness.send_refresh_quota(None).await;
    assert_eq!(refresh_all_502["success"], json!(0));
    assert_eq!(refresh_all_502["failed"], json!(2));
    assert!(refresh_all_502["results"]
        .as_array()
        .expect("results array should exist")
        .iter()
        .all(|item| item["status"] == json!("error")));

    let failed_probe_resp_2 = harness.send_generate("step4a-all-502").await;
    assert_eq!(
        failed_probe_resp_2.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "{mode_label}/step4a: loadCodeAssist 200 + fetchAvailableModels 502 must keep accounts blocked, served counts={:?}",
        snapshot_counts()
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
    let stored_a = harness.read_catalog_key(&key_a_id).await;
    let cas_applied = harness
        .gateway_state
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
        let mut guard = harness.mock_state.lock().expect("mock state should lock");
        guard.reset_rfc3339 = format_rfc3339_secs(current_unix_secs() + 3600);
        guard.account_a = claude_quota_policy(200, 0.8, 0.8);
        guard.account_b = claude_quota_policy(200, 0.8, 0.8);
    }
    let refresh_manual_ban = harness.send_refresh_quota(None).await;
    assert_eq!(refresh_manual_ban["success"], json!(2));
    assert_eq!(refresh_manual_ban["failed"], json!(0));

    assert_valid_gemini_response(
        harness.send_generate("step4b-manual-ban").await,
        mode_label,
        "step4b-manual-ban",
        expected_model,
        expected_response_id,
        expected_text,
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
            harness.read_sticky().await.as_deref(),
            Some(key_b_id.as_str()),
            "{mode_label}/step4b: manual ban on sticky A should fail over and rebind Pool sticky session to B"
        );
    }
    println!(
        "[antigravity-quota-http][{mode_label}] step4b_manual_ban gen_a={} gen_b={} probe_a={} probe_b={}",
        c4b_ban.0, c4b_ban.1, c4b_ban.2, c4b_ban.3
    );

    let clear_ban_resp = harness
        .send_admin_post(&format!(
            "/api/admin/endpoints/keys/{key_a_id}/clear-quota-exhausted"
        ))
        .await;
    assert_eq!(clear_ban_resp.status(), StatusCode::OK);
    let clear_ban_json: serde_json::Value = clear_ban_resp
        .json()
        .await
        .expect("clear-quota-exhausted payload should parse");
    assert_eq!(clear_ban_json["cleared"], json!(true));

    // Step 4b (2): Manually disabled A (is_active=false) still cannot serve after positive quota refresh.
    let disable_body = json!({ "is_active": false });
    let disable_a_resp = harness
        .send_admin_put(
            &format!("/api/admin/endpoints/keys/{key_a_id}"),
            &disable_body,
        )
        .await;
    assert_eq!(disable_a_resp.status(), StatusCode::OK);

    {
        let mut guard = harness.mock_state.lock().expect("mock state should lock");
        guard.reset_rfc3339 = format_rfc3339_secs(current_unix_secs() + 3600);
        guard.account_a = claude_quota_policy(200, 0.8, 0.8);
        guard.account_b = claude_quota_policy(200, 0.0, 0.8);
    }
    let refresh_disabled = harness
        .send_refresh_quota(Some(json!({ "key_ids": [key_a_id, key_b_id] })))
        .await;
    assert_eq!(refresh_disabled["success"], json!(2));
    assert_eq!(refresh_disabled["failed"], json!(0));

    let disabled_resp = harness.send_generate("step4b-disabled-a").await;
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

    let enable_body = json!({ "is_active": true });
    let enable_a_resp = harness
        .send_admin_put(
            &format!("/api/admin/endpoints/keys/{key_a_id}"),
            &enable_body,
        )
        .await;
    assert_eq!(enable_a_resp.status(), StatusCode::OK);
    assert_valid_gemini_response(
        harness.send_generate("step4b-reenabled-a").await,
        mode_label,
        "step4b-reenabled-a",
        expected_model,
        expected_response_id,
        expected_text,
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
            harness.read_sticky().await.as_deref(),
            Some(key_a_id.as_str()),
            "{mode_label}/step4b: re-enabled sole-eligible A should rebind Pool sticky session to A"
        );
    }
    // Step 4c: All active, unbanned accounts exhausted -> HTTP 503 with zero generation requests.
    {
        let mut guard = harness.mock_state.lock().expect("mock state should lock");
        guard.reset_rfc3339 = format_rfc3339_secs(current_unix_secs() + 3600);
        guard.account_a = claude_quota_policy(200, 0.0, 0.8);
        guard.account_b = claude_quota_policy(200, 0.0, 0.8);
    }
    let refresh_all_exhausted = harness.send_refresh_quota(None).await;
    assert_eq!(refresh_all_exhausted["success"], json!(2));
    assert_eq!(refresh_all_exhausted["failed"], json!(0));

    let all_exhausted_resp = harness.send_generate("step4c-all-exhausted").await;
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
}

#[test]
fn gateway_antigravity_account_scoped_argon_admission() {
    run_gemini_cli_sync_test(
        "gateway_antigravity_account_scoped_argon_admission",
        gateway_antigravity_account_scoped_argon_admission_impl,
    );
}

async fn gateway_antigravity_account_scoped_argon_admission_impl() {
    run_antigravity_argon_admission_mode(false).await;
    run_antigravity_argon_admission_mode(true).await;
}

/// 复现事故 `b92c3b1f` 的逐账号模型准入：Provider 映射 `gemini-4-argon` ->
/// `gemini-4-argon-high`，高优先级 Pro 账号的权威目录只有占位模型
/// `gemini-4-argon-disabled`（仅 display_name，无伪造额度），合格兄弟账号目录包含
/// 真实的 high/medium 条目。两个账号都是 `auto_fetch_models=false` 且
/// `allowed_models=null`，因此准入只能来自账号自己的发现目录。
async fn run_antigravity_argon_admission_mode(pool_enabled: bool) {
    let mode_label = if pool_enabled { "pool" } else { "ordinary" };
    let provider_model = "gemini-4-argon-high";
    let expected_response_id = "resp_antigravity_argon_admission";
    let expected_text = "Hello Antigravity Argon";

    let harness = start_antigravity_http_harness(AntigravityHttpConfig {
        mode_label: mode_label.to_string(),
        pool_enabled,
        provider_id: format!("provider-antigravity-argon-{mode_label}"),
        endpoint_id: format!("endpoint-antigravity-argon-{mode_label}"),
        model_id: format!("model-antigravity-argon-{mode_label}"),
        global_model_name: "gemini-4-argon".to_string(),
        provider_model_name: provider_model.to_string(),
        secondary_model_id: None,
        secondary_global_model_name: None,
        secondary_provider_model_name: None,
        generation_response_id: expected_response_id.to_string(),
        generation_text: expected_text.to_string(),
        reset_rfc3339: format_rfc3339_secs(current_unix_secs() + 3600),
        key_a: AccountFixtureSpec {
            key_id: format!("key-antigravity-argon-a-{mode_label}"),
            key_name: "account-pro-argon-placeholder".to_string(),
            priority: 1,
            access_token: "ya29.fake-antigravity-argon-token-a".to_string(),
            refresh_token: "rt-fake-antigravity-argon-a".to_string(),
            project_id: "project-antigravity-argon-a".to_string(),
            session_id: "sess-antigravity-argon-a".to_string(),
            subscription_tier: "pro".to_string(),
            allowed_models: None,
            auto_fetch_models: false,
            models: BTreeMap::from([(
                "gemini-4-argon-disabled".to_string(),
                ModelQuotaFixture::display_only("Gemini 4 Argon (retired placeholder)"),
            )]),
        },
        key_b: AccountFixtureSpec {
            key_id: format!("key-antigravity-argon-b-{mode_label}"),
            key_name: "account-ultra-argon-catalog".to_string(),
            priority: 2,
            access_token: "ya29.fake-antigravity-argon-token-b".to_string(),
            refresh_token: "rt-fake-antigravity-argon-b".to_string(),
            project_id: "project-antigravity-argon-b".to_string(),
            session_id: "sess-antigravity-argon-b".to_string(),
            subscription_tier: "ultra".to_string(),
            allowed_models: None,
            auto_fetch_models: false,
            models: BTreeMap::from([
                (
                    provider_model.to_string(),
                    ModelQuotaFixture::with_fraction("Gemini 4 Argon High", 0.8),
                ),
                (
                    "gemini-4-argon-medium".to_string(),
                    ModelQuotaFixture::with_fraction("Gemini 4 Argon Medium", 0.9),
                ),
            ]),
        },
    })
    .await;

    let key_a_id = harness.key_a_id.clone();
    let key_b_id = harness.key_b_id.clone();

    assert_eq!(
        harness.read_sticky().await,
        None,
        "{mode_label}: Pool sticky session should start unbound before the incident request"
    );

    // Step 1: higher-priority Pro account exposes only the disabled placeholder; the eligible
    // sibling must serve the exact mapped model natively with zero unsupported generation hits.
    let sticky_before = harness.read_sticky().await;
    let response = harness.send_generate("step1-pro-placeholder").await;
    let status = response.status();
    let body = if status == StatusCode::OK {
        Some(
            response
                .json::<serde_json::Value>()
                .await
                .expect("gemini response JSON should parse"),
        )
    } else {
        None
    };
    let served_model = body
        .as_ref()
        .and_then(|body| body["modelVersion"].as_str())
        .map(str::to_string);
    let sticky_after = harness.read_sticky().await;
    let c1 = harness.counts();
    print_antigravity_http_observation(
        mode_label,
        "step1-pro-placeholder",
        Some(status),
        served_model.as_deref(),
        sticky_before.as_deref(),
        sticky_after.as_deref(),
        c1,
    );
    assert_eq!(
        (c1.unsupported_a, c1.unsupported_b),
        (0, 0),
        "{mode_label}/step1: no upstream generation may target an unsupported account"
    );
    assert_eq!(
        (c1.gen_a, c1.gen_b),
        (0, 1),
        "{mode_label}/step1: only the eligible sibling may receive an upstream generation"
    );
    assert_eq!(
        status,
        StatusCode::OK,
        "{mode_label}/step1: client must receive a native Gemini completion"
    );
    assert_gemini_response_body(
        body.as_ref()
            .expect("200 response should carry a Gemini body"),
        mode_label,
        "step1-pro-placeholder",
        provider_model,
        expected_response_id,
        expected_text,
    );
    assert_eq!(
        served_model.as_deref(),
        Some(provider_model),
        "{mode_label}/step1: sibling must serve the exact mapped upstream model"
    );
    if pool_enabled {
        assert_eq!(
            sticky_after.as_deref(),
            Some(key_b_id.as_str()),
            "{mode_label}/step1: serving sibling should become the Pool sticky binding"
        );
    }

    // Step 1b: the incident surface itself — public native Gemini SSE (`alt=sse`) through the
    // execution runtime stream frame protocol must be served by the same eligible sibling.
    let sticky_before = harness.read_sticky().await;
    let response = harness.send_stream_generate("step1b-native-stream").await;
    let status = response.status();
    let content_type = response
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body_text = response
        .text()
        .await
        .expect("gemini stream response body should read");
    let frames = gemini_sse_data_frames(&body_text);
    let stop_frame = frames.iter().find(|frame| {
        frame
            .pointer("/candidates/0/finishReason")
            .and_then(serde_json::Value::as_str)
            == Some("STOP")
    });
    let served_model = stop_frame
        .and_then(|frame| frame["modelVersion"].as_str())
        .map(str::to_string);
    let sticky_after = harness.read_sticky().await;
    let c1b = harness.counts();
    print_antigravity_http_observation(
        mode_label,
        "step1b-native-stream",
        Some(status),
        served_model.as_deref(),
        sticky_before.as_deref(),
        sticky_after.as_deref(),
        c1b,
    );
    println!(
        "[antigravity-account-model-http][{mode_label}] step1b-native-stream content_type={content_type} sse_frames={} stop_frames={}",
        frames.len(),
        usize::from(stop_frame.is_some())
    );
    assert_eq!(
        (c1b.unsupported_a, c1b.unsupported_b),
        (0, 0),
        "{mode_label}/step1b: native stream must not generate on an unsupported account"
    );
    assert_eq!(
        (c1b.gen_a, c1b.gen_b),
        (0, 2),
        "{mode_label}/step1b: the eligible sibling must serve the native stream"
    );
    assert_eq!(
        status,
        StatusCode::OK,
        "{mode_label}/step1b: native stream must complete with HTTP 200"
    );
    assert!(
        content_type.contains("text/event-stream"),
        "{mode_label}/step1b: alt=sse must return native SSE, got content-type '{content_type}'"
    );
    let stop_frame = stop_frame.expect("stream response should carry a STOP frame");
    assert_eq!(
        stop_frame["candidates"][0]["content"]["parts"][0]["text"].as_str(),
        Some(expected_text),
        "{mode_label}/step1b: stream content text mismatch"
    );
    assert_eq!(
        stop_frame["modelVersion"].as_str(),
        Some(provider_model),
        "{mode_label}/step1b: stream must carry the exact mapped upstream model"
    );
    if pool_enabled {
        assert_eq!(
            sticky_after.as_deref(),
            Some(key_b_id.as_str()),
            "{mode_label}/step1b: native stream should keep the Pool sticky binding on the sibling"
        );
    }

    // Step 2: discovery update. The bound sibling loses the target model while the Pro account
    // gains real high/medium entries. A stale sticky binding must not reach the sibling.
    {
        let mut guard = harness.mock_state.lock().expect("mock state should lock");
        guard.reset_rfc3339 = format_rfc3339_secs(current_unix_secs() + 3600);
        guard.account_a.models = BTreeMap::from([
            (
                provider_model.to_string(),
                ModelQuotaFixture::with_fraction("Gemini 4 Argon High", 0.8),
            ),
            (
                "gemini-4-argon-medium".to_string(),
                ModelQuotaFixture::with_fraction("Gemini 4 Argon Medium", 0.9),
            ),
        ]);
        guard.account_b.models = BTreeMap::from([(
            "gemini-4-argon-medium".to_string(),
            ModelQuotaFixture::with_fraction("Gemini 4 Argon Medium", 0.9),
        )]);
    }
    let refresh = harness.send_refresh_quota(None).await;
    assert_eq!(
        refresh["success"],
        json!(2),
        "{mode_label}/step2: refresh should succeed"
    );
    assert_eq!(
        refresh["failed"],
        json!(0),
        "{mode_label}/step2: refresh should fail none"
    );
    {
        let c = harness.counts();
        assert_eq!(
            (c.gen_a, c.gen_b),
            (0, 2),
            "{mode_label}/step2: discovery refresh must not generate upstream"
        );
    }

    let sticky_before = harness.read_sticky().await;
    let response = harness
        .send_generate("step2-sticky-sibling-loses-target")
        .await;
    let status = response.status();
    let body = if status == StatusCode::OK {
        Some(
            response
                .json::<serde_json::Value>()
                .await
                .expect("gemini response JSON should parse"),
        )
    } else {
        None
    };
    let served_model = body
        .as_ref()
        .and_then(|body| body["modelVersion"].as_str())
        .map(str::to_string);
    let sticky_after = harness.read_sticky().await;
    let c2 = harness.counts();
    print_antigravity_http_observation(
        mode_label,
        "step2-sticky-sibling-loses-target",
        Some(status),
        served_model.as_deref(),
        sticky_before.as_deref(),
        sticky_after.as_deref(),
        c2,
    );
    assert_eq!(
        (c2.unsupported_a, c2.unsupported_b),
        (0, 0),
        "{mode_label}/step2: stale sticky must not send an upstream generation to its unsupported account"
    );
    assert_eq!(
        (c2.gen_a, c2.gen_b),
        (1, 2),
        "{mode_label}/step2: the newly eligible account must serve and the stale binding must not"
    );
    assert_eq!(
        status,
        StatusCode::OK,
        "{mode_label}/step2: client must receive a native Gemini completion"
    );
    assert_gemini_response_body(
        body.as_ref()
            .expect("200 response should carry a Gemini body"),
        mode_label,
        "step2-sticky-sibling-loses-target",
        provider_model,
        expected_response_id,
        expected_text,
    );
    assert_eq!(
        served_model.as_deref(),
        Some(provider_model),
        "{mode_label}/step2: fallback account must serve the exact mapped upstream model"
    );
    if pool_enabled {
        assert_eq!(
            sticky_after.as_deref(),
            Some(key_a_id.as_str()),
            "{mode_label}/step2: serving fallback should rebind the Pool sticky session"
        );
    }

    // Step 3: every account loses the target after another discovery update. No upstream
    // generation may be attempted and the client must see an all-unavailable 503.
    {
        let mut guard = harness.mock_state.lock().expect("mock state should lock");
        guard.reset_rfc3339 = format_rfc3339_secs(current_unix_secs() + 3600);
        guard.account_a.models = BTreeMap::from([(
            "gemini-4-argon-disabled".to_string(),
            ModelQuotaFixture::display_only("Gemini 4 Argon (retired placeholder)"),
        )]);
        guard.account_b.models = BTreeMap::from([(
            "gemini-4-argon-medium".to_string(),
            ModelQuotaFixture::with_fraction("Gemini 4 Argon Medium", 0.9),
        )]);
    }
    let refresh = harness.send_refresh_quota(None).await;
    assert_eq!(
        refresh["success"],
        json!(2),
        "{mode_label}/step3: refresh should succeed"
    );
    assert_eq!(
        refresh["failed"],
        json!(0),
        "{mode_label}/step3: refresh should fail none"
    );

    let sticky_before = harness.read_sticky().await;
    let response = harness.send_generate("step3-all-unsupported").await;
    let status = response.status();
    let sticky_after = harness.read_sticky().await;
    let c3 = harness.counts();
    print_antigravity_http_observation(
        mode_label,
        "step3-all-unsupported",
        Some(status),
        None,
        sticky_before.as_deref(),
        sticky_after.as_deref(),
        c3,
    );
    assert_eq!(
        (c3.unsupported_a, c3.unsupported_b),
        (0, 0),
        "{mode_label}/step3: no unsupported upstream generation may be attempted"
    );
    assert_eq!(
        (c3.gen_a, c3.gen_b),
        (1, 2),
        "{mode_label}/step3: all-unsupported accounts must not receive upstream generation"
    );
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{mode_label}/step3: all-unsupported accounts must produce 503"
    );
}

fn assert_runtime_quota_account_isolation_delta(
    before: (usize, usize, usize, usize),
    after: (usize, usize, usize, usize),
    expected_delta: (usize, usize, usize, usize),
    mode_label: &str,
    step: &str,
    expectation: &str,
) {
    let delta = (
        after.0.saturating_sub(before.0),
        after.1.saturating_sub(before.1),
        after.2.saturating_sub(before.2),
        after.3.saturating_sub(before.3),
    );
    assert_eq!(
        delta, expected_delta,
        "{mode_label}/{step}: (account_a_primary, account_a_secondary, account_b_primary, account_b_secondary) delta {expectation}"
    );
}

async fn drain_antigravity_response_status(response: reqwest::Response) -> StatusCode {
    let status = response.status();
    let _ = response.text().await;
    status
}

fn antigravity_response_body_prefix(body: &[u8]) -> String {
    const MAX_PREFIX_BYTES: usize = 400;
    let end = body.len().min(MAX_PREFIX_BYTES);
    String::from_utf8_lossy(&body[..end]).to_string()
}

async fn assert_gemini_model_completion(
    response: reqwest::Response,
    mode_label: &str,
    step: &str,
    expected_model: &str,
) -> serde_json::Value {
    let status = response.status();
    let content_type = response
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("<missing>")
        .to_string();
    let body_bytes = response
        .bytes()
        .await
        .expect("gemini response body should read");
    let body_prefix = antigravity_response_body_prefix(&body_bytes);
    assert_eq!(
        status,
        StatusCode::OK,
        "{mode_label}/{step}: expected HTTP 200, got {status} content_type={content_type} body={body_prefix:?}"
    );
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_else(|err| {
        panic!(
            "{mode_label}/{step}: gemini response JSON should parse: {err}; content_type={content_type} body={body_prefix:?}"
        )
    });
    assert_eq!(
        body.pointer("/candidates/0/finishReason")
            .and_then(serde_json::Value::as_str),
        Some("STOP"),
        "{mode_label}/{step}: completion should finish with STOP"
    );
    assert_eq!(
        body.get("modelVersion").and_then(serde_json::Value::as_str),
        Some(expected_model),
        "{mode_label}/{step}: completion must carry the exact upstream model"
    );
    body
}

async fn assert_gemini_stream_model_completion(
    response: reqwest::Response,
    mode_label: &str,
    step: &str,
    expected_model: &str,
    expected_text: &str,
) {
    let status = response.status();
    let content_type = response
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body_text = response
        .text()
        .await
        .expect("gemini stream response body should read");
    assert_eq!(
        status,
        StatusCode::OK,
        "{mode_label}/{step}: native stream must complete with HTTP 200"
    );
    assert!(
        content_type.contains("text/event-stream"),
        "{mode_label}/{step}: alt=sse must return native SSE, got content-type '{content_type}'"
    );
    let frames = gemini_sse_data_frames(&body_text);
    let stop_frame = frames
        .iter()
        .find(|frame| {
            frame
                .pointer("/candidates/0/finishReason")
                .and_then(serde_json::Value::as_str)
                == Some("STOP")
        })
        .unwrap_or_else(|| panic!("{mode_label}/{step}: stream should carry a STOP frame"));
    assert_eq!(
        stop_frame
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(serde_json::Value::as_str),
        Some(expected_text),
        "{mode_label}/{step}: stream completion text mismatch"
    );
    assert_eq!(
        stop_frame
            .get("modelVersion")
            .and_then(serde_json::Value::as_str),
        Some(expected_model),
        "{mode_label}/{step}: stream must carry the exact upstream model"
    );
    println!(
        "[antigravity-runtime-quota-http][{mode_label}] {step} sse_frames={} stop_model={}",
        frames.len(),
        expected_model
    );
}

fn assert_no_account_wide_penalty(
    snapshot: &AntigravityAccountPenaltySnapshot,
    mode_label: &str,
    step: &str,
) {
    assert!(
        snapshot.cooldown_reason.is_none(),
        "{mode_label}/{step}: a scoped model quota failure must not set an account-wide pool cooldown, got {:?}",
        snapshot.cooldown_reason
    );
    assert!(
        snapshot
            .pool_hard_states
            .iter()
            .all(|state| state.schedulable()),
        "{mode_label}/{step}: a scoped model quota failure must not mark the account Pool member unschedulable, got {:?}",
        snapshot.pool_hard_states
    );
    assert_eq!(
        snapshot.health_consecutive_failures, 0,
        "{mode_label}/{step}: a scoped model quota failure must not record a shared health failure"
    );
    assert_eq!(
        snapshot.adaptive_rpm_429_count, 0,
        "{mode_label}/{step}: a scoped model quota failure must not reduce adaptive RPM limits"
    );
    assert!(
        snapshot.adaptive_last_429_at_unix_secs.is_none(),
        "{mode_label}/{step}: a scoped model quota failure must not stamp an account-wide 429"
    );
    assert!(
        snapshot.adaptive_last_429_type.is_none(),
        "{mode_label}/{step}: a scoped model quota failure must not record an account-wide 429 type, got {:?}",
        snapshot.adaptive_last_429_type
    );
    assert!(
        !matches!(
            snapshot.scheduling_code.as_deref(),
            Some("quota_exhausted") | Some("quota_suspected")
        ),
        "{mode_label}/{step}: a scoped model quota failure must not write an account-wide scheduling block, got {:?}",
        snapshot.scheduling_code
    );
    assert!(
        snapshot.oauth_invalid_at_unix_secs.is_none(),
        "{mode_label}/{step}: a scoped model quota failure must not invalidate the account credential"
    );
}

fn print_antigravity_runtime_quota_observation(
    mode_label: &str,
    step: &str,
    status: Option<StatusCode>,
    counts: (usize, usize, usize, usize),
    penalty: &AntigravityAccountPenaltySnapshot,
) {
    println!(
        "[antigravity-runtime-quota-http][{mode_label}] {step} status={} gen_a_primary={} gen_a_secondary={} gen_b_primary={} gen_b_secondary={} cooldown={} pool_hard_states={:?} health_failures={} rpm_429_count={} last_429_type={} scheduling={}",
        status.map_or_else(|| "none".to_string(), |status| status.as_u16().to_string()),
        counts.0,
        counts.1,
        counts.2,
        counts.3,
        penalty
            .cooldown_reason
            .as_deref()
            .unwrap_or("<none>"),
        penalty.pool_hard_states,
        penalty.health_consecutive_failures,
        penalty.adaptive_rpm_429_count,
        penalty.adaptive_last_429_type.as_deref().unwrap_or("<none>"),
        penalty.scheduling_code.as_deref().unwrap_or("<none>"),
    );
}

fn antigravity_runtime_quota_models(
    target_remaining_fraction: f64,
) -> BTreeMap<String, ModelQuotaFixture> {
    BTreeMap::from([
        (
            "gemini-3-pro-high".to_string(),
            ModelQuotaFixture::with_fraction("Gemini 3 Pro High", target_remaining_fraction),
        ),
        (
            "claude-sonnet-4-5".to_string(),
            ModelQuotaFixture::with_fraction("Claude Sonnet 4.5", target_remaining_fraction),
        ),
    ])
}

fn antigravity_runtime_quota_secondary_only() -> BTreeMap<String, ModelQuotaFixture> {
    BTreeMap::from([(
        "gemini-3-pro-high".to_string(),
        ModelQuotaFixture::with_fraction("Gemini 3 Pro High", 0.9),
    )])
}

const ANTIGRAVITY_RUNTIME_PRIMARY_MODEL: &str = "gemini-3-pro-high";
const ANTIGRAVITY_RUNTIME_SECONDARY_MODEL: &str = "claude-sonnet-4-5";
const ANTIGRAVITY_RUNTIME_PRIMARY_GLOBAL_MODEL: &str = "gemini-3-pro-preview";
const ANTIGRAVITY_RUNTIME_SECONDARY_GLOBAL_MODEL: &str = "gemini-claude-sonnet-4-5";
const ANTIGRAVITY_RUNTIME_COMPLETION_TEXT: &str = "Runtime quota isolation";

fn antigravity_runtime_quota_http_config(
    mode_label: &str,
    pool_enabled: bool,
) -> AntigravityHttpConfig {
    AntigravityHttpConfig {
        mode_label: mode_label.to_string(),
        pool_enabled,
        provider_id: format!("provider-antigravity-runtime-quota-{mode_label}"),
        endpoint_id: format!("endpoint-antigravity-runtime-quota-{mode_label}"),
        model_id: format!("model-antigravity-runtime-quota-{mode_label}"),
        global_model_name: ANTIGRAVITY_RUNTIME_PRIMARY_GLOBAL_MODEL.to_string(),
        provider_model_name: ANTIGRAVITY_RUNTIME_PRIMARY_MODEL.to_string(),
        secondary_model_id: Some(format!(
            "model-antigravity-runtime-quota-secondary-{mode_label}"
        )),
        secondary_global_model_name: Some(ANTIGRAVITY_RUNTIME_SECONDARY_GLOBAL_MODEL.to_string()),
        secondary_provider_model_name: Some(ANTIGRAVITY_RUNTIME_SECONDARY_MODEL.to_string()),
        generation_response_id: "resp_antigravity_runtime_quota".to_string(),
        generation_text: ANTIGRAVITY_RUNTIME_COMPLETION_TEXT.to_string(),
        reset_rfc3339: format_rfc3339_secs(current_unix_secs() + 3600),
        key_a: AccountFixtureSpec {
            key_id: format!("key-antigravity-runtime-quota-a-{mode_label}"),
            key_name: "account-a".to_string(),
            priority: 1,
            access_token: "ya29.fake-antigravity-runtime-quota-token-a".to_string(),
            refresh_token: "rt-fake-antigravity-runtime-quota-a".to_string(),
            project_id: "project-antigravity-runtime-quota-a".to_string(),
            session_id: "sess-antigravity-runtime-quota-a".to_string(),
            subscription_tier: "pro".to_string(),
            allowed_models: None,
            auto_fetch_models: false,
            models: antigravity_runtime_quota_models(0.9),
        },
        key_b: AccountFixtureSpec {
            key_id: format!("key-antigravity-runtime-quota-b-{mode_label}"),
            key_name: "account-b".to_string(),
            priority: 2,
            access_token: "ya29.fake-antigravity-runtime-quota-token-b".to_string(),
            refresh_token: "rt-fake-antigravity-runtime-quota-b".to_string(),
            project_id: "project-antigravity-runtime-quota-b".to_string(),
            session_id: "sess-antigravity-runtime-quota-b".to_string(),
            subscription_tier: "pro".to_string(),
            allowed_models: None,
            auto_fetch_models: false,
            models: antigravity_runtime_quota_secondary_only(),
        },
    }
}

#[test]
fn gateway_antigravity_runtime_quota_isolation() {
    run_gemini_cli_sync_test(
        "gateway_antigravity_runtime_quota_isolation",
        gateway_antigravity_runtime_quota_isolation_impl,
    );
}

async fn gateway_antigravity_runtime_quota_isolation_impl() {
    run_antigravity_runtime_quota_isolation_mode(false).await;
    run_antigravity_runtime_quota_isolation_mode(true).await;
}

/// 真实上游 `RESOURCE_EXHAUSTED`（RetryInfo/Retry-After + 明确模型身份）必须只约束
/// 实际服务的账号+上游模型，不能阻断同账号的另一个模型，也不能降低账号级健康、
/// 自适应 RPM、Pool 分数或写入账号冷却。
async fn run_antigravity_runtime_quota_isolation_mode(pool_enabled: bool) {
    let mode_label = if pool_enabled { "pool" } else { "ordinary" };
    let primary_model = ANTIGRAVITY_RUNTIME_PRIMARY_MODEL;
    let secondary_model = ANTIGRAVITY_RUNTIME_SECONDARY_MODEL;
    let primary_global_model = ANTIGRAVITY_RUNTIME_PRIMARY_GLOBAL_MODEL;
    let secondary_global_model = ANTIGRAVITY_RUNTIME_SECONDARY_GLOBAL_MODEL;

    let harness = start_antigravity_http_harness(antigravity_runtime_quota_http_config(
        mode_label,
        pool_enabled,
    ))
    .await;

    let key_a_id = harness.key_a_id.clone();
    harness.materialize_pool_scores().await;
    let counts = || harness.model_generation_counts(primary_model, secondary_model);
    let print_step = |step: &str,
                      status: Option<StatusCode>,
                      penalty: &AntigravityAccountPenaltySnapshot| {
        print_antigravity_runtime_quota_observation(mode_label, step, status, counts(), penalty);
    };

    let mut baseline = counts();
    assert_eq!(
        baseline,
        (0, 0, 0, 0),
        "{mode_label}: generation counters should start empty"
    );

    // Step 1: both models warm on the preferred account A.
    assert_gemini_model_completion(
        harness
            .send_generate_model(primary_global_model, "step1-primary-warmup")
            .await,
        mode_label,
        "step1-primary-warmup",
        primary_model,
    )
    .await;
    let after = counts();
    assert_runtime_quota_account_isolation_delta(
        baseline,
        after,
        (1, 0, 0, 0),
        mode_label,
        "step1-primary-warmup",
        "must be served once by account A",
    );
    baseline = after;
    assert_gemini_model_completion(
        harness
            .send_generate_model(secondary_global_model, "step1-secondary-warmup")
            .await,
        mode_label,
        "step1-secondary-warmup",
        secondary_model,
    )
    .await;
    let after = counts();
    assert_runtime_quota_account_isolation_delta(
        baseline,
        after,
        (0, 1, 0, 0),
        mode_label,
        "step1-secondary-warmup",
        "must be served once by account A",
    );
    baseline = after;
    let penalty = harness.read_penalty_snapshot(&key_a_id).await;
    print_step("step1-warmup", None, &penalty);
    assert_no_account_wide_penalty(&penalty, mode_label, "step1-warmup");

    // Step 2: reverse direction — a real Claude quota 429 on A must not block Gemini on A.
    harness.set_generation_quota_error(true, secondary_model, 30);
    let response = harness
        .send_generate_model(secondary_global_model, "step2-secondary-quota-429")
        .await;
    let status = drain_antigravity_response_status(response).await;
    let penalty = harness.read_penalty_snapshot(&key_a_id).await;
    print_step("step2-secondary-quota-429", Some(status), &penalty);
    let after = counts();
    assert!(
        after.1 > baseline.1,
        "{mode_label}/step2-secondary-quota-429: account A must attempt the exhausted upstream model"
    );
    assert_eq!(
        (after.0, after.2, after.3),
        (baseline.0, baseline.2, baseline.3),
        "{mode_label}/step2-secondary-quota-429: the exhausted secondary model must not reach another account"
    );
    baseline = after;
    assert_no_account_wide_penalty(&penalty, mode_label, "step2-secondary-quota-429");

    assert_gemini_model_completion(
        harness
            .send_generate_model(primary_global_model, "step2-primary-still-served")
            .await,
        mode_label,
        "step2-primary-still-served",
        primary_model,
    )
    .await;
    let after = counts();
    assert_runtime_quota_account_isolation_delta(
        baseline,
        after,
        (1, 0, 0, 0),
        mode_label,
        "step2-primary-still-served",
        "must remain immediately schedulable on account A after the secondary model quota error",
    );
    baseline = after;

    // The scoped fact must already be visible to the native stream path without sleeping.
    let response = harness
        .send_stream_generate_model(secondary_global_model, "step2-secondary-stream-blocked")
        .await;
    let status = drain_antigravity_response_status(response).await;
    let penalty = harness.read_penalty_snapshot(&key_a_id).await;
    print_step("step2-secondary-stream-blocked", Some(status), &penalty);
    let after = counts();
    assert_runtime_quota_account_isolation_delta(
        baseline,
        after,
        (0, 0, 0, 0),
        mode_label,
        "step2-secondary-stream-blocked",
        "must not repeat the exhausted model on account A in the native stream path",
    );
    baseline = after;
    assert_no_account_wide_penalty(&penalty, mode_label, "step2-secondary-stream-blocked");

    // Step 3: a positive persisted quota refresh restores only the affected scoped model.
    harness.clear_generation_quota_error(true, secondary_model);
    harness.set_quota_reset_rfc3339(format_rfc3339_secs(current_unix_secs() + 7200));
    harness.set_account_models(true, antigravity_runtime_quota_models(0.7));
    harness.set_account_models(false, antigravity_runtime_quota_secondary_only());
    let refresh = harness.send_refresh_quota(None).await;
    assert_eq!(
        refresh["success"],
        json!(2),
        "{mode_label}/step3: refresh should succeed"
    );
    assert_eq!(
        refresh["failed"],
        json!(0),
        "{mode_label}/step3: refresh should fail none"
    );
    assert_gemini_model_completion(
        harness
            .send_generate_model(secondary_global_model, "step3-secondary-refreshed")
            .await,
        mode_label,
        "step3-secondary-refreshed",
        secondary_model,
    )
    .await;
    let after = counts();
    assert_runtime_quota_account_isolation_delta(
        baseline,
        after,
        (0, 1, 0, 0),
        mode_label,
        "step3-secondary-refreshed",
        "positive persisted refresh must restore the scoped model on account A",
    );
    baseline = after;

    // Step 4: forward direction — a real Gemini quota 429 on A must not poison Claude on A.
    harness.set_generation_quota_error(true, primary_model, 45);
    assert_gemini_model_completion(
        harness
            .send_generate_model(primary_global_model, "step4-primary-quota-429")
            .await,
        mode_label,
        "step4-primary-quota-429",
        primary_model,
    )
    .await;
    let penalty = harness.read_penalty_snapshot(&key_a_id).await;
    print_step("step4-primary-quota-429", Some(StatusCode::OK), &penalty);
    let after = counts();
    assert!(
        after.0 > baseline.0,
        "{mode_label}/step4-primary-quota-429: account A must attempt the exhausted upstream model"
    );
    assert_eq!(
        (after.1, after.3),
        (baseline.1, baseline.3),
        "{mode_label}/step4-primary-quota-429: unrelated account/model counters must stay unchanged"
    );
    assert_eq!(
        after.2,
        baseline.2 + 1,
        "{mode_label}/step4-primary-quota-429: account B must serve the same upstream model exactly once"
    );
    baseline = after;
    assert_no_account_wide_penalty(&penalty, mode_label, "step4-primary-quota-429");

    // Step 5: the next affected-model request must skip account A without sleeping.
    assert_gemini_model_completion(
        harness
            .send_generate_model(primary_global_model, "step5-primary-skip-exhausted")
            .await,
        mode_label,
        "step5-primary-skip-exhausted",
        primary_model,
    )
    .await;
    let after = counts();
    assert_runtime_quota_account_isolation_delta(
        baseline,
        after,
        (0, 0, 1, 0),
        mode_label,
        "step5-primary-skip-exhausted",
        "the exhausted account+model must not receive a repeated upstream generation",
    );
    baseline = after;
    assert_gemini_stream_model_completion(
        harness
            .send_stream_generate_model(primary_global_model, "step5-primary-stream-skip")
            .await,
        mode_label,
        "step5-primary-stream-skip",
        primary_model,
        ANTIGRAVITY_RUNTIME_COMPLETION_TEXT,
    )
    .await;
    let after = counts();
    assert_runtime_quota_account_isolation_delta(
        baseline,
        after,
        (0, 0, 1, 0),
        mode_label,
        "step5-primary-stream-skip",
        "the native stream path must consume the same scoped fact",
    );
    baseline = after;

    // Step 6: the unrelated Claude model on account A must still complete immediately.
    assert_gemini_model_completion(
        harness
            .send_generate_model(
                secondary_global_model,
                "step6-secondary-after-primary-quota",
            )
            .await,
        mode_label,
        "step6-secondary-after-primary-quota",
        secondary_model,
    )
    .await;
    let after = counts();
    assert_runtime_quota_account_isolation_delta(
        baseline,
        after,
        (0, 1, 0, 0),
        mode_label,
        "step6-secondary-after-primary-quota",
        "an unrelated model quota on account A must remain immediately schedulable",
    );
    baseline = after;
    assert_gemini_stream_model_completion(
        harness
            .send_stream_generate_model(
                secondary_global_model,
                "step6-secondary-stream-after-primary-quota",
            )
            .await,
        mode_label,
        "step6-secondary-stream-after-primary-quota",
        secondary_model,
        ANTIGRAVITY_RUNTIME_COMPLETION_TEXT,
    )
    .await;
    let after = counts();
    assert_runtime_quota_account_isolation_delta(
        baseline,
        after,
        (0, 1, 0, 0),
        mode_label,
        "step6-secondary-stream-after-primary-quota",
        "the native stream path must serve the unrelated model on account A",
    );
    baseline = after;
    let penalty = harness.read_penalty_snapshot(&key_a_id).await;
    print_step("step6-secondary-served", Some(StatusCode::OK), &penalty);
    assert_no_account_wide_penalty(&penalty, mode_label, "step6-secondary-served");

    // Step 7: a failed refresh (loadCodeAssist 200 + fetchAvailableModels 502) must not clear
    // the active scoped model restriction nor affect unrelated model serving.
    harness.set_account_fetch_models_status(true, 502);
    let failed_refresh = harness.send_refresh_quota(None).await;
    assert_eq!(
        failed_refresh["success"],
        json!(1),
        "{mode_label}/step7: only the healthy account may refresh"
    );
    assert_eq!(
        failed_refresh["failed"],
        json!(1),
        "{mode_label}/step7: account A refresh must fail"
    );
    assert_gemini_model_completion(
        harness
            .send_generate_model(primary_global_model, "step7-primary-after-failed-refresh")
            .await,
        mode_label,
        "step7-primary-after-failed-refresh",
        primary_model,
    )
    .await;
    let after = counts();
    assert_runtime_quota_account_isolation_delta(
        baseline,
        after,
        (0, 0, 1, 0),
        mode_label,
        "step7-primary-after-failed-refresh",
        "a failed refresh must retain the active scoped exhaustion",
    );
    baseline = after;
    assert_gemini_model_completion(
        harness
            .send_generate_model(
                secondary_global_model,
                "step7-secondary-after-failed-refresh",
            )
            .await,
        mode_label,
        "step7-secondary-after-failed-refresh",
        secondary_model,
    )
    .await;
    let after = counts();
    assert_runtime_quota_account_isolation_delta(
        baseline,
        after,
        (0, 1, 0, 0),
        mode_label,
        "step7-secondary-after-failed-refresh",
        "a failed refresh must not disturb unrelated model quotas",
    );
    baseline = after;
    let penalty = harness.read_penalty_snapshot(&key_a_id).await;
    print_step("step7-failed-refresh", None, &penalty);
    assert_no_account_wide_penalty(&penalty, mode_label, "step7-failed-refresh");

    let totals = harness.counts();
    assert_eq!(
        (totals.unsupported_a, totals.unsupported_b),
        (0, 0),
        "{mode_label}: no upstream generation may target an account whose discovered catalog lacks the model"
    );

    println!(
        "[antigravity-runtime-quota-http][{mode_label}] final gen_a_primary={} gen_a_secondary={} gen_b_primary={} gen_b_secondary={}",
        baseline.0, baseline.1, baseline.2, baseline.3
    );
}

/// 显式 `RATE_LIMIT_EXCEEDED` / per-minute 429 是真实账号限流而不是可重置的模型
/// 额度：既有的账号级冷却、Pool 硬状态、健康与自适应惩罚必须保持，不能被模型
/// 额度隔离逻辑顺带吞掉。
#[test]
fn gateway_antigravity_generic_rate_limit_remains_account_scoped() {
    run_gemini_cli_sync_test(
        "gateway_antigravity_generic_rate_limit_remains_account_scoped",
        gateway_antigravity_generic_rate_limit_remains_account_scoped_impl,
    );
}

async fn gateway_antigravity_generic_rate_limit_remains_account_scoped_impl() {
    let mode_label = "pool";
    let primary_model = ANTIGRAVITY_RUNTIME_PRIMARY_MODEL;
    let secondary_model = ANTIGRAVITY_RUNTIME_SECONDARY_MODEL;
    let primary_global_model = ANTIGRAVITY_RUNTIME_PRIMARY_GLOBAL_MODEL;
    let secondary_global_model = ANTIGRAVITY_RUNTIME_SECONDARY_GLOBAL_MODEL;
    let harness =
        start_antigravity_http_harness(antigravity_runtime_quota_http_config(mode_label, true))
            .await;
    let key_a_id = harness.key_a_id.clone();
    harness.materialize_pool_scores().await;
    let counts = || harness.model_generation_counts(primary_model, secondary_model);

    assert_gemini_model_completion(
        harness
            .send_generate_model(primary_global_model, "step1-primary-warmup")
            .await,
        mode_label,
        "step1-primary-warmup",
        primary_model,
    )
    .await;
    assert_gemini_model_completion(
        harness
            .send_generate_model(secondary_global_model, "step1-secondary-warmup")
            .await,
        mode_label,
        "step1-secondary-warmup",
        secondary_model,
    )
    .await;
    let baseline = counts();
    assert_eq!(
        baseline,
        (1, 1, 0, 0),
        "{mode_label}: both models should warm on account A"
    );

    harness.set_generation_rate_limit_error(true, primary_model, 20);
    assert_gemini_model_completion(
        harness
            .send_generate_model(primary_global_model, "step2-account-rate-limit")
            .await,
        mode_label,
        "step2-account-rate-limit",
        primary_model,
    )
    .await;
    let after = counts();
    assert!(
        after.0 > baseline.0,
        "{mode_label}/step2-account-rate-limit: account A must attempt the rate-limited upstream model"
    );
    assert_eq!(
        (after.1, after.3),
        (baseline.1, baseline.3),
        "{mode_label}/step2-account-rate-limit: unrelated counters must stay unchanged"
    );
    assert_eq!(
        after.2,
        baseline.2 + 1,
        "{mode_label}/step2-account-rate-limit: account B must serve the same upstream model exactly once"
    );

    let penalty = harness.read_penalty_snapshot(&key_a_id).await;
    print_antigravity_runtime_quota_observation(
        mode_label,
        "step2-account-rate-limit",
        Some(StatusCode::OK),
        after,
        &penalty,
    );
    assert_eq!(
        penalty.cooldown_reason.as_deref(),
        Some("rate_limited_429"),
        "{mode_label}/step2-account-rate-limit: a genuine rate limit must keep the account-wide transient cooldown"
    );
    assert!(
        penalty
            .pool_hard_states
            .iter()
            .any(|state| *state == PoolMemberHardState::Cooldown),
        "{mode_label}/step2-account-rate-limit: a genuine rate limit must keep the account Pool member in Cooldown, got {:?}",
        penalty.pool_hard_states
    );
    assert!(
        !penalty
            .pool_hard_states
            .contains(&PoolMemberHardState::QuotaExhausted),
        "{mode_label}/step2-account-rate-limit: a rate limit must not be projected as model quota exhaustion"
    );
    assert!(
        penalty.health_consecutive_failures > 0,
        "{mode_label}/step2-account-rate-limit: a genuine rate limit must keep the shared health failure"
    );
    assert!(
        penalty.adaptive_rpm_429_count > 0,
        "{mode_label}/step2-account-rate-limit: a genuine rate limit must keep adaptive RPM feedback"
    );
    assert_eq!(
        penalty.adaptive_last_429_type.as_deref(),
        Some("rpm"),
        "{mode_label}/step2-account-rate-limit: adaptive feedback must record the upstream 429 type"
    );

    let response = harness
        .send_generate_model(secondary_global_model, "step3-secondary-account-blocked")
        .await;
    let status = drain_antigravity_response_status(response).await;
    let final_counts = counts();
    println!(
        "[antigravity-runtime-quota-http][{mode_label}] step3-secondary-account-blocked status={} gen_a_primary={} gen_a_secondary={} gen_b_primary={} gen_b_secondary={}",
        status.as_u16(),
        final_counts.0,
        final_counts.1,
        final_counts.2,
        final_counts.3
    );
    assert_eq!(
        final_counts, after,
        "{mode_label}/step3-secondary-account-blocked: the account-wide rate limit must still block the unrelated model on account A"
    );
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{mode_label}/step3-secondary-account-blocked: no eligible account may serve the unrelated model under an account-wide rate limit"
    );
}
