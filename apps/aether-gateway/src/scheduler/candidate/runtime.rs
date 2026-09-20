use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;

use aether_admin::provider::{
    pool as admin_provider_pool_pure, status as admin_provider_status_pure,
};
use aether_data_contracts::repository::candidates::StoredRequestCandidate;
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
use aether_provider_pool::{
    provider_pool_key_balance_below_minimum, provider_pool_key_runtime_quota_blocked,
};
use aether_scheduler_core::{
    auth_api_key_concurrency_limit_reached, build_provider_concurrent_limit_map,
    candidate_runtime_skip_reason_with_state, effective_provider_key_rpm_limit,
    CandidateRuntimeSelectabilityInput,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::Instant;

use crate::data::auth::GatewayAuthApiKeySnapshot;
use crate::GatewayError;

use super::{SchedulerMinimalCandidateSelectionCandidate, SchedulerRuntimeState};

pub(super) use aether_scheduler_core::should_skip_provider_quota;

pub(super) struct CandidateRuntimeSelectionSnapshot {
    pub(super) recent_candidates: Vec<StoredRequestCandidate>,
    pub(super) provider_concurrent_limits: BTreeMap<String, usize>,
    pub(super) provider_key_rpm_states: BTreeMap<String, StoredProviderCatalogKey>,
    pub(super) pool_provider_ids: BTreeSet<String>,
    provider_quota_blocks_requests: BTreeMap<String, bool>,
    key_quota_hard_blocked: BTreeMap<String, bool>,
    key_account_quota_exhausted: BTreeMap<String, bool>,
    /// 真实 Key 的标准余额是否明确低于内部调度下限。
    key_balance_below_minimum: BTreeMap<String, bool>,
    key_oauth_invalid: BTreeMap<String, bool>,
    provider_key_rpm_reset_ats: BTreeMap<String, Option<u64>>,
}

/// 一次性读取候选准入所需运行态，确保排序和最终诊断消费同一余额事实。
pub(super) async fn read_candidate_runtime_selection_snapshot(
    state: &(impl SchedulerRuntimeState + ?Sized),
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    now_unix_secs: u64,
) -> Result<CandidateRuntimeSelectionSnapshot, GatewayError> {
    let provider_concurrent_limits = read_provider_concurrent_limits(state, candidates).await?;
    let provider_pool_state = read_provider_pool_state_map(state, candidates).await?;
    let provider_skip_exhausted_accounts = provider_pool_state
        .iter()
        .map(|(provider_id, state)| (provider_id.clone(), state.skip_exhausted_accounts))
        .collect::<BTreeMap<_, _>>();
    let pool_provider_ids = provider_pool_state
        .iter()
        .filter_map(|(provider_id, state)| state.pool_enabled.then_some(provider_id.clone()))
        .collect::<BTreeSet<_>>();
    let provider_key_rpm_states = read_provider_key_rpm_states(state, candidates).await?;
    let recent_candidates = if runtime_snapshot_requires_recent_candidates(
        auth_snapshot,
        &provider_concurrent_limits,
        &provider_key_rpm_states,
        now_unix_secs,
    ) {
        state.read_recent_request_candidates(128).await?
    } else {
        Vec::new()
    };
    let key_account_quota_exhausted = read_key_account_quota_exhaustion_map(
        candidates,
        &provider_key_rpm_states,
        &provider_skip_exhausted_accounts,
    );
    let key_quota_hard_blocked =
        read_key_quota_hard_blocked_map(candidates, &provider_key_rpm_states);
    let key_balance_below_minimum =
        read_key_balance_below_minimum_map(candidates, &provider_key_rpm_states);
    let key_oauth_invalid =
        read_key_oauth_invalid_map(candidates, &provider_key_rpm_states, now_unix_secs);
    let provider_quota_blocks_requests =
        read_provider_quota_block_map(state, candidates, now_unix_secs).await?;
    let provider_key_rpm_reset_ats =
        read_provider_key_rpm_reset_at_map(state, candidates, now_unix_secs);

    Ok(CandidateRuntimeSelectionSnapshot {
        recent_candidates,
        provider_concurrent_limits,
        provider_key_rpm_states,
        pool_provider_ids,
        provider_quota_blocks_requests,
        key_quota_hard_blocked,
        key_account_quota_exhausted,
        key_balance_below_minimum,
        key_oauth_invalid,
        provider_key_rpm_reset_ats,
    })
}

fn runtime_snapshot_requires_recent_candidates(
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    provider_concurrent_limits: &BTreeMap<String, usize>,
    provider_key_rpm_states: &BTreeMap<String, StoredProviderCatalogKey>,
    now_unix_secs: u64,
) -> bool {
    if auth_snapshot
        .and_then(|snapshot| snapshot.api_key_concurrent_limit)
        .is_some_and(|limit| limit > 0)
    {
        return true;
    }

    if provider_concurrent_limits.values().any(|limit| *limit > 0) {
        return true;
    }

    provider_key_rpm_states.values().any(|key| {
        key.concurrent_limit.is_some_and(|limit| limit > 0)
            || effective_provider_key_rpm_limit(key, now_unix_secs).is_some()
    })
}

pub(super) fn auth_snapshot_concurrency_limit_reached(
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    snapshot: &CandidateRuntimeSelectionSnapshot,
    now_unix_secs: u64,
) -> bool {
    auth_snapshot_concurrency_limit(auth_snapshot).is_some_and(|(api_key_id, limit)| {
        auth_api_key_concurrency_limit_reached(
            &snapshot.recent_candidates,
            now_unix_secs,
            api_key_id,
            limit,
        )
    })
}

fn auth_snapshot_concurrency_limit(
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
) -> Option<(&str, usize)> {
    let snapshot = auth_snapshot?;
    let limit = usize::try_from(snapshot.api_key_concurrent_limit?).ok()?;
    (limit > 0).then_some((snapshot.api_key_id.as_str(), limit))
}

async fn read_auth_api_key_concurrency_limit_reached(
    state: &(impl SchedulerRuntimeState + ?Sized),
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
) -> Result<bool, GatewayError> {
    let Some((api_key_id, limit)) = auth_snapshot_concurrency_limit(auth_snapshot) else {
        return Ok(false);
    };
    let recent_candidates = state.read_recent_request_candidates(128).await?;
    Ok(auth_api_key_concurrency_limit_reached(
        &recent_candidates,
        crate::clock::current_unix_secs(),
        api_key_id,
        limit,
    ))
}

/// A retry always rebuilds candidates, including at the deadline. Only the
/// intervening polls omit catalog, quota and ranking work while auth is blocked.
pub(crate) async fn wait_for_auth_api_key_concurrency_retry(
    state: &(impl SchedulerRuntimeState + ?Sized),
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    deadline: Instant,
    poll_interval: Duration,
) -> Result<bool, GatewayError> {
    if Instant::now() >= deadline {
        return Ok(false);
    }
    let poll_interval = poll_interval.max(Duration::from_millis(1));
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        tokio::time::sleep(poll_interval.min(remaining)).await;
        if Instant::now() >= deadline
            || !read_auth_api_key_concurrency_limit_reached(state, auth_snapshot).await?
        {
            return Ok(true);
        }
    }
}

pub(crate) async fn select_with_auth_concurrency_wait<T, Select, Selection>(
    state: &(impl SchedulerRuntimeState + ?Sized),
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    now_unix_secs: u64,
    wait_timeout: Duration,
    poll_interval: Duration,
    mut select: Select,
) -> Result<T, GatewayError>
where
    Select: FnMut(u64) -> Selection,
    Selection: Future<Output = Result<(T, bool), GatewayError>>,
{
    let deadline = Instant::now() + wait_timeout;
    let mut attempt_now_unix_secs = now_unix_secs;
    loop {
        let (result, auth_limit_blocked) = select(attempt_now_unix_secs).await?;
        if !auth_limit_blocked
            || !wait_for_auth_api_key_concurrency_retry(
                state,
                auth_snapshot,
                deadline,
                poll_interval,
            )
            .await?
        {
            return Ok(result);
        }
        attempt_now_unix_secs = crate::clock::current_unix_secs();
    }
}

/// 准入布尔值与诊断共享同一判定，避免 PoolGroup 的代表 Key 过滤发生分歧。
pub(super) fn is_candidate_selectable(
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
    snapshot: &CandidateRuntimeSelectionSnapshot,
    now_unix_secs: u64,
) -> bool {
    current_candidate_runtime_skip_reason(candidate, snapshot, now_unix_secs).is_none()
}

/// 返回与准入判断一致的诊断原因，PoolGroup 的真实 Key 留在 Pool 展开阶段过滤。
pub(super) fn current_candidate_runtime_skip_reason(
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
    snapshot: &CandidateRuntimeSelectionSnapshot,
    now_unix_secs: u64,
) -> Option<&'static str> {
    let pool_group = snapshot
        .pool_provider_ids
        .contains(candidate.provider_id.as_str());
    let provider_quota_blocks_requests = snapshot
        .provider_quota_blocks_requests
        .get(candidate.provider_id.as_str())
        .copied()
        .unwrap_or(false);
    let rpm_reset_at = (!pool_group)
        .then(|| {
            snapshot
                .provider_key_rpm_reset_ats
                .get(candidate.key_id.as_str())
                .copied()
                .flatten()
        })
        .flatten();
    let empty_key_runtime_states = BTreeMap::new();

    candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
        candidate,
        recent_candidates: &snapshot.recent_candidates,
        provider_concurrent_limits: &snapshot.provider_concurrent_limits,
        // 代表 Key 的并发、健康和 RPM 不能淘汰整组；展开后对真实 Key 执行相同检查。
        provider_key_rpm_states: if pool_group {
            &empty_key_runtime_states
        } else {
            &snapshot.provider_key_rpm_states
        },
        now_unix_secs,
        provider_quota_blocks_requests,
        quota_hard_blocked: !pool_group
            && snapshot
                .key_quota_hard_blocked
                .get(candidate.key_id.as_str())
                .copied()
                .unwrap_or(false),
        account_quota_exhausted: !pool_group
            && snapshot
                .key_account_quota_exhausted
                .get(candidate.key_id.as_str())
                .copied()
                .unwrap_or(false),
        balance_below_minimum: !pool_group
            && snapshot
                .key_balance_below_minimum
                .get(candidate.key_id.as_str())
                .copied()
                .unwrap_or(false),
        oauth_invalid: !pool_group
            && snapshot
                .key_oauth_invalid
                .get(candidate.key_id.as_str())
                .copied()
                .unwrap_or(false),
        enforce_key_circuit_breaker: !pool_group,
        rpm_reset_at,
    })
}

pub(super) async fn read_provider_concurrent_limits(
    state: &(impl SchedulerRuntimeState + ?Sized),
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
) -> Result<BTreeMap<String, usize>, GatewayError> {
    let provider_ids = candidates
        .iter()
        .map(|candidate| candidate.provider_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if provider_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let providers = state
        .read_provider_catalog_providers_by_ids(&provider_ids)
        .await?;
    Ok(build_provider_concurrent_limit_map(providers))
}

pub(super) async fn read_provider_key_rpm_states(
    state: &(impl SchedulerRuntimeState + ?Sized),
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
) -> Result<BTreeMap<String, StoredProviderCatalogKey>, GatewayError> {
    let key_ids = candidates
        .iter()
        .map(|candidate| candidate.key_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if key_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    // A runtime quota block is persistent and may have been written by a
    // different gateway instance; do not let the five-second catalog cache
    // admit a stale candidate.
    let keys = state
        .read_provider_catalog_keys_by_ids_strong(&key_ids)
        .await?;
    Ok(keys
        .into_iter()
        .map(|key| (key.id.clone(), key))
        .collect::<BTreeMap<_, _>>())
}

async fn read_provider_quota_block_map(
    state: &(impl SchedulerRuntimeState + ?Sized),
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
    now_unix_secs: u64,
) -> Result<BTreeMap<String, bool>, GatewayError> {
    let provider_ids = candidates
        .iter()
        .map(|candidate| candidate.provider_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut quota_blocks = BTreeMap::new();

    for provider_id in provider_ids {
        let blocks_requests = state
            .read_provider_quota_snapshot(&provider_id)
            .await?
            .as_ref()
            .is_some_and(|quota| should_skip_provider_quota(quota, now_unix_secs));
        quota_blocks.insert(provider_id, blocks_requests);
    }

    Ok(quota_blocks)
}

#[derive(Debug, Clone, Copy, Default)]
struct ProviderPoolState {
    pool_enabled: bool,
    skip_exhausted_accounts: bool,
}

async fn read_provider_pool_state_map(
    state: &(impl SchedulerRuntimeState + ?Sized),
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
) -> Result<BTreeMap<String, ProviderPoolState>, GatewayError> {
    let provider_ids = candidates
        .iter()
        .map(|candidate| candidate.provider_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if provider_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let providers = state
        .read_provider_catalog_providers_by_ids(&provider_ids)
        .await?;
    Ok(providers
        .into_iter()
        .map(|provider| {
            let pool_advanced = provider
                .config
                .as_ref()
                .and_then(|value| value.get("pool_advanced"));
            let skip_exhausted_accounts = pool_advanced
                .and_then(serde_json::Value::as_object)
                .and_then(|value| value.get("skip_exhausted_accounts"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            (
                provider.id,
                ProviderPoolState {
                    pool_enabled: pool_advanced.is_some(),
                    skip_exhausted_accounts,
                },
            )
        })
        .collect())
}

fn read_key_account_quota_exhaustion_map(
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
    provider_key_rpm_states: &BTreeMap<String, StoredProviderCatalogKey>,
    provider_skip_exhausted_accounts: &BTreeMap<String, bool>,
) -> BTreeMap<String, bool> {
    candidates
        .iter()
        .map(|candidate| {
            let exhausted = provider_key_rpm_states
                .get(candidate.key_id.as_str())
                .is_some_and(|key| {
                    let account_exhausted =
                        admin_provider_pool_pure::admin_pool_key_model_quota_exhausted(
                            key,
                            candidate.provider_type.as_str(),
                            candidate.selected_provider_model_name.as_str(),
                        )
                        .unwrap_or_else(|| {
                            admin_provider_pool_pure::admin_pool_key_account_quota_exhausted(
                                key,
                                candidate.provider_type.as_str(),
                            )
                        });
                    let hard_blocked =
                        admin_provider_pool_pure::admin_pool_key_model_quota_hard_blocked(
                            key,
                            candidate.provider_type.as_str(),
                            candidate.selected_provider_model_name.as_str(),
                        );
                    let skip_configured = provider_skip_exhausted_accounts
                        .get(candidate.provider_id.as_str())
                        .copied()
                        .unwrap_or(false);
                    hard_blocked || (skip_configured && account_exhausted)
                });
            (candidate.key_id.clone(), exhausted)
        })
        .collect()
}

fn read_key_quota_hard_blocked_map(
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
    provider_key_rpm_states: &BTreeMap<String, StoredProviderCatalogKey>,
) -> BTreeMap<String, bool> {
    candidates
        .iter()
        .map(|candidate| {
            let blocked = provider_key_rpm_states
                .get(candidate.key_id.as_str())
                .is_some_and(provider_pool_key_runtime_quota_blocked);
            (candidate.key_id.clone(), blocked)
        })
        .collect()
}

/// 从目录 Key 构建独立余额事实；PoolGroup 代表 Key 是否应用该事实由统一准入层决定。
fn read_key_balance_below_minimum_map(
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
    provider_key_rpm_states: &BTreeMap<String, StoredProviderCatalogKey>,
) -> BTreeMap<String, bool> {
    candidates
        .iter()
        .map(|candidate| {
            let below_minimum = provider_key_rpm_states
                .get(candidate.key_id.as_str())
                .is_some_and(|key| {
                    provider_pool_key_balance_below_minimum(key, candidate.provider_type.as_str())
                });
            (candidate.key_id.clone(), below_minimum)
        })
        .collect()
}

fn read_key_oauth_invalid_map(
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
    provider_key_rpm_states: &BTreeMap<String, StoredProviderCatalogKey>,
    now_unix_secs: u64,
) -> BTreeMap<String, bool> {
    candidates
        .iter()
        .map(|candidate| {
            let oauth_invalid = provider_key_rpm_states
                .get(candidate.key_id.as_str())
                .is_some_and(|key| {
                    key_requires_oauth_reauth(key, candidate.provider_type.as_str(), now_unix_secs)
                });
            (candidate.key_id.clone(), oauth_invalid)
        })
        .collect()
}

fn key_requires_oauth_reauth(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    now_unix_secs: u64,
) -> bool {
    if !key.auth_type.trim().eq_ignore_ascii_case("oauth") {
        return false;
    }

    let invalid_reason = key
        .oauth_invalid_reason
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    if !invalid_reason.is_empty() {
        return oauth_invalid_reason_blocks_scheduling(
            key,
            provider_type,
            invalid_reason,
            now_unix_secs,
        );
    }

    false
}

fn oauth_invalid_reason_blocks_scheduling(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    invalid_reason: &str,
    now_unix_secs: u64,
) -> bool {
    let trimmed_reason = invalid_reason.trim();

    let account_state = admin_provider_status_pure::resolve_pool_account_state(
        Some(provider_type),
        key.upstream_metadata.as_ref(),
        Some(trimmed_reason),
    );
    if account_state.blocked
        && !account_state.recoverable
        && account_state
            .code
            .as_deref()
            .is_some_and(oauth_account_state_code_is_hard_block)
    {
        return true;
    }

    if oauth_invalid_reason_has_tag(trimmed_reason, "[REFRESH_FAILED]") {
        return oauth_access_token_expired(key, now_unix_secs);
    }

    false
}

fn oauth_invalid_reason_has_tag(reason: &str, tag: &str) -> bool {
    reason
        .lines()
        .map(str::trim)
        .any(|line| line.starts_with(tag))
}

fn oauth_access_token_expired(key: &StoredProviderCatalogKey, now_unix_secs: u64) -> bool {
    let now_unix_secs = if now_unix_secs == 0 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
    } else {
        now_unix_secs
    };
    key.expires_at_unix_secs
        .is_none_or(|expires_at| expires_at == 0 || expires_at <= now_unix_secs)
}

fn oauth_account_state_code_is_hard_block(code: &str) -> bool {
    matches!(
        code.trim().to_ascii_lowercase().as_str(),
        "account_banned"
            | "account_suspended"
            | "account_disabled"
            | "workspace_deactivated"
            | "account_forbidden"
            | "account_blocked"
            | "account_verification"
            | "oauth_token_invalid"
    )
}

fn read_provider_key_rpm_reset_at_map(
    state: &(impl SchedulerRuntimeState + ?Sized),
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
    now_unix_secs: u64,
) -> BTreeMap<String, Option<u64>> {
    candidates
        .iter()
        .map(|candidate| {
            (
                candidate.key_id.clone(),
                state.provider_key_rpm_reset_at(candidate.key_id.as_str(), now_unix_secs),
            )
        })
        .collect::<BTreeMap<_, _>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 构造覆盖普通候选与 PoolGroup 代表候选的最小调度事实。
    fn sample_candidate() -> SchedulerMinimalCandidateSelectionCandidate {
        SchedulerMinimalCandidateSelectionCandidate {
            provider_id: "provider-1".to_string(),
            provider_name: "DeepSeek".to_string(),
            provider_type: "deepseek".to_string(),
            provider_priority: 0,
            endpoint_id: "endpoint-1".to_string(),
            endpoint_api_format: "openai:chat".to_string(),
            key_id: "key-1".to_string(),
            key_name: "key-1".to_string(),
            key_auth_type: "api_key".to_string(),
            key_internal_priority: 0,
            key_global_priority_for_format: None,
            key_capabilities: None,
            routing_facts: Default::default(),
            model_id: "model-1".to_string(),
            global_model_id: "global-model-1".to_string(),
            global_model_name: "deepseek-chat".to_string(),
            selected_provider_model_name: "deepseek-chat".to_string(),
            supports_streaming: true,
            mapping_matched_model: None,
        }
    }

    /// 构造 fresh 且余额恰好等于内部下限的目录 Key。
    fn low_balance_key() -> StoredProviderCatalogKey {
        let mut key = StoredProviderCatalogKey::new(
            "key-1".to_string(),
            "provider-1".to_string(),
            "key-1".to_string(),
            "api_key".to_string(),
            None,
            true,
        )
        .expect("key should build");
        key.status_snapshot = Some(json!({
            "quota": {
                "schema_version": 1,
                "provider_type": "deepseek",
                "kind": "balance",
                "freshness": "fresh",
                "balances": [{"unit": "CNY", "available": "1"}]
            }
        }));
        key
    }

    #[test]
    fn runtime_balance_fact_blocks_real_key_but_not_pool_group_representative() {
        // 验证普通 Key 消费余额事实，而 PoolGroup 留给展开后的真实 Key 过滤。
        let candidate = sample_candidate();
        let key = low_balance_key();
        let provider_key_rpm_states = BTreeMap::from([(key.id.clone(), key)]);
        let key_balance_below_minimum = read_key_balance_below_minimum_map(
            std::slice::from_ref(&candidate),
            &provider_key_rpm_states,
        );
        let mut snapshot = CandidateRuntimeSelectionSnapshot {
            recent_candidates: Vec::new(),
            provider_concurrent_limits: BTreeMap::new(),
            provider_key_rpm_states,
            pool_provider_ids: BTreeSet::new(),
            provider_quota_blocks_requests: BTreeMap::new(),
            key_quota_hard_blocked: BTreeMap::new(),
            key_account_quota_exhausted: BTreeMap::new(),
            key_balance_below_minimum,
            key_oauth_invalid: BTreeMap::new(),
            provider_key_rpm_reset_ats: BTreeMap::new(),
        };

        assert_eq!(
            current_candidate_runtime_skip_reason(&candidate, &snapshot, 100),
            Some("key_balance_below_minimum")
        );
        snapshot
            .pool_provider_ids
            .insert(candidate.provider_id.clone());
        assert_eq!(
            current_candidate_runtime_skip_reason(&candidate, &snapshot, 100),
            None
        );
    }

    #[test]
    fn runtime_key_guards_do_not_block_pool_group_representative() {
        // 代表 Key 的三种限制只约束真实 Key，Provider 级限制仍必须约束整个组。
        let candidate = sample_candidate();
        let active_request = StoredRequestCandidate::new(
            "candidate-1".to_string(),
            "request-1".to_string(),
            None,
            None,
            None,
            None,
            0,
            0,
            Some(candidate.provider_id.clone()),
            Some(candidate.endpoint_id.clone()),
            Some(candidate.key_id.clone()),
            aether_data_contracts::repository::candidates::RequestCandidateStatus::Streaming,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            95_000,
            Some(95_000),
            None,
        )
        .expect("active request should build");
        for (health, concurrent_limit, rpm_limit, reason) in [
            (0.0, None, None, "key_health_score_zero"),
            (1.0, Some(1), None, "provider_key_concurrency_limit_reached"),
            (1.0, None, Some(1), "key_rpm_exhausted"),
        ] {
            let mut key = low_balance_key();
            key.status_snapshot = None;
            key.health_by_format = Some(json!({"openai:chat": {"health_score": health}}));
            key.concurrent_limit = concurrent_limit;
            key.rpm_limit = rpm_limit;
            let mut snapshot = CandidateRuntimeSelectionSnapshot {
                recent_candidates: vec![active_request.clone()],
                provider_concurrent_limits: BTreeMap::new(),
                provider_key_rpm_states: BTreeMap::from([(key.id.clone(), key)]),
                pool_provider_ids: BTreeSet::new(),
                provider_quota_blocks_requests: BTreeMap::new(),
                key_quota_hard_blocked: BTreeMap::new(),
                key_account_quota_exhausted: BTreeMap::new(),
                key_balance_below_minimum: BTreeMap::new(),
                key_oauth_invalid: BTreeMap::new(),
                provider_key_rpm_reset_ats: BTreeMap::new(),
            };
            assert_eq!(
                current_candidate_runtime_skip_reason(&candidate, &snapshot, 100),
                Some(reason)
            );
            assert!(!is_candidate_selectable(&candidate, &snapshot, 100));

            snapshot
                .pool_provider_ids
                .insert(candidate.provider_id.clone());
            assert_eq!(
                current_candidate_runtime_skip_reason(&candidate, &snapshot, 100),
                None
            );
            assert!(is_candidate_selectable(&candidate, &snapshot, 100));

            snapshot
                .provider_concurrent_limits
                .insert(candidate.provider_id.clone(), 1);
            assert_eq!(
                current_candidate_runtime_skip_reason(&candidate, &snapshot, 100),
                Some("provider_concurrency_limit_reached")
            );
            assert!(!is_candidate_selectable(&candidate, &snapshot, 100));
            snapshot.provider_concurrent_limits.clear();
            snapshot
                .provider_quota_blocks_requests
                .insert(candidate.provider_id.clone(), true);
            assert_eq!(
                current_candidate_runtime_skip_reason(&candidate, &snapshot, 100),
                Some("provider_quota_blocked")
            );
            assert!(!is_candidate_selectable(&candidate, &snapshot, 100));
        }
    }
}
