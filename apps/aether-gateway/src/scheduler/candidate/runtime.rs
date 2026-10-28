use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;

use aether_admin::provider::{
    pool as admin_provider_pool_pure, status as admin_provider_status_pure,
};
use aether_data_contracts::repository::candidates::StoredRequestCandidate;
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
use aether_provider_pool::{
    provider_pool_key_balance_below_minimum, provider_pool_key_runtime_quota_blocked,
    provider_pool_key_supports_discovered_model, resolve_provider_pool_key_thinking_model,
};
use aether_scheduler_core::{
    auth_api_key_concurrency_limit_reached, build_provider_concurrent_limit_map,
    candidate_runtime_skip_reason_with_state, effective_provider_key_rpm_limit,
    CandidateRuntimeSelectabilityInput,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::Instant;
use tracing::warn;

use crate::data::auth::GatewayAuthApiKeySnapshot;
use crate::orchestration::AntigravityModelQuotaCandidate;
use crate::provider_transport::codex_oauth_discovered_model_skip_reason;
use crate::scheduler::state::SchedulerModelQuotaState;
use crate::GatewayError;

use super::selection::SchedulerSkippedCandidate;
use super::SchedulerThinkingSelection;
use super::{SchedulerMinimalCandidateSelectionCandidate, SchedulerRuntimeState};

const ANTIGRAVITY_PROVIDER_TYPE: &str = "antigravity";

/// 运行时模型额度事实的共享后端读取失败；它是状态不可用，不是额度耗尽。
pub(super) const KEY_QUOTA_STATE_UNAVAILABLE_SKIP_REASON: &str = "key_quota_state_unavailable";

pub(super) use aether_scheduler_core::should_skip_provider_quota;

pub(super) struct CandidateRuntimeSelectionSnapshot {
    pub(super) recent_candidates: Vec<StoredRequestCandidate>,
    pub(super) provider_concurrent_limits: BTreeMap<String, usize>,
    pub(super) provider_key_rpm_states: BTreeMap<String, StoredProviderCatalogKey>,
    pub(super) pool_provider_ids: BTreeSet<String>,
    provider_quota_blocks_requests: BTreeMap<String, bool>,
    key_quota_hard_blocked: BTreeMap<String, bool>,
    key_account_quota_exhausted: BTreeMap<String, BTreeMap<String, bool>>,
    /// 共享运行态上报的精确模型额度耗尽，只索引候选自身上游模型名。
    key_runtime_model_quota_blocked: BTreeMap<String, BTreeSet<String>>,
    /// 共享运行态读取失败：该 Key/模型按状态不可用处理，绝不伪造成额度耗尽。
    key_runtime_model_quota_state_unavailable: BTreeMap<String, BTreeSet<String>>,
    /// 真实 Key 的标准余额是否明确低于内部调度下限。
    key_balance_below_minimum: BTreeMap<String, bool>,
    key_oauth_invalid: BTreeMap<String, bool>,
    provider_key_rpm_reset_ats: BTreeMap<String, Option<u64>>,
}

/// 一次性读取候选准入所需运行态，确保排序和最终诊断消费同一余额事实。
///
/// thinking 选择在强读 Key 之后、任何模型级额度/发现过滤之前解析实际上游模型；
/// 缺失请求档位的账号在这里被提前拒绝，不会以默认档位进入额度快照。
pub(super) async fn read_candidate_runtime_selection_snapshot(
    state: &(impl SchedulerRuntimeState + ?Sized),
    candidates: &mut Vec<SchedulerMinimalCandidateSelectionCandidate>,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    now_unix_secs: u64,
    thinking: Option<SchedulerThinkingSelection<'_>>,
) -> Result<
    (
        CandidateRuntimeSelectionSnapshot,
        Vec<SchedulerSkippedCandidate>,
    ),
    GatewayError,
> {
    // 强读目录事实必须先于模型解析：发现目录是账号级事实，缓存读取会污染档位选择。
    let provider_key_rpm_states = read_provider_key_rpm_states(state, candidates).await?;
    let thinking_skipped = thinking
        .map(|thinking| {
            resolve_candidate_thinking_models(candidates, &provider_key_rpm_states, thinking)
        })
        .unwrap_or_default();
    let provider_concurrent_limits = read_provider_concurrent_limits(state, candidates).await?;
    let provider_pool_state = read_provider_pool_state_map(state, candidates).await?;
    let pool_provider_ids = provider_pool_state
        .iter()
        .filter_map(|(provider_id, state)| state.pool_enabled.then_some(provider_id.clone()))
        .collect::<BTreeSet<_>>();
    // 强读目录事实之后再批量读取共享运行态事实；非 Antigravity 与 PoolGroup
    // 代表候选不构建描述符，也不触发额外后端读取。
    let runtime_model_quota = read_runtime_model_quota_facts(
        state,
        candidates,
        &provider_key_rpm_states,
        &pool_provider_ids,
        now_unix_secs,
    )
    .await;
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
        &provider_pool_state,
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

    Ok((
        CandidateRuntimeSelectionSnapshot {
            recent_candidates,
            provider_concurrent_limits,
            provider_key_rpm_states,
            pool_provider_ids,
            provider_quota_blocks_requests,
            key_quota_hard_blocked,
            key_account_quota_exhausted,
            key_runtime_model_quota_blocked: runtime_model_quota.blocked,
            key_runtime_model_quota_state_unavailable: runtime_model_quota.state_unavailable,
            key_balance_below_minimum,
            key_oauth_invalid,
            provider_key_rpm_reset_ats,
        },
        thinking_skipped,
    ))
}

/// 在额度快照之前按账号强读到的发现目录解析实际上游模型。
///
/// PoolGroup 代表候选只持有占位模型，必须等真实成员展开后各自解析，因此这里跳过。
/// 缺少请求档位的账号以 `key_model_disabled` 拒绝；真实 Key 强读缺失以
/// `key_quota_state_unavailable` 跳过（状态不可用，不是额度耗尽或模型禁用），
/// 都不会回退到默认档位或其他账号。
fn resolve_candidate_thinking_models(
    candidates: &mut Vec<SchedulerMinimalCandidateSelectionCandidate>,
    keys: &BTreeMap<String, StoredProviderCatalogKey>,
    thinking: SchedulerThinkingSelection<'_>,
) -> Vec<SchedulerSkippedCandidate> {
    let mut skipped = Vec::new();
    candidates.retain_mut(|candidate| {
        if candidate.routing_facts.provider_pool_enabled {
            return true;
        }
        // 只有 Antigravity 候选消费该档位选择；其他 Provider 保持既有模型名且不查目录。
        if !candidate
            .provider_type
            .trim()
            .eq_ignore_ascii_case(ANTIGRAVITY_PROVIDER_TYPE)
        {
            return true;
        }
        let Some(key) = keys.get(candidate.key_id.as_str()) else {
            // 强读目录缺失真实 Antigravity Key：报告状态不可用，绝不猜测额度或模型档位，
            // 并立即跳过该候选，避免无解析结果的候选进入 transport/执行准入而饿死兄弟账号。
            skipped.push(SchedulerSkippedCandidate {
                candidate: candidate.clone(),
                skip_reason: KEY_QUOTA_STATE_UNAVAILABLE_SKIP_REASON,
            });
            return false;
        };
        match resolve_provider_pool_key_thinking_model(
            key,
            candidate.provider_type.as_str(),
            thinking.requested_model,
            candidate.selected_provider_model_name.as_str(),
            Some(thinking.level),
        ) {
            Some(model) if model == candidate.selected_provider_model_name.as_str() => true,
            Some(model) => {
                candidate.selected_provider_model_name = model.to_string();
                true
            }
            None => {
                skipped.push(SchedulerSkippedCandidate {
                    candidate: candidate.clone(),
                    skip_reason: "key_model_disabled",
                });
                false
            }
        }
    });
    skipped
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
    let runtime_model_quota_blocked = !pool_group
        && snapshot
            .key_runtime_model_quota_blocked
            .get(candidate.key_id.as_str())
            .is_some_and(|models| models.contains(candidate.selected_provider_model_name.trim()));
    let runtime_model_quota_state_unavailable = !pool_group
        && snapshot
            .key_runtime_model_quota_state_unavailable
            .get(candidate.key_id.as_str())
            .is_some_and(|models| models.contains(candidate.selected_provider_model_name.trim()));
    let empty_key_runtime_states = BTreeMap::new();

    let reason = candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
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
            && (runtime_model_quota_blocked
                || snapshot
                    .key_account_quota_exhausted
                    .get(candidate.key_id.as_str())
                    .and_then(|models| models.get(candidate.selected_provider_model_name.as_str()))
                    .copied()
                    .unwrap_or(false)),
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
    });
    if reason.is_some() {
        return reason;
    }
    // 账号发现目录属于该 Key 自身的准入事实；代表 Key 不得据此淘汰整个 PoolGroup。
    if !pool_group {
        if let Some(key) = snapshot.provider_key_rpm_states.get(&candidate.key_id) {
            if let Some(reason) = codex_oauth_discovered_model_skip_reason(
                candidate.provider_type.as_str(),
                key.auth_type.as_str(),
                candidate.endpoint_api_format.as_str(),
                candidate.selected_provider_model_name.as_str(),
                key.upstream_metadata.as_ref(),
            ) {
                return Some(reason);
            }
            if !provider_pool_key_supports_discovered_model(
                key,
                candidate.provider_type.as_str(),
                candidate.selected_provider_model_name.as_str(),
            ) {
                return Some("key_model_disabled");
            }
        }
    }
    // 读取失败只表示无法确认该模型的运行态事实；报告状态不可用，绝不推断额度耗尽。
    if runtime_model_quota_state_unavailable {
        return Some(KEY_QUOTA_STATE_UNAVAILABLE_SKIP_REASON);
    }
    None
}

#[derive(Debug, Default)]
struct RuntimeModelQuotaFacts {
    blocked: BTreeMap<String, BTreeSet<String>>,
    state_unavailable: BTreeMap<String, BTreeSet<String>>,
}

/// 只读取真正会消费该事实的候选：非 Antigravity 与非 PoolGroup 代表候选既不构建
/// 描述符也不读取共享后端。结果按描述符顺序与身份一一对应。
async fn read_runtime_model_quota_facts(
    state: &(impl SchedulerRuntimeState + ?Sized),
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
    provider_key_rpm_states: &BTreeMap<String, StoredProviderCatalogKey>,
    pool_provider_ids: &BTreeSet<String>,
    now_unix_secs: u64,
) -> RuntimeModelQuotaFacts {
    let mut identities = Vec::<(String, String)>::new();
    let mut seen = BTreeSet::<(&str, &str)>::new();
    let mut descriptors = Vec::<AntigravityModelQuotaCandidate<'_>>::new();

    for candidate in candidates {
        if pool_provider_ids.contains(candidate.provider_id.as_str())
            || !candidate
                .provider_type
                .trim()
                .eq_ignore_ascii_case(ANTIGRAVITY_PROVIDER_TYPE)
            || candidate.selected_provider_model_name.trim().is_empty()
        {
            continue;
        }
        let Some(key) = provider_key_rpm_states.get(candidate.key_id.as_str()) else {
            continue;
        };
        let model = candidate.selected_provider_model_name.trim();
        if !seen.insert((candidate.key_id.as_str(), model)) {
            continue;
        }
        identities.push((candidate.key_id.clone(), model.to_string()));
        descriptors.push(AntigravityModelQuotaCandidate {
            key,
            provider_type: candidate.provider_type.as_str(),
            model,
        });
    }

    if descriptors.is_empty() {
        return RuntimeModelQuotaFacts::default();
    }

    let read = match state
        .read_antigravity_model_quota_blocked(&descriptors, now_unix_secs)
        .await
    {
        Ok(values) => Ok(values),
        Err(err) => {
            warn!(
                error = ?err,
                candidate_count = identities.len(),
                "failed to read antigravity runtime model quota state"
            );
            Err(())
        }
    };
    runtime_model_quota_facts_from_read(&identities, read)
}

/// Consumes one aligned batch result. `StateUnavailable` entries are per
/// candidate (for example one malformed ciphertext) and must not remove valid
/// siblings; `Err` means the shared backend failed for the whole batch.
fn runtime_model_quota_facts_from_read(
    identities: &[(String, String)],
    read: Result<Vec<SchedulerModelQuotaState>, ()>,
) -> RuntimeModelQuotaFacts {
    let Ok(values) = read else {
        return unavailable_runtime_model_quota_facts(identities);
    };
    if values.len() != identities.len() {
        warn!(
            expected = identities.len(),
            received = values.len(),
            "antigravity runtime model quota read returned a mismatched batch"
        );
        return unavailable_runtime_model_quota_facts(identities);
    }

    let mut facts = RuntimeModelQuotaFacts::default();
    for ((key_id, model), state) in identities.iter().zip(values) {
        match state {
            SchedulerModelQuotaState::Blocked => {
                facts
                    .blocked
                    .entry(key_id.clone())
                    .or_default()
                    .insert(model.clone());
            }
            SchedulerModelQuotaState::Available => {}
            SchedulerModelQuotaState::StateUnavailable => {
                facts
                    .state_unavailable
                    .entry(key_id.clone())
                    .or_default()
                    .insert(model.clone());
            }
        }
    }
    facts
}

fn unavailable_runtime_model_quota_facts(
    identities: &[(String, String)],
) -> RuntimeModelQuotaFacts {
    let mut facts = RuntimeModelQuotaFacts::default();
    for (key_id, model) in identities {
        facts
            .state_unavailable
            .entry(key_id.clone())
            .or_default()
            .insert(model.clone());
    }
    facts
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
    reserve_minimum_quota: bool,
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
                .and_then(|value| value.get("ignore_exhausted_accounts"))
                .and_then(serde_json::Value::as_bool)
                .map(|ignore| !ignore)
                .unwrap_or(true);
            let reserve_minimum_quota = pool_advanced
                .and_then(serde_json::Value::as_object)
                .and_then(|value| value.get("reserve_minimum_quota"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            (
                provider.id,
                ProviderPoolState {
                    pool_enabled: pool_advanced.is_some(),
                    skip_exhausted_accounts,
                    reserve_minimum_quota,
                },
            )
        })
        .collect())
}

fn read_key_account_quota_exhaustion_map(
    candidates: &[SchedulerMinimalCandidateSelectionCandidate],
    provider_key_rpm_states: &BTreeMap<String, StoredProviderCatalogKey>,
    provider_pool_state: &BTreeMap<String, ProviderPoolState>,
) -> BTreeMap<String, BTreeMap<String, bool>> {
    let mut exhausted_by_key = BTreeMap::<String, BTreeMap<String, bool>>::new();
    for candidate in candidates {
        if let Some(models) = exhausted_by_key.get_mut(candidate.key_id.as_str()) {
            if models.contains_key(candidate.selected_provider_model_name.as_str()) {
                continue;
            }
            models
                .entry(candidate.selected_provider_model_name.clone())
                .or_insert_with(|| {
                    candidate_account_quota_exhausted(
                        candidate,
                        provider_key_rpm_states,
                        provider_pool_state,
                    )
                });
            continue;
        }
        exhausted_by_key
            .entry(candidate.key_id.clone())
            .or_default()
            .entry(candidate.selected_provider_model_name.clone())
            .or_insert_with(|| {
                candidate_account_quota_exhausted(
                    candidate,
                    provider_key_rpm_states,
                    provider_pool_state,
                )
            });
    }
    exhausted_by_key
}

fn candidate_account_quota_exhausted(
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
    provider_key_rpm_states: &BTreeMap<String, StoredProviderCatalogKey>,
    provider_pool_state: &BTreeMap<String, ProviderPoolState>,
) -> bool {
    provider_key_rpm_states
        .get(candidate.key_id.as_str())
        .is_some_and(|key| {
            let account_exhausted = admin_provider_pool_pure::admin_pool_key_model_quota_exhausted(
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
            let hard_blocked = admin_provider_pool_pure::admin_pool_key_model_quota_hard_blocked(
                key,
                candidate.provider_type.as_str(),
                candidate.selected_provider_model_name.as_str(),
            );
            let pool_state = provider_pool_state
                .get(candidate.provider_id.as_str())
                .copied()
                .unwrap_or_default();
            let reserve_reached = pool_state.reserve_minimum_quota
                && admin_provider_pool_pure::admin_pool_key_minimum_quota_reached(
                    key,
                    candidate.provider_type.as_str(),
                    Some(candidate.selected_provider_model_name.as_str()),
                );
            hard_blocked
                || reserve_reached
                || (pool_state.skip_exhausted_accounts && account_exhausted)
        })
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
    use aether_provider_transport::antigravity::AntigravityThinkingLevel;
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
            key_runtime_model_quota_blocked: BTreeMap::new(),
            key_runtime_model_quota_state_unavailable: BTreeMap::new(),
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
    fn codex_model_discovery_runtime_checks_real_key_without_rejecting_pool_representative() {
        let mut candidate = sample_candidate();
        candidate.provider_type = "codex".to_string();
        candidate.key_auth_type = "oauth".to_string();
        candidate.endpoint_api_format = "openai:responses".to_string();
        candidate.global_model_name = "client-alias".to_string();
        candidate.selected_provider_model_name = "gpt-6.1-sol".to_string();

        let mut key = low_balance_key();
        key.auth_type = "oauth".to_string();
        key.status_snapshot = None;
        key.allowed_models = Some(json!(["client-alias", "gpt-6.1-sol"]));
        let mut snapshot = CandidateRuntimeSelectionSnapshot {
            recent_candidates: Vec::new(),
            provider_concurrent_limits: BTreeMap::new(),
            provider_key_rpm_states: BTreeMap::from([(key.id.clone(), key)]),
            pool_provider_ids: BTreeSet::new(),
            provider_quota_blocks_requests: BTreeMap::new(),
            key_quota_hard_blocked: BTreeMap::new(),
            key_account_quota_exhausted: BTreeMap::new(),
            key_runtime_model_quota_blocked: BTreeMap::new(),
            key_runtime_model_quota_state_unavailable: BTreeMap::new(),
            key_balance_below_minimum: BTreeMap::new(),
            key_oauth_invalid: BTreeMap::new(),
            provider_key_rpm_reset_ats: BTreeMap::new(),
        };
        assert_eq!(
            current_candidate_runtime_skip_reason(&candidate, &snapshot, 100),
            Some("key_model_disabled")
        );

        snapshot
            .pool_provider_ids
            .insert(candidate.provider_id.clone());
        assert_eq!(
            current_candidate_runtime_skip_reason(&candidate, &snapshot, 100),
            None
        );
        snapshot.pool_provider_ids.clear();
        snapshot
            .provider_key_rpm_states
            .get_mut("key-1")
            .unwrap()
            .upstream_metadata =
            Some(json!({"codex_model_discovery": {"model_ids": ["gpt-basic-test"]}}));
        assert_eq!(
            current_candidate_runtime_skip_reason(&candidate, &snapshot, 100),
            Some("key_model_disabled")
        );

        snapshot
            .provider_key_rpm_states
            .get_mut("key-1")
            .unwrap()
            .upstream_metadata =
            Some(json!({"codex_model_discovery": {"model_ids": ["gpt-6.1-sol"]}}));
        assert!(is_candidate_selectable(&candidate, &snapshot, 100));
        snapshot
            .key_oauth_invalid
            .insert(candidate.key_id.clone(), true);
        snapshot
            .provider_key_rpm_states
            .get_mut("key-1")
            .unwrap()
            .upstream_metadata = None;
        assert_eq!(
            current_candidate_runtime_skip_reason(&candidate, &snapshot, 100),
            Some("oauth_invalid")
        );
    }

    #[test]
    fn runtime_antigravity_discovered_model_denies_real_key_but_not_pool_group_representative() {
        // 事故形态：自动抓取关闭、无 allowlist，发现映射只有无关占位模型。
        let mut candidate = sample_candidate();
        candidate.provider_type = "antigravity".to_string();
        candidate.endpoint_api_format = "gemini:generate_content".to_string();
        candidate.selected_provider_model_name = "gemini-4-argon-high".to_string();

        let mut key = low_balance_key();
        key.status_snapshot = None;
        key.auto_fetch_models = false;
        key.allowed_models = None;
        key.upstream_metadata = Some(json!({
            "antigravity": {
                "quota_by_model": {
                    "gemini-4-argon-disabled": {}
                }
            }
        }));

        let mut snapshot = CandidateRuntimeSelectionSnapshot {
            recent_candidates: Vec::new(),
            provider_concurrent_limits: BTreeMap::new(),
            provider_key_rpm_states: BTreeMap::from([(key.id.clone(), key.clone())]),
            pool_provider_ids: BTreeSet::new(),
            provider_quota_blocks_requests: BTreeMap::new(),
            key_quota_hard_blocked: BTreeMap::new(),
            key_account_quota_exhausted: BTreeMap::new(),
            key_runtime_model_quota_blocked: BTreeMap::new(),
            key_runtime_model_quota_state_unavailable: BTreeMap::new(),
            key_balance_below_minimum: BTreeMap::new(),
            key_oauth_invalid: BTreeMap::new(),
            provider_key_rpm_reset_ats: BTreeMap::new(),
        };

        assert_eq!(
            current_candidate_runtime_skip_reason(&candidate, &snapshot, 100),
            Some("key_model_disabled")
        );
        assert!(!is_candidate_selectable(&candidate, &snapshot, 100));

        // PoolGroup 代表 Key 不得因发现约束淘汰整组，真实成员在展开后检查。
        snapshot
            .pool_provider_ids
            .insert(candidate.provider_id.clone());
        assert_eq!(
            current_candidate_runtime_skip_reason(&candidate, &snapshot, 100),
            None
        );
        assert!(is_candidate_selectable(&candidate, &snapshot, 100));
        snapshot.pool_provider_ids.clear();

        // 精确发现同名上游模型后恢复正常准入。
        snapshot
            .provider_key_rpm_states
            .get_mut("key-1")
            .expect("key snapshot should exist")
            .upstream_metadata = Some(json!({
            "antigravity": {
                "quota_by_model": {
                    "gemini-4-argon-high": {}
                }
            }
        }));
        assert_eq!(
            current_candidate_runtime_skip_reason(&candidate, &snapshot, 100),
            None
        );

        // 既有强运行态诊断保持优先，模型禁用不会覆盖健康/额度原因。
        key.health_by_format = Some(json!({
            "gemini:generate_content": { "health_score": 0.0 }
        }));
        snapshot.provider_key_rpm_states.insert(key.id.clone(), key);
        assert_eq!(
            current_candidate_runtime_skip_reason(&candidate, &snapshot, 100),
            Some("key_health_score_zero")
        );
    }

    #[test]
    fn thinking_selection_resolves_ordinary_variant_and_rejects_missing_tier() {
        // 客户端未显式选择后缀时，正文档位必须在该账号自己的实际模型上生效。
        let mut candidate = sample_candidate();
        candidate.provider_type = "antigravity".to_string();
        candidate.endpoint_api_format = "gemini:generate_content".to_string();
        candidate.global_model_name = "gemini-3-flash".to_string();
        candidate.selected_provider_model_name = "gemini-3-flash-high".to_string();

        let mut key = low_balance_key();
        key.status_snapshot = None;
        key.upstream_metadata = Some(json!({
            "antigravity": {
                "quota_by_model": {
                    "gemini-3-flash-low": {},
                    "gemini-3-flash-medium": {},
                    "gemini-3-flash-high": {}
                }
            }
        }));
        let keys = BTreeMap::from([(key.id.clone(), key)]);
        let selection = SchedulerThinkingSelection {
            requested_model: "gemini-3-flash",
            level: AntigravityThinkingLevel::Low,
        };

        let mut candidates = vec![candidate.clone()];
        let skipped = resolve_candidate_thinking_models(&mut candidates, &keys, selection);
        assert!(skipped.is_empty());
        assert_eq!(
            candidates[0].selected_provider_model_name,
            "gemini-3-flash-low"
        );

        // 缺少请求档位的账号必须被拒绝，而不是回退默认 high。
        let mut missing_key = keys.get("key-1").expect("key should exist").clone();
        missing_key.upstream_metadata = Some(json!({
            "antigravity": {
                "quota_by_model": {
                    "gemini-3-flash-high": {}
                }
            }
        }));
        let missing_keys = BTreeMap::from([(missing_key.id.clone(), missing_key)]);
        let mut missing_candidates = vec![candidate.clone()];
        let skipped =
            resolve_candidate_thinking_models(&mut missing_candidates, &missing_keys, selection);
        assert!(missing_candidates.is_empty());
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].skip_reason, "key_model_disabled");

        // PoolGroup 代表候选只保留占位模型，真实成员展开后才解析。
        let mut pool_candidate = candidate;
        pool_candidate.routing_facts.provider_pool_enabled = true;
        let mut pool_candidates = vec![pool_candidate];
        let skipped = resolve_candidate_thinking_models(&mut pool_candidates, &keys, selection);
        assert!(skipped.is_empty());
        assert_eq!(
            pool_candidates[0].selected_provider_model_name,
            "gemini-3-flash-high"
        );
    }

    #[test]
    fn thinking_selection_respects_key_allowed_models_for_resolved_variant() {
        // 只允许 low 的账号：low 请求命中自身变体，high 请求不得沿用默认 high。
        let mut candidate = sample_candidate();
        candidate.provider_type = "antigravity".to_string();
        candidate.endpoint_api_format = "gemini:generate_content".to_string();
        candidate.global_model_name = "gemini-3-flash".to_string();
        candidate.selected_provider_model_name = "gemini-3-flash-high".to_string();

        let mut key = low_balance_key();
        key.status_snapshot = None;
        key.allowed_models = Some(json!(["gemini-3-flash-low"]));
        key.upstream_metadata = Some(json!({
            "antigravity": {
                "quota_by_model": {
                    "gemini-3-flash-low": {},
                    "gemini-3-flash-high": {}
                }
            }
        }));
        let keys = BTreeMap::from([(key.id.clone(), key)]);

        let mut low_candidates = vec![candidate.clone()];
        let skipped = resolve_candidate_thinking_models(
            &mut low_candidates,
            &keys,
            SchedulerThinkingSelection {
                requested_model: "gemini-3-flash",
                level: AntigravityThinkingLevel::Low,
            },
        );
        assert!(skipped.is_empty());
        assert_eq!(
            low_candidates[0].selected_provider_model_name,
            "gemini-3-flash-low"
        );

        let mut high_candidates = vec![candidate];
        let skipped = resolve_candidate_thinking_models(
            &mut high_candidates,
            &keys,
            SchedulerThinkingSelection {
                requested_model: "gemini-3-flash",
                level: AntigravityThinkingLevel::High,
            },
        );
        assert!(high_candidates.is_empty());
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].skip_reason, "key_model_disabled");
    }

    #[test]
    fn thinking_selection_skips_missing_strong_key_without_starving_sibling() {
        // 首选 Antigravity Key 强读缺失时只能以状态不可用跳过；兄弟账号必须保留。
        let mut missing = sample_candidate();
        missing.provider_type = "antigravity".to_string();
        missing.endpoint_api_format = "gemini:generate_content".to_string();
        missing.global_model_name = "gemini-3-flash".to_string();
        missing.selected_provider_model_name = "gemini-3-flash-high".to_string();
        missing.key_id = "key-missing".to_string();

        let mut sibling = missing.clone();
        sibling.key_id = "key-sibling".to_string();

        let mut sibling_key = low_balance_key();
        sibling_key.id = "key-sibling".to_string();
        sibling_key.status_snapshot = None;
        sibling_key.upstream_metadata = Some(json!({
            "antigravity": {
                "quota_by_model": {
                    "gemini-3-flash-high": {}
                }
            }
        }));
        let keys = BTreeMap::from([(sibling_key.id.clone(), sibling_key)]);

        let mut candidates = vec![missing, sibling];
        let skipped = resolve_candidate_thinking_models(
            &mut candidates,
            &keys,
            SchedulerThinkingSelection {
                requested_model: "gemini-3-flash",
                level: AntigravityThinkingLevel::High,
            },
        );

        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].candidate.key_id, "key-missing");
        assert_eq!(
            skipped[0].skip_reason,
            KEY_QUOTA_STATE_UNAVAILABLE_SKIP_REASON
        );
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key_id, "key-sibling");
        assert_eq!(
            candidates[0].selected_provider_model_name,
            "gemini-3-flash-high"
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
                key_runtime_model_quota_blocked: BTreeMap::new(),
                key_runtime_model_quota_state_unavailable: BTreeMap::new(),
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

    #[test]
    fn antigravity_same_key_multi_model_quota_preserves_each_model_across_orders_and_switches() {
        let mut first_account_candidate = sample_candidate();
        first_account_candidate.provider_type = "antigravity".to_string();
        first_account_candidate.endpoint_api_format = "gemini:generate_content".to_string();
        first_account_candidate.key_id = "key-0".to_string();
        first_account_candidate.key_name = "key-0".to_string();
        first_account_candidate.global_model_name = "gemini-cli".to_string();
        first_account_candidate.selected_provider_model_name = "gemini-2.5-flash".to_string();

        let mut exhausted_candidate = first_account_candidate.clone();
        exhausted_candidate.key_id = "key-1".to_string();
        exhausted_candidate.key_name = "key-1".to_string();
        exhausted_candidate.model_id = "model-sonnet-4-5".to_string();
        exhausted_candidate.selected_provider_model_name = "claude-sonnet-4-5".to_string();

        let mut duplicate_exhausted_candidate = exhausted_candidate.clone();
        duplicate_exhausted_candidate.endpoint_id = "endpoint-2".to_string();

        let mut available_candidate = exhausted_candidate.clone();
        available_candidate.model_id = "model-sonnet-4-6".to_string();
        available_candidate.selected_provider_model_name = "claude-sonnet-4-6".to_string();

        let mut first_key = low_balance_key();
        first_key.id = "key-0".to_string();
        first_key.name = "key-0".to_string();
        first_key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "antigravity",
                "exhausted": false,
                "windows": [{
                    "code": "model:gemini-2.5-flash",
                    "scope": "model",
                    "model": "gemini-2.5-flash",
                    "remaining_ratio": 0.0,
                    "used_ratio": 1.0,
                    "is_exhausted": true,
                    "reset_at": 4_102_444_800u64
                }]
            }
        }));

        let quota_payload = json!({
            "version": 2,
            "provider_type": "antigravity",
            "exhausted": false,
            "windows": [
                {
                    "code": "model:claude-sonnet-4-5",
                    "scope": "model",
                    "model": "claude-sonnet-4-5",
                    "remaining_ratio": 0.0,
                    "used_ratio": 1.0,
                    "is_exhausted": true,
                    "reset_at": 4_102_444_800u64
                },
                {
                    "code": "model:claude-sonnet-4-6",
                    "scope": "model",
                    "model": "claude-sonnet-4-6",
                    "remaining_ratio": 0.8,
                    "used_ratio": 0.2,
                    "is_exhausted": false,
                    "reset_at": 4_102_444_800u64
                }
            ]
        });

        for ignore_exhausted_accounts in [false, true] {
            let pool_state = BTreeMap::from([(
                exhausted_candidate.provider_id.clone(),
                ProviderPoolState {
                    pool_enabled: false,
                    skip_exhausted_accounts: !ignore_exhausted_accounts,
                    reserve_minimum_quota: false,
                },
            )]);
            for candidates in [
                vec![
                    first_account_candidate.clone(),
                    exhausted_candidate.clone(),
                    duplicate_exhausted_candidate.clone(),
                    available_candidate.clone(),
                ],
                vec![
                    first_account_candidate.clone(),
                    available_candidate.clone(),
                    exhausted_candidate.clone(),
                    duplicate_exhausted_candidate.clone(),
                ],
            ] {
                let mut key = low_balance_key();
                key.status_snapshot = Some(json!({"quota": quota_payload.clone()}));
                let provider_key_rpm_states = BTreeMap::from([
                    (first_key.id.clone(), first_key.clone()),
                    (key.id.clone(), key.clone()),
                ]);
                let key_account_quota_exhausted = read_key_account_quota_exhaustion_map(
                    &candidates,
                    &provider_key_rpm_states,
                    &pool_state,
                );
                let key_models = key_account_quota_exhausted
                    .get("key-1")
                    .expect("key-1 model map should exist");
                assert_eq!(key_models.len(), 2);
                assert_eq!(key_models.get("claude-sonnet-4-5"), Some(&true));
                assert_eq!(key_models.get("claude-sonnet-4-6"), Some(&false));
                assert!(!key_models.contains_key("gemini-cli"));
                assert!(!key_models.contains_key("gemini-2.5-flash"));

                let mut snapshot = CandidateRuntimeSelectionSnapshot {
                    recent_candidates: Vec::new(),
                    provider_concurrent_limits: BTreeMap::new(),
                    provider_key_rpm_states,
                    pool_provider_ids: BTreeSet::new(),
                    provider_quota_blocks_requests: BTreeMap::new(),
                    key_quota_hard_blocked: BTreeMap::new(),
                    key_account_quota_exhausted,
                    key_runtime_model_quota_blocked: BTreeMap::new(),
                    key_runtime_model_quota_state_unavailable: BTreeMap::new(),
                    key_balance_below_minimum: BTreeMap::new(),
                    key_oauth_invalid: BTreeMap::new(),
                    provider_key_rpm_reset_ats: BTreeMap::new(),
                };

                assert_eq!(
                    current_candidate_runtime_skip_reason(&exhausted_candidate, &snapshot, 100),
                    Some("account_quota_exhausted")
                );
                assert_eq!(
                    current_candidate_runtime_skip_reason(
                        &duplicate_exhausted_candidate,
                        &snapshot,
                        100,
                    ),
                    Some("account_quota_exhausted")
                );
                assert!(!is_candidate_selectable(
                    &exhausted_candidate,
                    &snapshot,
                    100
                ));
                assert_eq!(
                    current_candidate_runtime_skip_reason(&available_candidate, &snapshot, 100),
                    None
                );
                assert!(is_candidate_selectable(
                    &available_candidate,
                    &snapshot,
                    100
                ));

                snapshot
                    .pool_provider_ids
                    .insert(exhausted_candidate.provider_id.clone());
                assert_eq!(
                    current_candidate_runtime_skip_reason(&exhausted_candidate, &snapshot, 100),
                    None
                );
                assert_eq!(
                    current_candidate_runtime_skip_reason(&available_candidate, &snapshot, 100),
                    None
                );

                let mut manually_blocked_key = key;
                manually_blocked_key.status_snapshot = Some(json!({
                    "scheduling": {
                        "code": "quota_exhausted",
                        "blocked": true,
                        "requires_manual_recovery": true
                    },
                    "quota": quota_payload.clone()
                }));
                let manual_rpm_states = BTreeMap::from([
                    (first_key.id.clone(), first_key.clone()),
                    (manually_blocked_key.id.clone(), manually_blocked_key),
                ]);
                snapshot.key_quota_hard_blocked =
                    read_key_quota_hard_blocked_map(&candidates, &manual_rpm_states);
                snapshot.key_account_quota_exhausted = read_key_account_quota_exhaustion_map(
                    &candidates,
                    &manual_rpm_states,
                    &pool_state,
                );
                snapshot.provider_key_rpm_states = manual_rpm_states;
                assert_eq!(
                    current_candidate_runtime_skip_reason(&exhausted_candidate, &snapshot, 100),
                    None
                );
                assert_eq!(
                    current_candidate_runtime_skip_reason(&available_candidate, &snapshot, 100),
                    None
                );

                snapshot.pool_provider_ids.clear();
                assert_eq!(
                    current_candidate_runtime_skip_reason(&exhausted_candidate, &snapshot, 100),
                    Some("key_quota_exhausted")
                );
                assert_eq!(
                    current_candidate_runtime_skip_reason(&available_candidate, &snapshot, 100),
                    Some("key_quota_exhausted")
                );
            }
        }
    }

    #[test]
    fn non_antigravity_same_key_multi_model_quota_honors_ignore_exhausted_switch() {
        let mut exhausted_candidate = sample_candidate();
        exhausted_candidate.global_model_name = "client-alias".to_string();
        exhausted_candidate.selected_provider_model_name = "vendor-alpha".to_string();

        let mut available_candidate = exhausted_candidate.clone();
        available_candidate.model_id = "model-2".to_string();
        available_candidate.selected_provider_model_name = "omega-sibling".to_string();

        let mut key = low_balance_key();
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "deepseek",
                "exhausted": false,
                "windows": [
                    {
                        "code": "model:vendor-alpha",
                        "scope": "model",
                        "model": "vendor-alpha",
                        "remaining_ratio": 0.0,
                        "used_ratio": 1.0,
                        "is_exhausted": true,
                        "reset_at": 4_102_444_800u64
                    },
                    {
                        "code": "model:omega-sibling",
                        "scope": "model",
                        "model": "omega-sibling",
                        "remaining_ratio": 0.8,
                        "used_ratio": 0.2,
                        "is_exhausted": false,
                        "reset_at": 4_102_444_800u64
                    }
                ]
            }
        }));
        let provider_key_rpm_states = BTreeMap::from([(key.id.clone(), key)]);

        for ignore_exhausted_accounts in [false, true] {
            let pool_state = BTreeMap::from([(
                exhausted_candidate.provider_id.clone(),
                ProviderPoolState {
                    pool_enabled: false,
                    skip_exhausted_accounts: !ignore_exhausted_accounts,
                    reserve_minimum_quota: false,
                },
            )]);
            for candidates in [
                vec![exhausted_candidate.clone(), available_candidate.clone()],
                vec![available_candidate.clone(), exhausted_candidate.clone()],
            ] {
                let snapshot = CandidateRuntimeSelectionSnapshot {
                    recent_candidates: Vec::new(),
                    provider_concurrent_limits: BTreeMap::new(),
                    provider_key_rpm_states: provider_key_rpm_states.clone(),
                    pool_provider_ids: BTreeSet::new(),
                    provider_quota_blocks_requests: BTreeMap::new(),
                    key_quota_hard_blocked: BTreeMap::new(),
                    key_account_quota_exhausted: read_key_account_quota_exhaustion_map(
                        &candidates,
                        &provider_key_rpm_states,
                        &pool_state,
                    ),
                    key_runtime_model_quota_blocked: BTreeMap::new(),
                    key_runtime_model_quota_state_unavailable: BTreeMap::new(),
                    key_balance_below_minimum: BTreeMap::new(),
                    key_oauth_invalid: BTreeMap::new(),
                    provider_key_rpm_reset_ats: BTreeMap::new(),
                };
                let expected_exhausted_reason =
                    (!ignore_exhausted_accounts).then_some("account_quota_exhausted");
                assert_eq!(
                    current_candidate_runtime_skip_reason(&exhausted_candidate, &snapshot, 100),
                    expected_exhausted_reason
                );
                assert_eq!(
                    current_candidate_runtime_skip_reason(&available_candidate, &snapshot, 100),
                    None
                );
            }
        }
    }
}

#[cfg(test)]
mod reserve_minimum_quota_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reserve_minimum_quota_is_independent_of_skip_exhausted_accounts() {
        let candidate = SchedulerMinimalCandidateSelectionCandidate {
            provider_id: "provider-codex".to_string(),
            provider_name: "codex".to_string(),
            provider_type: "codex".to_string(),
            provider_priority: 0,
            endpoint_id: "endpoint-codex".to_string(),
            endpoint_api_format: "openai:responses".to_string(),
            key_id: "key-codex".to_string(),
            key_name: "codex".to_string(),
            key_auth_type: "oauth".to_string(),
            key_internal_priority: 0,
            key_global_priority_for_format: None,
            key_capabilities: None,
            routing_facts: Default::default(),
            model_id: "model-codex".to_string(),
            global_model_id: "global-model-codex".to_string(),
            global_model_name: "gpt-5".to_string(),
            selected_provider_model_name: "gpt-5".to_string(),
            supports_streaming: true,
            mapping_matched_model: None,
        };
        let mut key = StoredProviderCatalogKey::new(
            candidate.key_id.clone(),
            candidate.provider_id.clone(),
            "codex".to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("key should build");
        for reserve_enabled in [false, true] {
            for used_percent in [99.0, 98.0] {
                key.upstream_metadata =
                    Some(json!({"codex": {"primary_used_percent": used_percent}}));
                let exhausted = read_key_account_quota_exhaustion_map(
                    std::slice::from_ref(&candidate),
                    &BTreeMap::from([(key.id.clone(), key.clone())]),
                    &BTreeMap::from([(
                        candidate.provider_id.clone(),
                        ProviderPoolState {
                            pool_enabled: true,
                            reserve_minimum_quota: reserve_enabled,
                            skip_exhausted_accounts: false,
                        },
                    )]),
                );
                assert_eq!(
                    exhausted
                        .get(key.id.as_str())
                        .and_then(|models| {
                            models.get(candidate.selected_provider_model_name.as_str())
                        })
                        .copied(),
                    Some(reserve_enabled && used_percent >= 99.0)
                );
            }
        }
    }
}

#[cfg(test)]
mod runtime_model_quota_tests {
    use super::*;

    fn antigravity_candidate(
        key_id: &str,
        model: &str,
    ) -> SchedulerMinimalCandidateSelectionCandidate {
        SchedulerMinimalCandidateSelectionCandidate {
            provider_id: "provider-antigravity".to_string(),
            provider_name: "antigravity".to_string(),
            provider_type: ANTIGRAVITY_PROVIDER_TYPE.to_string(),
            provider_priority: 0,
            endpoint_id: "endpoint-gemini".to_string(),
            endpoint_api_format: "gemini:generate_content".to_string(),
            key_id: key_id.to_string(),
            key_name: key_id.to_string(),
            key_auth_type: "oauth".to_string(),
            key_internal_priority: 0,
            key_global_priority_for_format: None,
            key_capabilities: None,
            routing_facts: Default::default(),
            model_id: format!("model-{model}"),
            global_model_id: format!("global-{model}"),
            global_model_name: model.to_string(),
            selected_provider_model_name: model.to_string(),
            supports_streaming: true,
            mapping_matched_model: None,
        }
    }

    fn runtime_snapshot() -> CandidateRuntimeSelectionSnapshot {
        CandidateRuntimeSelectionSnapshot {
            recent_candidates: Vec::new(),
            provider_concurrent_limits: BTreeMap::new(),
            provider_key_rpm_states: BTreeMap::new(),
            pool_provider_ids: BTreeSet::new(),
            provider_quota_blocks_requests: BTreeMap::new(),
            key_quota_hard_blocked: BTreeMap::new(),
            key_account_quota_exhausted: BTreeMap::new(),
            key_runtime_model_quota_blocked: BTreeMap::new(),
            key_runtime_model_quota_state_unavailable: BTreeMap::new(),
            key_balance_below_minimum: BTreeMap::new(),
            key_oauth_invalid: BTreeMap::new(),
            provider_key_rpm_reset_ats: BTreeMap::new(),
        }
    }

    fn block(snapshot: &mut CandidateRuntimeSelectionSnapshot, key_id: &str, model: &str) {
        snapshot
            .key_runtime_model_quota_blocked
            .entry(key_id.to_string())
            .or_default()
            .insert(model.to_string());
    }

    fn mark_state_unavailable(
        snapshot: &mut CandidateRuntimeSelectionSnapshot,
        key_id: &str,
        model: &str,
    ) {
        snapshot
            .key_runtime_model_quota_state_unavailable
            .entry(key_id.to_string())
            .or_default()
            .insert(model.to_string());
    }

    #[test]
    fn runtime_model_quota_blocks_only_the_exact_model_and_key() {
        let gemini = antigravity_candidate("key-a", "gemini-3-pro");
        let claude = antigravity_candidate("key-a", "claude-sonnet-4-5");
        let sibling_account_gemini = antigravity_candidate("key-b", "gemini-3-pro");
        let mut snapshot = runtime_snapshot();
        block(&mut snapshot, "key-a", "gemini-3-pro");

        assert_eq!(
            current_candidate_runtime_skip_reason(&gemini, &snapshot, 100),
            Some("account_quota_exhausted")
        );
        assert_eq!(
            current_candidate_runtime_skip_reason(&claude, &snapshot, 100),
            None,
            "an exhausted model quota must not become an account-wide limit"
        );
        assert_eq!(
            current_candidate_runtime_skip_reason(&sibling_account_gemini, &snapshot, 100),
            None,
            "another account's quota must not be borrowed for the same model"
        );
    }

    #[test]
    fn runtime_model_quota_does_not_restrict_pool_group_representative() {
        let gemini = antigravity_candidate("key-a", "gemini-3-pro");
        let mut snapshot = runtime_snapshot();
        block(&mut snapshot, "key-a", "gemini-3-pro");
        snapshot
            .pool_provider_ids
            .insert(gemini.provider_id.clone());

        assert_eq!(
            current_candidate_runtime_skip_reason(&gemini, &snapshot, 100),
            None,
            "the representative Key defers model quota filtering to expanded real Keys"
        );
    }

    #[test]
    fn runtime_model_quota_read_failure_is_state_unavailable_not_exhaustion() {
        let gemini = antigravity_candidate("key-a", "gemini-3-pro");
        let claude = antigravity_candidate("key-a", "claude-sonnet-4-5");
        let mut snapshot = runtime_snapshot();
        mark_state_unavailable(&mut snapshot, "key-a", "gemini-3-pro");

        assert_eq!(
            current_candidate_runtime_skip_reason(&gemini, &snapshot, 100),
            Some(KEY_QUOTA_STATE_UNAVAILABLE_SKIP_REASON)
        );
        assert_eq!(
            current_candidate_runtime_skip_reason(&claude, &snapshot, 100),
            None,
            "state unavailability for one model must not restrict sibling models"
        );
    }

    #[test]
    fn runtime_model_quota_preserves_stronger_runtime_restriction_priority() {
        let gemini = antigravity_candidate("key-a", "gemini-3-pro");
        let mut snapshot = runtime_snapshot();
        block(&mut snapshot, "key-a", "gemini-3-pro");
        snapshot
            .key_quota_hard_blocked
            .insert("key-a".to_string(), true);

        assert_eq!(
            current_candidate_runtime_skip_reason(&gemini, &snapshot, 100),
            Some("key_quota_exhausted"),
            "an administrator-recoverable permanent block keeps its own diagnostic"
        );
    }

    #[test]
    fn runtime_model_quota_per_candidate_credential_failure_keeps_siblings_available() {
        // 单个账号的凭据无法解密时只让该 (Key, 模型) 状态不可用，
        // 同批次其他有效账号仍按共享事实调度。
        let identities = vec![
            ("key-a".to_string(), "gemini-3-pro".to_string()),
            ("key-b".to_string(), "gemini-3-pro".to_string()),
        ];
        let facts = runtime_model_quota_facts_from_read(
            &identities,
            Ok(vec![
                SchedulerModelQuotaState::StateUnavailable,
                SchedulerModelQuotaState::Blocked,
            ]),
        );

        assert!(facts
            .state_unavailable
            .get("key-a")
            .is_some_and(|models| models.contains("gemini-3-pro")));
        assert!(facts
            .blocked
            .get("key-b")
            .is_some_and(|models| models.contains("gemini-3-pro")));
        assert!(!facts.blocked.contains_key("key-a"));
        assert!(!facts.state_unavailable.contains_key("key-b"));

        // 共享后端失败继续按整批状态不可用处理。
        let failed = runtime_model_quota_facts_from_read(&identities, Err(()));
        assert!(failed.blocked.is_empty());
        assert_eq!(failed.state_unavailable.len(), 2);
    }
}
