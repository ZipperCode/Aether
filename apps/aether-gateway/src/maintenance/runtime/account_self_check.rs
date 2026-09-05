use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aether_data_contracts::repository::pool_scores::{
    ListPoolMemberScoresQuery, PoolMemberHardState, PoolMemberIdentity, PoolMemberProbeAttempt,
    PoolMemberProbeResult, PoolMemberProbeStatus, POOL_KIND_PROVIDER_KEY_POOL,
};
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use aether_provider_pool::{provider_pool_key_runtime_quota_blocked, ProviderQuotaServingPolicy};
use aether_runtime_state::{RuntimeLockLease, RuntimeState};
use futures_util::{stream, StreamExt};
use serde_json::{json, Value};
use tracing::{debug, info, warn};

use crate::admin_api::{
    admin_provider_pool_config, provider_quota_refresh_endpoint_for_provider,
    provider_quota_serving_policy, provider_type_supports_quota_refresh,
    refresh_provider_pool_quota_locally, AdminAppState, QuotaRefreshSource,
};
use crate::handlers::shared::provider_pool::{
    admin_provider_pool_config_from_config_value, AdminProviderPoolConfig,
};
use crate::{AppState, GatewayError};

use super::{shared_auth_maintenance_gate, AuthMaintenanceGate};

const ACCOUNT_SELF_CHECK_REDIS_PREFIX: &str = "ap:account_self_check:last";
const ACCOUNT_SELF_CHECK_LOCK_TTL_MS: u64 = 30_000;
const ACCOUNT_SELF_CHECK_DEFAULT_SCAN_INTERVAL_SECONDS: u64 = 60;
const ACCOUNT_SELF_CHECK_MIN_SCAN_INTERVAL_SECONDS: u64 = 15;
const ACCOUNT_SELF_CHECK_DEFAULT_MAX_KEYS_PER_PROVIDER: usize = 200;
const ACCOUNT_SELF_CHECK_DEFAULT_GLOBAL_CONCURRENCY: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) struct AccountSelfCheckRunSummary {
    pub(crate) providers_checked: usize,
    pub(crate) providers_checked_with_keys: usize,
    pub(crate) providers_skipped: usize,
    pub(crate) scanned_keys: usize,
    pub(crate) selected_keys: usize,
    pub(crate) succeeded: usize,
    pub(crate) blocked: usize,
    pub(crate) failed: usize,
    pub(crate) skipped: usize,
    pub(crate) auto_removed: usize,
}

impl AccountSelfCheckRunSummary {
    const fn empty() -> Self {
        Self {
            providers_checked: 0,
            providers_checked_with_keys: 0,
            providers_skipped: 0,
            scanned_keys: 0,
            selected_keys: 0,
            succeeded: 0,
            blocked: 0,
            failed: 0,
            skipped: 0,
            auto_removed: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AccountSelfCheckWorkerConfig {
    pub(crate) scan_interval: Duration,
    pub(crate) max_keys_per_provider: usize,
    pub(crate) global_concurrency: usize,
}

impl AccountSelfCheckWorkerConfig {
    fn from_env() -> Self {
        let scan_interval_seconds = env_u64(
            "ACCOUNT_SELF_CHECK_SCAN_INTERVAL_SECONDS",
            ACCOUNT_SELF_CHECK_DEFAULT_SCAN_INTERVAL_SECONDS,
        )
        .max(ACCOUNT_SELF_CHECK_MIN_SCAN_INTERVAL_SECONDS);
        let max_keys_per_provider = env_usize(
            "ACCOUNT_SELF_CHECK_MAX_KEYS_PER_PROVIDER",
            ACCOUNT_SELF_CHECK_DEFAULT_MAX_KEYS_PER_PROVIDER,
        )
        .max(1);
        let global_concurrency = env_usize(
            "ACCOUNT_SELF_CHECK_GLOBAL_CONCURRENCY",
            ACCOUNT_SELF_CHECK_DEFAULT_GLOBAL_CONCURRENCY,
        )
        .clamp(1, 256);
        Self {
            scan_interval: Duration::from_secs(scan_interval_seconds),
            max_keys_per_provider,
            global_concurrency,
        }
    }
}

enum AccountSelfCheckOutcome {
    Success {
        status_code: Option<u16>,
        message: Option<String>,
        exhausted: bool,
    },
    Blocked {
        status_code: Option<u16>,
        message: String,
    },
    AutoRemoved {
        status_code: Option<u16>,
        message: String,
    },
    Failed {
        status_code: Option<u16>,
        message: String,
    },
    Skipped {
        message: String,
    },
}

impl AccountSelfCheckOutcome {
    fn score_status(&self) -> &'static str {
        match self {
            Self::Success { .. } => "success",
            Self::Blocked { .. } => "blocked",
            Self::AutoRemoved { .. } => "auto_removed",
            Self::Failed { .. } => "failed",
            Self::Skipped { .. } => "skipped",
        }
    }

    fn status_code(&self) -> Option<u16> {
        match self {
            Self::Success { status_code, .. }
            | Self::Blocked { status_code, .. }
            | Self::AutoRemoved { status_code, .. }
            | Self::Failed { status_code, .. } => *status_code,
            Self::Skipped { .. } => None,
        }
    }

    fn message(&self) -> Option<&str> {
        match self {
            Self::Success { message, .. } => message.as_deref(),
            Self::Blocked { message, .. }
            | Self::AutoRemoved { message, .. }
            | Self::Failed { message, .. }
            | Self::Skipped { message, .. } => Some(message.as_str()),
        }
    }
}

fn env_u64(name: &str, default_value: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default_value)
}

fn env_usize(name: &str, default_value: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(default_value)
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn parse_check_stamp(raw_value: Option<&str>) -> Option<u64> {
    let parsed = raw_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<f64>().ok())?;
    if parsed <= 0.0 {
        return None;
    }
    Some(parsed as u64)
}

fn check_stamp_key(provider_id: &str, key_id: &str) -> String {
    format!("{ACCOUNT_SELF_CHECK_REDIS_PREFIX}:{provider_id}:{key_id}")
}

async fn load_check_timestamps(
    runtime: &RuntimeState,
    provider_id: &str,
    key_ids: &[String],
) -> BTreeMap<String, u64> {
    if key_ids.is_empty() {
        return BTreeMap::new();
    }

    let runtime_keys = key_ids
        .iter()
        .map(|key_id| check_stamp_key(provider_id, key_id))
        .collect::<Vec<_>>();
    let Ok(values) = runtime.kv_get_many(&runtime_keys).await else {
        debug!("gateway account self-check: failed to read runtime check stamps");
        return BTreeMap::new();
    };

    key_ids
        .iter()
        .zip(values)
        .filter_map(|(key_id, raw)| {
            parse_check_stamp(raw.as_deref()).map(|ts| (key_id.clone(), ts))
        })
        .collect()
}

async fn mark_check_timestamps(
    runtime: &RuntimeState,
    provider_id: &str,
    key_ids: &[String],
    now_ts: u64,
    interval_seconds: u64,
) {
    if key_ids.is_empty() {
        return;
    }

    let ttl_seconds = interval_seconds.saturating_mul(2).max(120);
    let value = now_ts.to_string();
    for key_id in key_ids {
        if runtime
            .kv_set(
                &check_stamp_key(provider_id, key_id),
                value.clone(),
                Some(Duration::from_secs(ttl_seconds)),
            )
            .await
            .is_err()
        {
            debug!("gateway account self-check: failed to write runtime check stamp");
        }
    }
}

async fn acquire_provider_self_check_lock(
    runtime: &RuntimeState,
    provider_id: &str,
) -> Option<RuntimeLockLease> {
    let owner = format!("aether-gateway-account-self-check-{}", std::process::id());
    match runtime
        .lock_try_acquire(
            &format!("account_self_check:{provider_id}"),
            &owner,
            Duration::from_millis(ACCOUNT_SELF_CHECK_LOCK_TTL_MS),
        )
        .await
    {
        Ok(lease) => lease,
        Err(err) => {
            debug!(
                provider_id,
                error = %err,
                "gateway account self-check: failed to acquire runtime provider lock"
            );
            None
        }
    }
}

async fn release_provider_self_check_lock(runtime: &RuntimeState, lease: Option<RuntimeLockLease>) {
    let Some(lease) = lease else {
        return;
    };
    if let Err(err) = runtime.lock_release(&lease).await {
        debug!(
            error = %err,
            "gateway account self-check: failed to release runtime provider lock"
        );
    }
}

pub(crate) fn select_account_self_check_key_ids(
    key_ids: &[String],
    now_ts: u64,
    interval_seconds: u64,
    last_check_timestamps: &BTreeMap<String, u64>,
    limit: usize,
) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }

    let mut stale = key_ids
        .iter()
        .filter_map(|key_id| {
            let last_check_ts = last_check_timestamps.get(key_id).copied().unwrap_or(0);
            (last_check_ts == 0 || now_ts.saturating_sub(last_check_ts) >= interval_seconds)
                .then(|| (last_check_ts, key_id.clone()))
        })
        .collect::<Vec<_>>();
    stale.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    stale
        .into_iter()
        .take(limit)
        .map(|(_, key_id)| key_id)
        .collect()
}

/// 在 Provider 分布式锁内选出到期的轻量 Key ID，不提前读取或保留完整凭据。
async fn select_key_ids_for_provider(
    state: &AppState,
    runtime: &RuntimeState,
    provider: &StoredProviderCatalogProvider,
    interval_seconds: u64,
    max_keys_per_provider: usize,
    now_ts: u64,
) -> Result<Vec<String>, GatewayError> {
    let lease = acquire_provider_self_check_lock(runtime, &provider.id).await;
    if lease.is_none() {
        return Ok(Vec::new());
    }

    let result = async {
        let candidates = state
            .list_provider_catalog_auth_maintenance_candidates_by_provider_ids(
                std::slice::from_ref(&provider.id),
            )
            .await?
            .into_iter()
            .filter(|candidate| candidate.is_active && candidate.provider_id == provider.id)
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let key_ids = candidates
            .iter()
            .map(|candidate| candidate.id.clone())
            .collect::<Vec<_>>();
        let check_stamps = load_check_timestamps(runtime, &provider.id, &key_ids).await;
        let selected_ids = select_account_self_check_key_ids(
            &key_ids,
            now_ts,
            interval_seconds,
            &check_stamps,
            max_keys_per_provider,
        );
        if selected_ids.is_empty() {
            return Ok(Vec::new());
        }

        mark_check_timestamps(
            runtime,
            &provider.id,
            &selected_ids,
            now_ts,
            interval_seconds,
        )
        .await;
        Ok(selected_ids)
    }
    .await;

    release_provider_self_check_lock(runtime, lease).await;
    result
}

fn quota_payload_result_for_key(key_id: &str, payload: Option<Value>) -> AccountSelfCheckOutcome {
    let Some(payload) = payload else {
        return AccountSelfCheckOutcome::Failed {
            status_code: None,
            message: "quota refresh returned no payload".to_string(),
        };
    };
    let Some(results) = payload.get("results").and_then(Value::as_array) else {
        return AccountSelfCheckOutcome::Failed {
            status_code: None,
            message: "quota refresh returned no result list".to_string(),
        };
    };
    let Some(item) = results.iter().find(|item| {
        item.get("key_id")
            .and_then(Value::as_str)
            .is_some_and(|value| value == key_id)
    }) else {
        return AccountSelfCheckOutcome::Failed {
            status_code: None,
            message: "quota refresh result missing key".to_string(),
        };
    };

    let status = item
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let status_code = item
        .get("status_code")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok());
    let message = item
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let auto_removed = item
        .get("auto_removed")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if status == "backoff" {
        return AccountSelfCheckOutcome::Skipped {
            message: "quota refresh deferred by backoff".to_string(),
        };
    }
    if status == "success" {
        return AccountSelfCheckOutcome::Success {
            status_code,
            message,
            exhausted: item
                .get("quota_snapshot")
                .and_then(|snapshot| snapshot.get("exhausted"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        };
    }
    if auto_removed {
        return AccountSelfCheckOutcome::AutoRemoved {
            status_code,
            message: message.unwrap_or_else(|| "已自动删除".to_string()),
        };
    }
    if quota_result_status_is_blocked(&status, status_code, message.as_deref()) {
        return AccountSelfCheckOutcome::Blocked {
            status_code,
            message: message.unwrap_or_else(|| status.clone()),
        };
    }
    AccountSelfCheckOutcome::Failed {
        status_code,
        message: message.unwrap_or_else(|| status.clone()),
    }
}

fn quota_result_status_is_blocked(
    status: &str,
    status_code: Option<u16>,
    message: Option<&str>,
) -> bool {
    matches!(
        status,
        "banned" | "forbidden" | "workspace_deactivated" | "auth_invalid"
    ) || matches!(status_code, Some(401 | 403 | 423))
        || aether_admin::provider::status::resolve_pool_account_state(None, None, message).blocked
}

async fn perform_quota_refresh_check(
    state: &AdminAppState<'_>,
    provider: &StoredProviderCatalogProvider,
    endpoint: &StoredProviderCatalogEndpoint,
    provider_type: &str,
    key: StoredProviderCatalogKey,
) -> Result<AccountSelfCheckOutcome, GatewayError> {
    let key_id = key.id.clone();
    let payload = refresh_provider_pool_quota_locally(
        state,
        provider,
        endpoint,
        provider_type,
        vec![key],
        None,
        QuotaRefreshSource::AccountSelfCheck,
    )
    .await?;
    Ok(quota_payload_result_for_key(&key_id, payload))
}

/// 在共享许可内强读取并复核单个 Key，执行额度/OAuth 自检并完成对应状态持久化。
async fn perform_account_self_check_candidate_under_gate(
    state: &AppState,
    admin_state: &AdminAppState<'_>,
    provider: &StoredProviderCatalogProvider,
    endpoint: &StoredProviderCatalogEndpoint,
    provider_type: &str,
    key_id: String,
    attempted_at: u64,
    serving_policy: ProviderQuotaServingPolicy,
    gate: AuthMaintenanceGate,
) -> Result<Option<AccountSelfCheckOutcome>, GatewayError> {
    let _permit = gate.acquire().await?;
    let Some(key) = state
        .list_provider_catalog_keys_by_ids_strong(std::slice::from_ref(&key_id))
        .await?
        .into_iter()
        .find(|key| key.id == key_id && key.provider_id == provider.id)
    else {
        // 与旧版批量强读保持一致：已删除 Key 不进入 selected/scanned 汇总。
        return Ok(None);
    };
    if !key.is_active || provider_pool_key_runtime_quota_blocked(&key) {
        // 资格在轻量选中后发生变化时只跳过本 Key，不把它伪计为一次检查。
        return Ok(None);
    }

    if matches!(serving_policy, ProviderQuotaServingPolicy::ServingProbe) {
        record_score_probe_in_progress_for_key(state, &provider.id, &key.id, attempted_at).await;
    }
    let outcome = match perform_quota_refresh_check(
        admin_state,
        provider,
        endpoint,
        provider_type,
        key,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(err) => AccountSelfCheckOutcome::Failed {
            status_code: None,
            message: gateway_error_message(err),
        },
    };
    record_score_probe_result_for_key(
        state,
        &provider.id,
        &key_id,
        attempted_at,
        &outcome,
        serving_policy,
    )
    .await;
    Ok(Some(outcome))
}

async fn record_score_probe_in_progress_for_key(
    state: &AppState,
    provider_id: &str,
    key_id: &str,
    attempted_at: u64,
) {
    if !state.data.has_pool_score_writer() {
        return;
    }
    let projection_lock = match state
        .acquire_provider_key_quota_projection_lock(provider_id, key_id)
        .await
    {
        Ok(lease) => lease,
        Err(err) => {
            debug!(
                provider_id,
                key_id,
                error = ?err,
                "gateway account self-check: skipped in-progress score update while quota fence is busy"
            );
            return;
        }
    };
    let quota_blocked = state
        .provider_key_runtime_quota_blocked_strong(provider_id, key_id)
        .await
        .ok()
        .flatten()
        .unwrap_or(true);
    if quota_blocked {
        state
            .release_provider_key_quota_projection_lock(projection_lock)
            .await;
        return;
    }

    // Self-check attempts share the score row with runtime quota projection;
    // serialize the read-modify-write so the manual block remains dominant.
    let attempt = PoolMemberProbeAttempt {
        identity: PoolMemberIdentity::provider_api_key(provider_id.to_string(), key_id.to_string()),
        scope: None,
        attempted_at,
        score_reason_patch: Some(json!({
            "last_probe": {
                "source": "account_self_check",
                "status": "in_progress"
            },
            "last_self_check": {
                "source": "account_self_check",
                "status": "in_progress",
                "attempted_at": attempted_at
            }
        })),
    };
    if let Err(err) = state.data.mark_pool_member_probe_in_progress(attempt).await {
        debug!(
            provider_id,
            key_id,
            error = ?err,
            "gateway account self-check: failed to mark score probe in progress"
        );
    }
    state
        .release_provider_key_quota_projection_lock(projection_lock)
        .await;
}

async fn record_score_probe_result_for_key(
    state: &AppState,
    provider_id: &str,
    key_id: &str,
    attempted_at: u64,
    outcome: &AccountSelfCheckOutcome,
    serving_policy: ProviderQuotaServingPolicy,
) {
    if !state.data.has_pool_score_writer() {
        return;
    }
    if matches!(serving_policy, ProviderQuotaServingPolicy::ObservationOnly) {
        return;
    }

    let projection_lock = match state
        .acquire_provider_key_quota_projection_lock(provider_id, key_id)
        .await
    {
        Ok(lease) => lease,
        Err(err) => {
            debug!(
                provider_id,
                key_id,
                error = ?err,
                "gateway account self-check: skipped score result while quota fence is busy"
            );
            return;
        }
    };
    let quota_blocked = state
        .provider_key_runtime_quota_blocked_strong(provider_id, key_id)
        .await
        .ok()
        .flatten()
        .unwrap_or(true);
    if !quota_blocked {
        record_score_probe_result_for_key_locked(
            state,
            provider_id,
            key_id,
            attempted_at,
            outcome,
            serving_policy,
        )
        .await;
    }
    state
        .release_provider_key_quota_projection_lock(projection_lock)
        .await;
}

async fn record_score_probe_result_for_key_locked(
    state: &AppState,
    provider_id: &str,
    key_id: &str,
    attempted_at: u64,
    outcome: &AccountSelfCheckOutcome,
    serving_policy: ProviderQuotaServingPolicy,
) {
    let subscription_hard_state = if matches!(
        serving_policy,
        ProviderQuotaServingPolicy::SubscriptionExhaustionOnly
    ) {
        match outcome {
            AccountSelfCheckOutcome::Success {
                exhausted: true, ..
            } => Some(PoolMemberHardState::QuotaExhausted),
            AccountSelfCheckOutcome::Success {
                exhausted: false, ..
            } => {
                let scope = crate::ai_serving::provider_key_pool_score_scope();
                state
                    .data
                    .list_pool_member_scores(&ListPoolMemberScoresQuery {
                        pool_kind: POOL_KIND_PROVIDER_KEY_POOL.to_string(),
                        pool_id: provider_id.to_string(),
                        capability: Some(scope.capability),
                        scope_kind: Some(scope.scope_kind),
                        scope_id: scope.scope_id,
                        hard_states: vec![PoolMemberHardState::QuotaExhausted],
                        probe_statuses: None,
                        offset: 0,
                        limit: 10_000,
                    })
                    .await
                    .ok()
                    .and_then(|scores| {
                        scores.into_iter().find(|score| {
                            score.member_id == key_id
                                && score
                                    .score_reason
                                    .pointer("/quota_refresh_health/state")
                                    .and_then(Value::as_str)
                                    == Some("quota_exhausted")
                        })
                    })
                    .map(|_| PoolMemberHardState::Available)
            }
            AccountSelfCheckOutcome::Blocked { .. }
            | AccountSelfCheckOutcome::AutoRemoved { .. }
            | AccountSelfCheckOutcome::Failed { .. }
            | AccountSelfCheckOutcome::Skipped { .. } => None,
        }
    } else {
        None
    };
    if matches!(
        serving_policy,
        ProviderQuotaServingPolicy::SubscriptionExhaustionOnly
    ) && subscription_hard_state.is_none()
    {
        return;
    }
    let (succeeded, hard_state, probe_status) = match outcome {
        AccountSelfCheckOutcome::Success { .. } => (
            true,
            subscription_hard_state.or(Some(PoolMemberHardState::Available)),
            PoolMemberProbeStatus::Ok,
        ),
        AccountSelfCheckOutcome::Blocked { .. } => (
            false,
            Some(PoolMemberHardState::Banned),
            PoolMemberProbeStatus::Failed,
        ),
        AccountSelfCheckOutcome::AutoRemoved { .. } => (
            false,
            Some(PoolMemberHardState::Banned),
            PoolMemberProbeStatus::Failed,
        ),
        AccountSelfCheckOutcome::Failed { .. } => (
            false,
            Some(PoolMemberHardState::Cooldown),
            PoolMemberProbeStatus::Failed,
        ),
        AccountSelfCheckOutcome::Skipped { .. } => (
            false,
            Some(PoolMemberHardState::Unknown),
            PoolMemberProbeStatus::Never,
        ),
    };
    let mut score_reason_patch = json!({
        "last_probe": {
            "source": "account_self_check",
            "status": outcome.score_status(),
            "status_code": outcome.status_code(),
            "message": outcome.message()
        },
        "last_self_check": {
            "source": "account_self_check",
            "status": outcome.score_status(),
            "status_code": outcome.status_code(),
            "message": outcome.message(),
            "attempted_at": attempted_at
        }
    });
    if let Some(state_name) = match subscription_hard_state {
        Some(PoolMemberHardState::QuotaExhausted) => Some("quota_exhausted"),
        Some(PoolMemberHardState::Available) => Some("available"),
        Some(
            PoolMemberHardState::Unknown
            | PoolMemberHardState::Cooldown
            | PoolMemberHardState::AuthInvalid
            | PoolMemberHardState::Banned
            | PoolMemberHardState::Inactive,
        )
        | None => None,
    } {
        score_reason_patch["quota_refresh_health"] = json!({"state": state_name});
    }
    let result = PoolMemberProbeResult {
        identity: PoolMemberIdentity::provider_api_key(provider_id.to_string(), key_id.to_string()),
        scope: None,
        attempted_at,
        succeeded,
        hard_state,
        probe_status,
        score_reason_patch: Some(score_reason_patch),
    };
    if let Err(err) = state.data.record_pool_member_probe_result(result).await {
        debug!(
            provider_id,
            key_id,
            error = ?err,
            "gateway account self-check: failed to record score probe result"
        );
    }
}

fn endpoint_for_self_check(
    provider_type: &str,
    endpoints: &[StoredProviderCatalogEndpoint],
) -> Option<StoredProviderCatalogEndpoint> {
    provider_quota_refresh_endpoint_for_provider(provider_type, endpoints, true)
}

fn gateway_error_message(err: GatewayError) -> String {
    err.into_message()
}

fn update_summary_from_outcome(
    summary: &mut AccountSelfCheckRunSummary,
    outcome: &AccountSelfCheckOutcome,
) {
    match outcome {
        AccountSelfCheckOutcome::Success { .. } => {
            summary.succeeded = summary.succeeded.saturating_add(1);
        }
        AccountSelfCheckOutcome::Blocked { .. } => {
            summary.blocked = summary.blocked.saturating_add(1);
        }
        AccountSelfCheckOutcome::AutoRemoved { .. } => {
            summary.auto_removed = summary.auto_removed.saturating_add(1);
        }
        AccountSelfCheckOutcome::Failed { .. } => {
            summary.failed = summary.failed.saturating_add(1);
        }
        AccountSelfCheckOutcome::Skipped { .. } => {
            summary.skipped = summary.skipped.saturating_add(1);
        }
    }
}

/// 合并一个强读复核后的候选结果；返回该候选是否应计入本轮 selected/scanned。
fn update_summary_from_revalidated_candidate(
    summary: &mut AccountSelfCheckRunSummary,
    outcome: Option<&AccountSelfCheckOutcome>,
) -> bool {
    let Some(outcome) = outcome else {
        return false;
    };
    summary.scanned_keys = summary.scanned_keys.saturating_add(1);
    summary.selected_keys = summary.selected_keys.saturating_add(1);
    update_summary_from_outcome(summary, outcome);
    true
}

/// 解析账户自检配置；ObservationOnly 余额提供商默认启用，其他策略仍遵循显式开关。
fn account_self_check_config_for_provider(
    provider: &StoredProviderCatalogProvider,
    provider_type: &str,
) -> Option<AdminProviderPoolConfig> {
    let pool_config = admin_provider_pool_config(provider);
    if matches!(
        provider_quota_serving_policy(provider_type),
        Some(ProviderQuotaServingPolicy::ObservationOnly)
    ) {
        return pool_config.or_else(|| {
            admin_provider_pool_config_from_config_value(Some(&json!({"pool_advanced": {}})))
        });
    }
    pool_config.filter(|config| config.account_self_check_enabled)
}

/// 执行一轮账户自检；低余额 Key 仍按原周期刷新，以便余额恢复后自动重新准入。
pub(crate) async fn perform_account_self_check_once_with_config(
    state: &AppState,
    config: AccountSelfCheckWorkerConfig,
) -> Result<AccountSelfCheckRunSummary, GatewayError> {
    perform_account_self_check_once_with_config_and_gate(
        state,
        config,
        shared_auth_maintenance_gate(),
    )
    .await
}

/// 以指定共享闸门执行一轮账号自检，供测试隔离验证跨 worker 的并发边界。
async fn perform_account_self_check_once_with_config_and_gate(
    state: &AppState,
    config: AccountSelfCheckWorkerConfig,
    gate: AuthMaintenanceGate,
) -> Result<AccountSelfCheckRunSummary, GatewayError> {
    if !state.has_provider_catalog_data_reader() || !state.has_provider_catalog_data_writer() {
        return Ok(AccountSelfCheckRunSummary::empty());
    }

    let providers = state
        .list_provider_catalog_providers(true)
        .await?
        .into_iter()
        .filter_map(|provider| {
            let provider_type = provider.provider_type.trim().to_ascii_lowercase();
            let pool_config =
                account_self_check_config_for_provider(&provider, provider_type.as_str())?;
            Some((provider, provider_type, pool_config))
        })
        .collect::<Vec<_>>();

    if providers.is_empty() {
        return Ok(AccountSelfCheckRunSummary::empty());
    }

    let provider_ids = providers
        .iter()
        .map(|(provider, _, _)| provider.id.clone())
        .collect::<Vec<_>>();
    let mut endpoints_by_provider = BTreeMap::<String, Vec<StoredProviderCatalogEndpoint>>::new();
    for endpoint in state
        .list_provider_catalog_endpoints_by_provider_ids(&provider_ids)
        .await?
    {
        endpoints_by_provider
            .entry(endpoint.provider_id.clone())
            .or_default()
            .push(endpoint);
    }

    let admin_state = AdminAppState::new(state);
    let now_ts = now_unix_secs();
    let mut summary = AccountSelfCheckRunSummary {
        providers_checked: providers.len(),
        ..AccountSelfCheckRunSummary::empty()
    };

    for (provider, provider_type, pool_config) in providers {
        let provider_endpoints = endpoints_by_provider
            .get(&provider.id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let Some(endpoint) = endpoint_for_self_check(&provider_type, provider_endpoints) else {
            summary.providers_skipped = summary.providers_skipped.saturating_add(1);
            continue;
        };
        if !provider_type_supports_quota_refresh(&provider_type) {
            summary.providers_skipped = summary.providers_skipped.saturating_add(1);
            continue;
        }
        let serving_policy = provider_quota_serving_policy(&provider_type)
            .unwrap_or(ProviderQuotaServingPolicy::ServingProbe);

        let interval_seconds = pool_config
            .account_self_check_interval_minutes
            .clamp(1, 1440)
            .saturating_mul(60);
        let provider_limit = config.max_keys_per_provider;
        let key_ids = select_key_ids_for_provider(
            state,
            state.runtime_state.as_ref(),
            &provider,
            interval_seconds,
            provider_limit,
            now_ts,
        )
        .await?;
        if key_ids.is_empty() {
            continue;
        }

        let provider_short_id = provider.id.chars().take(8).collect::<String>();
        let concurrency = (pool_config.account_self_check_concurrency as usize)
            .clamp(1, 64)
            .min(config.global_concurrency)
            .min(gate.concurrency())
            .max(1);
        let admin_state_ref = &admin_state;
        let provider_ref = &provider;
        let endpoint_ref = &endpoint;
        let provider_type_ref = provider_type.as_str();
        let check_results = stream::iter(key_ids.into_iter().map(|key_id| {
            let gate = gate.clone();
            async move {
                perform_account_self_check_candidate_under_gate(
                    state,
                    admin_state_ref,
                    provider_ref,
                    endpoint_ref,
                    provider_type_ref,
                    key_id,
                    now_ts,
                    serving_policy,
                    gate,
                )
                .await
            }
        }))
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await;

        let mut selected_count = 0usize;
        for result in check_results {
            // 强读仓储错误沿用旧版边界：终止本轮，而不是伪装为上游检查失败。
            let outcome = result?;
            if update_summary_from_revalidated_candidate(&mut summary, outcome.as_ref()) {
                selected_count = selected_count.saturating_add(1);
            }
        }

        // 只有强读后仍具备资格的 Key 才沿用旧版 selected/scanned 统计口径。
        if selected_count == 0 {
            continue;
        }
        summary.providers_checked_with_keys = summary.providers_checked_with_keys.saturating_add(1);

        info!(
            provider_id = %provider_short_id,
            provider_type,
            selected = selected_count,
            concurrency,
            "gateway account self-check completed"
        );
    }

    Ok(summary)
}

/// 使用环境配置与进程级共享闸门执行一轮账号自检。
pub(crate) async fn perform_account_self_check_once(
    state: &AppState,
) -> Result<AccountSelfCheckRunSummary, GatewayError> {
    perform_account_self_check_once_with_config(state, AccountSelfCheckWorkerConfig::from_env())
        .await
}

pub(crate) fn spawn_account_self_check_worker(
    state: AppState,
) -> Option<tokio::task::JoinHandle<()>> {
    if !state.has_provider_catalog_data_reader() || !state.has_provider_catalog_data_writer() {
        return None;
    }

    let config = AccountSelfCheckWorkerConfig::from_env();
    Some(crate::task_runtime::spawn_singleton_worker(
        state,
        crate::task_runtime::TASK_KEY_ACCOUNT_SELF_CHECK,
        move |state| async move {
            let mut interval = tokio::time::interval(config.scan_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            interval.tick().await;
            let mut deferred_since = None;
            loop {
                interval.tick().await;
                if state
                    .data
                    .should_defer_maintenance_for_database_pool_pressure(&mut deferred_since)
                {
                    debug!(
                        event_name = "maintenance_worker_deferred",
                        log_type = "ops",
                        worker = "account_self_check",
                        "gateway account self-check deferred because database pool has no idle reserve"
                    );
                    continue;
                }
                if let Err(err) = perform_account_self_check_once_with_config(&state, config).await
                {
                    warn!(
                        error = ?err,
                        "gateway account self-check worker tick failed"
                    );
                }
            }
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        account_self_check_config_for_provider, quota_payload_result_for_key,
        record_score_probe_result_for_key, select_account_self_check_key_ids,
        update_summary_from_outcome, update_summary_from_revalidated_candidate,
        AccountSelfCheckOutcome, AccountSelfCheckRunSummary,
    };
    use serde_json::json;
    use std::collections::BTreeMap;

    use std::sync::Arc;

    use aether_data::repository::pool_scores::InMemoryPoolMemberScoreRepository;
    use aether_data_contracts::repository::pool_scores::{
        GetPoolMemberScoresByIdsQuery, PoolMemberHardState, PoolMemberProbeStatus,
        StoredPoolMemberScore,
    };
    use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogProvider;
    use aether_provider_pool::ProviderQuotaServingPolicy;

    use crate::{data::GatewayDataState, AppState};

    /// 构造账户自检策略测试所需的最小 Provider。
    fn provider(
        provider_type: &str,
        config: Option<serde_json::Value>,
    ) -> StoredProviderCatalogProvider {
        let mut provider = StoredProviderCatalogProvider::new(
            format!("provider-{provider_type}"),
            provider_type.to_string(),
            None,
            provider_type.to_string(),
        )
        .expect("provider should build");
        provider.config = config;
        provider
    }

    #[test]
    fn observation_only_provider_self_check_is_automatic_with_existing_defaults() {
        // 验证余额观察型 Provider 无需手工开关，且保留自定义周期和并发。
        let defaults =
            account_self_check_config_for_provider(&provider("deepseek", None), "deepseek")
                .expect("observation-only provider should enable self-check");
        assert_eq!(defaults.account_self_check_interval_minutes, 60);
        assert_eq!(defaults.account_self_check_concurrency, 4);

        let configured = account_self_check_config_for_provider(
            &provider(
                "deepseek",
                Some(json!({
                    "pool_advanced": {
                        "account_self_check_enabled": false,
                        "account_self_check_interval_minutes": 90,
                        "account_self_check_concurrency": 5
                    }
                })),
            ),
            "deepseek",
        )
        .expect("observation-only provider should keep configured cadence");
        assert_eq!(configured.account_self_check_interval_minutes, 90);
        assert_eq!(configured.account_self_check_concurrency, 5);

        assert!(account_self_check_config_for_provider(
            &provider("codex", Some(json!({"pool_advanced": {}}))),
            "codex"
        )
        .is_none());
        assert!(account_self_check_config_for_provider(
            &provider(
                "codex",
                Some(json!({
                    "pool_advanced": {"account_self_check_enabled": true}
                })),
            ),
            "codex"
        )
        .is_some());
    }

    #[tokio::test]
    async fn observation_only_failure_does_not_write_pool_hard_state() {
        // 验证余额查询失败只保留 stale 快照语义，不写 Pool cooldown 或 hard-state。
        let repository = Arc::new(InMemoryPoolMemberScoreRepository::seed([]));
        let state = AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::disabled()
                    .with_pool_score_repository_for_tests(Arc::clone(&repository)),
            );

        record_score_probe_result_for_key(
            &state,
            "provider-deepseek",
            "key-1",
            100,
            &AccountSelfCheckOutcome::Failed {
                status_code: Some(503),
                message: "upstream unavailable".to_string(),
            },
            ProviderQuotaServingPolicy::ObservationOnly,
        )
        .await;

        let scores = state
            .data
            .get_pool_member_scores_by_ids(&GetPoolMemberScoresByIdsQuery {
                ids: vec!["score-key-1".to_string()],
            })
            .await
            .expect("score lookup should succeed");
        assert!(scores.is_empty());
    }

    #[test]
    fn quota_backoff_is_skipped_and_never_failed() {
        // Given
        let payload = json!({
            "total": 1,
            "success": 0,
            "failed": 0,
            "skipped": 1,
            "results": [{"key_id":"key-1","status":"backoff"}]
        });
        let mut summary = AccountSelfCheckRunSummary::empty();

        // When
        let outcome = quota_payload_result_for_key("key-1", Some(payload));
        update_summary_from_outcome(&mut summary, &outcome);

        // Then
        assert!(matches!(outcome, AccountSelfCheckOutcome::Skipped { .. }));
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.blocked, 0);
    }

    #[test]
    fn revalidation_skip_is_not_counted_as_selected_or_scanned() {
        // 轻量选中后已删除、停用或额度封禁的 Key 不属于旧版强读后的最终候选集。
        let mut summary = AccountSelfCheckRunSummary::empty();

        let counted = update_summary_from_revalidated_candidate(&mut summary, None);

        assert!(!counted);
        assert_eq!(summary.scanned_keys, 0);
        assert_eq!(summary.selected_keys, 0);
        assert_eq!(summary.succeeded, 0);
        assert_eq!(summary.failed, 0);
    }

    #[tokio::test]
    async fn subscription_refresh_success_preserves_unrelated_hard_state() {
        // Given
        let existing = StoredPoolMemberScore {
            id: "score-key-1".to_string(),
            pool_kind: "provider_key_pool".to_string(),
            pool_id: "provider-kimi".to_string(),
            member_kind: "provider_api_key".to_string(),
            member_id: "key-1".to_string(),
            capability: "account".to_string(),
            scope_kind: "account".to_string(),
            scope_id: None,
            score: 0.25,
            hard_state: PoolMemberHardState::Banned,
            score_version: 1,
            score_reason: json!({"serving_failure": {"source": "network"}}),
            last_ranked_at: None,
            last_scheduled_at: None,
            last_success_at: None,
            last_failure_at: Some(90),
            failure_count: 2,
            last_probe_attempt_at: Some(90),
            last_probe_success_at: None,
            last_probe_failure_at: Some(90),
            probe_failure_count: 2,
            probe_status: PoolMemberProbeStatus::Failed,
            updated_at: 90,
        };
        let repository = Arc::new(InMemoryPoolMemberScoreRepository::seed([existing]));
        let state = AppState::new()
            .expect("gateway should build")
            .with_data_state_for_tests(
                GatewayDataState::disabled()
                    .with_pool_score_repository_for_tests(Arc::clone(&repository)),
            );

        // When
        record_score_probe_result_for_key(
            &state,
            "provider-kimi",
            "key-1",
            100,
            &AccountSelfCheckOutcome::Success {
                status_code: Some(200),
                message: None,
                exhausted: false,
            },
            ProviderQuotaServingPolicy::SubscriptionExhaustionOnly,
        )
        .await;

        // Then
        let scores = state
            .data
            .get_pool_member_scores_by_ids(&GetPoolMemberScoresByIdsQuery {
                ids: vec!["score-key-1".to_string()],
            })
            .await
            .expect("score should load");
        assert_eq!(scores[0].hard_state, PoolMemberHardState::Banned);
        assert_eq!(scores[0].score, 0.25);
        assert_eq!(scores[0].probe_failure_count, 2);
    }

    #[test]
    fn selects_never_and_stale_self_check_keys_first() {
        let key_ids = vec![
            "fresh".to_string(),
            "never".to_string(),
            "stale".to_string(),
        ];
        let stamps = BTreeMap::from([("fresh".to_string(), 1_950), ("stale".to_string(), 1_000)]);

        let selected = select_account_self_check_key_ids(&key_ids, 2_000, 600, &stamps, 2);

        assert_eq!(selected, vec!["never".to_string(), "stale".to_string()]);
    }
}
