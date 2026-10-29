use std::collections::{btree_map::Entry, BTreeMap, BTreeSet, VecDeque};
use std::sync::{
    atomic::{AtomicU64, Ordering as AtomicOrdering},
    Arc, LazyLock,
};

use aether_admin::provider::{
    pool as admin_provider_pool_pure, status as admin_provider_status_pure,
};
use aether_data_contracts::repository::candidate_selection::{
    StoredMinimalCandidateSelectionRow, StoredPoolKeyCandidateOrder,
    StoredPoolKeyCandidateRowsByKeyIdsQuery, StoredPoolKeyCandidateRowsQuery,
};
use aether_data_contracts::repository::pool_scores::{
    ListRankedPoolMembersQuery, PoolMemberHardState, PoolMemberIdentity,
    PoolMemberScheduleFeedback, PoolScoreScope, StoredPoolMemberScore, POOL_KIND_PROVIDER_KEY_POOL,
};
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
use aether_pool_core::{
    run_pool_scheduler, PoolCandidateFacts, PoolCandidateInput, PoolCandidateOrchestration,
    PoolMemberSignals, PoolRuntimeState, PoolSchedulingConfig, PoolSchedulingPreset,
    POOL_ACCOUNT_BLOCKED_SKIP_REASON, POOL_ACCOUNT_EXHAUSTED_SKIP_REASON,
    POOL_BALANCE_BELOW_MINIMUM_SKIP_REASON, POOL_COOLDOWN_SKIP_REASON,
    POOL_COST_LIMIT_REACHED_SKIP_REASON, POOL_KEY_QUOTA_EXHAUSTED_SKIP_REASON,
    POOL_KEY_STATE_UNAVAILABLE_SKIP_REASON,
};
use aether_provider_pool::{
    provider_pool_key_supports_discovered_model, resolve_provider_pool_key_thinking_model,
    ProviderPoolService,
};
use aether_provider_transport::antigravity::AntigravityThinkingLevel;
use aether_routing_core::{RankingOverlay, ResolvedRoutingPolicy};
use aether_scheduler_core::{
    candidate_runtime_skip_reason_with_state, effective_provider_key_rpm_limit,
    CandidateRuntimeSelectabilityInput,
};
use tokio::sync::Semaphore;
use tracing::{debug, warn};

use crate::ai_serving::{
    candidate_auth_channel_skip_reason, candidate_common_transport_skip_reason,
    provider_key_pool_score_scope, read_candidate_transport_snapshot,
    record_local_runtime_candidate_skip_reason, CandidateTransportPolicyFacts,
    EligibleLocalExecutionCandidate, LocalExecutionCandidateKind, PlannerAppState,
    SkippedLocalExecutionCandidate,
};
use crate::clock::current_unix_ms;
use crate::data::candidate_selection::{
    relax_dynamic_thinking_row_allowed_models, row_is_dynamic_thinking_antigravity,
};
use crate::handlers::shared::provider_pool::{
    admin_provider_pool_cache_affinity_enabled, admin_provider_pool_config_from_config_value,
};
use crate::handlers::shared::provider_pool::{
    admin_provider_pool_quota_probe_active_members_key,
    read_admin_provider_pool_key_cooldown_reason, AdminProviderPoolConfig,
    AdminProviderPoolRuntimeState, AdminProviderPoolSchedulingPreset,
};
use crate::handlers::shared::provider_pool::{
    read_provider_pool_scheduling_runtime_state, read_provider_pool_sticky_bound_key_id,
};
use crate::handlers::shared::{parse_catalog_auth_config_json, provider_key_health_summary};
use crate::maintenance::spawn_pool_quota_probe_replenish_for_request;
use crate::orchestration::{AntigravityModelQuotaCandidate, LocalExecutionCandidateMetadata};
use crate::provider_transport::codex_oauth_discovered_model_skip_reason;
use crate::stage_metrics::observe_gateway_stage_ms;

static LOAD_BALANCE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static POOL_SCORE_SCHEDULE_INTEREST_SEMAPHORE: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(POOL_SCORE_SCHEDULE_INTEREST_CONCURRENCY)));
const POOL_ACTIVE_PROBE_SEALED_SKIP_REASON: &str = "pool_active_probe_sealed";
const ROUTING_PROFILE_DISALLOWED_KEY_SKIP_REASON: &str = "routing_profile_disallowed_key";
const POOL_SCORE_SCHEDULE_INTEREST_CONCURRENCY: usize = 4;
const POOL_SCORE_SCHEDULE_INTEREST_MAX_PER_BATCH: usize = 16;
const POOL_SCORE_SCHEDULE_INTEREST_MIN_INTERVAL_SECS: u64 = 60;
/// 只有该 Provider 的候选会消费精确模型的运行时额度事实。
const POOL_RUNTIME_MODEL_QUOTA_PROVIDER_TYPE: &str = "antigravity";

type PoolCatalogKeyContext = PoolMemberSignals;
/// 精确到实际上游模型名的运行时跳过原因；同 Key 的其他模型不受影响。
type PoolModelRuntimeSkipReasons = BTreeMap<String, BTreeMap<String, &'static str>>;

/// 展开请求中的 Pool 组候选并收集跳过事实；普通候选保持原顺序进入后续执行。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn apply_local_execution_pool_scheduler(
    state: PlannerAppState<'_>,
    candidates: Vec<EligibleLocalExecutionCandidate>,
    sticky_session_token: Option<&str>,
    requested_model: Option<&str>,
    request_operation: Option<&str>,
    request_auth_channel: Option<&str>,
    thinking_level: Option<AntigravityThinkingLevel>,
) -> (
    Vec<EligibleLocalExecutionCandidate>,
    Vec<SkippedLocalExecutionCandidate>,
) {
    if candidates.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let sticky_session_token = sticky_session_token
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let mut scheduled = Vec::new();
    let mut skipped = Vec::new();
    for candidate in candidates {
        if candidate.kind == LocalExecutionCandidateKind::PoolGroup {
            let mut expanded = expand_pool_group_candidate(
                state,
                candidate,
                sticky_session_token,
                requested_model,
                request_operation,
                request_auth_channel,
                thinking_level,
            )
            .await;
            scheduled.append(&mut expanded.0);
            skipped.append(&mut expanded.1);
        } else {
            scheduled.push(candidate);
        }
    }

    (scheduled, skipped)
}

/// 对一页 Pool 候选统一执行目录、额度、余额和运行时硬过滤。
///
/// 每个成员的 `selected_provider_model_name` 是该账号实际发送给 Provider 的模型名，
/// 仅收窄可恢复的模型额度窗口；目录缺失、余额下限和管理员恢复型阻断仍保持 Key 级语义。
async fn schedule_pool_page_candidates(
    state: PlannerAppState<'_>,
    mut candidates: Vec<EligibleLocalExecutionCandidate>,
    sticky_session_token: Option<&str>,
    effective_pool_config: Option<&AdminProviderPoolConfig>,
    requested_model: Option<&str>,
    thinking_level: Option<AntigravityThinkingLevel>,
) -> (
    Vec<EligibleLocalExecutionCandidate>,
    Vec<SkippedLocalExecutionCandidate>,
) {
    if candidates.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut provider_runtime_requirements =
        BTreeMap::<String, (AdminProviderPoolConfig, BTreeSet<String>)>::new();
    for candidate in &candidates {
        let Some(pool_config) = effective_pool_config
            .cloned()
            .or_else(|| pool_config_for_candidate(candidate))
        else {
            continue;
        };
        let entry = provider_runtime_requirements
            .entry(candidate.candidate.provider_id.clone())
            .or_insert_with(|| (pool_config.clone(), BTreeSet::new()));
        entry.1.insert(candidate.candidate.key_id.clone());
    }

    let (key_context_by_id, keys_by_id, mut resolution_skipped) =
        read_pool_catalog_key_contexts_by_id(
            state,
            &mut candidates,
            effective_pool_config,
            requested_model,
            thinking_level,
        )
        .await;
    if candidates.is_empty() {
        return (Vec::new(), resolution_skipped);
    }
    let now_unix_secs = crate::clock::current_unix_secs();
    // 复用普通 Key 的计数窗口和 RPM 重置水位；实际执行仍由原子并发准入兜住竞争。
    let recent_candidates = if keys_by_id.values().any(|key| {
        key.concurrent_limit.is_some_and(|limit| limit > 0)
            || effective_provider_key_rpm_limit(key, now_unix_secs).is_some()
    }) {
        state.app().read_recent_request_candidates(128).await
    } else {
        Ok(Vec::new())
    };
    if let Err(err) = &recent_candidates {
        warn!(
            error = ?err,
            key_count = keys_by_id.len(),
            "gateway pool scheduler: failed to read key runtime counters"
        );
    }
    let key_runtime_skip_reasons = candidates
        .iter()
        .filter_map(|candidate| {
            let reason = match &recent_candidates {
                Ok(recent_candidates) => {
                    candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
                        candidate: &candidate.candidate,
                        recent_candidates,
                        provider_concurrent_limits: &BTreeMap::new(),
                        provider_key_rpm_states: &keys_by_id,
                        now_unix_secs,
                        // Provider 准入已完成；Pool 自己保留额度、认证、冷却的判定及原因优先级。
                        provider_quota_blocks_requests: false,
                        quota_hard_blocked: false,
                        account_quota_exhausted: false,
                        balance_below_minimum: false,
                        oauth_invalid: false,
                        enforce_key_circuit_breaker: false,
                        rpm_reset_at: state
                            .app()
                            .provider_key_rpm_reset_at(&candidate.candidate.key_id, now_unix_secs),
                    })
                    .or_else(|| {
                        // 每个真实成员只按自己强读到的发现目录判定模型准入。
                        let key = keys_by_id.get(candidate.candidate.key_id.as_str())?;
                        codex_oauth_discovered_model_skip_reason(
                            candidate.candidate.provider_type.as_str(),
                            key.auth_type.as_str(),
                            candidate.candidate.endpoint_api_format.as_str(),
                            candidate.candidate.selected_provider_model_name.as_str(),
                            key.upstream_metadata.as_ref(),
                        )
                        .or_else(|| {
                            (!provider_pool_key_supports_discovered_model(
                                key,
                                candidate.candidate.provider_type.as_str(),
                                candidate.candidate.selected_provider_model_name.as_str(),
                            ))
                            .then_some("key_model_disabled")
                        })
                    })
                }
                Err(_) => Some(POOL_KEY_STATE_UNAVAILABLE_SKIP_REASON),
            }?;
            Some((candidate.candidate.key_id.clone(), reason))
        })
        .collect::<BTreeMap<_, _>>();
    // 实际成员的精确模型额度事实必须在冻结窗口和热池回退之前生效；只读取 Antigravity
    // 候选，读取失败只报告状态不可用，绝不伪造成额度耗尽。
    let model_runtime_skip_reasons =
        read_pool_model_runtime_skip_reasons(state, &candidates, &keys_by_id, now_unix_secs).await;
    let mut runtime_by_provider = BTreeMap::new();
    let mut pool_config_by_provider = BTreeMap::new();
    let mut burst_provider_ids = BTreeSet::<String>::new();
    for (provider_id, (pool_config, key_ids)) in provider_runtime_requirements {
        let key_ids = key_ids.into_iter().collect::<Vec<_>>();
        let runtime = if key_ids.is_empty() {
            AdminProviderPoolRuntimeState::default()
        } else {
            let runtime_started_at = std::time::Instant::now();
            let runtime = read_provider_pool_scheduling_runtime_state(
                state.app().runtime_state.as_ref(),
                provider_id.as_str(),
                &key_ids,
                &pool_config,
                sticky_session_token,
            )
            .await;
            observe_gateway_stage_ms(
                "pool_runtime_state",
                runtime_started_at.elapsed().as_millis() as u64,
            );
            runtime
        };
        pool_config_by_provider.insert(provider_id.clone(), pool_config);
        runtime_by_provider.insert(provider_id, runtime);
    }

    let preflight_evictions = prune_unschedulable_active_probe_members_for_request(
        &mut runtime_by_provider,
        &candidates,
        &key_context_by_id,
    );
    spawn_active_probe_member_evictions_for_request(state, &preflight_evictions);
    burst_provider_ids.extend(preflight_evictions.keys().cloned());

    for (provider_id, pool_config) in &pool_config_by_provider {
        let Some(runtime) = runtime_by_provider.get(provider_id) else {
            continue;
        };
        if should_trigger_active_probe_burst_for_request(pool_config, runtime) {
            burst_provider_ids.insert(provider_id.clone());
        }
    }

    let effective_pool_config_by_provider = effective_pool_config
        .map(|config| {
            BTreeMap::from([(candidates[0].candidate.provider_id.clone(), config.clone())])
        })
        .unwrap_or_default();
    let outcome = apply_local_execution_pool_scheduler_with_runtime_map_outcome_and_configs(
        candidates,
        &runtime_by_provider,
        &key_context_by_id,
        &effective_pool_config_by_provider,
        &key_runtime_skip_reasons,
        &model_runtime_skip_reasons,
    );
    let scheduled = outcome.candidates;
    let mut skipped = outcome.skipped;
    skipped.append(&mut resolution_skipped);
    burst_provider_ids.extend(outcome.active_probe_seal_fallback_provider_ids);
    spawn_active_probe_member_evictions_for_request(
        state,
        &outcome.active_probe_evicted_members_by_provider,
    );
    burst_provider_ids.extend(
        outcome
            .active_probe_evicted_members_by_provider
            .keys()
            .cloned(),
    );

    for skipped_candidate in &skipped {
        if skipped_candidate.skip_reason == POOL_ACTIVE_PROBE_SEALED_SKIP_REASON {
            burst_provider_ids.insert(skipped_candidate.candidate.provider_id.clone());
        }
    }

    for provider_id in burst_provider_ids {
        let _ = spawn_pool_quota_probe_replenish_for_request(state.app().clone(), provider_id);
    }

    (scheduled, skipped)
}

async fn remove_active_probe_members_for_request(
    state: PlannerAppState<'_>,
    evicted_members_by_provider: &BTreeMap<String, BTreeSet<String>>,
) {
    remove_active_probe_members(state.app().clone(), evicted_members_by_provider).await;
}

fn spawn_active_probe_member_evictions_for_request(
    state: PlannerAppState<'_>,
    evicted_members_by_provider: &BTreeMap<String, BTreeSet<String>>,
) {
    if evicted_members_by_provider.is_empty() {
        return;
    }
    let app = state.app().clone();
    let evicted_members_by_provider = evicted_members_by_provider.clone();
    tokio::spawn(async move {
        remove_active_probe_members(app, &evicted_members_by_provider).await;
    });
}

async fn remove_active_probe_members(
    app: crate::AppState,
    evicted_members_by_provider: &BTreeMap<String, BTreeSet<String>>,
) {
    for (provider_id, key_ids) in evicted_members_by_provider {
        let set_key = admin_provider_pool_quota_probe_active_members_key(provider_id);
        for key_id in key_ids {
            if let Err(err) = app
                .runtime_state
                .as_ref()
                .set_remove(&set_key, key_id)
                .await
            {
                warn!(
                    event_name = "pool_active_probe_member_evict_failed",
                    log_type = "event",
                    provider_id,
                    key_id,
                    error = ?err,
                    "gateway pool scheduler failed to evict unschedulable active probe member"
                );
            }
        }
    }
}

fn prune_unschedulable_active_probe_members_for_request(
    runtime_by_provider: &mut BTreeMap<String, AdminProviderPoolRuntimeState>,
    candidates: &[EligibleLocalExecutionCandidate],
    key_context_by_id: &BTreeMap<String, PoolCatalogKeyContext>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut evicted = BTreeMap::<String, BTreeSet<String>>::new();
    for candidate in candidates {
        let Some(pool_config) = pool_config_for_candidate(candidate) else {
            continue;
        };
        if !should_enforce_active_probe_sealed_pool(&pool_config) {
            continue;
        }
        let provider_id = candidate.candidate.provider_id.as_str();
        let key_id = candidate.candidate.key_id.as_str();
        let Some(runtime) = runtime_by_provider.get_mut(provider_id) else {
            continue;
        };
        if !runtime.active_probe_member_ids.contains(key_id) {
            continue;
        }
        if !active_probe_member_is_unschedulable_for_request(
            &pool_config,
            runtime,
            key_id,
            key_context_by_id.get(key_id),
        ) {
            continue;
        }
        runtime.active_probe_member_ids.remove(key_id);
        evicted
            .entry(provider_id.to_string())
            .or_default()
            .insert(key_id.to_string());
    }
    evicted
}

/// 判断 active-probe 成员是否已不适合当前请求，余额不足与其他硬过滤保持一致。
fn active_probe_member_is_unschedulable_for_request(
    pool_config: &AdminProviderPoolConfig,
    runtime: &AdminProviderPoolRuntimeState,
    key_id: &str,
    key_context: Option<&PoolCatalogKeyContext>,
) -> bool {
    if runtime.cooldown_reason_by_key.contains_key(key_id) {
        return true;
    }
    if pool_config.cost_limit_per_key_tokens.is_some_and(|limit| {
        runtime
            .cost_window_usage_by_key
            .get(key_id)
            .copied()
            .unwrap_or(0)
            >= limit
    }) {
        return true;
    }
    key_context.is_some_and(|context| {
        context.catalog_state_unavailable
            || context.account_blocked
            || context.runtime_quota_hard_blocked
            || context.quota_hard_blocked
            || (pool_config.skip_exhausted_accounts && context.quota_exhausted)
            || context.balance_below_minimum
    })
}

#[allow(clippy::too_many_arguments)]
async fn expand_pool_group_candidate(
    state: PlannerAppState<'_>,
    group: EligibleLocalExecutionCandidate,
    sticky_session_token: Option<&str>,
    requested_model: Option<&str>,
    request_operation: Option<&str>,
    request_auth_channel: Option<&str>,
    thinking_level: Option<AntigravityThinkingLevel>,
) -> (
    Vec<EligibleLocalExecutionCandidate>,
    Vec<SkippedLocalExecutionCandidate>,
) {
    let mut cursor = PoolKeyCursor::new(
        state,
        group,
        sticky_session_token,
        requested_model,
        request_auth_channel,
    )
    .with_request_operation(request_operation)
    .with_thinking_level(thinking_level);
    let mut scheduled = Vec::new();
    let mut skipped = Vec::new();

    while let Some(candidate) = cursor.next_key().await {
        scheduled.push(candidate);
    }
    skipped.append(&mut cursor.take_skipped_candidates());

    if scheduled.is_empty() {
        cursor.log_exhausted();
    }
    (scheduled, skipped)
}

pub(crate) struct PoolKeyCursor<'a> {
    state: PlannerAppState<'a>,
    group: EligibleLocalExecutionCandidate,
    sticky_session_token: Option<String>,
    requested_model: Option<String>,
    /// 请求操作（如 Responses 会话压缩）；参与操作范围映射的逐账号模型解析。
    request_operation: Option<String>,
    /// 与预选同源的指令路由模型快照（指令基座）；解析输入缺失时沿用原始请求名。
    routing_model: Option<String>,
    /// 请求级 Antigravity thinking 档位；sticky 与分页成员在各自强读 Key 后解析。
    thinking_level: Option<AntigravityThinkingLevel>,
    request_auth_channel: Option<String>,
    routing_overlay: Option<RankingOverlay>,
    routing_allowed_key_ids: Option<Vec<String>>,
    effective_pool_config: Option<AdminProviderPoolConfig>,
    runtime_miss_trace_id: Option<String>,
    record_runtime_miss_diagnostic: bool,
    pool_key_order: StoredPoolKeyCandidateOrder,
    next_offset: u32,
    scanned_keys: u32,
    budget_scanned_keys: u32,
    window_size: u32,
    page_size: u32,
    max_scanned_keys: u32,
    absolute_max_scanned_keys: u32,
    score_top_n: u32,
    score_next_offset: u32,
    score_phase_exhausted: bool,
    score_schedule_interest_count: usize,
    routing_allowed_key_offset: usize,
    routing_allowed_rows: Option<VecDeque<StoredMinimalCandidateSelectionRow>>,
    skip_reason_counts: BTreeMap<&'static str, u32>,
    next_pool_key_index: u32,
    sticky_candidate_loaded: bool,
    seen_key_ids: BTreeSet<String>,
    queued_candidates: VecDeque<EligibleLocalExecutionCandidate>,
    skipped_candidates: Vec<SkippedLocalExecutionCandidate>,
    exhausted_logged: bool,
    returned_key_count: u32,
    exhaustion_skip_recorded: bool,
}

impl<'a> PoolKeyCursor<'a> {
    pub(crate) fn provider_id(&self) -> &str {
        self.group.candidate.provider_id.as_str()
    }

    pub(crate) fn endpoint_id(&self) -> &str {
        self.group.candidate.endpoint_id.as_str()
    }

    pub(crate) fn model_id(&self) -> &str {
        self.group.candidate.model_id.as_str()
    }

    pub(crate) fn new(
        state: PlannerAppState<'a>,
        group: EligibleLocalExecutionCandidate,
        sticky_session_token: Option<&str>,
        requested_model: Option<&str>,
        request_auth_channel: Option<&str>,
    ) -> Self {
        Self::new_with_routing_policy(
            state,
            group,
            sticky_session_token,
            requested_model,
            request_auth_channel,
            None,
        )
    }

    pub(crate) fn new_with_routing_policy(
        state: PlannerAppState<'a>,
        group: EligibleLocalExecutionCandidate,
        sticky_session_token: Option<&str>,
        requested_model: Option<&str>,
        request_auth_channel: Option<&str>,
        routing_policy: Option<&ResolvedRoutingPolicy>,
    ) -> Self {
        let effective_pool_config = effective_pool_config_for_group(&group, routing_policy);
        let pool_key_order =
            pool_key_candidate_order_for_group(&group, effective_pool_config.as_ref());
        let routing_overlay = routing_policy.map(|policy| policy.ranking_overlay.clone());
        let routing_allowed_key_ids = routing_policy
            .map(|policy| &policy.ranking_overlay.allowed_keys)
            .filter(|key_ids| !key_ids.is_empty())
            .map(|key_ids| {
                let mut seen = BTreeSet::new();
                key_ids
                    .iter()
                    .filter(|key_id| seen.insert((*key_id).clone()))
                    .cloned()
                    .collect::<Vec<_>>()
            });
        let score_top_n = effective_pool_config
            .as_ref()
            .map(|config| config.score_top_n)
            .unwrap_or(u64::from(aether_dispatch_core::DEFAULT_POOL_PAGE_SIZE))
            .clamp(1, u64::from(u32::MAX)) as u32;
        let configured_max_scanned_keys = effective_pool_config
            .as_ref()
            .map(|config| config.score_fallback_scan_limit)
            .unwrap_or(u64::from(aether_dispatch_core::DEFAULT_POOL_MAX_SCAN))
            .clamp(1, u64::from(u32::MAX)) as u32;
        let window_config = crate::dispatch::pool::default_pool_window_config().normalized();
        let max_scanned_keys = configured_max_scanned_keys.min(window_config.max_scan);
        let absolute_max_scanned_keys = configured_max_scanned_keys.max(max_scanned_keys);
        Self {
            state,
            group,
            sticky_session_token: sticky_session_token.map(str::to_string),
            requested_model: requested_model.map(str::to_string),
            request_operation: None,
            routing_model: None,
            thinking_level: None,
            request_auth_channel: request_auth_channel.map(str::to_string),
            routing_overlay,
            routing_allowed_key_ids,
            effective_pool_config,
            runtime_miss_trace_id: None,
            record_runtime_miss_diagnostic: false,
            pool_key_order,
            next_offset: 0,
            scanned_keys: 0,
            budget_scanned_keys: 0,
            window_size: window_config.window_size,
            page_size: window_config.page_size,
            max_scanned_keys: max_scanned_keys.max(window_config.window_size),
            absolute_max_scanned_keys: absolute_max_scanned_keys.max(window_config.window_size),
            score_top_n,
            score_next_offset: 0,
            score_phase_exhausted: false,
            score_schedule_interest_count: 0,
            routing_allowed_key_offset: 0,
            routing_allowed_rows: None,
            skip_reason_counts: BTreeMap::new(),
            next_pool_key_index: 0,
            sticky_candidate_loaded: false,
            seen_key_ids: BTreeSet::new(),
            queued_candidates: VecDeque::new(),
            skipped_candidates: Vec::new(),
            exhausted_logged: false,
            returned_key_count: 0,
            exhaustion_skip_recorded: false,
        }
    }

    pub(crate) fn with_runtime_miss_diagnostic(
        mut self,
        trace_id: &str,
        record_runtime_miss_diagnostic: bool,
    ) -> Self {
        if record_runtime_miss_diagnostic {
            self.runtime_miss_trace_id = Some(trace_id.to_string());
            self.record_runtime_miss_diagnostic = true;
        }
        self
    }

    /// 附加本次请求的操作（如 Responses 会话压缩），供操作范围映射解析逐账号模型。
    pub(crate) fn with_request_operation(mut self, request_operation: Option<&str>) -> Self {
        self.request_operation = request_operation
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        self
    }

    /// 附加与预选同源的指令路由模型快照（指令基座或空）；未附带时沿用原始请求名。
    pub(crate) fn with_routing_model(mut self, routing_model: Option<&str>) -> Self {
        self.routing_model = routing_model
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        self
    }

    /// 附加请求级 thinking 档位；每个真实成员在强读目录后按自己的发现目录解析模型。
    pub(crate) fn with_thinking_level(
        mut self,
        thinking_level: Option<AntigravityThinkingLevel>,
    ) -> Self {
        self.thinking_level = thinking_level;
        self
    }

    pub(crate) async fn next_key(&mut self) -> Option<EligibleLocalExecutionCandidate> {
        let started_at = std::time::Instant::now();
        let mut observed = false;
        loop {
            if let Some(candidate) = self.next_queued_candidate().await {
                self.returned_key_count = self.returned_key_count.saturating_add(1);
                if !observed {
                    observe_gateway_stage_ms(
                        "pool_cursor_next_key",
                        started_at.elapsed().as_millis() as u64,
                    );
                }
                return Some(candidate);
            }

            if !self.sticky_candidate_loaded {
                self.sticky_candidate_loaded = true;
                if let Some(candidate) = self.sticky_candidate().await {
                    self.queued_candidates.push_back(candidate);
                    continue;
                }
            }

            if !self.refill_queued_candidates().await {
                if !observed {
                    observe_gateway_stage_ms(
                        "pool_cursor_next_key",
                        started_at.elapsed().as_millis() as u64,
                    );
                    observed = true;
                }
                return None;
            }
        }
    }

    pub(crate) fn take_skipped_candidates(&mut self) -> Vec<SkippedLocalExecutionCandidate> {
        std::mem::take(&mut self.skipped_candidates)
    }

    pub(crate) fn exhausted_group_skipped_candidate(
        &self,
    ) -> Option<SkippedLocalExecutionCandidate> {
        if self.returned_key_count > 0 {
            return None;
        }

        let skip_reason_counts = self
            .skip_reason_counts
            .iter()
            .map(|(reason, count)| ((*reason).to_string(), serde_json::json!(count)))
            .collect::<serde_json::Map<String, serde_json::Value>>();
        Some(SkippedLocalExecutionCandidate {
            candidate: self.group.candidate.clone(),
            skip_reason: "pool_group_exhausted",
            transport: Some(self.group.transport.clone()),
            ranking: self.group.ranking.clone(),
            extra_data: Some(serde_json::json!({
                "pool_group_exhaustion": {
                    "scanned_keys": self.scanned_keys,
                    "budget_scanned_keys": self.budget_scanned_keys,
                    "skip_reason_counts": skip_reason_counts,
                }
            })),
        })
    }

    /// 只记录一次 Pool 耗尽事件，并同步每个真实跳过原因到请求级运行时诊断。
    pub(crate) fn log_exhausted(&mut self) {
        if self.exhausted_logged {
            return;
        }
        self.exhausted_logged = true;
        warn!(
            event_name = "pool_group_exhausted",
            log_type = "event",
            provider_id = %self.group.candidate.provider_id,
            endpoint_id = %self.group.candidate.endpoint_id,
            model_id = %self.group.candidate.model_id,
            scanned_keys = self.scanned_keys,
            budget_scanned_keys = self.budget_scanned_keys,
            max_scanned_keys = self.max_scanned_keys,
            absolute_max_scanned_keys = self.absolute_max_scanned_keys,
            skip_reason_counts = ?self.skip_reason_counts,
            "gateway pool scheduler exhausted pool group without a schedulable key"
        );
        self.record_runtime_miss_pool_exhaustion_skip_reason();
    }

    /// 将 Pool 内每一种真实跳过原因写入运行时诊断，避免聚合原因掩盖容量与额度差异。
    fn record_runtime_miss_pool_exhaustion_skip_reason(&mut self) {
        if self.exhaustion_skip_recorded
            || !self.record_runtime_miss_diagnostic
            || self.returned_key_count > 0
        {
            return;
        }
        let Some(trace_id) = self.runtime_miss_trace_id.as_deref() else {
            return;
        };
        self.exhaustion_skip_recorded = true;
        if self.skip_reason_counts.is_empty() {
            record_local_runtime_candidate_skip_reason(
                self.state.app(),
                trace_id,
                "pool_group_exhausted",
            );
            return;
        }
        for reason in self.skip_reason_counts.keys() {
            record_local_runtime_candidate_skip_reason(self.state.app(), trace_id, reason);
        }
    }

    async fn next_page_candidates(&mut self) -> Option<Vec<EligibleLocalExecutionCandidate>> {
        if self.routing_allowed_key_ids.is_some() {
            return self.next_routing_allowed_candidates().await;
        }

        if !self.score_phase_exhausted {
            if let Some(score_candidates) = self.next_score_candidates().await {
                if !score_candidates.is_empty() {
                    return Some(score_candidates);
                }
            }
        }

        if self.budget_scanned_keys >= self.max_scanned_keys
            || self.scanned_keys >= self.absolute_max_scanned_keys
        {
            return None;
        }

        let limit = self
            .page_size
            .min(self.max_scanned_keys - self.budget_scanned_keys)
            .min(self.absolute_max_scanned_keys - self.scanned_keys);
        let query = StoredPoolKeyCandidateRowsQuery {
            api_format: self.group.candidate.endpoint_api_format.clone(),
            provider_id: self.group.candidate.provider_id.clone(),
            endpoint_id: self.group.candidate.endpoint_id.clone(),
            model_id: self.group.candidate.model_id.clone(),
            selected_provider_model_name: self.group.candidate.selected_provider_model_name.clone(),
            order: self.pool_key_order.clone(),
            offset: self.next_offset,
            limit,
        };
        let rows = match self
            .state
            .app()
            .list_pool_key_candidate_rows_for_group(&query)
            .await
        {
            Ok(rows) => rows,
            Err(err) => {
                warn!(
                    event_name = "pool_group_key_page_load_failed",
                    log_type = "event",
                    provider_id = %self.group.candidate.provider_id,
                    endpoint_id = %self.group.candidate.endpoint_id,
                    model_id = %self.group.candidate.model_id,
                    selected_provider_model_name = %self.group.candidate.selected_provider_model_name,
                    offset = self.next_offset,
                    limit,
                    error = ?err,
                    "gateway pool scheduler failed to read pool key page"
                );
                return None;
            }
        };
        if rows.is_empty() {
            return None;
        }

        let row_count = u32::try_from(rows.len()).unwrap_or(u32::MAX);
        self.next_offset = self.next_offset.saturating_add(row_count);
        let seen_count_before = self.seen_key_ids.len();
        let candidates = self.build_page_eligible_candidates(rows).await;
        let distinct_row_count = self.seen_key_ids.len().saturating_sub(seen_count_before);
        self.scanned_keys = self.scanned_keys.saturating_add(row_count);
        self.budget_scanned_keys = self
            .budget_scanned_keys
            .saturating_add(u32::try_from(distinct_row_count).unwrap_or(u32::MAX));
        Some(candidates)
    }

    /// 分页读取路由允许的 Pool Key，并按候选格式的 Key 优先级和现有额度条件排序。
    async fn next_routing_allowed_candidates(
        &mut self,
    ) -> Option<Vec<EligibleLocalExecutionCandidate>> {
        loop {
            if self.budget_scanned_keys >= self.max_scanned_keys
                || self.scanned_keys >= self.absolute_max_scanned_keys
            {
                return None;
            }

            let key_ids = self.routing_allowed_key_ids.as_ref()?;
            let has_buffered_rows = self
                .routing_allowed_rows
                .as_ref()
                .is_some_and(|rows| !rows.is_empty());
            if !has_buffered_rows && self.routing_allowed_key_offset >= key_ids.len() {
                return None;
            }
            let limit = self
                .page_size
                .min(self.max_scanned_keys - self.budget_scanned_keys)
                .min(self.absolute_max_scanned_keys - self.scanned_keys)
                as usize;
            if matches!(
                self.pool_key_order,
                StoredPoolKeyCandidateOrder::InternalPriority
            ) && self.routing_allowed_rows.is_none()
            {
                let query = StoredPoolKeyCandidateRowsByKeyIdsQuery {
                    api_format: self.group.candidate.endpoint_api_format.clone(),
                    provider_id: self.group.candidate.provider_id.clone(),
                    endpoint_id: self.group.candidate.endpoint_id.clone(),
                    model_id: self.group.candidate.model_id.clone(),
                    selected_provider_model_name: self
                        .group
                        .candidate
                        .selected_provider_model_name
                        .clone(),
                    key_ids: key_ids.clone(),
                };
                let mut rows = match self
                    .state
                    .app()
                    .list_pool_key_candidate_rows_for_group_key_ids(&query)
                    .await
                {
                    Ok(rows) => rows,
                    Err(err) => {
                        warn!(
                            event_name = "pool_group_routing_key_load_failed",
                            log_type = "event",
                            provider_id = %self.group.candidate.provider_id,
                            endpoint_id = %self.group.candidate.endpoint_id,
                            model_id = %self.group.candidate.model_id,
                            allowed_key_count = query.key_ids.len(),
                            error = ?err,
                            "gateway pool scheduler failed to materialize routing-allowed pool keys"
                        );
                        return None;
                    }
                };
                let api_format = self.group.candidate.endpoint_api_format.as_str();
                rows.sort_by(|left, right| {
                    let left_priority = self.routing_overlay.as_ref().map_or(
                        left.key_internal_priority,
                        |overlay| {
                            overlay.key_priority_for_format(
                                &left.key_id,
                                api_format,
                                left.key_internal_priority,
                            )
                        },
                    );
                    let right_priority = self.routing_overlay.as_ref().map_or(
                        right.key_internal_priority,
                        |overlay| {
                            overlay.key_priority_for_format(
                                &right.key_id,
                                api_format,
                                right.key_internal_priority,
                            )
                        },
                    );
                    left_priority
                        .cmp(&right_priority)
                        .then(left.key_id.cmp(&right.key_id))
                });
                self.routing_allowed_key_offset = key_ids.len();
                self.routing_allowed_rows = Some(rows.into());
            }

            let rows = if let Some(rows) = self.routing_allowed_rows.as_mut() {
                let page_len = limit.min(rows.len());
                rows.drain(..page_len).collect::<Vec<_>>()
            } else {
                let end = self
                    .routing_allowed_key_offset
                    .saturating_add(limit)
                    .min(key_ids.len());
                let page_key_ids = key_ids[self.routing_allowed_key_offset..end].to_vec();
                self.routing_allowed_key_offset = end;
                let query = StoredPoolKeyCandidateRowsByKeyIdsQuery {
                    api_format: self.group.candidate.endpoint_api_format.clone(),
                    provider_id: self.group.candidate.provider_id.clone(),
                    endpoint_id: self.group.candidate.endpoint_id.clone(),
                    model_id: self.group.candidate.model_id.clone(),
                    selected_provider_model_name: self
                        .group
                        .candidate
                        .selected_provider_model_name
                        .clone(),
                    key_ids: page_key_ids,
                };
                match self
                    .state
                    .app()
                    .list_pool_key_candidate_rows_for_group_key_ids(&query)
                    .await
                {
                    Ok(rows) => rows,
                    Err(err) => {
                        warn!(
                            event_name = "pool_group_routing_key_load_failed",
                            log_type = "event",
                            provider_id = %self.group.candidate.provider_id,
                            endpoint_id = %self.group.candidate.endpoint_id,
                            model_id = %self.group.candidate.model_id,
                            allowed_key_count = query.key_ids.len(),
                            error = ?err,
                            "gateway pool scheduler failed to materialize routing-allowed pool keys"
                        );
                        return None;
                    }
                }
            };
            if rows.is_empty() {
                if self.routing_allowed_rows.is_some() {
                    return None;
                }
                continue;
            }

            let row_count = u32::try_from(rows.len()).unwrap_or(u32::MAX);
            let seen_count_before = self.seen_key_ids.len();
            let candidates = self.build_page_eligible_candidates(rows).await;
            let distinct_row_count = self.seen_key_ids.len().saturating_sub(seen_count_before);
            self.scanned_keys = self.scanned_keys.saturating_add(row_count);
            self.budget_scanned_keys = self
                .budget_scanned_keys
                .saturating_add(u32::try_from(distinct_row_count).unwrap_or(u32::MAX));
            return Some(candidates);
        }
    }

    async fn next_score_candidates(&mut self) -> Option<Vec<EligibleLocalExecutionCandidate>> {
        if self.score_phase_exhausted
            || self.score_next_offset >= self.score_top_n
            || self.scanned_keys >= self.absolute_max_scanned_keys
        {
            self.score_phase_exhausted = true;
            return None;
        }
        let limit = self
            .page_size
            .min(self.score_top_n - self.score_next_offset)
            .min(self.absolute_max_scanned_keys - self.scanned_keys);
        if limit == 0 {
            self.score_phase_exhausted = true;
            return None;
        }
        let scope = provider_key_pool_score_scope();
        let query = ListRankedPoolMembersQuery {
            pool_kind: POOL_KIND_PROVIDER_KEY_POOL.to_string(),
            pool_id: self.group.candidate.provider_id.clone(),
            capability: scope.capability.clone(),
            scope_kind: scope.scope_kind.clone(),
            scope_id: scope.scope_id.clone(),
            hard_states: vec![PoolMemberHardState::Available, PoolMemberHardState::Unknown],
            probe_statuses: None,
            offset: self.score_next_offset as usize,
            limit: limit as usize,
        };
        let score_started_at = std::time::Instant::now();
        let scores = match self.state.app().data.list_ranked_pool_members(&query).await {
            Ok(scores) => scores,
            Err(err) => {
                observe_gateway_stage_ms(
                    "pool_score_load",
                    score_started_at.elapsed().as_millis() as u64,
                );
                warn!(
                    event_name = "pool_group_score_load_failed",
                    log_type = "event",
                    provider_id = %self.group.candidate.provider_id,
                    endpoint_id = %self.group.candidate.endpoint_id,
                    model_id = %self.group.candidate.model_id,
                    selected_provider_model_name = %self.group.candidate.selected_provider_model_name,
                    error = ?err,
                    "gateway pool scheduler failed to read ranked pool member scores"
                );
                self.score_phase_exhausted = true;
                return None;
            }
        };
        observe_gateway_stage_ms(
            "pool_score_load",
            score_started_at.elapsed().as_millis() as u64,
        );
        if scores.is_empty() {
            self.score_phase_exhausted = true;
            return None;
        }
        let score_count = u32::try_from(scores.len()).unwrap_or(u32::MAX);
        self.score_next_offset = self.score_next_offset.saturating_add(score_count);
        if score_count < limit || self.score_next_offset >= self.score_top_n {
            self.score_phase_exhausted = true;
        }

        self.spawn_score_schedule_interest_recording(&scores);

        let key_ids = scores
            .iter()
            .map(|score| score.member_id.clone())
            .collect::<Vec<_>>();
        let rows_query = StoredPoolKeyCandidateRowsByKeyIdsQuery {
            api_format: self.group.candidate.endpoint_api_format.clone(),
            provider_id: self.group.candidate.provider_id.clone(),
            endpoint_id: self.group.candidate.endpoint_id.clone(),
            model_id: self.group.candidate.model_id.clone(),
            selected_provider_model_name: self.group.candidate.selected_provider_model_name.clone(),
            key_ids,
        };
        let rows_started_at = std::time::Instant::now();
        let rows = match self
            .state
            .app()
            .list_pool_key_candidate_rows_for_group_key_ids(&rows_query)
            .await
        {
            Ok(rows) => rows,
            Err(err) => {
                observe_gateway_stage_ms(
                    "pool_score_key_rows",
                    rows_started_at.elapsed().as_millis() as u64,
                );
                warn!(
                    event_name = "pool_group_score_key_load_failed",
                    log_type = "event",
                    provider_id = %self.group.candidate.provider_id,
                    endpoint_id = %self.group.candidate.endpoint_id,
                    model_id = %self.group.candidate.model_id,
                    selected_provider_model_name = %self.group.candidate.selected_provider_model_name,
                    score_count = scores.len(),
                    error = ?err,
                    "gateway pool scheduler failed to materialize ranked pool keys"
                );
                self.score_phase_exhausted = true;
                return None;
            }
        };
        observe_gateway_stage_ms(
            "pool_score_key_rows",
            rows_started_at.elapsed().as_millis() as u64,
        );
        let materialized_row_count = u32::try_from(rows.len()).unwrap_or(u32::MAX);
        let missing_score_count = scores.len().saturating_sub(rows.len());
        if missing_score_count > 0 {
            *self
                .skip_reason_counts
                .entry("pool_score_member_missing")
                .or_insert(0) += u32::try_from(missing_score_count).unwrap_or(u32::MAX);
        }
        let seen_count_before = self.seen_key_ids.len();
        let candidates = self.build_page_eligible_candidates(rows).await;
        let distinct_row_count = self.seen_key_ids.len().saturating_sub(seen_count_before);
        self.scanned_keys = self.scanned_keys.saturating_add(materialized_row_count);
        self.budget_scanned_keys = self
            .budget_scanned_keys
            .saturating_add(u32::try_from(distinct_row_count).unwrap_or(u32::MAX));
        Some(candidates)
    }

    /// 读取 sticky Key 后以 singleton 复用共享 Pool 调度，并携带实际模型名参与额度过滤。
    /// 亲和命中不会绕过目录、余额或永久阻断等共享硬过滤。
    async fn sticky_candidate(&mut self) -> Option<EligibleLocalExecutionCandidate> {
        let pool_config = self.effective_pool_config.as_ref()?.clone();
        if !admin_provider_pool_cache_affinity_enabled(&pool_config) {
            return None;
        }
        let sticky_key_id = read_provider_pool_sticky_bound_key_id(
            self.state.app().runtime_state.as_ref(),
            self.group.candidate.provider_id.as_str(),
            &pool_config,
            self.sticky_session_token.as_deref(),
        )
        .await?;
        if self
            .routing_overlay
            .as_ref()
            .is_some_and(|overlay| !overlay.key_allowed(sticky_key_id.as_str()))
        {
            return None;
        }
        if self.seen_key_ids.contains(&sticky_key_id) {
            return None;
        }

        let key = match self
            .state
            .app()
            .list_provider_catalog_keys_by_ids_strong(std::slice::from_ref(&sticky_key_id))
            .await
        {
            Ok(mut keys) => keys.pop()?,
            Err(err) => {
                warn!(
                    event_name = "pool_group_sticky_key_load_failed",
                    log_type = "event",
                    provider_id = %self.group.candidate.provider_id,
                    endpoint_id = %self.group.candidate.endpoint_id,
                    model_id = %self.group.candidate.model_id,
                    key_id = %sticky_key_id,
                    error = ?err,
                    "gateway pool scheduler failed to read sticky pool key"
                );
                return None;
            }
        };
        if key.provider_id != self.group.candidate.provider_id {
            return None;
        }

        // sticky 成员必须使用自己行上的映射数据解析实际上游模型；行缺失时按映射缺失
        // 关闭该成员，不把其他账号的模型目标套到该账号上。
        let row = match self
            .state
            .app()
            .list_pool_key_candidate_rows_for_group_key_ids(
                &StoredPoolKeyCandidateRowsByKeyIdsQuery {
                    api_format: self.group.candidate.endpoint_api_format.clone(),
                    provider_id: self.group.candidate.provider_id.clone(),
                    endpoint_id: self.group.candidate.endpoint_id.clone(),
                    model_id: self.group.candidate.model_id.clone(),
                    selected_provider_model_name: self
                        .group
                        .candidate
                        .selected_provider_model_name
                        .clone(),
                    key_ids: vec![key.id.clone()],
                },
            )
            .await
        {
            Ok(rows) => rows.into_iter().find(|row| row.key_id == key.id),
            Err(err) => {
                warn!(
                    event_name = "pool_group_sticky_key_row_load_failed",
                    log_type = "event",
                    provider_id = %self.group.candidate.provider_id,
                    endpoint_id = %self.group.candidate.endpoint_id,
                    model_id = %self.group.candidate.model_id,
                    key_id = %key.id,
                    error = ?err,
                    "gateway pool scheduler failed to read sticky pool key model row"
                );
                None
            }
        };
        let Some(mut row) = row else {
            self.seen_key_ids.insert(key.id.clone());
            self.record_skip_reason("mapped_model_missing");
            return None;
        };
        let member_model = self.pool_member_model_for_row(&mut row);
        let Some(member_model) = member_model else {
            self.seen_key_ids.insert(key.id.clone());
            self.record_skip_reason("key_model_disabled");
            return None;
        };
        // Sticky 的真实成员同样先按自己的发现目录解析档位，再进入保底额度判定。
        let Some(member_model) = self.resolve_pool_member_thinking_model(&key, member_model) else {
            self.seen_key_ids.insert(key.id.clone());
            self.record_skip_reason("key_model_disabled");
            return None;
        };

        if pool_config.reserve_minimum_quota
            && admin_provider_pool_pure::admin_pool_key_minimum_quota_reached(
                &key,
                self.group.candidate.provider_type.as_str(),
                Some(member_model.0.as_str()),
            )
        {
            self.seen_key_ids.insert(key.id.clone());
            self.record_skip_reason(POOL_ACCOUNT_EXHAUSTED_SKIP_REASON);
            self.skipped_candidates
                .push(SkippedLocalExecutionCandidate {
                    candidate: pool_candidate_from_catalog_key(&self.group, key, member_model),
                    skip_reason: POOL_ACCOUNT_EXHAUSTED_SKIP_REASON,
                    transport: None,
                    ranking: self.group.ranking.clone(),
                    extra_data: None,
                });
            return None;
        }

        let candidate = pool_candidate_from_catalog_key(&self.group, key, member_model);
        let candidate = self.build_eligible_candidate(candidate).await?;
        let (mut scheduled, mut skipped) = schedule_pool_page_candidates(
            self.state,
            vec![candidate],
            self.sticky_session_token.as_deref(),
            self.effective_pool_config.as_ref(),
            self.requested_model.as_deref(),
            self.thinking_level,
        )
        .await;
        self.record_skipped_candidates(&skipped);
        self.skipped_candidates.append(&mut skipped);
        scheduled.pop()
    }

    /// 分页补充候选队列，并用每个成员自己的实际 Provider 模型执行同一组共享过滤。
    async fn refill_queued_candidates(&mut self) -> bool {
        let refill_target = self.window_size.max(1) as usize;

        loop {
            let mut candidates = Vec::new();
            // Keep pool expansion bounded; the cursor freezes one small window at a time.
            while candidates.len() < refill_target {
                let Some(mut page_candidates) = self.next_page_candidates().await else {
                    break;
                };
                candidates.append(&mut page_candidates);
            }

            if candidates.is_empty() {
                return false;
            }

            let (mut scheduled, mut skipped) = schedule_pool_page_candidates(
                self.state,
                candidates,
                self.sticky_session_token.as_deref(),
                self.effective_pool_config.as_ref(),
                self.requested_model.as_deref(),
                self.thinking_level,
            )
            .await;
            self.record_skipped_candidates(&skipped);
            self.skipped_candidates.append(&mut skipped);

            if scheduled.is_empty() {
                continue;
            }

            scheduled.truncate(refill_target);
            self.queued_candidates.extend(scheduled.drain(..));
            return true;
        }
    }

    async fn next_queued_candidate(&mut self) -> Option<EligibleLocalExecutionCandidate> {
        while let Some(candidate) = self.queued_candidates.pop_front() {
            let mut candidate = candidate;
            if self.skip_candidate_if_routing_profile_disallowed(&candidate) {
                continue;
            }
            if self.skip_candidate_if_runtime_cooldown(&candidate).await {
                continue;
            }
            if candidate.orchestration.candidate_group_id.is_none() {
                candidate.orchestration.candidate_group_id =
                    Some(pool_cursor_candidate_group_id(&self.group));
            }
            candidate.orchestration.pool_key_index = Some(self.next_pool_key_index);
            self.next_pool_key_index = self.next_pool_key_index.saturating_add(1);
            return Some(candidate);
        }

        None
    }

    fn skip_candidate_if_routing_profile_disallowed(
        &mut self,
        candidate: &EligibleLocalExecutionCandidate,
    ) -> bool {
        let Some(overlay) = self.routing_overlay.as_ref() else {
            return false;
        };
        if overlay.key_allowed(candidate.candidate.key_id.as_str()) {
            return false;
        }
        self.record_skip_reason(ROUTING_PROFILE_DISALLOWED_KEY_SKIP_REASON);
        self.skipped_candidates
            .push(SkippedLocalExecutionCandidate {
                candidate: candidate.candidate.clone(),
                skip_reason: ROUTING_PROFILE_DISALLOWED_KEY_SKIP_REASON,
                transport: Some(candidate.transport.clone()),
                ranking: candidate.ranking.clone(),
                extra_data: None,
            });
        true
    }

    async fn skip_candidate_if_runtime_cooldown(
        &mut self,
        candidate: &EligibleLocalExecutionCandidate,
    ) -> bool {
        match read_admin_provider_pool_key_cooldown_reason(
            self.state.app().runtime_state.as_ref(),
            candidate.candidate.provider_id.as_str(),
            candidate.candidate.key_id.as_str(),
        )
        .await
        {
            Ok(Some(_)) => {
                self.record_skip_reason("pool_cooldown");
                self.skipped_candidates
                    .push(SkippedLocalExecutionCandidate {
                        candidate: candidate.candidate.clone(),
                        skip_reason: "pool_cooldown",
                        transport: Some(candidate.transport.clone()),
                        ranking: candidate.ranking.clone(),
                        extra_data: None,
                    });
                self.spawn_active_probe_member_eviction_and_replenish(candidate);
                true
            }
            Ok(None) => false,
            Err(err) => {
                warn!(
                    event_name = "pool_key_cooldown_check_failed",
                    log_type = "event",
                    provider_id = %candidate.candidate.provider_id,
                    endpoint_id = %candidate.candidate.endpoint_id,
                    model_id = %candidate.candidate.model_id,
                    key_id = %candidate.candidate.key_id,
                    error = ?err,
                    "gateway pool scheduler failed to read pool key cooldown; scheduling key"
                );
                false
            }
        }
    }

    fn spawn_active_probe_member_eviction_and_replenish(
        &self,
        candidate: &EligibleLocalExecutionCandidate,
    ) {
        let Some(config) = pool_config_for_candidate(candidate) else {
            return;
        };
        if !should_enforce_active_probe_sealed_pool(&config) {
            return;
        }
        let provider_id = candidate.candidate.provider_id.as_str();
        let key_id = candidate.candidate.key_id.as_str();
        spawn_active_probe_member_evictions_for_request(
            self.state,
            &BTreeMap::from([(
                provider_id.to_string(),
                BTreeSet::from([key_id.to_string()]),
            )]),
        );
        let _ = spawn_pool_quota_probe_replenish_for_request(
            self.state.app().clone(),
            provider_id.to_string(),
        );
    }

    fn spawn_score_schedule_interest_recording(&mut self, scores: &[StoredPoolMemberScore]) {
        if scores.is_empty()
            || self.score_schedule_interest_count >= POOL_SCORE_SCHEDULE_INTEREST_MAX_PER_BATCH
            || !self.state.app().data.has_pool_score_writer()
        {
            return;
        }

        let remaining_interest_budget = POOL_SCORE_SCHEDULE_INTEREST_MAX_PER_BATCH
            .saturating_sub(self.score_schedule_interest_count);
        let scheduled_at = current_unix_ms() / 1000;
        let provider_id = self.group.candidate.provider_id.clone();
        let endpoint_id = self.group.candidate.endpoint_id.clone();
        let model_id = self.group.candidate.model_id.clone();
        let feedback = scores
            .iter()
            .filter(|score| {
                score.last_scheduled_at.is_none_or(|last_scheduled_at| {
                    scheduled_at.saturating_sub(last_scheduled_at)
                        >= POOL_SCORE_SCHEDULE_INTEREST_MIN_INTERVAL_SECS
                })
            })
            .take(remaining_interest_budget)
            .map(|score| PoolMemberScheduleFeedback {
                identity: PoolMemberIdentity {
                    pool_kind: score.pool_kind.clone(),
                    pool_id: score.pool_id.clone(),
                    member_kind: score.member_kind.clone(),
                    member_id: score.member_id.clone(),
                },
                scope: Some(PoolScoreScope {
                    capability: score.capability.clone(),
                    scope_kind: score.scope_kind.clone(),
                    scope_id: score.scope_id.clone(),
                }),
                scheduled_at,
                succeeded: None,
                hard_state: None,
                score_delta: None,
                score_reason_patch: Some(serde_json::json!({
                    "last_schedule_interest": {
                        "provider_id": provider_id.as_str(),
                        "endpoint_id": endpoint_id.as_str(),
                        "model_id": model_id.as_str()
                    }
                })),
            })
            .collect::<Vec<_>>();
        let score_count = feedback.len();
        if feedback.is_empty() {
            return;
        }

        let Ok(permit) = POOL_SCORE_SCHEDULE_INTEREST_SEMAPHORE
            .clone()
            .try_acquire_owned()
        else {
            debug!(
                event_name = "pool_group_score_interest_dropped",
                log_type = "event",
                provider_id = %self.group.candidate.provider_id,
                endpoint_id = %self.group.candidate.endpoint_id,
                model_id = %self.group.candidate.model_id,
                score_count = scores.len(),
                "gateway pool scheduler dropped score schedule interest because the background writer is saturated"
            );
            return;
        };
        self.score_schedule_interest_count = self
            .score_schedule_interest_count
            .saturating_add(score_count);

        let app = self.state.app().clone();

        tokio::spawn(async move {
            let _permit = permit;
            let mut failed = 0usize;
            for feedback in feedback {
                let feedback_provider_id = feedback.identity.pool_id.clone();
                let feedback_key_id = feedback.identity.member_id.clone();
                let projection_lock = match app
                    .acquire_provider_key_quota_projection_lock(
                        &feedback_provider_id,
                        &feedback_key_id,
                    )
                    .await
                {
                    Ok(lease) => lease,
                    Err(_) => {
                        failed += 1;
                        continue;
                    }
                };

                // Schedule-interest feedback is a read-modify-write operation.
                // Serialize it with the runtime quota projection so a late
                // background write cannot downgrade QuotaExhausted.
                let should_record = match app
                    .provider_key_runtime_quota_blocked_strong(
                        &feedback_provider_id,
                        &feedback_key_id,
                    )
                    .await
                {
                    Ok(Some(false)) => true,
                    Ok(Some(true) | None) => false,
                    Err(_) => {
                        failed += 1;
                        false
                    }
                };
                if should_record
                    && app
                        .data
                        .record_pool_member_schedule_feedback(feedback)
                        .await
                        .is_err()
                {
                    failed += 1;
                }
                app.release_provider_key_quota_projection_lock(projection_lock)
                    .await;
            }
            if failed > 0 {
                warn!(
                    event_name = "pool_group_score_interest_update_failed",
                    log_type = "event",
                    provider_id = %provider_id,
                    endpoint_id = %endpoint_id,
                    model_id = %model_id,
                    failed_count = failed,
                    score_count,
                    "gateway pool scheduler failed to record some pool score schedule interests"
                );
            }
        });
    }

    async fn build_page_eligible_candidates(
        &mut self,
        mut rows: Vec<StoredMinimalCandidateSelectionRow>,
    ) -> Vec<EligibleLocalExecutionCandidate> {
        // effort 敏感行必须在 transport 的模型清单独裁前拿到强读 Key 的已解析档位；
        // 静态映射可能落在默认 high，只允许请求档位的账号会被 pre-gate 误杀。
        let key_by_id = if self.thinking_level.is_some() {
            let key_ids = rows
                .iter()
                .filter(|row| row_is_dynamic_thinking_antigravity(row))
                .map(|row| row.key_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            if key_ids.is_empty() {
                BTreeMap::new()
            } else {
                match self
                    .state
                    .app()
                    .list_provider_catalog_keys_by_ids_strong(&key_ids)
                    .await
                {
                    Ok(keys) => keys
                        .into_iter()
                        .map(|key| (key.id.clone(), key))
                        .collect::<BTreeMap<_, _>>(),
                    Err(err) => {
                        // 读取失败保持既有 fail-closed 路径；后续调度读取会报告状态不可用。
                        warn!(
                            event_name = "pool_group_page_key_load_failed",
                            log_type = "event",
                            provider_id = %self.group.candidate.provider_id,
                            endpoint_id = %self.group.candidate.endpoint_id,
                            model_id = %self.group.candidate.model_id,
                            key_count = key_ids.len(),
                            error = ?err,
                            "gateway pool scheduler failed to read page keys for dynamic thinking"
                        );
                        BTreeMap::new()
                    }
                }
            }
        } else {
            BTreeMap::new()
        };
        let mut candidates = Vec::with_capacity(rows.len());
        for mut row in rows {
            let Some(member_model) = self.pool_member_model_for_row(&mut row) else {
                // 显式账号模型清单与本次请求无交集或为空：在 transport 门禁之前
                // 以模型禁用语义拒绝整个成员，避免无清单账号的模型目标污染受限账号。
                if self.seen_key_ids.insert(row.key_id.clone()) {
                    self.record_skip_reason("key_model_disabled");
                }
                continue;
            };
            let member_model = match key_by_id.get(row.key_id.as_str()) {
                Some(key) => {
                    let Some(member_model) =
                        self.resolve_pool_member_thinking_model(key, member_model)
                    else {
                        if self.seen_key_ids.insert(row.key_id.clone()) {
                            self.record_skip_reason("key_model_disabled");
                        }
                        continue;
                    };
                    member_model
                }
                None => member_model,
            };
            let candidate = pool_candidate_from_row(&self.group, row, member_model);
            if let Some(candidate) = self.build_eligible_candidate(candidate).await {
                candidates.push(candidate);
            }
        }
        candidates
    }

    /// 解析该账号行的实际发送模型名；显式清单被拒绝时返回空。
    /// 解析输入是构建时固定的指令路由模型快照；未附带时沿用请求模型，
    /// 请求未声明模型时沿用组的解析全局名。
    fn pool_member_model_for_row(
        &self,
        row: &mut StoredMinimalCandidateSelectionRow,
    ) -> Option<(String, Option<String>)> {
        // 静态映射可能选默认 high；effort 敏感行先放开清单，最终清单由强读 Key 解析器执行。
        relax_dynamic_thinking_row_allowed_models(std::slice::from_mut(row), self.thinking_level);
        pool_member_model_resolution(
            &self.group,
            row,
            self.routing_model
                .as_deref()
                .or(self.requested_model.as_deref())
                .unwrap_or(self.group.candidate.global_model_name.as_str()),
            self.request_operation.as_deref(),
        )
    }

    /// 按真实成员 Key 自己的发现目录把映射模型解析为请求档位变体。
    ///
    /// 返回空表示该账号缺少请求档位（或目录为空/禁用），成员必须跳过而不是回退默认档位。
    /// 后缀优先级使用原始客户端模型，而不是组默认映射产生的后缀。
    fn resolve_pool_member_thinking_model(
        &self,
        key: &StoredProviderCatalogKey,
        member_model: (String, Option<String>),
    ) -> Option<(String, Option<String>)> {
        let Some(level) = self.thinking_level else {
            return Some(member_model);
        };
        if !self
            .group
            .candidate
            .provider_type
            .trim()
            .eq_ignore_ascii_case("antigravity")
        {
            return Some(member_model);
        }
        let requested_model = self
            .requested_model
            .as_deref()
            .unwrap_or(self.group.candidate.global_model_name.as_str());
        let mapping_matched_model = member_model.1;
        let resolved_model = resolve_provider_pool_key_thinking_model(
            key,
            self.group.candidate.provider_type.as_str(),
            requested_model,
            member_model.0.as_str(),
            Some(level),
        )?;
        if resolved_model == member_model.0.as_str() {
            return Some((member_model.0, mapping_matched_model));
        }
        Some((resolved_model.to_string(), mapping_matched_model))
    }

    async fn build_eligible_candidate(
        &mut self,
        candidate: aether_scheduler_core::SchedulerMinimalCandidateSelectionCandidate,
    ) -> Option<EligibleLocalExecutionCandidate> {
        if self
            .routing_overlay
            .as_ref()
            .is_some_and(|overlay| !overlay.key_allowed(candidate.key_id.as_str()))
        {
            self.record_skip_reason(ROUTING_PROFILE_DISALLOWED_KEY_SKIP_REASON);
            return None;
        }
        if !self.seen_key_ids.insert(candidate.key_id.clone()) {
            return None;
        }

        let transport_started_at = std::time::Instant::now();
        let Some(transport) = read_candidate_transport_snapshot(self.state, &candidate).await
        else {
            observe_gateway_stage_ms(
                "candidate_transport_snapshot",
                transport_started_at.elapsed().as_millis() as u64,
            );
            self.record_skip_reason("transport_snapshot_missing");
            return None;
        };
        observe_gateway_stage_ms(
            "candidate_transport_snapshot",
            transport_started_at.elapsed().as_millis() as u64,
        );
        if let Some(skip_reason) =
            candidate_auth_channel_skip_reason(&transport, self.request_auth_channel.as_deref())
        {
            self.record_skip_reason(skip_reason);
            return None;
        }
        if let Some(skip_reason) = candidate_common_transport_skip_reason(
            &transport,
            pool_candidate_transport_policy_facts(&candidate),
            self.requested_model.as_deref(),
        ) {
            self.record_skip_reason(skip_reason);
            return None;
        }
        Some(EligibleLocalExecutionCandidate {
            kind: LocalExecutionCandidateKind::SingleKey,
            candidate,
            provider_api_format: transport.endpoint.api_format.trim().to_ascii_lowercase(),
            transport: std::sync::Arc::new(transport),
            orchestration: LocalExecutionCandidateMetadata::default(),
            ranking: self.group.ranking.clone(),
        })
    }

    fn record_skip_reason(&mut self, reason: &'static str) {
        *self.skip_reason_counts.entry(reason).or_insert(0) += 1;
    }

    fn record_skipped_candidates(&mut self, skipped_candidates: &[SkippedLocalExecutionCandidate]) {
        for skipped_candidate in skipped_candidates {
            self.record_skip_reason(skipped_candidate.skip_reason);
        }
        let prefiltered_count = skipped_candidates
            .iter()
            .filter(|candidate| pool_skip_reason_releases_scan_budget(candidate.skip_reason))
            .count();
        self.budget_scanned_keys = self
            .budget_scanned_keys
            .saturating_sub(u32::try_from(prefiltered_count).unwrap_or(u32::MAX));
    }
}

/// 目录已明确不可调度的 Key 不占用游标扫描预算，避免低余额池提前停止回退扫描。
fn pool_skip_reason_releases_scan_budget(skip_reason: &str) -> bool {
    matches!(
        skip_reason,
        POOL_ACCOUNT_EXHAUSTED_SKIP_REASON
            | POOL_ACCOUNT_BLOCKED_SKIP_REASON
            | POOL_KEY_QUOTA_EXHAUSTED_SKIP_REASON
            | POOL_KEY_STATE_UNAVAILABLE_SKIP_REASON
            | POOL_BALANCE_BELOW_MINIMUM_SKIP_REASON
    )
}

fn pool_candidate_transport_policy_facts(
    candidate: &aether_scheduler_core::SchedulerMinimalCandidateSelectionCandidate,
) -> CandidateTransportPolicyFacts<'_> {
    CandidateTransportPolicyFacts {
        endpoint_api_format: candidate.endpoint_api_format.as_str(),
        global_model_name: candidate.global_model_name.as_str(),
        selected_provider_model_name: candidate.selected_provider_model_name.as_str(),
        mapping_matched_model: candidate.mapping_matched_model.as_deref(),
    }
}

/// 按调度核心映射规则解析成员实际发送的上游模型名。
///
/// 所有成员统一走同一解析器：无账号模型清单时保持行自身的默认/映射模型名并保留
/// Endpoint 与操作范围，不继承其他账号的模型目标；显式清单（含空清单）必须与请求
/// 模型或全局映射产生交集，否则整行拒绝。`requested_model_name` 必须是与预选一致的
/// 路由模型（指令基座或原始请求名），`enable_model_directives` 固定关闭以复用预选的
/// 指令语义，绝不比枚举更宽松。
fn pool_member_model_resolution(
    group: &EligibleLocalExecutionCandidate,
    row: &StoredMinimalCandidateSelectionRow,
    requested_model_name: &str,
    request_operation: Option<&str>,
) -> Option<(String, Option<String>)> {
    aether_scheduler_core::resolve_provider_model_name_with_model_directives_and_request_operation(
        row,
        requested_model_name,
        group.candidate.endpoint_api_format.as_str(),
        false,
        request_operation,
    )
}

/// 将数据库候选行还原为当前 Pool 组的精确 Provider、Endpoint 与模型候选，
/// 并应用该账号自己的上游模型解析结果。
fn pool_candidate_from_row(
    group: &EligibleLocalExecutionCandidate,
    row: StoredMinimalCandidateSelectionRow,
    member_model: (String, Option<String>),
) -> aether_scheduler_core::SchedulerMinimalCandidateSelectionCandidate {
    let mut candidate = group.candidate.clone();
    candidate.key_id = row.key_id;
    candidate.key_name = row.key_name;
    candidate.key_auth_type = row.key_auth_type;
    candidate.key_internal_priority = row.key_internal_priority;
    candidate.key_global_priority_for_format =
        aether_scheduler_core::extract_global_priority_for_format(
            row.key_global_priority_by_format.as_ref(),
            group.candidate.endpoint_api_format.as_str(),
        )
        .ok()
        .flatten();
    candidate.key_capabilities = row.key_capabilities;
    candidate.routing_facts = row.routing_facts;
    candidate.selected_provider_model_name = member_model.0;
    candidate.mapping_matched_model = member_model.1;
    candidate
}

/// 用强读得到的目录 Key 替换 Pool 占位候选，同时保留精确 Endpoint 与模型绑定，
/// 并应用该账号自己的上游模型解析结果。
fn pool_candidate_from_catalog_key(
    group: &EligibleLocalExecutionCandidate,
    key: StoredProviderCatalogKey,
    member_model: (String, Option<String>),
) -> aether_scheduler_core::SchedulerMinimalCandidateSelectionCandidate {
    let mut candidate = group.candidate.clone();
    candidate.key_id = key.id;
    candidate.key_name = key.name;
    candidate.key_auth_type = key.auth_type;
    candidate.key_internal_priority = key.internal_priority;
    candidate.key_global_priority_for_format =
        aether_scheduler_core::extract_global_priority_for_format(
            key.global_priority_by_format.as_ref(),
            group.candidate.endpoint_api_format.as_str(),
        )
        .ok()
        .flatten();
    candidate.key_capabilities = key.capabilities;
    candidate.selected_provider_model_name = member_model.0;
    candidate.mapping_matched_model = member_model.1;
    candidate
}

#[derive(Debug, Default, PartialEq, Eq)]
struct PoolRuntimeModelQuotaFacts {
    /// 已确认耗尽的 (Key, 实际上游模型) 事实。
    blocked: BTreeMap<String, BTreeSet<String>>,
    /// 读取失败的 (Key, 实际上游模型)；只能报告状态不可用。
    state_unavailable: BTreeMap<String, BTreeSet<String>>,
}

impl PoolRuntimeModelQuotaFacts {
    /// 按描述符顺序消费批量读取结果；长度不一致按状态不可用处理。
    fn from_read(
        identities: &[(&StoredProviderCatalogKey, &str, &str)],
        blocked: Option<Vec<bool>>,
    ) -> Self {
        let Some(blocked) = blocked.filter(|blocked| blocked.len() == identities.len()) else {
            return Self::unavailable(identities);
        };
        let mut facts = Self::default();
        for ((key, _, model), blocked) in identities.iter().zip(blocked) {
            if blocked {
                facts
                    .blocked
                    .entry(key.id.clone())
                    .or_default()
                    .insert((*model).to_string());
            }
        }
        facts
    }

    fn unavailable(identities: &[(&StoredProviderCatalogKey, &str, &str)]) -> Self {
        let mut facts = Self::default();
        for (key, _, model) in identities {
            facts
                .state_unavailable
                .entry(key.id.clone())
                .or_default()
                .insert((*model).to_string());
        }
        facts
    }
}

/// 收集会消费运行时模型额度事实的真实成员：只有 Antigravity、已强读目录、且实际
/// 上游模型名非空的候选才参与，其他候选不产生任何描述符或后端读取。
fn pool_model_runtime_quota_identities<'a>(
    candidates: &'a [EligibleLocalExecutionCandidate],
    keys_by_id: &'a BTreeMap<String, StoredProviderCatalogKey>,
) -> Vec<(&'a StoredProviderCatalogKey, &'a str, &'a str)> {
    let mut identities = Vec::new();
    let mut seen = BTreeSet::<(&str, &str)>::new();
    for candidate in candidates {
        let provider_type = candidate.transport.provider.provider_type.as_str();
        if !provider_type
            .trim()
            .eq_ignore_ascii_case(POOL_RUNTIME_MODEL_QUOTA_PROVIDER_TYPE)
        {
            continue;
        }
        let model = candidate.candidate.selected_provider_model_name.trim();
        if model.is_empty() {
            continue;
        }
        let Some(key) = keys_by_id.get(candidate.candidate.key_id.as_str()) else {
            continue;
        };
        if !seen.insert((key.id.as_str(), model)) {
            continue;
        }
        identities.push((key, provider_type, model));
    }
    identities
}

/// 读取同一页真实成员的精确模型额度事实，并投影为带模型边界的跳过原因。
async fn read_pool_model_runtime_skip_reasons(
    state: PlannerAppState<'_>,
    candidates: &[EligibleLocalExecutionCandidate],
    keys_by_id: &BTreeMap<String, StoredProviderCatalogKey>,
    now_unix_secs: u64,
) -> PoolModelRuntimeSkipReasons {
    let identities = pool_model_runtime_quota_identities(candidates, keys_by_id);
    if identities.is_empty() {
        return PoolModelRuntimeSkipReasons::new();
    }
    let app = state.app();
    // The shared fact is keyed by the account-stable refresh-token generation;
    // decrypt the same credentials the writer observed. A malformed/legacy
    // ciphertext on one account only marks that (key, model) unavailable and is
    // excluded from the shared lookup so valid siblings keep serving.
    let mut descriptors = Vec::with_capacity(identities.len());
    let mut readable_identities = Vec::with_capacity(identities.len());
    let mut auth_configs = Vec::with_capacity(identities.len());
    let mut unresolved = Vec::new();
    for &(key, provider_type, model) in &identities {
        match app.decrypt_provider_catalog_key_auth_config(key) {
            Ok(auth_config) => {
                readable_identities.push((key, provider_type, model));
                descriptors.push(AntigravityModelQuotaCandidate {
                    key,
                    provider_type,
                    model,
                });
                auth_configs.push(auth_config);
            }
            Err(err) => {
                warn!(
                    error = ?err,
                    provider_id = %key.provider_id,
                    key_id = %key.id,
                    "gateway pool scheduler: failed to decrypt antigravity key auth config"
                );
                unresolved.push((key.id.clone(), model.to_string()));
            }
        }
    }
    let read = if descriptors.is_empty() {
        Ok(Vec::new())
    } else {
        let auth_config_refs = auth_configs
            .iter()
            .map(Option::as_deref)
            .collect::<Vec<_>>();
        crate::orchestration::antigravity_model_quota_blocked_many_with_auth_configs(
            app.runtime_state.as_ref(),
            &descriptors,
            &auth_config_refs,
            now_unix_secs,
        )
        .await
        .map_err(|err| {
            warn!(
                error = ?err,
                candidate_count = identities.len(),
                "gateway pool scheduler: failed to read antigravity runtime model quota state"
            );
        })
    };
    let mut facts = PoolRuntimeModelQuotaFacts::from_read(&readable_identities, read.ok());
    for (key_id, model) in unresolved {
        facts
            .state_unavailable
            .entry(key_id)
            .or_default()
            .insert(model);
    }
    pool_model_runtime_skip_reasons(candidates, &facts)
}

/// 将运行时事实投影为 Pool 既有诊断：耗尽复用账号额度原因，读取失败复用状态不可用
/// 原因。原因以实际上游模型名为边界，同 Key 的其他模型保持可调度。
fn pool_model_runtime_skip_reasons(
    candidates: &[EligibleLocalExecutionCandidate],
    facts: &PoolRuntimeModelQuotaFacts,
) -> PoolModelRuntimeSkipReasons {
    let mut reasons = PoolModelRuntimeSkipReasons::new();
    for candidate in candidates {
        if !candidate
            .transport
            .provider
            .provider_type
            .trim()
            .eq_ignore_ascii_case(POOL_RUNTIME_MODEL_QUOTA_PROVIDER_TYPE)
        {
            continue;
        }
        let key_id = candidate.candidate.key_id.as_str();
        let model = candidate.candidate.selected_provider_model_name.trim();
        let reason = if facts
            .state_unavailable
            .get(key_id)
            .is_some_and(|models| models.contains(model))
        {
            Some(POOL_KEY_STATE_UNAVAILABLE_SKIP_REASON)
        } else if facts
            .blocked
            .get(key_id)
            .is_some_and(|models| models.contains(model))
        {
            Some(POOL_ACCOUNT_EXHAUSTED_SKIP_REASON)
        } else {
            None
        };
        if let Some(reason) = reason {
            reasons
                .entry(key_id.to_string())
                .or_default()
                .entry(model.to_string())
                .or_insert(reason);
        }
    }
    reasons
}

/// 强读候选 Key 的目录事实并构建调度上下文；读取失败或缺项时显式标记不可用以关闭候选。
/// 每个成员自己解析出的模型名只交给 Provider 额度适配器，不改变目录缺失时的 fail-closed 行为。
async fn read_pool_catalog_key_contexts_by_id(
    state: PlannerAppState<'_>,
    candidates: &mut Vec<EligibleLocalExecutionCandidate>,
    effective_pool_config: Option<&AdminProviderPoolConfig>,
    requested_model: Option<&str>,
    thinking_level: Option<AntigravityThinkingLevel>,
) -> (
    BTreeMap<String, PoolCatalogKeyContext>,
    BTreeMap<String, StoredProviderCatalogKey>,
    Vec<SkippedLocalExecutionCandidate>,
) {
    let mut key_ids = Vec::new();
    let mut provider_type_by_key_id = BTreeMap::<String, String>::new();
    let mut reserve_minimum_quota_key_ids = BTreeSet::new();

    for candidate in candidates.iter() {
        let Some(pool_config) = effective_pool_config
            .cloned()
            .or_else(|| pool_config_for_candidate(candidate))
        else {
            continue;
        };
        let key_id = candidate.candidate.key_id.clone();
        if pool_config.reserve_minimum_quota {
            reserve_minimum_quota_key_ids.insert(key_id.clone());
        }
        if let Entry::Vacant(entry) = provider_type_by_key_id.entry(key_id.clone()) {
            entry.insert(candidate.transport.provider.provider_type.clone());
            key_ids.push(key_id);
        }
    }

    if key_ids.is_empty() {
        return (BTreeMap::new(), BTreeMap::new(), Vec::new());
    }

    let keys = match state
        .app()
        .list_provider_catalog_keys_by_ids_strong(&key_ids)
        .await
    {
        Ok(keys) => keys,
        Err(err) => {
            warn!(
                error = ?err,
                key_count = key_ids.len(),
                "gateway pool scheduler: failed to read catalog key metadata"
            );
            // Do not fail open when the persistent scheduling fact cannot be
            // read. Keep this distinct from a confirmed quota block so
            // diagnostics do not instruct administrators to recover the Key.
            let contexts = key_ids
                .into_iter()
                .map(|key_id| {
                    (
                        key_id,
                        PoolCatalogKeyContext {
                            catalog_state_unavailable: true,
                            ..PoolCatalogKeyContext::default()
                        },
                    )
                })
                .collect();
            return (contexts, BTreeMap::new(), Vec::new());
        }
    };

    let keys_by_id = keys
        .into_iter()
        .map(|key| (key.id.clone(), key))
        .collect::<BTreeMap<_, _>>();

    // 真实成员在目录强读之后、任何额度/发现/余额判定之前解析自己的档位模型；
    // 缺少请求档位（或空目录/禁用项）的成员在这里被拒绝，绝不回退默认档位。
    let mut resolution_skipped = Vec::new();
    if let Some(level) = thinking_level {
        let requested_model = requested_model.unwrap_or_default();
        candidates.retain_mut(|candidate| {
            // 只有 Antigravity 真实成员消费档位选择；其他 Provider 保持原模型名。
            if !candidate
                .candidate
                .provider_type
                .trim()
                .eq_ignore_ascii_case("antigravity")
            {
                return true;
            }
            let Some(key) = keys_by_id.get(candidate.candidate.key_id.as_str()) else {
                // 目录 Key 缺失由既有 fail-closed 路径处理，这里不改变其优先级。
                return true;
            };
            match resolve_provider_pool_key_thinking_model(
                key,
                candidate.candidate.provider_type.as_str(),
                requested_model,
                candidate.candidate.selected_provider_model_name.as_str(),
                Some(level),
            ) {
                Some(model)
                    if model == candidate.candidate.selected_provider_model_name.as_str() =>
                {
                    true
                }
                Some(model) => {
                    candidate.candidate.selected_provider_model_name = model.to_string();
                    true
                }
                None => {
                    resolution_skipped.push(SkippedLocalExecutionCandidate {
                        candidate: candidate.candidate.clone(),
                        skip_reason: "key_model_disabled",
                        transport: Some(candidate.transport.clone()),
                        ranking: candidate.ranking.clone(),
                        extra_data: None,
                    });
                    false
                }
            }
        });
    }

    let mut model_name_by_key_id = BTreeMap::<String, String>::new();
    for candidate in candidates.iter() {
        model_name_by_key_id
            .entry(candidate.candidate.key_id.clone())
            .or_insert_with(|| candidate.candidate.selected_provider_model_name.clone());
    }

    let provider_pool_service = ProviderPoolService::with_builtin_adapters();
    let mut contexts = key_ids
        .into_iter()
        .map(|key_id| {
            (
                key_id,
                PoolCatalogKeyContext {
                    catalog_state_unavailable: true,
                    ..PoolCatalogKeyContext::default()
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    for key in keys_by_id.values() {
        let Some(provider_type) = provider_type_by_key_id.get(&key.id) else {
            continue;
        };
        // 混合账号允许不同映射模型；额度窗口与保底额度按各自实际发送的模型判定。
        let provider_model_name = model_name_by_key_id.get(&key.id).map(String::as_str);
        let mut context = build_pool_catalog_key_context(
            state,
            &provider_pool_service,
            key,
            provider_type.as_str(),
            provider_model_name,
        );
        let reserve_reached = reserve_minimum_quota_key_ids.contains(&key.id)
            && admin_provider_pool_pure::admin_pool_key_minimum_quota_reached(
                key,
                provider_type.as_str(),
                provider_model_name,
            );
        context.quota_exhausted |= reserve_reached;
        context.quota_hard_blocked |= reserve_reached;
        contexts.insert(key.id.clone(), context);
    }
    (contexts, keys_by_id, resolution_skipped)
}

/// 将单个持久化 Key 投影为 Pool 调度信号，并保持余额、永久阻断和模型额度的边界。
fn build_pool_catalog_key_context(
    state: PlannerAppState<'_>,
    provider_pool_service: &ProviderPoolService,
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    provider_model_name: Option<&str>,
) -> PoolCatalogKeyContext {
    let (health_score, _, _, _, _) = provider_key_health_summary(key);
    let health_score = key
        .health_by_format
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .filter(|payload| !payload.is_empty())
        .map(|_| health_score);
    let latency_avg_ms = key
        .success_count
        .filter(|count| *count > 0)
        .zip(key.total_response_time_ms)
        .map(|(success_count, total_response_time_ms)| {
            total_response_time_ms as f64 / f64::from(success_count)
        })
        .filter(|value| value.is_finite() && *value >= 0.0);

    let auth_config = parse_catalog_auth_config_json(state.app(), key);
    let mut signals = provider_pool_service.member_signals(
        provider_type,
        key,
        auth_config.as_ref(),
        provider_model_name,
    );
    // Provider 适配器只按请求模型判断可恢复额度；持久化运行时阻断始终是整个 Key 的强事实。
    signals.runtime_quota_hard_blocked =
        aether_provider_pool::provider_pool_key_runtime_quota_blocked(key);
    signals.account_blocked |= admin_provider_pool_pure::admin_pool_key_is_known_banned(key);
    signals.account_blocked |=
        pool_key_requires_reauth_for_scheduling(key, current_unix_ms().saturating_div(1000));
    signals.health_score = health_score;
    signals.latency_avg_ms = latency_avg_ms;
    signals.catalog_lru_score = Some(key.last_used_at_unix_secs.unwrap_or(0) as f64);
    signals
}

fn pool_key_requires_reauth_for_scheduling(
    key: &StoredProviderCatalogKey,
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
        let account_state = admin_provider_status_pure::resolve_pool_account_state(
            None,
            key.upstream_metadata.as_ref(),
            Some(invalid_reason),
        );
        if account_state.blocked && !account_state.recoverable {
            return true;
        }
        if pool_oauth_reason_has_tag(invalid_reason, "[ACCOUNT_BLOCK]") {
            return true;
        }
        if pool_oauth_reason_has_tag(invalid_reason, "[REQUEST_FAILED]") {
            return false;
        }
        if pool_oauth_reason_has_tag(invalid_reason, "[REFRESH_FAILED]") {
            return key
                .expires_at_unix_secs
                .is_none_or(|expires_at| expires_at == 0 || expires_at <= now_unix_secs);
        }
        if pool_oauth_reason_has_tag(invalid_reason, "[OAUTH_EXPIRED]") {
            return false;
        }
        return true;
    }

    key.oauth_invalid_at_unix_secs.is_some()
}

fn pool_oauth_reason_has_tag(reason: &str, tag: &str) -> bool {
    reason
        .lines()
        .map(str::trim)
        .any(|line| line.starts_with(tag))
}

fn apply_local_execution_pool_scheduler_with_runtime_map(
    candidates: Vec<EligibleLocalExecutionCandidate>,
    runtime_by_provider: &BTreeMap<String, AdminProviderPoolRuntimeState>,
    key_context_by_id: &BTreeMap<String, PoolCatalogKeyContext>,
) -> (
    Vec<EligibleLocalExecutionCandidate>,
    Vec<SkippedLocalExecutionCandidate>,
) {
    let outcome = apply_local_execution_pool_scheduler_with_runtime_map_outcome(
        candidates,
        runtime_by_provider,
        key_context_by_id,
    );
    (outcome.candidates, outcome.skipped)
}

struct PoolSchedulerApplyOutcome {
    candidates: Vec<EligibleLocalExecutionCandidate>,
    skipped: Vec<SkippedLocalExecutionCandidate>,
    active_probe_seal_fallback_provider_ids: BTreeSet<String>,
    active_probe_evicted_members_by_provider: BTreeMap<String, BTreeSet<String>>,
}

fn apply_local_execution_pool_scheduler_with_runtime_map_outcome(
    candidates: Vec<EligibleLocalExecutionCandidate>,
    runtime_by_provider: &BTreeMap<String, AdminProviderPoolRuntimeState>,
    key_context_by_id: &BTreeMap<String, PoolCatalogKeyContext>,
) -> PoolSchedulerApplyOutcome {
    apply_local_execution_pool_scheduler_with_runtime_map_outcome_and_configs(
        candidates,
        runtime_by_provider,
        key_context_by_id,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &PoolModelRuntimeSkipReasons::new(),
    )
}

fn apply_local_execution_pool_scheduler_with_runtime_map_outcome_and_configs(
    candidates: Vec<EligibleLocalExecutionCandidate>,
    runtime_by_provider: &BTreeMap<String, AdminProviderPoolRuntimeState>,
    key_context_by_id: &BTreeMap<String, PoolCatalogKeyContext>,
    effective_pool_config_by_provider: &BTreeMap<String, AdminProviderPoolConfig>,
    key_runtime_skip_reasons: &BTreeMap<String, &'static str>,
    model_runtime_skip_reasons: &PoolModelRuntimeSkipReasons,
) -> PoolSchedulerApplyOutcome {
    let (scheduled, skipped) = run_local_execution_pool_scheduler_with_runtime_map(
        candidates.clone(),
        runtime_by_provider,
        key_context_by_id,
        effective_pool_config_by_provider,
        key_runtime_skip_reasons,
        model_runtime_skip_reasons,
        true,
    );
    let mut active_probe_evicted_members_by_provider =
        active_probe_evicted_members_from_skipped(&skipped, runtime_by_provider);
    let active_probe_seal_fallback_provider_ids = if scheduled.is_empty() {
        skipped
            .iter()
            .filter(|skipped| skipped.skip_reason == POOL_ACTIVE_PROBE_SEALED_SKIP_REASON)
            .map(|skipped| skipped.candidate.provider_id.clone())
            .collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };

    if active_probe_seal_fallback_provider_ids.is_empty() {
        return PoolSchedulerApplyOutcome {
            candidates: scheduled,
            skipped,
            active_probe_seal_fallback_provider_ids,
            active_probe_evicted_members_by_provider,
        };
    }

    let (scheduled, skipped) = run_local_execution_pool_scheduler_with_runtime_map(
        candidates,
        runtime_by_provider,
        key_context_by_id,
        effective_pool_config_by_provider,
        key_runtime_skip_reasons,
        model_runtime_skip_reasons,
        false,
    );
    merge_active_probe_evictions(
        &mut active_probe_evicted_members_by_provider,
        active_probe_evicted_members_from_skipped(&skipped, runtime_by_provider),
    );
    PoolSchedulerApplyOutcome {
        candidates: scheduled,
        skipped,
        active_probe_seal_fallback_provider_ids,
        active_probe_evicted_members_by_provider,
    }
}

fn merge_active_probe_evictions(
    target: &mut BTreeMap<String, BTreeSet<String>>,
    source: BTreeMap<String, BTreeSet<String>>,
) {
    for (provider_id, key_ids) in source {
        target.entry(provider_id).or_default().extend(key_ids);
    }
}

fn active_probe_evicted_members_from_skipped(
    skipped: &[SkippedLocalExecutionCandidate],
    runtime_by_provider: &BTreeMap<String, AdminProviderPoolRuntimeState>,
) -> BTreeMap<String, BTreeSet<String>> {
    if skipped.is_empty() {
        return BTreeMap::new();
    }

    let mut evicted = BTreeMap::<String, BTreeSet<String>>::new();
    for skipped_candidate in skipped {
        if !matches!(
            skipped_candidate.skip_reason,
            POOL_ACCOUNT_BLOCKED_SKIP_REASON
                | POOL_ACCOUNT_EXHAUSTED_SKIP_REASON
                | POOL_KEY_QUOTA_EXHAUSTED_SKIP_REASON
                | POOL_KEY_STATE_UNAVAILABLE_SKIP_REASON
                | POOL_BALANCE_BELOW_MINIMUM_SKIP_REASON
                | POOL_COOLDOWN_SKIP_REASON
                | POOL_COST_LIMIT_REACHED_SKIP_REASON
        ) {
            continue;
        }
        let Some(runtime) = runtime_by_provider.get(&skipped_candidate.candidate.provider_id)
        else {
            continue;
        };
        if runtime
            .active_probe_member_ids
            .contains(&skipped_candidate.candidate.key_id)
        {
            evicted
                .entry(skipped_candidate.candidate.provider_id.clone())
                .or_default()
                .insert(skipped_candidate.candidate.key_id.clone());
        }
    }
    evicted
}

fn run_local_execution_pool_scheduler_with_runtime_map(
    candidates: Vec<EligibleLocalExecutionCandidate>,
    runtime_by_provider: &BTreeMap<String, AdminProviderPoolRuntimeState>,
    key_context_by_id: &BTreeMap<String, PoolCatalogKeyContext>,
    effective_pool_config_by_provider: &BTreeMap<String, AdminProviderPoolConfig>,
    key_runtime_skip_reasons: &BTreeMap<String, &'static str>,
    model_runtime_skip_reasons: &PoolModelRuntimeSkipReasons,
    enforce_active_probe_seal: bool,
) -> (
    Vec<EligibleLocalExecutionCandidate>,
    Vec<SkippedLocalExecutionCandidate>,
) {
    let scheduler_runtime_by_provider = runtime_by_provider
        .iter()
        .map(|(provider_id, runtime)| (provider_id.clone(), pool_runtime_state(runtime)))
        .collect::<BTreeMap<_, _>>();
    let mut inputs = Vec::new();
    let mut skipped_candidates = Vec::new();
    for candidate in candidates {
        let key_context = key_context_by_id
            .get(&candidate.candidate.key_id)
            .cloned()
            .unwrap_or_else(|| {
                // An explicitly non-empty metadata snapshot should contain
                // every catalog key in this page. If one disappeared between
                // reads, fail closed for that key instead of sending traffic
                // with an unknown quota state. Empty maps are retained for
                // callers/tests that intentionally provide no runtime context.
                if key_context_by_id.is_empty() {
                    PoolCatalogKeyContext::default()
                } else {
                    PoolCatalogKeyContext {
                        quota_hard_blocked: true,
                        ..PoolCatalogKeyContext::default()
                    }
                }
            });
        let admin_pool_config = effective_pool_config_by_provider
            .get(&candidate.candidate.provider_id)
            .cloned()
            .or_else(|| pool_config_for_candidate(&candidate));

        if let Some(config) = admin_pool_config.as_ref() {
            if enforce_active_probe_seal && should_enforce_active_probe_sealed_pool(config) {
                let active_member_ids = runtime_by_provider
                    .get(&candidate.candidate.provider_id)
                    .map(|runtime| &runtime.active_probe_member_ids);
                let should_seal_cold_member = active_member_ids.is_some_and(|members| {
                    !members.is_empty() && !members.contains(&candidate.candidate.key_id)
                });
                if should_seal_cold_member {
                    skipped_candidates.push(SkippedLocalExecutionCandidate {
                        candidate: candidate.candidate.clone(),
                        skip_reason: POOL_ACTIVE_PROBE_SEALED_SKIP_REASON,
                        transport: Some(candidate.transport.clone()),
                        ranking: candidate.ranking.clone(),
                        extra_data: None,
                    });
                    continue;
                }
            }
        }

        let pool_config = admin_pool_config.map(|config| {
            pool_scheduling_config(config, candidate.transport.provider.provider_type.as_str())
        });
        inputs.push(PoolCandidateInput {
            facts: pool_candidate_facts(&candidate),
            pool_config,
            key_context,
            candidate,
        });
    }
    let outcome = run_pool_scheduler(
        inputs,
        &scheduler_runtime_by_provider,
        pool_sort_seed().as_str(),
    );

    let candidates = outcome
        .candidates
        .into_iter()
        .filter_map(|scheduled| {
            let candidate = scheduled.candidate;
            // 在冻结窗口和热池回退判定之前过滤真实 Key，同时保留前面 Pool 硬过滤的原因。
            let model_skip_reason = model_runtime_skip_reasons
                .get(candidate.candidate.key_id.as_str())
                .and_then(|models| {
                    models.get(candidate.candidate.selected_provider_model_name.trim())
                })
                .copied();
            let skip_reason = key_runtime_skip_reasons
                .get(&candidate.candidate.key_id)
                .copied()
                .or(model_skip_reason);
            if let Some(skip_reason) = skip_reason {
                debug!(
                    event_name = "pool_candidate_runtime_skipped",
                    provider_id = %candidate.candidate.provider_id,
                    key_id = %candidate.candidate.key_id,
                    selected_provider_model_name = %candidate.candidate.selected_provider_model_name,
                    skip_reason,
                    "gateway pool scheduler skipped key during runtime selectability resolution"
                );
                skipped_candidates.push(SkippedLocalExecutionCandidate {
                    candidate: candidate.candidate,
                    skip_reason,
                    transport: Some(candidate.transport),
                    ranking: candidate.ranking,
                    extra_data: None,
                });
                return None;
            }
            Some(apply_pool_orchestration(candidate, scheduled.orchestration))
        })
        .collect::<Vec<_>>();
    skipped_candidates.extend(outcome.skipped_candidates.into_iter().map(|skipped| {
        SkippedLocalExecutionCandidate {
            candidate: skipped.candidate.candidate,
            skip_reason: skipped.skip_reason,
            transport: Some(skipped.candidate.transport),
            ranking: skipped.candidate.ranking,
            extra_data: None,
        }
    }));

    (candidates, skipped_candidates)
}

fn pool_config_for_candidate(
    candidate: &EligibleLocalExecutionCandidate,
) -> Option<AdminProviderPoolConfig> {
    admin_provider_pool_config_from_config_value(candidate.transport.provider.config.as_ref())
}

fn effective_pool_config_for_group(
    group: &EligibleLocalExecutionCandidate,
    routing_policy: Option<&ResolvedRoutingPolicy>,
) -> Option<AdminProviderPoolConfig> {
    let mut pool_config = pool_config_for_candidate(group)?;
    let override_policy = routing_policy
        .and_then(|policy| {
            policy
                .pool_policy_overrides
                .get(group.candidate.provider_id.as_str())
        })
        .filter(|override_policy| !override_policy.scheduling_presets.is_empty());
    if let Some(override_policy) = override_policy {
        let scheduling_presets = override_policy
            .scheduling_presets
            .iter()
            .map(|preset| AdminProviderPoolSchedulingPreset {
                preset: preset.preset.clone(),
                enabled: preset.enabled,
                mode: preset.mode.clone(),
            })
            .collect::<Vec<_>>();
        pool_config.lru_enabled = scheduling_presets
            .iter()
            .any(|preset| preset.enabled && preset.preset.eq_ignore_ascii_case("lru"));
        pool_config.scheduling_presets = scheduling_presets;
    }
    Some(pool_config)
}

/// 判断当前 Pool 是否要求活跃探测集合封闭，防止未探测成员越过热集边界。
fn should_enforce_active_probe_sealed_pool(pool_config: &AdminProviderPoolConfig) -> bool {
    pool_config.probing_enabled
}

/// 判断当前活跃探测成员是否低于目标热容量，从而决定是否触发请求级补充探测。
fn should_trigger_active_probe_burst_for_request(
    pool_config: &AdminProviderPoolConfig,
    runtime: &AdminProviderPoolRuntimeState,
) -> bool {
    if !should_enforce_active_probe_sealed_pool(pool_config) {
        return false;
    }
    if runtime.provider_burst_pending {
        return false;
    }
    let active_count = runtime.active_probe_member_ids.len();
    runtime.provider_desired_hot > 0 && active_count < runtime.provider_desired_hot
}

/// 把互斥的分配预设映射到既有数据库排序；缓存亲和仅决定未命中时的二级顺序。
fn pool_key_candidate_order_for_group(
    group: &EligibleLocalExecutionCandidate,
    pool_config: Option<&AdminProviderPoolConfig>,
) -> StoredPoolKeyCandidateOrder {
    let Some(pool_config) = pool_config else {
        return StoredPoolKeyCandidateOrder::InternalPriority;
    };
    let presets = pool_config
        .scheduling_presets
        .iter()
        .map(|preset| PoolSchedulingPreset {
            preset: preset.preset.clone(),
            enabled: preset.enabled,
            mode: preset.mode.clone(),
        })
        .collect::<Vec<_>>();
    let active_presets = ProviderPoolService::with_builtin_adapters()
        .normalize_scheduling_presets(group.transport.provider.provider_type.as_str(), &presets);
    if let Some(distribution_mode) = active_presets
        .iter()
        .find(|preset| pool_distribution_mode_preset(preset.preset.as_str()))
    {
        return match distribution_mode.preset.as_str() {
            "cache_affinity" => match distribution_mode.mode.as_deref() {
                Some("lru") => StoredPoolKeyCandidateOrder::Lru,
                Some("single_account") => StoredPoolKeyCandidateOrder::SingleAccount,
                _ => StoredPoolKeyCandidateOrder::CacheAffinity,
            },
            "load_balance" => StoredPoolKeyCandidateOrder::LoadBalance {
                seed: pool_sort_seed(),
            },
            "single_account" => StoredPoolKeyCandidateOrder::SingleAccount,
            _ => StoredPoolKeyCandidateOrder::InternalPriority,
        };
    }
    if pool_config.lru_enabled {
        return StoredPoolKeyCandidateOrder::Lru;
    }
    StoredPoolKeyCandidateOrder::InternalPriority
}

fn pool_distribution_mode_preset(preset: &str) -> bool {
    matches!(preset, "cache_affinity" | "load_balance" | "single_account")
}

fn pool_sort_seed() -> String {
    let now_ms = current_unix_ms();
    let sequence = LOAD_BALANCE_SEQUENCE.fetch_add(1, AtomicOrdering::Relaxed);
    format!("{now_ms}:{sequence}")
}

fn pool_candidate_facts(candidate: &EligibleLocalExecutionCandidate) -> PoolCandidateFacts {
    PoolCandidateFacts {
        provider_id: candidate.candidate.provider_id.clone(),
        endpoint_id: candidate.candidate.endpoint_id.clone(),
        model_id: candidate.candidate.model_id.clone(),
        selected_provider_model_name: candidate.candidate.selected_provider_model_name.clone(),
        provider_api_format: candidate.provider_api_format.clone(),
        key_id: candidate.candidate.key_id.clone(),
        key_internal_priority: candidate.candidate.key_internal_priority,
    }
}

fn pool_cursor_candidate_group_id(group: &EligibleLocalExecutionCandidate) -> String {
    format!(
        "provider={}|endpoint={}|model={}|selected_model={}|api_format={}|singleton_key=*",
        group.candidate.provider_id,
        group.candidate.endpoint_id,
        group.candidate.model_id,
        group.candidate.selected_provider_model_name,
        group.provider_api_format,
    )
}

fn pool_scheduling_config(
    config: AdminProviderPoolConfig,
    provider_type: &str,
) -> PoolSchedulingConfig {
    let service = ProviderPoolService::with_builtin_adapters();
    let scheduling_presets = config
        .scheduling_presets
        .into_iter()
        .map(|preset| PoolSchedulingPreset {
            preset: preset.preset,
            enabled: preset.enabled,
            mode: preset.mode,
        })
        .collect::<Vec<_>>();
    PoolSchedulingConfig {
        scheduling_presets: service
            .normalize_scheduling_presets(provider_type, &scheduling_presets),
        lru_enabled: config.lru_enabled,
        skip_exhausted_accounts: config.skip_exhausted_accounts,
        cost_limit_per_key_tokens: config.cost_limit_per_key_tokens,
    }
}

fn pool_runtime_state(runtime: &AdminProviderPoolRuntimeState) -> PoolRuntimeState {
    PoolRuntimeState {
        sticky_bound_key_id: runtime.sticky_bound_key_id.clone(),
        cooldown_reason_by_key: runtime.cooldown_reason_by_key.clone(),
        cost_window_usage_by_key: runtime.cost_window_usage_by_key.clone(),
        latency_avg_ms_by_key: runtime.latency_avg_ms_by_key.clone(),
        lru_score_by_key: runtime.lru_score_by_key.clone(),
    }
}

/// 将 Pool 分组索引与租约附加到候选，同时保留请求级粘性尝试预算和亲和纪元。
fn apply_pool_orchestration(
    mut candidate: EligibleLocalExecutionCandidate,
    orchestration: PoolCandidateOrchestration,
) -> EligibleLocalExecutionCandidate {
    let scheduler_affinity_epoch = candidate.orchestration.scheduler_affinity_epoch;
    let sticky_key_attempts = candidate.orchestration.sticky_key_attempts;
    candidate.orchestration = LocalExecutionCandidateMetadata {
        candidate_group_id: orchestration.candidate_group_id,
        pool_key_index: orchestration.pool_key_index,
        pool_key_lease: None,
        scheduler_affinity_epoch,
        sticky_key_attempts,
    };
    candidate
}

#[cfg(test)]
mod tests {
    use super::{
        admin_provider_pool_quota_probe_active_members_key, apply_local_execution_pool_scheduler,
        apply_local_execution_pool_scheduler_with_runtime_map,
        apply_local_execution_pool_scheduler_with_runtime_map_outcome,
        apply_local_execution_pool_scheduler_with_runtime_map_outcome_and_configs,
        build_pool_catalog_key_context, effective_pool_config_for_group, pool_config_for_candidate,
        pool_key_candidate_order_for_group, pool_key_requires_reauth_for_scheduling,
        pool_model_runtime_quota_identities, pool_model_runtime_skip_reasons,
        pool_skip_reason_releases_scan_budget,
        prune_unschedulable_active_probe_members_for_request, read_pool_catalog_key_contexts_by_id,
        remove_active_probe_members_for_request, should_trigger_active_probe_burst_for_request,
        PoolCatalogKeyContext, PoolKeyCursor, PoolModelRuntimeSkipReasons,
        PoolRuntimeModelQuotaFacts, POOL_ACTIVE_PROBE_SEALED_SKIP_REASON,
        ROUTING_PROFILE_DISALLOWED_KEY_SKIP_REASON,
    };
    use crate::ai_serving::{
        apply_local_runtime_candidate_terminal_reason, provider_key_pool_score_id,
        provider_key_pool_score_scope, EligibleLocalExecutionCandidate,
        LocalExecutionCandidateKind, PlannerAppState,
    };
    use crate::data::GatewayDataState;
    use crate::handlers::shared::provider_pool::{
        admin_provider_pool_cache_affinity_enabled, admin_provider_pool_config_from_config_value,
        read_admin_provider_pool_runtime_state, read_provider_pool_scheduling_runtime_state,
        record_admin_provider_pool_error, record_admin_provider_pool_success,
        AdminProviderPoolRuntimeState,
    };
    use crate::orchestration::LocalExecutionCandidateMetadata;
    use crate::{AppState, LocalExecutionRuntimeMissDiagnostic};
    use aether_data::repository::candidate_selection::InMemoryMinimalCandidateSelectionReadRepository;
    use aether_data::repository::pool_scores::InMemoryPoolMemberScoreRepository;
    use aether_data::repository::provider_catalog::InMemoryProviderCatalogReadRepository;
    use aether_data_contracts::repository::candidate_selection::{
        StoredMinimalCandidateSelectionRow, StoredPoolKeyCandidateOrder,
    };
    use aether_data_contracts::repository::pool_scores::{
        PoolMemberHardState, PoolMemberIdentity, PoolMemberProbeStatus, StoredPoolMemberScore,
    };
    use aether_data_contracts::repository::provider_catalog::{
        StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
    };
    use aether_pool_core::{
        PoolMemberSignals, PoolSchedulingPreset, POOL_ACCOUNT_EXHAUSTED_SKIP_REASON,
        POOL_BALANCE_BELOW_MINIMUM_SKIP_REASON, POOL_KEY_STATE_UNAVAILABLE_SKIP_REASON,
    };
    use aether_provider_pool::ProviderPoolService;
    use aether_provider_transport::antigravity::AntigravityThinkingLevel;
    use aether_provider_transport::snapshot::{
        GatewayProviderTransportEndpoint, GatewayProviderTransportKey,
        GatewayProviderTransportProvider,
    };
    use aether_routing_core::{
        RankingOverlay, ResolvedRoutingPolicy, RoutingPoolPolicyOverride, RoutingSchedulingMode,
        RoutingSchedulingPreset, RoutingSetPriorityMode,
    };
    use aether_scheduler_core::SchedulerMinimalCandidateSelectionCandidate;
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    use std::sync::Arc;

    #[tokio::test]
    async fn scheduling_runtime_preserves_pool_ranking_and_cost_rejections() {
        let runtime = aether_runtime_state::RuntimeState::memory(
            aether_runtime_state::MemoryRuntimeStateConfig::default(),
        );
        let writer_config = admin_provider_pool_config_from_config_value(Some(&json!({
            "pool_advanced": {
                "cost_limit_per_key_tokens": 100,
                "scheduling_presets": [
                    {"preset": "cache_affinity", "enabled": true},
                    {"preset": "latency_first", "enabled": true}
                ]
            }
        })))
        .expect("writer pool config");
        for (key_id, cost, latency) in [("key-a", 100, 10), ("key-b", 20, 100)] {
            record_admin_provider_pool_success(
                &runtime,
                "provider-pool",
                key_id,
                &writer_config,
                Some(key_id),
                cost,
                Some(latency),
            )
            .await;
        }
        let key_ids = vec!["key-a".to_string(), "key-b".to_string()];
        for (preset, cost_limit) in [
            ("cache_affinity", None),
            ("priority_first", None),
            ("latency_first", None),
            ("cost_first", None),
            ("quota_balanced", None),
            ("latency_first", Some(100)),
        ] {
            let provider_config = json!({
                "pool_advanced": {
                    "cost_limit_per_key_tokens": cost_limit,
                    "scheduling_presets": [{"preset": preset, "enabled": true}]
                }
            });
            let pool_config = admin_provider_pool_config_from_config_value(Some(&provider_config))
                .expect("reader pool config");
            let admin = read_admin_provider_pool_runtime_state(
                &runtime,
                "provider-pool",
                &key_ids,
                &pool_config,
                Some("key-a"),
            )
            .await;
            let scheduling = read_provider_pool_scheduling_runtime_state(
                &runtime,
                "provider-pool",
                &key_ids,
                &pool_config,
                Some("key-a"),
            )
            .await;
            let run = |snapshot| {
                let candidates = key_ids
                    .iter()
                    .map(|key_id| {
                        sample_eligible_candidate(
                            "provider-pool",
                            "endpoint-1",
                            key_id,
                            10,
                            Some(provider_config.clone()),
                        )
                    })
                    .collect();
                let (scheduled, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
                    candidates,
                    &BTreeMap::from([("provider-pool".to_string(), snapshot)]),
                    &BTreeMap::new(),
                );
                (
                    scheduled
                        .into_iter()
                        .map(|item| item.candidate.key_id)
                        .collect::<Vec<_>>(),
                    skipped
                        .into_iter()
                        .map(|item| (item.candidate.key_id, item.skip_reason))
                        .collect::<Vec<_>>(),
                )
            };
            let expected = run(admin);
            let actual = run(scheduling);
            assert_eq!(
                actual, expected,
                "preset: {preset}, cost limit: {cost_limit:?}"
            );
            if cost_limit.is_some() {
                assert_eq!(actual.0, vec!["key-b"]);
                assert_eq!(
                    actual.1,
                    vec![("key-a".to_string(), "pool_cost_limit_reached")]
                );
            }
        }
    }

    #[test]
    fn pool_scheduler_groups_interleaved_candidates_and_reorders_internal_keys() {
        let pool_first = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-pool-a",
            10,
            Some(json!({ "pool_advanced": { "lru_enabled": true } })),
        );
        let other =
            sample_eligible_candidate("provider-other", "endpoint-2", "key-other", 10, None);
        let pool_second = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-pool-b",
            10,
            Some(json!({ "pool_advanced": { "lru_enabled": true } })),
        );

        let mut runtime_by_provider = BTreeMap::new();
        runtime_by_provider.insert(
            "provider-pool".to_string(),
            AdminProviderPoolRuntimeState {
                lru_score_by_key: BTreeMap::from([
                    ("key-pool-a".to_string(), 20.0),
                    ("key-pool-b".to_string(), 10.0),
                ]),
                ..AdminProviderPoolRuntimeState::default()
            },
        );

        let (reordered, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![pool_first, other, pool_second],
            &runtime_by_provider,
            &BTreeMap::new(),
        );

        assert!(skipped.is_empty());
        assert_eq!(
            reordered
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-pool-b", "key-pool-a", "key-other"]
        );
    }

    #[test]
    fn pool_scheduler_uses_catalog_last_used_when_runtime_lru_is_missing() {
        let recent_key = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-recent",
            10,
            Some(json!({ "pool_advanced": { "lru_enabled": true } })),
        );
        let older_key = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-older",
            10,
            Some(json!({ "pool_advanced": { "lru_enabled": true } })),
        );

        let key_context_by_id = BTreeMap::from([
            (
                "key-recent".to_string(),
                PoolCatalogKeyContext {
                    catalog_lru_score: Some(200.0),
                    ..PoolCatalogKeyContext::default()
                },
            ),
            (
                "key-older".to_string(),
                PoolCatalogKeyContext {
                    catalog_lru_score: Some(100.0),
                    ..PoolCatalogKeyContext::default()
                },
            ),
        ]);

        let (reordered, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![recent_key, older_key],
            &BTreeMap::new(),
            &key_context_by_id,
        );

        assert!(skipped.is_empty());
        assert_eq!(
            reordered
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-older", "key-recent"]
        );
    }

    /// 验证 Pool 排序结果携带组索引、Key 索引、租约及粘性尝试预算。
    #[test]
    fn pool_scheduler_allows_quota_exhausted_key_when_ignored() {
        let exhausted = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-exhausted",
            10,
            Some(json!({ "pool_advanced": { "ignore_exhausted_accounts": true } })),
        );
        let contexts = BTreeMap::from([(
            "key-exhausted".to_string(),
            PoolCatalogKeyContext {
                quota_exhausted: true,
                ..PoolCatalogKeyContext::default()
            },
        )]);
        let (scheduled, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![exhausted],
            &BTreeMap::new(),
            &contexts,
        );
        assert_eq!(scheduled.len(), 1);
        assert!(skipped.is_empty());
    }

    #[test]
    fn pool_scheduler_skips_quota_exhausted_key_when_flag_is_false() {
        let ready = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-ready",
            10,
            Some(json!({ "pool_advanced": {} })),
        );
        let exhausted = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-exhausted",
            10,
            Some(json!({ "pool_advanced": { "skip_exhausted_accounts": false } })),
        );
        let key_context_by_id = BTreeMap::from([
            ("key-ready".to_string(), PoolCatalogKeyContext::default()),
            (
                "key-exhausted".to_string(),
                PoolCatalogKeyContext {
                    quota_exhausted: true,
                    ..PoolCatalogKeyContext::default()
                },
            ),
        ]);

        let (scheduled, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![ready, exhausted],
            &BTreeMap::new(),
            &key_context_by_id,
        );

        assert_eq!(
            scheduled
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-ready"]
        );
        assert_eq!(
            skipped
                .iter()
                .map(|item| (item.candidate.key_id.as_str(), item.skip_reason))
                .collect::<Vec<_>>(),
            vec![("key-exhausted", POOL_ACCOUNT_EXHAUSTED_SKIP_REASON)]
        );
    }

    #[test]
    fn pool_scheduler_attaches_group_and_pool_metadata_to_ranked_candidates() {
        let pool_first = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-pool-a",
            10,
            Some(json!({ "pool_advanced": { "lru_enabled": true } })),
        );
        let other =
            sample_eligible_candidate("provider-other", "endpoint-2", "key-other", 10, None);
        let pool_second = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-pool-b",
            10,
            Some(json!({ "pool_advanced": { "lru_enabled": true } })),
        );

        let mut runtime_by_provider = BTreeMap::new();
        runtime_by_provider.insert(
            "provider-pool".to_string(),
            AdminProviderPoolRuntimeState {
                lru_score_by_key: BTreeMap::from([
                    ("key-pool-a".to_string(), 20.0),
                    ("key-pool-b".to_string(), 10.0),
                ]),
                ..AdminProviderPoolRuntimeState::default()
            },
        );

        let (reordered, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![pool_first, other, pool_second],
            &runtime_by_provider,
            &BTreeMap::new(),
        );

        assert!(skipped.is_empty());
        assert_eq!(reordered.len(), 3);
        assert_eq!(
            reordered[0].orchestration,
            LocalExecutionCandidateMetadata {
                candidate_group_id: Some(
                    "provider=provider-pool|endpoint=endpoint-1|model=model-1|selected_model=gpt-5|api_format=openai:chat|singleton_key=*"
                        .to_string(),
                ),
                pool_key_index: Some(0),
                pool_key_lease: None,
                scheduler_affinity_epoch: None,
                sticky_key_attempts: None,
            }
        );
        assert_eq!(reordered[1].orchestration.pool_key_index, Some(1));
        assert_eq!(
            reordered[1].orchestration.candidate_group_id,
            reordered[0].orchestration.candidate_group_id
        );
        assert_eq!(
            reordered[2].orchestration,
            LocalExecutionCandidateMetadata {
                candidate_group_id: Some(
                    "provider=provider-other|endpoint=endpoint-2|model=model-1|selected_model=gpt-5|api_format=openai:chat|singleton_key=key-other"
                        .to_string(),
                ),
                pool_key_index: None,
                pool_key_lease: None,
                scheduler_affinity_epoch: None,
                sticky_key_attempts: None,
            }
        );
    }

    /// 验证缓存亲和命中优先于配置的 LRU 二级分配顺序。
    #[test]
    fn pool_scheduler_promotes_sticky_hit_before_lru_secondary_order() {
        let key_a = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-a",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [{
                        "preset": "cache_affinity",
                        "enabled": true,
                        "mode": "lru"
                    }]
                }
            })),
        );
        let key_b = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-b",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [{
                        "preset": "cache_affinity",
                        "enabled": true,
                        "mode": "lru"
                    }]
                }
            })),
        );

        let mut runtime_by_provider = BTreeMap::new();
        runtime_by_provider.insert(
            "provider-pool".to_string(),
            AdminProviderPoolRuntimeState {
                sticky_bound_key_id: Some("key-a".to_string()),
                lru_score_by_key: BTreeMap::from([
                    ("key-a".to_string(), 50.0),
                    ("key-b".to_string(), 10.0),
                ]),
                ..AdminProviderPoolRuntimeState::default()
            },
        );

        let (reordered, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![key_a, key_b],
            &runtime_by_provider,
            &BTreeMap::new(),
        );

        assert!(skipped.is_empty());
        assert_eq!(
            reordered
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-a", "key-b"]
        );
    }

    /// 验证缓存亲和的单号集中与 LRU 轮号映射到不同的既有排序。
    #[test]
    fn cache_affinity_secondary_modes_select_distinct_candidate_orders() {
        for (mode, expected) in [
            ("single_account", StoredPoolKeyCandidateOrder::SingleAccount),
            ("lru", StoredPoolKeyCandidateOrder::Lru),
        ] {
            let group = sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                "key-a",
                10,
                Some(json!({
                    "pool_advanced": {
                        "scheduling_presets": [{
                            "preset": "cache_affinity",
                            "enabled": true,
                            "mode": mode
                        }]
                    }
                })),
            );
            let config = pool_config_for_candidate(&group).expect("pool config should parse");

            assert!(admin_provider_pool_cache_affinity_enabled(&config));
            assert_eq!(
                pool_key_candidate_order_for_group(&group, Some(&config)),
                expected
            );
        }
    }

    #[test]
    fn pool_scheduler_ignores_sticky_hit_without_cache_affinity() {
        let key_a = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-a",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [{"preset": "quota_balanced", "enabled": true}]
                }
            })),
        );
        let key_b = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-b",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [{"preset": "quota_balanced", "enabled": true}]
                }
            })),
        );

        let runtime_by_provider = BTreeMap::from([(
            "provider-pool".to_string(),
            AdminProviderPoolRuntimeState {
                sticky_bound_key_id: Some("key-a".to_string()),
                ..AdminProviderPoolRuntimeState::default()
            },
        )]);

        let (reordered, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![key_b, key_a],
            &runtime_by_provider,
            &BTreeMap::new(),
        );

        assert!(skipped.is_empty());
        assert_eq!(
            reordered
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-b", "key-a"]
        );
    }

    #[test]
    fn pool_scheduler_skips_cooldown_and_cost_exhausted_keys() {
        let key_ready = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-ready",
            10,
            Some(json!({
                "pool_advanced": {
                    "cost_limit_per_key_tokens": 100
                }
            })),
        );
        let key_cooldown = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-cooldown",
            10,
            Some(json!({
                "pool_advanced": {
                    "cost_limit_per_key_tokens": 100
                }
            })),
        );
        let key_cost = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-cost",
            10,
            Some(json!({
                "pool_advanced": {
                    "cost_limit_per_key_tokens": 100
                }
            })),
        );

        let mut runtime_by_provider = BTreeMap::new();
        runtime_by_provider.insert(
            "provider-pool".to_string(),
            AdminProviderPoolRuntimeState {
                cooldown_reason_by_key: BTreeMap::from([(
                    "key-cooldown".to_string(),
                    "429".to_string(),
                )]),
                cost_window_usage_by_key: BTreeMap::from([("key-cost".to_string(), 100)]),
                ..AdminProviderPoolRuntimeState::default()
            },
        );

        let (reordered, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![key_ready, key_cooldown, key_cost],
            &runtime_by_provider,
            &BTreeMap::new(),
        );

        assert_eq!(
            reordered
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-ready"]
        );
        assert_eq!(
            skipped
                .iter()
                .map(|item| (item.candidate.key_id.as_str(), item.skip_reason))
                .collect::<Vec<_>>(),
            vec![
                ("key-cooldown", "pool_cooldown"),
                ("key-cost", "pool_cost_limit_reached"),
            ]
        );
    }

    #[test]
    fn pool_scheduler_uses_only_active_probe_members_when_active_probe_enabled() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "probing_enabled": true
            }
        }));
        let key_active = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-active",
            10,
            provider_config.clone(),
        );
        let key_sealed = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-sealed",
            10,
            provider_config,
        );

        let runtime_by_provider = BTreeMap::from([(
            "provider-pool".to_string(),
            AdminProviderPoolRuntimeState {
                active_probe_member_ids: BTreeSet::from(["key-active".to_string()]),
                ..AdminProviderPoolRuntimeState::default()
            },
        )]);

        let (scheduled, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![key_sealed, key_active],
            &runtime_by_provider,
            &BTreeMap::new(),
        );

        assert_eq!(
            scheduled
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-active"]
        );
        assert_eq!(
            skipped
                .iter()
                .map(|item| (item.candidate.key_id.as_str(), item.skip_reason))
                .collect::<Vec<_>>(),
            vec![("key-sealed", POOL_ACTIVE_PROBE_SEALED_SKIP_REASON)]
        );
    }

    #[test]
    fn pool_scheduler_falls_back_when_active_probe_members_are_unschedulable() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "probing_enabled": true
            }
        }));
        let key_hot = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-hot",
            10,
            provider_config.clone(),
        );
        let key_cold = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-cold",
            10,
            provider_config,
        );

        let runtime_by_provider = BTreeMap::from([(
            "provider-pool".to_string(),
            AdminProviderPoolRuntimeState {
                active_probe_member_ids: BTreeSet::from(["key-hot".to_string()]),
                cooldown_reason_by_key: BTreeMap::from([(
                    "key-hot".to_string(),
                    "429".to_string(),
                )]),
                provider_desired_hot: 1,
                ..AdminProviderPoolRuntimeState::default()
            },
        )]);

        let outcome = apply_local_execution_pool_scheduler_with_runtime_map_outcome(
            vec![key_hot, key_cold],
            &runtime_by_provider,
            &BTreeMap::new(),
        );
        let scheduled = outcome.candidates;
        let skipped = outcome.skipped;

        assert_eq!(
            scheduled
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-cold"]
        );
        assert_eq!(
            skipped
                .iter()
                .map(|item| (item.candidate.key_id.as_str(), item.skip_reason))
                .collect::<Vec<_>>(),
            vec![("key-hot", "pool_cooldown")]
        );
        assert_eq!(
            outcome
                .active_probe_evicted_members_by_provider
                .get("provider-pool"),
            Some(&BTreeSet::from(["key-hot".to_string()]))
        );
    }

    /// 验证完整目录快照中的低余额热成员退队，并回退到仍可用的冷成员。
    #[test]
    fn pool_scheduler_evicts_low_balance_active_probe_member() {
        let provider_config = Some(json!({
            "pool_advanced": {"probing_enabled": true}
        }));
        let key_hot = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-hot",
            10,
            provider_config.clone(),
        );
        let key_cold = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-cold",
            10,
            provider_config,
        );
        let runtime_by_provider = BTreeMap::from([(
            "provider-pool".to_string(),
            AdminProviderPoolRuntimeState {
                active_probe_member_ids: BTreeSet::from(["key-hot".to_string()]),
                provider_desired_hot: 1,
                ..AdminProviderPoolRuntimeState::default()
            },
        )]);
        let key_context_by_id = BTreeMap::from([
            (
                "key-hot".to_string(),
                PoolMemberSignals {
                    balance_below_minimum: true,
                    ..PoolMemberSignals::default()
                },
            ),
            ("key-cold".to_string(), PoolMemberSignals::default()),
        ]);

        let outcome = apply_local_execution_pool_scheduler_with_runtime_map_outcome(
            vec![key_hot, key_cold],
            &runtime_by_provider,
            &key_context_by_id,
        );

        assert_eq!(
            outcome
                .candidates
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-cold"]
        );
        assert_eq!(
            outcome
                .skipped
                .iter()
                .map(|item| (item.candidate.key_id.as_str(), item.skip_reason))
                .collect::<Vec<_>>(),
            vec![(
                "key-hot",
                aether_pool_core::POOL_BALANCE_BELOW_MINIMUM_SKIP_REASON
            )]
        );
        assert_eq!(
            outcome
                .active_probe_evicted_members_by_provider
                .get("provider-pool"),
            Some(&BTreeSet::from(["key-hot".to_string()]))
        );
    }

    #[test]
    fn pool_low_balance_skip_releases_cursor_scan_budget() {
        // 验证目录事实过滤不会消耗有限的 Pool 游标扫描预算。
        assert!(pool_skip_reason_releases_scan_budget(
            aether_pool_core::POOL_BALANCE_BELOW_MINIMUM_SKIP_REASON
        ));
    }

    #[test]
    fn pool_scheduler_prunes_cold_active_probe_members_before_scheduling() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "probing_enabled": true
            }
        }));
        let key_hot = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-hot",
            10,
            provider_config,
        );
        let mut runtime_by_provider = BTreeMap::from([(
            "provider-pool".to_string(),
            AdminProviderPoolRuntimeState {
                active_probe_member_ids: BTreeSet::from(["key-hot".to_string()]),
                cooldown_reason_by_key: BTreeMap::from([(
                    "key-hot".to_string(),
                    "429".to_string(),
                )]),
                provider_desired_hot: 1,
                ..AdminProviderPoolRuntimeState::default()
            },
        )]);

        let evicted = prune_unschedulable_active_probe_members_for_request(
            &mut runtime_by_provider,
            &[key_hot],
            &BTreeMap::new(),
        );

        assert_eq!(
            evicted.get("provider-pool"),
            Some(&BTreeSet::from(["key-hot".to_string()]))
        );
        assert!(runtime_by_provider
            .get("provider-pool")
            .expect("runtime should exist")
            .active_probe_member_ids
            .is_empty());
    }

    /// 验证已知订阅耗尽始终使热成员退队，历史开关的任一取值都不能绕过过滤。
    #[test]
    fn active_probe_subscription_exhaustion_ignores_historical_switch() {
        let key_context_by_id = BTreeMap::from([(
            "key-hot".to_string(),
            PoolMemberSignals {
                quota_exhausted: true,
                ..PoolMemberSignals::default()
            },
        )]);
        for skip_exhausted_accounts in [false, true] {
            let candidate = sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                "key-hot",
                10,
                Some(json!({
                    "pool_advanced": {
                        "probing_enabled": true,
                        "skip_exhausted_accounts": skip_exhausted_accounts
                    }
                })),
            );
            let mut runtime = BTreeMap::from([(
                "provider-pool".to_string(),
                AdminProviderPoolRuntimeState {
                    active_probe_member_ids: BTreeSet::from(["key-hot".to_string()]),
                    ..AdminProviderPoolRuntimeState::default()
                },
            )]);
            let evicted = prune_unschedulable_active_probe_members_for_request(
                &mut runtime,
                &[candidate],
                &key_context_by_id,
            );
            assert_eq!(
                evicted.get("provider-pool"),
                Some(&BTreeSet::from(["key-hot".to_string()]))
            );
            assert!(runtime["provider-pool"].active_probe_member_ids.is_empty());
        }
    }

    #[tokio::test]
    async fn pool_scheduler_removes_unschedulable_member_from_active_probe_set() {
        let app = AppState::new().expect("state should build");
        let set_key = admin_provider_pool_quota_probe_active_members_key("provider-pool");
        app.runtime_state
            .set_add(&set_key, "key-hot")
            .await
            .expect("active member should insert");

        remove_active_probe_members_for_request(
            PlannerAppState::new(&app),
            &BTreeMap::from([(
                "provider-pool".to_string(),
                BTreeSet::from(["key-hot".to_string()]),
            )]),
        )
        .await;

        let members = app
            .runtime_state
            .set_members(&set_key)
            .await
            .expect("active members should read");
        assert!(members.is_empty());
    }

    #[test]
    fn pool_scheduler_allows_cold_start_when_active_probe_pool_is_empty() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "probing_enabled": true
            }
        }));
        let key_a = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-a",
            10,
            provider_config.clone(),
        );
        let key_b =
            sample_eligible_candidate("provider-pool", "endpoint-1", "key-b", 10, provider_config);

        let runtime_by_provider = BTreeMap::from([(
            "provider-pool".to_string(),
            AdminProviderPoolRuntimeState::default(),
        )]);

        let (scheduled, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![key_a, key_b],
            &runtime_by_provider,
            &BTreeMap::new(),
        );

        assert_eq!(
            scheduled
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-a", "key-b"]
        );
        assert_eq!(
            skipped
                .iter()
                .map(|item| (item.candidate.key_id.as_str(), item.skip_reason))
                .collect::<Vec<_>>(),
            Vec::<(&str, &str)>::new()
        );
    }

    #[test]
    fn pool_scheduler_triggers_burst_when_auto_hot_target_has_gap() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "probing_enabled": true
            }
        }));
        let candidate =
            sample_eligible_candidate("provider-pool", "endpoint-1", "key-a", 10, provider_config);
        let pool_config = pool_config_for_candidate(&candidate).expect("pool config should parse");
        let runtime = AdminProviderPoolRuntimeState {
            active_probe_member_ids: BTreeSet::from(["key-a".to_string()]),
            provider_desired_hot: 3,
            ..AdminProviderPoolRuntimeState::default()
        };

        assert!(should_trigger_active_probe_burst_for_request(
            &pool_config,
            &runtime
        ));
    }

    #[test]
    fn pool_scheduler_ignores_legacy_threshold_fields_for_burst_target() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "probing_enabled": true,
                "probing_target_percent": 60,
                "probing_target_count": 10
            }
        }));
        let candidate =
            sample_eligible_candidate("provider-pool", "endpoint-1", "key-a", 10, provider_config);
        let pool_config = pool_config_for_candidate(&candidate).expect("pool config should parse");
        let runtime = AdminProviderPoolRuntimeState {
            active_probe_member_ids: BTreeSet::from(["key-a".to_string(), "key-b".to_string()]),
            provider_desired_hot: 2,
            ..AdminProviderPoolRuntimeState::default()
        };

        assert!(!should_trigger_active_probe_burst_for_request(
            &pool_config,
            &runtime
        ));
    }

    #[test]
    fn pool_scheduler_skips_burst_when_active_probe_target_is_met() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "probing_enabled": true,
                "probing_target_count": 1
            }
        }));
        let candidate =
            sample_eligible_candidate("provider-pool", "endpoint-1", "key-a", 10, provider_config);
        let pool_config = pool_config_for_candidate(&candidate).expect("pool config should parse");
        let runtime = AdminProviderPoolRuntimeState {
            active_probe_member_ids: BTreeSet::from(["key-a".to_string()]),
            provider_desired_hot: 1,
            ..AdminProviderPoolRuntimeState::default()
        };

        assert!(!should_trigger_active_probe_burst_for_request(
            &pool_config,
            &runtime
        ));
    }

    #[test]
    fn pool_scheduler_applies_distribution_mode_before_strategy_presets() {
        let key_a = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-a",
            50,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [
                        {"preset": "cache_affinity", "enabled": true},
                        {"preset": "priority_first", "enabled": true}
                    ]
                }
            })),
        );
        let key_b = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-b",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [
                        {"preset": "cache_affinity", "enabled": true},
                        {"preset": "priority_first", "enabled": true}
                    ]
                }
            })),
        );

        let mut runtime_by_provider = BTreeMap::new();
        runtime_by_provider.insert(
            "provider-pool".to_string(),
            AdminProviderPoolRuntimeState {
                lru_score_by_key: BTreeMap::from([
                    ("key-a".to_string(), 100.0),
                    ("key-b".to_string(), 5.0),
                ]),
                ..AdminProviderPoolRuntimeState::default()
            },
        );

        let (reordered, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![key_a, key_b],
            &runtime_by_provider,
            &BTreeMap::new(),
        );

        assert!(skipped.is_empty());
        assert_eq!(
            reordered
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-a", "key-b"]
        );
    }

    #[test]
    fn pool_scheduler_uses_plan_preset_with_catalog_context() {
        let key_free = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-free",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [{"preset": "plus_first", "enabled": true}]
                }
            })),
        );
        let key_plus = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-plus",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [{"preset": "plus_first", "enabled": true}]
                }
            })),
        );

        let key_context_by_id = BTreeMap::from([
            (
                "key-free".to_string(),
                PoolCatalogKeyContext {
                    plan_tier: Some("free".to_string()),
                    ..PoolCatalogKeyContext::default()
                },
            ),
            (
                "key-plus".to_string(),
                PoolCatalogKeyContext {
                    plan_tier: Some("plus".to_string()),
                    ..PoolCatalogKeyContext::default()
                },
            ),
        ]);

        let (reordered, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![key_free, key_plus],
            &BTreeMap::new(),
            &key_context_by_id,
        );

        assert!(skipped.is_empty());
        assert_eq!(
            reordered
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-plus", "key-free"]
        );
    }

    #[test]
    fn pool_scheduler_plus_first_treats_plus_and_pro_as_top_tier() {
        let key_plus = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-plus",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [{"preset": "plus_first", "enabled": true}]
                }
            })),
        );
        let key_pro = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-pro",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [{"preset": "plus_first", "enabled": true}]
                }
            })),
        );
        let key_team = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-team",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [{"preset": "plus_first", "enabled": true}]
                }
            })),
        );

        let key_context_by_id = BTreeMap::from([
            (
                "key-plus".to_string(),
                PoolCatalogKeyContext {
                    plan_tier: Some("plus".to_string()),
                    catalog_lru_score: Some(300.0),
                    ..PoolCatalogKeyContext::default()
                },
            ),
            (
                "key-pro".to_string(),
                PoolCatalogKeyContext {
                    plan_tier: Some("pro".to_string()),
                    catalog_lru_score: Some(100.0),
                    ..PoolCatalogKeyContext::default()
                },
            ),
            (
                "key-team".to_string(),
                PoolCatalogKeyContext {
                    plan_tier: Some("team".to_string()),
                    catalog_lru_score: Some(50.0),
                    ..PoolCatalogKeyContext::default()
                },
            ),
        ]);

        let (reordered, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![key_plus, key_pro, key_team],
            &BTreeMap::new(),
            &key_context_by_id,
        );

        assert!(skipped.is_empty());
        assert_eq!(
            reordered
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-pro", "key-plus", "key-team"]
        );
    }

    #[test]
    fn pool_scheduler_supports_pro_first_plan_preset() {
        let key_plus = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-plus",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [{"preset": "pro_first", "enabled": true}]
                }
            })),
        );
        let key_pro = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-pro",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [{"preset": "pro_first", "enabled": true}]
                }
            })),
        );
        let key_team = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-team",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [{"preset": "pro_first", "enabled": true}]
                }
            })),
        );

        let key_context_by_id = BTreeMap::from([
            (
                "key-plus".to_string(),
                PoolCatalogKeyContext {
                    plan_tier: Some("plus".to_string()),
                    ..PoolCatalogKeyContext::default()
                },
            ),
            (
                "key-pro".to_string(),
                PoolCatalogKeyContext {
                    plan_tier: Some("pro".to_string()),
                    ..PoolCatalogKeyContext::default()
                },
            ),
            (
                "key-team".to_string(),
                PoolCatalogKeyContext {
                    plan_tier: Some("team".to_string()),
                    ..PoolCatalogKeyContext::default()
                },
            ),
        ]);

        let (reordered, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![key_plus, key_team, key_pro],
            &BTreeMap::new(),
            &key_context_by_id,
        );

        assert!(skipped.is_empty());
        assert_eq!(
            reordered
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-pro", "key-plus", "key-team"]
        );
    }

    #[test]
    fn pool_scheduler_defaults_empty_pool_advanced_to_cache_affinity() {
        let key_a = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-a",
            10,
            Some(json!({ "pool_advanced": {} })),
        );
        let key_b = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-b",
            10,
            Some(json!({ "pool_advanced": {} })),
        );

        let runtime_by_provider = BTreeMap::from([(
            "provider-pool".to_string(),
            AdminProviderPoolRuntimeState {
                lru_score_by_key: BTreeMap::from([
                    ("key-a".to_string(), 10.0),
                    ("key-b".to_string(), 200.0),
                ]),
                ..AdminProviderPoolRuntimeState::default()
            },
        )]);

        let (reordered, skipped) = apply_local_execution_pool_scheduler_with_runtime_map(
            vec![key_a, key_b],
            &runtime_by_provider,
            &BTreeMap::new(),
        );

        assert!(skipped.is_empty());
        assert_eq!(
            reordered
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-b", "key-a"]
        );
    }

    #[test]
    fn normalizes_distribution_mode_before_strategy_presets() {
        let presets = ProviderPoolService::with_builtin_adapters()
            .normalize_scheduling_presets(
                "openai",
                &[
                    PoolSchedulingPreset {
                        preset: "lru".to_string(),
                        enabled: false,
                        mode: None,
                    },
                    PoolSchedulingPreset {
                        preset: "single_account".to_string(),
                        enabled: true,
                        mode: None,
                    },
                    PoolSchedulingPreset {
                        preset: "cache_affinity".to_string(),
                        enabled: true,
                        mode: None,
                    },
                    PoolSchedulingPreset {
                        preset: "priority_first".to_string(),
                        enabled: true,
                        mode: None,
                    },
                ],
            )
            .into_iter()
            .map(|preset| preset.preset)
            .collect::<Vec<_>>();

        assert_eq!(presets, ["single_account", "priority_first"]);
    }

    #[test]
    fn pool_key_cursor_uses_distribution_order_for_page_queries() {
        let app = AppState::new().expect("state should build");
        let load_balance_group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [
                        {"preset": "load_balance", "enabled": true},
                        {"preset": "priority_first", "enabled": true}
                    ]
                }
            })),
        );
        let load_balance_cursor = PoolKeyCursor::new(
            PlannerAppState::new(&app),
            load_balance_group,
            None,
            None,
            None,
        );
        assert!(matches!(
            load_balance_cursor.pool_key_order,
            StoredPoolKeyCandidateOrder::LoadBalance { ref seed } if !seed.is_empty()
        ));

        let lru_group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            Some(json!({ "pool_advanced": { "lru_enabled": true } })),
        );
        let lru_cursor =
            PoolKeyCursor::new(PlannerAppState::new(&app), lru_group, None, None, None);
        assert_eq!(lru_cursor.pool_key_order, StoredPoolKeyCandidateOrder::Lru);
    }

    #[test]
    fn routing_pool_override_is_effective_for_page_scheduling_and_sticky_mode() {
        let base_config = Some(json!({
            "pool_advanced": {
                "scheduling_presets": [{"preset": "plus_first", "enabled": true}]
            }
        }));
        let key_plus = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-plus",
            10,
            base_config.clone(),
        );
        let key_pro =
            sample_eligible_candidate("provider-pool", "endpoint-1", "key-pro", 10, base_config);
        let mut routing_policy = routing_policy_with_allowed_keys([]);
        routing_policy.pool_policy_overrides.insert(
            "provider-pool".to_string(),
            RoutingPoolPolicyOverride {
                scheduling_presets: vec![RoutingSchedulingPreset {
                    preset: "pro_first".to_string(),
                    enabled: true,
                    mode: Some("pro_only".to_string()),
                }],
            },
        );
        let effective_config = effective_pool_config_for_group(&key_plus, Some(&routing_policy))
            .expect("effective pool config should parse");
        let effective_configs = BTreeMap::from([("provider-pool".to_string(), effective_config)]);
        let key_context_by_id = BTreeMap::from([
            (
                "key-plus".to_string(),
                PoolCatalogKeyContext {
                    plan_tier: Some("plus".to_string()),
                    ..PoolCatalogKeyContext::default()
                },
            ),
            (
                "key-pro".to_string(),
                PoolCatalogKeyContext {
                    plan_tier: Some("pro".to_string()),
                    ..PoolCatalogKeyContext::default()
                },
            ),
        ]);

        let outcome = apply_local_execution_pool_scheduler_with_runtime_map_outcome_and_configs(
            vec![key_plus, key_pro],
            &BTreeMap::new(),
            &key_context_by_id,
            &effective_configs,
            &BTreeMap::new(),
            &PoolModelRuntimeSkipReasons::new(),
        );

        assert!(outcome.skipped.is_empty());
        assert_eq!(
            outcome
                .candidates
                .iter()
                .map(|candidate| candidate.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            ["key-pro", "key-plus"]
        );

        let app = AppState::new().expect("state should build");
        let cache_affinity_group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [{"preset": "load_balance", "enabled": true}]
                }
            })),
        );
        routing_policy.pool_policy_overrides.insert(
            "provider-pool".to_string(),
            RoutingPoolPolicyOverride {
                scheduling_presets: vec![RoutingSchedulingPreset {
                    preset: "cache_affinity".to_string(),
                    enabled: true,
                    mode: None,
                }],
            },
        );
        let cursor = PoolKeyCursor::new_with_routing_policy(
            PlannerAppState::new(&app),
            cache_affinity_group,
            None,
            None,
            None,
            Some(&routing_policy),
        );
        assert!(admin_provider_pool_cache_affinity_enabled(
            cursor
                .effective_pool_config
                .as_ref()
                .expect("effective config should exist")
        ));
    }

    /// 验证路由强制 LRU 时会重新计算派生排序状态，而非复用旧分配模式缓存。
    #[test]
    fn routing_pool_lru_override_recomputes_derived_lru_state() {
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [
                        {"preset": "cache_affinity", "enabled": true}
                    ]
                }
            })),
        );
        let mut routing_policy = routing_policy_with_allowed_keys([]);
        routing_policy.pool_policy_overrides.insert(
            "provider-pool".to_string(),
            RoutingPoolPolicyOverride {
                scheduling_presets: vec![RoutingSchedulingPreset {
                    preset: "lru".to_string(),
                    enabled: true,
                    mode: None,
                }],
            },
        );

        let effective_config = effective_pool_config_for_group(&group, Some(&routing_policy))
            .expect("effective pool config should parse");

        assert!(effective_config.lru_enabled);
        assert_eq!(
            pool_key_candidate_order_for_group(&group, Some(&effective_config)),
            StoredPoolKeyCandidateOrder::Lru
        );
    }

    /// 验证 Pool 未返回 Key 时会记录真实耗尽原因，供最终 429/503 分类使用。
    #[test]
    fn pool_key_cursor_records_runtime_miss_when_exhausted_without_returning_key() {
        let app = AppState::new().expect("state should build");
        let trace_id = "trace-pool-exhausted-runtime-miss";
        app.set_local_execution_runtime_miss_diagnostic(
            trace_id,
            LocalExecutionRuntimeMissDiagnostic {
                reason: "candidate_evaluation_incomplete".to_string(),
                requested_model: Some("gpt-5".to_string()),
                candidate_count: Some(1),
                ..LocalExecutionRuntimeMissDiagnostic::default()
            },
        );
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            Some(json!({ "pool_advanced": { "lru_enabled": true } })),
        );
        let mut cursor = PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None)
            .with_runtime_miss_diagnostic(trace_id, true);
        cursor.record_skip_reason("pool_cooldown");
        cursor.record_skip_reason("pool_cooldown");
        cursor.record_skip_reason("transport_snapshot_missing");

        cursor.log_exhausted();
        apply_local_runtime_candidate_terminal_reason(&app, trace_id, "no_local_sync_plans");

        let diagnostic = app
            .take_local_execution_runtime_miss_diagnostic(trace_id)
            .expect("runtime miss diagnostic should exist");
        assert_eq!(diagnostic.reason, "all_candidates_skipped");
        assert_eq!(diagnostic.skipped_candidate_count, Some(2));
        assert_eq!(diagnostic.skip_reasons.get("pool_cooldown"), Some(&1));
        assert_eq!(
            diagnostic.skip_reasons.get("transport_snapshot_missing"),
            Some(&1)
        );
    }

    #[tokio::test]
    async fn pool_key_cursor_exhausted_summary_uses_group_reason_and_suppresses_after_yield() {
        let app = AppState::new().expect("state should build");
        let trace_id = "trace-pool-exhausted-group-reason";
        app.set_local_execution_runtime_miss_diagnostic(
            trace_id,
            LocalExecutionRuntimeMissDiagnostic {
                reason: "candidate_evaluation_incomplete".to_string(),
                requested_model: Some("gpt-5".to_string()),
                candidate_count: Some(1),
                ..LocalExecutionRuntimeMissDiagnostic::default()
            },
        );
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            Some(json!({ "pool_advanced": {} })),
        );
        let mut cursor =
            PoolKeyCursor::new(PlannerAppState::new(&app), group.clone(), None, None, None)
                .with_runtime_miss_diagnostic(trace_id, true);
        cursor.record_skip_reason("pool_cooldown");
        cursor.record_skip_reason("key_model_disabled");
        cursor.record_skip_reason("key_model_disabled");

        let summary = cursor
            .exhausted_group_skipped_candidate()
            .expect("no-attempt group should emit summary");
        assert_eq!(summary.skip_reason, "pool_group_exhausted");
        assert_eq!(
            summary
                .extra_data
                .as_ref()
                .and_then(|value| value.get("pool_group_exhaustion"))
                .and_then(|value| value.get("skip_reason_counts")),
            Some(&json!({
                "key_model_disabled": 2,
                "pool_cooldown": 1,
            }))
        );

        cursor.log_exhausted();
        apply_local_runtime_candidate_terminal_reason(&app, trace_id, "no_local_sync_plans");
        let diagnostic = app
            .take_local_execution_runtime_miss_diagnostic(trace_id)
            .expect("runtime miss diagnostic should exist");
        assert_eq!(diagnostic.reason, "all_candidates_skipped");
        assert_eq!(diagnostic.skip_reasons.get("pool_cooldown"), Some(&1));
        assert_eq!(diagnostic.skip_reasons.get("key_model_disabled"), Some(&1));

        let mut yielded_cursor =
            PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None);
        yielded_cursor.queued_candidates = VecDeque::from([sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-a",
            10,
            Some(json!({ "pool_advanced": {} })),
        )]);
        yielded_cursor.record_skip_reason("key_model_disabled");
        assert!(yielded_cursor.next_key().await.is_some());
        assert!(yielded_cursor.exhausted_group_skipped_candidate().is_none());
    }

    #[tokio::test]
    async fn pool_key_cursor_rechecks_cooldown_for_frozen_window_candidates() {
        let app = AppState::new().expect("state should build");
        let provider_config = Some(json!({
            "pool_advanced": {
                "rate_limit_cooldown_seconds": 300
            }
        }));
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config.clone(),
        );
        let mut cursor =
            PoolKeyCursor::new(PlannerAppState::new(&app), group.clone(), None, None, None);
        cursor.queued_candidates = VecDeque::from([
            sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                "key-a",
                10,
                provider_config.clone(),
            ),
            sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                "key-b",
                10,
                provider_config.clone(),
            ),
            sample_eligible_candidate("provider-pool", "endpoint-1", "key-c", 10, provider_config),
        ]);

        let first = cursor.next_key().await.expect("first key should schedule");
        assert_eq!(first.candidate.key_id, "key-a");

        let pool_config = pool_config_for_candidate(&group).expect("pool config should parse");
        record_admin_provider_pool_error(
            app.runtime_state.as_ref(),
            "provider-pool",
            "key-b",
            &pool_config,
            429,
            None,
            None,
        )
        .await;

        let second = cursor
            .next_key()
            .await
            .expect("second key should skip the cooled-down frozen key");
        assert_eq!(second.candidate.key_id, "key-c");
        assert_eq!(second.orchestration.pool_key_index, Some(1));

        let skipped = cursor.take_skipped_candidates();
        assert_eq!(
            skipped
                .iter()
                .map(|item| (item.candidate.key_id.as_str(), item.skip_reason))
                .collect::<Vec<_>>(),
            vec![("key-b", "pool_cooldown")]
        );
    }

    #[tokio::test]
    async fn pool_key_cursor_filters_expanded_keys_by_routing_profile_allowed_keys() {
        let app = AppState::new().expect("state should build");
        let provider_config = Some(json!({ "pool_advanced": { "lru_enabled": true } }));
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config.clone(),
        );
        let routing_policy = routing_policy_with_allowed_keys(["key-b"]);
        let mut cursor = PoolKeyCursor::new_with_routing_policy(
            PlannerAppState::new(&app),
            group,
            None,
            None,
            None,
            Some(&routing_policy),
        );
        cursor.queued_candidates = VecDeque::from([
            sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                "key-a",
                10,
                provider_config.clone(),
            ),
            sample_eligible_candidate("provider-pool", "endpoint-1", "key-b", 10, provider_config),
        ]);

        let candidate = cursor
            .next_key()
            .await
            .expect("cursor should skip disallowed pool key and return allowed key");
        assert_eq!(candidate.candidate.key_id, "key-b");
        assert_eq!(candidate.orchestration.pool_key_index, Some(0));
        assert_eq!(
            candidate.orchestration.candidate_group_id.as_deref(),
            Some(
                "provider=provider-pool|endpoint=endpoint-1|model=model-1|selected_model=gpt-5|api_format=openai:chat|singleton_key=*"
            )
        );
        assert_eq!(
            cursor
                .skip_reason_counts
                .get(ROUTING_PROFILE_DISALLOWED_KEY_SKIP_REASON),
            Some(&1)
        );
        let skipped = cursor.take_skipped_candidates();
        assert_eq!(
            skipped
                .iter()
                .map(|item| (item.candidate.key_id.as_str(), item.skip_reason))
                .collect::<Vec<_>>(),
            vec![("key-a", ROUTING_PROFILE_DISALLOWED_KEY_SKIP_REASON)]
        );
    }

    #[tokio::test]
    async fn routing_allowed_key_outside_score_top_n_is_materialized_directly() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "score_top_n": 16,
                "score_fallback_scan_limit": 16
            }
        }));
        let (provider, endpoint, keys, rows) = large_pool_fixture(32, provider_config.clone());
        let scores = (0..16)
            .map(|index| {
                sample_provider_key_pool_score(
                    "provider-pool",
                    &format!("key-{index:05}"),
                    1_000.0 - index as f64,
                )
            })
            .collect::<Vec<_>>();
        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_pool_score_repository_for_tests(Arc::new(
                InMemoryPoolMemberScoreRepository::seed(scores),
            ))
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config,
        );
        let routing_policy = routing_policy_with_allowed_keys(["key-00031"]);
        let mut cursor = PoolKeyCursor::new_with_routing_policy(
            PlannerAppState::new(&app),
            group,
            None,
            None,
            None,
            Some(&routing_policy),
        );

        let candidate = cursor
            .next_key()
            .await
            .expect("the only routing-allowed key should not be lost to score preselection");

        assert_eq!(candidate.candidate.key_id, "key-00031");
        assert_eq!(cursor.scanned_keys, 1);
        assert_eq!(cursor.budget_scanned_keys, 1);
    }

    #[tokio::test]
    async fn routing_allowed_keys_continue_across_pool_pages() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "scheduling_presets": [
                    {"preset": "lru", "enabled": false}
                ],
                "score_top_n": 16,
                "score_fallback_scan_limit": 128
            }
        }));
        let (provider, endpoint, keys, rows) = large_pool_fixture(80, provider_config.clone());
        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config,
        );
        let mut routing_policy = routing_policy_with_allowed_keys([]);
        routing_policy.ranking_overlay.allowed_keys =
            (0..80).map(|index| format!("key-{index:05}")).collect();
        let mut cursor = PoolKeyCursor::new_with_routing_policy(
            PlannerAppState::new(&app),
            group,
            None,
            None,
            None,
            Some(&routing_policy),
        );

        let mut returned_key_ids = Vec::new();
        while let Some(candidate) = cursor.next_key().await {
            returned_key_ids.push(candidate.candidate.key_id);
        }

        assert_eq!(returned_key_ids.len(), 32);
        assert!(returned_key_ids.iter().any(|key_id| key_id == "key-00064"));
        assert!(returned_key_ids.iter().any(|key_id| key_id == "key-00079"));
        assert_eq!(cursor.scanned_keys, 80);
        assert_eq!(cursor.budget_scanned_keys, 80);
        assert_eq!(cursor.routing_allowed_key_offset, 80);
    }

    #[tokio::test]
    async fn routing_allowed_keys_preserve_internal_priority_without_active_presets() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "scheduling_presets": [
                    {"preset": "lru", "enabled": false}
                ],
                "score_top_n": 2,
                "score_fallback_scan_limit": 2
            }
        }));
        let (provider, endpoint, mut keys, mut rows) =
            large_pool_fixture(2, provider_config.clone());
        keys[0].internal_priority = 100;
        keys[1].internal_priority = 1;
        rows[0].key_internal_priority = 100;
        rows[1].key_internal_priority = 1;
        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config,
        );
        let routing_policy = routing_policy_with_allowed_keys(["key-00000", "key-00001"]);
        let mut cursor = PoolKeyCursor::new_with_routing_policy(
            PlannerAppState::new(&app),
            group,
            None,
            None,
            None,
            Some(&routing_policy),
        );

        let candidate = cursor
            .next_key()
            .await
            .expect("an allowed key should be schedulable");

        assert_eq!(candidate.candidate.key_id, "key-00001");
    }

    #[tokio::test]
    async fn pool_key_cursor_allows_parallel_requests_to_use_same_healthy_key() {
        let app = AppState::new().expect("state should build");
        let provider_config = Some(json!({ "pool_advanced": { "lru_enabled": true } }));
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config.clone(),
        );
        let mut cursor = PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None);
        cursor.queued_candidates = VecDeque::from([
            sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                "key-a",
                10,
                provider_config.clone(),
            ),
            sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                "key-b",
                10,
                provider_config.clone(),
            ),
        ]);

        let candidate = cursor
            .next_key()
            .await
            .expect("cursor should return the first healthy key");
        assert_eq!(candidate.candidate.key_id, "key-a");
        assert_eq!(candidate.orchestration.pool_key_index, Some(0));
        assert!(candidate.orchestration.pool_key_lease.is_none());
        assert!(!cursor
            .skip_reason_counts
            .contains_key("pool_key_lease_busy"));

        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config,
        );
        let mut second_cursor =
            PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None);
        second_cursor.queued_candidates = VecDeque::from([
            sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                "key-a",
                10,
                Some(json!({ "pool_advanced": { "lru_enabled": true } })),
            ),
            sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                "key-b",
                10,
                Some(json!({ "pool_advanced": { "lru_enabled": true } })),
            ),
        ]);
        let second_candidate = second_cursor
            .next_key()
            .await
            .expect("second request should also be allowed to pick the same healthy key");
        assert_eq!(second_candidate.candidate.key_id, "key-a");
        assert!(second_candidate.orchestration.pool_key_lease.is_none());
    }

    #[tokio::test]
    async fn pool_key_cursor_skips_key_after_account_cooldown_is_recorded() {
        let app = AppState::new().expect("state should build");
        let provider_config = Some(json!({ "pool_advanced": { "lru_enabled": true } }));
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config.clone(),
        );
        let pool_config = pool_config_for_candidate(&group).expect("pool config should parse");
        let mut cursor = PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None);
        cursor.queued_candidates = VecDeque::from([
            sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                "key-a",
                10,
                provider_config.clone(),
            ),
            sample_eligible_candidate("provider-pool", "endpoint-1", "key-b", 10, provider_config),
        ]);

        record_admin_provider_pool_error(
            app.runtime_state.as_ref(),
            "provider-pool",
            "key-a",
            &pool_config,
            429,
            None,
            None,
        )
        .await;

        let candidate = cursor
            .next_key()
            .await
            .expect("cursor should skip cooled-down key and return next key");
        assert_eq!(candidate.candidate.key_id, "key-b");
        assert_eq!(candidate.orchestration.pool_key_index, Some(0));
        assert!(candidate.orchestration.pool_key_lease.is_none());
        assert_eq!(cursor.skip_reason_counts.get("pool_cooldown"), Some(&1));
    }

    #[tokio::test]
    async fn pool_key_cursor_skips_low_balance_sticky_key_and_falls_back_once() {
        // 验证 sticky Key 复用共享过滤，留下证据后继续普通分页且不重复占用预算。
        let provider_config = Some(json!({
            "pool_advanced": {
                "scheduling_presets": [
                    {"preset": "cache_affinity", "enabled": true}
                ],
                "score_top_n": 2,
                "score_fallback_scan_limit": 2
            }
        }));
        let (provider, endpoint, mut keys, rows) = large_pool_fixture(2, provider_config.clone());
        keys[0].status_snapshot = Some(json!({
            "quota": {
                "schema_version": 1,
                "provider_type": "openai",
                "kind": "balance",
                "freshness": "fresh",
                "balances": [{"unit": "USD", "available": "1"}]
            }
        }));
        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config,
        );
        let pool_config = pool_config_for_candidate(&group).expect("pool config should parse");
        record_admin_provider_pool_success(
            app.runtime_state.as_ref(),
            "provider-pool",
            "key-00000",
            &pool_config,
            Some("session-1"),
            0,
            None,
        )
        .await;
        let mut cursor = PoolKeyCursor::new(
            PlannerAppState::new(&app),
            group,
            Some("session-1"),
            None,
            None,
        );

        let candidate = cursor
            .next_key()
            .await
            .expect("cursor should fall back to the active balance key");

        assert_eq!(candidate.candidate.key_id, "key-00001");
        assert_eq!(candidate.orchestration.pool_key_index, Some(0));
        assert!(cursor.seen_key_ids.contains("key-00000"));
        assert_eq!(cursor.budget_scanned_keys, 1);
        assert_eq!(
            cursor
                .skip_reason_counts
                .get(POOL_BALANCE_BELOW_MINIMUM_SKIP_REASON),
            Some(&1)
        );
        assert_eq!(
            cursor.take_skipped_candidates()[0].skip_reason,
            POOL_BALANCE_BELOW_MINIMUM_SKIP_REASON
        );
    }

    #[test]
    fn pool_runtime_guards_preserve_hot_pool_fallback_and_quota_reasons() {
        // 热成员仅因请求级限制不可用时，沿用既有冷池回退且不误写持久化额度原因。
        let candidates = ["key-hot", "key-cold"].map(|key_id| {
            sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                key_id,
                10,
                Some(json!({"pool_advanced": {"probing_enabled": true}})),
            )
        });
        let runtime = BTreeMap::from([(
            "provider-pool".to_string(),
            AdminProviderPoolRuntimeState {
                active_probe_member_ids: BTreeSet::from(["key-hot".to_string()]),
                provider_desired_hot: 1,
                ..Default::default()
            },
        )]);
        let mut reasons = BTreeMap::from([("key-hot".to_string(), "key_rpm_exhausted")]);
        let outcome = apply_local_execution_pool_scheduler_with_runtime_map_outcome_and_configs(
            candidates.to_vec(),
            &runtime,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &reasons,
            &PoolModelRuntimeSkipReasons::new(),
        );
        assert_eq!(outcome.candidates.len(), 1);
        assert_eq!(outcome.candidates[0].candidate.key_id, "key-cold");
        assert_eq!(outcome.skipped[0].skip_reason, "key_rpm_exhausted");
        assert!(outcome
            .active_probe_seal_fallback_provider_ids
            .contains("provider-pool"));
        assert!(outcome.active_probe_evicted_members_by_provider.is_empty());

        reasons.insert("key-cold".to_string(), "key_health_score_zero");
        let outcome = apply_local_execution_pool_scheduler_with_runtime_map_outcome_and_configs(
            candidates.to_vec(),
            &runtime,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &reasons,
            &PoolModelRuntimeSkipReasons::new(),
        );
        assert!(outcome.candidates.is_empty());
        assert_eq!(outcome.skipped.len(), 2);
        let contexts = BTreeMap::from([
            (
                "key-hot".to_string(),
                PoolCatalogKeyContext {
                    runtime_quota_hard_blocked: true,
                    ..Default::default()
                },
            ),
            (
                "key-cold".to_string(),
                PoolCatalogKeyContext {
                    catalog_state_unavailable: true,
                    ..Default::default()
                },
            ),
        ]);
        let outcome = apply_local_execution_pool_scheduler_with_runtime_map_outcome_and_configs(
            candidates.to_vec(),
            &runtime,
            &contexts,
            &BTreeMap::new(),
            &reasons,
            &PoolModelRuntimeSkipReasons::new(),
        );
        assert!(outcome.candidates.is_empty());
        assert!(outcome
            .skipped
            .iter()
            .any(|item| item.candidate.key_id == "key-hot"
                && item.skip_reason == "pool_key_quota_exhausted"));
        assert!(outcome
            .skipped
            .iter()
            .any(|item| item.candidate.key_id == "key-cold"
                && item.skip_reason == "pool_key_state_unavailable"));
    }

    fn antigravity_pool_candidate(key_id: &str, model: &str) -> EligibleLocalExecutionCandidate {
        let mut candidate =
            sample_eligible_candidate("provider-antigravity", "endpoint-gemini", key_id, 10, None);
        candidate.candidate.provider_type = "antigravity".to_string();
        candidate.candidate.endpoint_api_format = "gemini:generate_content".to_string();
        candidate.candidate.global_model_name = model.to_string();
        candidate.candidate.selected_provider_model_name = model.to_string();
        candidate.provider_api_format = "gemini:generate_content".to_string();
        let transport = Arc::make_mut(&mut candidate.transport);
        transport.provider.provider_type = "antigravity".to_string();
        transport.endpoint.api_format = "gemini:generate_content".to_string();
        transport.endpoint.api_family = Some("gemini".to_string());
        transport.endpoint.endpoint_kind = Some("generate_content".to_string());
        transport.key.auth_type = "oauth".to_string();
        candidate
    }

    fn runtime_quota_facts(
        blocked: &[(&str, &str)],
        unavailable: &[(&str, &str)],
    ) -> PoolRuntimeModelQuotaFacts {
        let mut facts = PoolRuntimeModelQuotaFacts::default();
        for (key_id, model) in blocked {
            facts
                .blocked
                .entry((*key_id).to_string())
                .or_default()
                .insert((*model).to_string());
        }
        for (key_id, model) in unavailable {
            facts
                .state_unavailable
                .entry((*key_id).to_string())
                .or_default()
                .insert((*model).to_string());
        }
        facts
    }

    #[test]
    fn pool_runtime_model_quota_scopes_skip_to_actual_upstream_model() {
        // 同一 Key 的实际上游模型互为独立额度桶；另一个账号的同名模型也不得被牵连。
        let gemini = antigravity_pool_candidate("key-a", "gemini-3-pro");
        let claude = antigravity_pool_candidate("key-a", "claude-sonnet-4-5");
        let other_account_gemini = antigravity_pool_candidate("key-b", "gemini-3-pro");
        let facts = runtime_quota_facts(&[("key-a", "gemini-3-pro")], &[]);
        let reasons =
            pool_model_runtime_skip_reasons(&[gemini, claude, other_account_gemini], &facts);

        assert_eq!(
            reasons
                .get("key-a")
                .and_then(|models| models.get("gemini-3-pro")),
            Some(&POOL_ACCOUNT_EXHAUSTED_SKIP_REASON)
        );
        assert!(!reasons
            .get("key-a")
            .is_some_and(|models| models.contains_key("claude-sonnet-4-5")));
        assert!(!reasons.contains_key("key-b"));
        assert_eq!(reasons.len(), 1);
    }

    #[test]
    fn pool_runtime_model_quota_read_failure_is_state_unavailable_for_scoped_model() {
        let gemini = antigravity_pool_candidate("key-a", "gemini-3-pro");
        let claude = antigravity_pool_candidate("key-a", "claude-sonnet-4-5");
        let codex = sample_eligible_candidate("provider-pool", "endpoint-1", "key-a", 10, None);
        let key = StoredProviderCatalogKey::new(
            "key-a".to_string(),
            "provider-antigravity".to_string(),
            "key-a".to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("key should build");
        let keys_by_id = BTreeMap::from([("key-a".to_string(), key)]);

        // 非 Antigravity 候选不构建身份，也不触发额外后端读取。
        assert!(pool_model_runtime_quota_identities(&[codex], &keys_by_id).is_empty());

        let scoped_candidates = [gemini.clone(), claude.clone()];
        let identities = pool_model_runtime_quota_identities(&scoped_candidates, &keys_by_id);
        assert_eq!(identities.len(), 2);
        let failed = PoolRuntimeModelQuotaFacts::from_read(&identities, None);
        assert!(failed.blocked.is_empty());
        assert_eq!(failed.state_unavailable.len(), 1);
        assert_eq!(
            failed
                .state_unavailable
                .get("key-a")
                .map(|models| models.len()),
            Some(2)
        );
        // 批量结果长度不一致同样只能视为状态不可用。
        let mismatched = PoolRuntimeModelQuotaFacts::from_read(&identities, Some(vec![true]));
        assert!(mismatched.blocked.is_empty());
        assert_eq!(mismatched.state_unavailable, failed.state_unavailable);

        let facts = runtime_quota_facts(&[], &[("key-a", "gemini-3-pro")]);
        let reasons = pool_model_runtime_skip_reasons(&[gemini, claude], &facts);
        assert_eq!(
            reasons
                .get("key-a")
                .and_then(|models| models.get("gemini-3-pro")),
            Some(&POOL_KEY_STATE_UNAVAILABLE_SKIP_REASON)
        );
        assert!(!reasons
            .get("key-a")
            .is_some_and(|models| models.contains_key("claude-sonnet-4-5")));
    }

    #[test]
    fn pool_runtime_model_quota_filters_before_window_truncation_without_shared_account_limit() {
        let blocked = antigravity_pool_candidate("key-a", "gemini-3-pro");
        let sibling_model = antigravity_pool_candidate("key-a", "claude-sonnet-4-5");
        let sibling_account = antigravity_pool_candidate("key-b", "gemini-3-pro");
        let facts = runtime_quota_facts(&[("key-a", "gemini-3-pro")], &[]);
        let model_reasons = pool_model_runtime_skip_reasons(
            &[blocked.clone(), sibling_model.clone(), sibling_account],
            &facts,
        );
        let outcome = apply_local_execution_pool_scheduler_with_runtime_map_outcome_and_configs(
            vec![blocked, sibling_model],
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &model_reasons,
        );

        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].candidate.key_id, "key-a");
        assert_eq!(
            outcome.skipped[0].candidate.selected_provider_model_name,
            "gemini-3-pro"
        );
        assert_eq!(
            outcome.skipped[0].skip_reason,
            POOL_ACCOUNT_EXHAUSTED_SKIP_REASON
        );
        assert_eq!(outcome.candidates.len(), 1);
        assert_eq!(outcome.candidates[0].candidate.key_id, "key-a");
        assert_eq!(
            outcome.candidates[0].candidate.selected_provider_model_name,
            "claude-sonnet-4-5"
        );
        // 与目录额度耗尽同一诊断，不占用游标扫描预算，因此可以继续扫描窗口之外。
        assert!(pool_skip_reason_releases_scan_budget(
            POOL_ACCOUNT_EXHAUSTED_SKIP_REASON
        ));
    }

    #[tokio::test]
    async fn pool_key_cursor_skips_runtime_model_quota_beyond_window_and_keeps_sibling_model() {
        use aether_data::repository::candidates::InMemoryRequestCandidateRepository;
        use aether_data::repository::quota::InMemoryProviderQuotaRepository;
        use aether_data_contracts::repository::provider_catalog::ProviderCatalogReadRepository;

        const TEST_AUTH_CONFIG: &str = r#"{"refresh_token":"fixture-runtime-quota-refresh"}"#;

        let provider_config = Some(json!({
            "pool_advanced": {
                "scheduling_presets": [
                    {"preset": "cache_affinity", "enabled": true},
                    {"preset": "priority_first", "enabled": true}
                ],
                "score_top_n": 64,
                "score_fallback_scan_limit": 64
            }
        }));
        let (mut provider, mut endpoint, mut keys, mut rows) =
            large_pool_fixture(3, provider_config.clone());
        provider.provider_type = "antigravity".to_string();
        endpoint.api_format = "gemini:generate_content".to_string();
        endpoint.api_family = Some("gemini".to_string());
        endpoint.endpoint_kind = Some("generate_content".to_string());
        endpoint.base_url = "https://antigravity.googleapis.com".to_string();

        let allowed_models = vec!["gemini-3-pro".to_string(), "claude-sonnet-4-5".to_string()];
        // 事实身份绑定解密后的凭据；给首个 Key 封装同一份 auth config，
        // 使读取端解密后与写入端的 generation 一致。
        let sealer = AppState::new()
            .expect("sealer state should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(Arc::new(
                    InMemoryProviderCatalogReadRepository::seed(vec![], vec![], vec![]),
                ))
                .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY),
            );
        let sealed_auth_config = sealer
            .seal_provider_catalog_key_auth_config("provider-pool", "key-00000", TEST_AUTH_CONFIG)
            .expect("auth config should seal");
        for (index, key) in keys.iter_mut().enumerate() {
            key.auth_type = "oauth".to_string();
            key.api_formats = Some(json!(["gemini:generate_content"]));
            key.allowed_models = Some(json!(allowed_models));
            key.expires_at_unix_secs = Some(4_102_444_800);
            key.internal_priority = if index == 0 { 0 } else { 10 };
            if index == 0 {
                key.encrypted_auth_config = Some(sealed_auth_config.clone());
            }
        }
        for (index, row) in rows.iter_mut().enumerate() {
            row.provider_type = "antigravity".to_string();
            row.endpoint_api_format = "gemini:generate_content".to_string();
            row.endpoint_api_family = Some("gemini".to_string());
            row.endpoint_kind = Some("generate_content".to_string());
            row.key_auth_type = "oauth".to_string();
            row.key_api_formats = Some(vec!["gemini:generate_content".to_string()]);
            row.key_allowed_models = Some(allowed_models.clone());
            row.key_internal_priority = if index == 0 { 0 } else { 10 };
            row.routing_facts.provider_pool_enabled = true;
            row.global_model_name = "gemini-3-pro".to_string();
            row.model_provider_model_name = "gemini-3-pro".to_string();
        }
        let sibling_rows = rows
            .iter()
            .cloned()
            .map(|mut row| {
                row.model_id = "model-sibling".to_string();
                row.global_model_id = "global-model-sibling".to_string();
                row.global_model_name = "claude-sonnet-4-5".to_string();
                row.model_provider_model_name = "claude-sonnet-4-5".to_string();
                row
            })
            .collect::<Vec<_>>();
        let blocked_key = keys[0].clone();
        let mut all_rows = rows;
        all_rows.extend(sibling_rows);

        let catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
            vec![provider],
            vec![endpoint],
            keys,
        ));
        let catalog_reader: Arc<dyn ProviderCatalogReadRepository> = catalog_repository.clone();
        let data_state =
            GatewayDataState::with_candidate_selection_provider_catalog_quota_and_request_candidates_for_tests(
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(all_rows)),
                catalog_reader,
                Arc::new(InMemoryProviderQuotaRepository::seed(Vec::new())),
                Arc::new(InMemoryRequestCandidateRepository::seed(Vec::new())),
            )
            .attach_provider_catalog_repository_for_tests(catalog_repository)
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);

        let now_unix_secs = crate::clock::current_unix_secs();
        crate::orchestration::record_antigravity_model_quota_exhaustion_with_auth_config(
            app.runtime_state.as_ref(),
            &blocked_key,
            Some(TEST_AUTH_CONFIG),
            "antigravity",
            "gemini-3-pro",
            now_unix_secs + 3_600,
            now_unix_secs,
            "test",
        )
        .await
        .expect("runtime model quota fact should record");

        let mut group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-00000",
            0,
            provider_config.clone(),
        );
        group.kind = LocalExecutionCandidateKind::PoolGroup;
        group.candidate.provider_type = "antigravity".to_string();
        group.candidate.endpoint_api_format = "gemini:generate_content".to_string();
        group.candidate.global_model_name = "gemini-3-pro".to_string();
        group.candidate.selected_provider_model_name = "gemini-3-pro".to_string();
        group.provider_api_format = "gemini:generate_content".to_string();
        let transport = Arc::make_mut(&mut group.transport);
        transport.provider.provider_type = "antigravity".to_string();
        transport.endpoint.api_format = "gemini:generate_content".to_string();
        transport.endpoint.api_family = Some("gemini".to_string());
        transport.endpoint.endpoint_kind = Some("generate_content".to_string());
        transport.endpoint.base_url = "https://antigravity.googleapis.com".to_string();
        transport.key.auth_type = "oauth".to_string();
        transport.key.api_formats = Some(vec!["gemini:generate_content".to_string()]);

        let mut sibling_group = group.clone();
        sibling_group.candidate.model_id = "model-sibling".to_string();
        sibling_group.candidate.global_model_id = "global-model-sibling".to_string();
        sibling_group.candidate.global_model_name = "claude-sonnet-4-5".to_string();
        sibling_group.candidate.selected_provider_model_name = "claude-sonnet-4-5".to_string();

        let pool_config = pool_config_for_candidate(&group).expect("pool config should parse");
        // 普通分页与 sticky 单例都必须越过被模型额度限制的窗口继续扫描。
        for session in [None, Some("session-runtime-quota")] {
            if let Some(session) = session {
                record_admin_provider_pool_success(
                    app.runtime_state.as_ref(),
                    "provider-pool",
                    "key-00000",
                    &pool_config,
                    Some(session),
                    0,
                    None,
                )
                .await;
            }
            let mut cursor = PoolKeyCursor::new(
                PlannerAppState::new(&app),
                group.clone(),
                session,
                Some("gemini-3-pro"),
                None,
            );
            cursor.window_size = 1;
            cursor.page_size = 1;
            let candidate = cursor
                .next_key()
                .await
                .expect("another account must serve after the scoped quota skip");
            assert_eq!(candidate.candidate.key_id, "key-00001");
            let rejected = cursor.take_skipped_candidates();
            assert!(rejected
                .iter()
                .any(|item| item.candidate.key_id == "key-00000"
                    && item.skip_reason == POOL_ACCOUNT_EXHAUSTED_SKIP_REASON));
            assert!(rejected
                .iter()
                .all(|item| item.skip_reason != "pool_key_quota_exhausted"));
        }

        // 同账号另一模型不继承共享账号限额，直接在该账号上调度。
        let mut sibling_cursor = PoolKeyCursor::new(
            PlannerAppState::new(&app),
            sibling_group,
            None,
            Some("claude-sonnet-4-5"),
            None,
        );
        let sibling = sibling_cursor
            .next_key()
            .await
            .expect("sibling model must remain schedulable on the same account");
        assert_eq!(sibling.candidate.key_id, "key-00000");
    }

    #[tokio::test]
    async fn pool_key_cursor_filters_real_key_runtime_guards_before_window_truncation() {
        use crate::scheduler::candidate::{
            list_selectable_candidates_with_skip_reasons, CandidateSchedulingContext,
        };
        use crate::scheduler::config::SchedulerOrderingConfig;
        use aether_data::repository::candidates::InMemoryRequestCandidateRepository;
        use aether_data::repository::quota::InMemoryProviderQuotaRepository;
        use aether_data_contracts::repository::candidates::{
            RequestCandidateStatus, StoredRequestCandidate,
        };

        // 唯一可用 Key 位于冻结窗口之后；代表与 sticky Key 均受限时仍须先过滤再截断。
        for (health, concurrent_limit, rpm_limit, reason) in [
            (0.0, None, None, "key_health_score_zero"),
            (1.0, Some(1), None, "provider_key_concurrency_limit_reached"),
            (1.0, None, Some(1), "key_rpm_exhausted"),
        ] {
            let provider_config = Some(json!({
                "pool_advanced": {
                    "scheduling_presets": [{"preset": "cache_affinity", "enabled": true}],
                    "score_top_n": 64,
                    "score_fallback_scan_limit": 64
                }
            }));
            let (provider, endpoint, mut keys, rows) =
                large_pool_fixture(18, provider_config.clone());
            let representative_rows =
                GatewayDataState::with_minimal_candidate_selection_reader_for_tests(Arc::new(
                    InMemoryMinimalCandidateSelectionReadRepository::seed(vec![rows[0].clone()]),
                ));
            let now_unix_secs = crate::clock::current_unix_secs();
            let observed_at_ms = (now_unix_secs.saturating_sub(1) * 1000) as i64;
            let mut recent_requests = Vec::new();
            for key in keys.iter_mut().take(17) {
                key.health_by_format = Some(json!({"openai:chat": {"health_score": health}}));
                key.concurrent_limit = concurrent_limit;
                key.rpm_limit = rpm_limit;
                recent_requests.push(
                    StoredRequestCandidate::new(
                        format!("candidate-{}", key.id),
                        format!("request-{}", key.id),
                        None,
                        None,
                        None,
                        None,
                        0,
                        0,
                        Some(key.provider_id.clone()),
                        Some("endpoint-1".to_string()),
                        Some(key.id.clone()),
                        RequestCandidateStatus::Streaming,
                        None,
                        false,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        observed_at_ms,
                        Some(observed_at_ms),
                        None,
                    )
                    .expect("active request should build"),
                );
            }
            let data_state = GatewayDataState::with_candidate_selection_provider_catalog_quota_and_request_candidates_for_tests(
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
                Arc::new(InMemoryProviderCatalogReadRepository::seed(vec![provider], vec![endpoint], keys)),
                Arc::new(InMemoryProviderQuotaRepository::seed(Vec::new())),
                Arc::new(InMemoryRequestCandidateRepository::seed(recent_requests)),
            ).with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
            let app = AppState::new()
                .expect("state should build")
                .with_data_state_for_tests(data_state);
            let (selected, skipped) = list_selectable_candidates_with_skip_reasons(
                &representative_rows,
                &app,
                "openai:chat",
                "gpt-5",
                false,
                None,
                None,
                None,
                CandidateSchedulingContext {
                    now_unix_secs,
                    load_balance_seed: 17,
                },
                false,
                SchedulerOrderingConfig::default(),
            )
            .await
            .expect("representative selection should succeed");
            assert_eq!(
                selected.len(),
                1,
                "representative must not block siblings: {reason}"
            );
            assert!(skipped.is_empty());
            let mut group = sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                "key-00000",
                0,
                provider_config,
            );
            group.kind = LocalExecutionCandidateKind::PoolGroup;
            group.candidate = selected[0].clone();
            let pool_config = pool_config_for_candidate(&group).expect("pool config should parse");
            record_admin_provider_pool_success(
                app.runtime_state.as_ref(),
                "provider-pool",
                "key-00000",
                &pool_config,
                Some("session-runtime-guards"),
                0,
                None,
            )
            .await;

            // 普通分页与 sticky singleton 都必须拒绝真实受限 Key，且不会重新返回已拒绝项。
            for session in [None, Some("session-runtime-guards")] {
                let mut cursor = PoolKeyCursor::new(
                    PlannerAppState::new(&app),
                    group.clone(),
                    session,
                    None,
                    None,
                );
                let candidate = cursor
                    .next_key()
                    .await
                    .expect("healthy sibling should survive runtime filtering");
                assert_eq!(candidate.candidate.key_id, "key-00017", "{reason}");
                assert_eq!(candidate.orchestration.pool_key_index, Some(0));
                assert!(cursor.next_key().await.is_none());
                let rejected = cursor.take_skipped_candidates();
                assert_eq!(rejected.len(), 17, "{reason}");
                assert!(rejected.iter().all(|item| item.skip_reason == reason));
                assert!(rejected
                    .iter()
                    .any(|item| item.candidate.key_id == "key-00000"));
            }

            // 真实 RPM 重置后旧窗口不再阻断，仍复用同一共享检查。
            if rpm_limit.is_some() {
                app.mark_provider_key_rpm_reset("key-00000", now_unix_secs);
                let mut cursor = PoolKeyCursor::new(
                    PlannerAppState::new(&app),
                    group,
                    Some("session-runtime-guards"),
                    None,
                    None,
                );
                assert_eq!(
                    cursor
                        .next_key()
                        .await
                        .expect("reset Key should recover")
                        .candidate
                        .key_id,
                    "key-00000"
                );
            }
        }
    }

    #[tokio::test]
    async fn pool_key_cursor_filters_antigravity_undiscovered_model_before_window_truncation() {
        // 事故形态：代表账号发现映射只有无关占位模型，仍需继续扫描精确支持的兄弟账号。
        let provider_config = Some(json!({
            "pool_advanced": {
                "scheduling_presets": [{"preset": "cache_affinity", "enabled": true}],
                "score_top_n": 8,
                "score_fallback_scan_limit": 8
            }
        }));
        let (mut provider, mut endpoint, mut keys, mut rows) =
            large_pool_fixture(2, provider_config.clone());
        provider.provider_type = "antigravity".to_string();
        endpoint.api_format = "gemini:generate_content".to_string();
        endpoint.api_family = Some("gemini".to_string());
        endpoint.endpoint_kind = Some("generate_content".to_string());
        endpoint.base_url = "https://antigravity.googleapis.com".to_string();

        let discovered_model = "gemini-4-argon-high";
        for key in keys.iter_mut() {
            key.auth_type = "oauth".to_string();
            key.api_formats = Some(json!(["gemini:generate_content"]));
            key.auto_fetch_models = false;
            key.allowed_models = None;
        }
        keys[0].upstream_metadata = Some(json!({
            "antigravity": {
                "quota_by_model": {
                    "gemini-4-argon-disabled": {}
                }
            }
        }));
        keys[1].upstream_metadata = Some(json!({
            "antigravity": {
                "quota_by_model": {
                    "gemini-4-argon-high": {}
                }
            }
        }));

        for row in rows.iter_mut() {
            row.provider_type = "antigravity".to_string();
            row.endpoint_api_format = "gemini:generate_content".to_string();
            row.endpoint_api_family = Some("gemini".to_string());
            row.endpoint_kind = Some("generate_content".to_string());
            row.key_auth_type = "oauth".to_string();
            row.key_api_formats = Some(vec!["gemini:generate_content".to_string()]);
            row.key_allowed_models = None;
            row.routing_facts.provider_pool_enabled = true;
            row.global_model_name = discovered_model.to_string();
            row.model_provider_model_name = discovered_model.to_string();
        }

        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);

        let mut group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "key-00000",
            0,
            provider_config.clone(),
        );
        group.kind = LocalExecutionCandidateKind::PoolGroup;
        group.provider_api_format = "gemini:generate_content".to_string();
        group.candidate.provider_type = "antigravity".to_string();
        group.candidate.endpoint_api_format = "gemini:generate_content".to_string();
        group.candidate.global_model_name = discovered_model.to_string();
        group.candidate.selected_provider_model_name = discovered_model.to_string();
        let transport = Arc::make_mut(&mut group.transport);
        transport.provider.provider_type = "antigravity".to_string();
        transport.endpoint.api_format = "gemini:generate_content".to_string();
        transport.endpoint.api_family = Some("gemini".to_string());
        transport.endpoint.endpoint_kind = Some("generate_content".to_string());
        transport.endpoint.base_url = "https://antigravity.googleapis.com".to_string();
        transport.key.auth_type = "oauth".to_string();
        transport.key.api_formats = Some(vec!["gemini:generate_content".to_string()]);

        let pool_config = pool_config_for_candidate(&group).expect("pool config should parse");
        record_admin_provider_pool_success(
            app.runtime_state.as_ref(),
            "provider-pool",
            "key-00000",
            &pool_config,
            Some("session-model-admission"),
            0,
            None,
        )
        .await;

        // 普通分页与 sticky 单例都必须拒绝未发现模型的真实账号。
        for session in [None, Some("session-model-admission")] {
            let mut cursor = PoolKeyCursor::new(
                PlannerAppState::new(&app),
                group.clone(),
                session,
                None,
                None,
            );
            let candidate = cursor
                .next_key()
                .await
                .expect("supported sibling should survive discovered-model filtering");
            assert_eq!(candidate.candidate.key_id, "key-00001");
            assert!(cursor.take_skipped_candidates().iter().any(|item| {
                item.candidate.key_id == "key-00000" && item.skip_reason == "key_model_disabled"
            }));
        }
    }

    #[tokio::test]
    async fn pool_key_cursor_antigravity_quota_refresh_recovers() {
        use crate::handlers::shared::sync_provider_key_quota_status_snapshot;
        use crate::scheduler::candidate::{
            list_selectable_candidates_with_skip_reasons, CandidateSchedulingContext,
        };
        use crate::scheduler::config::SchedulerOrderingConfig;
        use aether_admin::provider::quota::parse_antigravity_usage_response;
        use aether_data::repository::candidates::InMemoryRequestCandidateRepository;
        use aether_data::repository::quota::InMemoryProviderQuotaRepository;
        use aether_data_contracts::repository::provider_catalog::{
            ProviderCatalogKeyRuntimeMetadataUpdate, ProviderCatalogReadRepository,
        };
        use aether_pool_core::POOL_KEY_QUOTA_EXHAUSTED_SKIP_REASON;
        use aether_provider_pool::provider_pool_key_runtime_quota_blocked;

        fn build_antigravity_quota_state(
            sonnet_45_fraction: f64,
            updated_at_unix_secs: u64,
            reset_at_unix_secs: u64,
            existing_snapshot: Option<&serde_json::Value>,
        ) -> (serde_json::Value, serde_json::Value) {
            let antigravity_metadata = parse_antigravity_usage_response(
                &json!({
                    "models": {
                        "claude-sonnet-4-5": {
                            "displayName": "Claude Sonnet 4.5",
                            "quotaInfo": {
                                "remainingFraction": sonnet_45_fraction,
                                "resetTime": reset_at_unix_secs
                            }
                        },
                        "claude-sonnet-4-5-thinking": {
                            "displayName": "Claude Sonnet 4.5 Thinking",
                            "quotaInfo": {
                                "remainingFraction": 0.8,
                                "resetTime": reset_at_unix_secs
                            }
                        },
                        "claude-sonnet-4-6": {
                            "displayName": "Claude Sonnet 4.6",
                            "quotaInfo": {
                                "remainingFraction": 0.8,
                                "resetTime": reset_at_unix_secs
                            }
                        }
                    }
                }),
                updated_at_unix_secs,
            )
            .expect("antigravity usage response should parse");
            let upstream_metadata = json!({ "antigravity": antigravity_metadata.clone() });
            let status_snapshot = sync_provider_key_quota_status_snapshot(
                existing_snapshot,
                "antigravity",
                Some(&upstream_metadata),
                "refresh_api",
            )
            .expect("antigravity quota status snapshot should materialize");
            (antigravity_metadata, status_snapshot)
        }

        async fn refresh_antigravity_key_quota(
            app: &AppState,
            key_id: &str,
            sonnet_45_fraction: f64,
            updated_at_unix_secs: u64,
            reset_at_unix_secs: u64,
        ) {
            let key_id_owned = key_id.to_string();
            let existing = app
                .list_provider_catalog_keys_by_ids_strong(std::slice::from_ref(&key_id_owned))
                .await
                .expect("strong key read should succeed")
                .into_iter()
                .next()
                .expect("catalog key should exist");
            let expected_antigravity = existing
                .upstream_metadata
                .as_ref()
                .and_then(|value| value.get("antigravity"))
                .cloned();
            let (antigravity_metadata, status_snapshot) = build_antigravity_quota_state(
                sonnet_45_fraction,
                updated_at_unix_secs,
                reset_at_unix_secs,
                existing.status_snapshot.as_ref(),
            );
            let quota_value = status_snapshot
                .get("quota")
                .cloned()
                .expect("materialized quota field should exist");
            let update = ProviderCatalogKeyRuntimeMetadataUpdate {
                key_id: key_id_owned,
                namespace: "antigravity".to_string(),
                expected_upstream_metadata_value: expected_antigravity,
                upstream_metadata_value: antigravity_metadata,
                status_snapshot_patch: json!({ "quota": quota_value }),
                updated_at_unix_secs: Some(updated_at_unix_secs),
            };
            let updated = app
                .update_provider_catalog_key_runtime_metadata(&update)
                .await
                .expect("runtime metadata update should succeed");
            assert!(updated);
            app.invalidate_provider_quota_candidate_caches();
        }

        for ignore_exhausted_accounts in [false, true] {
            let provider_config = Some(json!({
                "pool_advanced": {
                    "ignore_exhausted_accounts": ignore_exhausted_accounts,
                    "scheduling_presets": [
                        {"preset": "cache_affinity", "enabled": true},
                        {"preset": "priority_first", "enabled": true}
                    ],
                    "score_top_n": 64,
                    "score_fallback_scan_limit": 64
                }
            }));
            let (mut provider, mut endpoint, mut keys, mut rows) =
                large_pool_fixture(18, provider_config.clone());
            provider.provider_type = "antigravity".to_string();
            endpoint.api_format = "gemini:generate_content".to_string();
            endpoint.api_family = Some("gemini".to_string());
            endpoint.endpoint_kind = Some("generate_content".to_string());
            endpoint.base_url = "https://antigravity.googleapis.com".to_string();

            let now_unix_secs = crate::clock::current_unix_secs();
            let initial_updated_at = now_unix_secs.saturating_sub(30);
            let future_reset_at = now_unix_secs + 3_600;
            let allowed_models = vec![
                "claude-sonnet-4-5".to_string(),
                "claude-sonnet-4-5-thinking".to_string(),
                "claude-sonnet-4-6".to_string(),
            ];

            for (index, key) in keys.iter_mut().enumerate() {
                key.auth_type = "oauth".to_string();
                key.api_formats = Some(json!(["gemini:generate_content"]));
                key.allowed_models = Some(json!(allowed_models));
                key.expires_at_unix_secs = Some(4_102_444_800);
                key.internal_priority = if index == 0 { 0 } else { 10 };
                let fraction = if index < 17 { 0.0 } else { 0.8 };
                let (antigravity_metadata, status_snapshot) = build_antigravity_quota_state(
                    fraction,
                    initial_updated_at,
                    future_reset_at,
                    None,
                );
                key.upstream_metadata = Some(json!({ "antigravity": antigravity_metadata }));
                key.status_snapshot = Some(status_snapshot);
                assert!(!provider_pool_key_runtime_quota_blocked(key));
            }

            for (index, row) in rows.iter_mut().enumerate() {
                row.provider_type = "antigravity".to_string();
                row.endpoint_api_format = "gemini:generate_content".to_string();
                row.endpoint_api_family = Some("gemini".to_string());
                row.endpoint_kind = Some("generate_content".to_string());
                row.key_auth_type = "oauth".to_string();
                row.key_api_formats = Some(vec!["gemini:generate_content".to_string()]);
                row.key_allowed_models = Some(allowed_models.clone());
                row.key_internal_priority = if index == 0 { 0 } else { 10 };
                row.routing_facts.provider_pool_enabled = true;
                row.global_model_name = "claude-sonnet-4-5".to_string();
                row.model_provider_model_name = "claude-sonnet-4-5".to_string();
            }

            let sibling_rows = rows
                .iter()
                .cloned()
                .map(|mut row| {
                    row.model_id = "model-sibling".to_string();
                    row.global_model_id = "global-model-sibling".to_string();
                    row.global_model_name = "claude-sonnet-4-6".to_string();
                    row.model_provider_model_name = "claude-sonnet-4-6".to_string();
                    row
                })
                .collect::<Vec<_>>();
            let representative_rows =
                GatewayDataState::with_minimal_candidate_selection_reader_for_tests(Arc::new(
                    InMemoryMinimalCandidateSelectionReadRepository::seed(vec![rows[0].clone()]),
                ));
            let mut all_rows = rows;
            all_rows.extend(sibling_rows);

            let catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
                vec![provider],
                vec![endpoint],
                keys,
            ));
            let catalog_reader: Arc<dyn ProviderCatalogReadRepository> = catalog_repository.clone();
            let data_state = GatewayDataState::with_candidate_selection_provider_catalog_quota_and_request_candidates_for_tests(
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(all_rows)),
                catalog_reader,
                Arc::new(InMemoryProviderQuotaRepository::seed(Vec::new())),
                Arc::new(InMemoryRequestCandidateRepository::seed(Vec::new())),
            )
            .attach_provider_catalog_repository_for_tests(catalog_repository)
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
            let app = AppState::new()
                .expect("state should build")
                .with_data_state_for_tests(data_state);

            let (selected, skipped) = list_selectable_candidates_with_skip_reasons(
                &representative_rows,
                &app,
                "gemini:generate_content",
                "claude-sonnet-4-5",
                false,
                None,
                None,
                None,
                CandidateSchedulingContext {
                    now_unix_secs,
                    load_balance_seed: 17,
                },
                false,
                SchedulerOrderingConfig::default(),
            )
            .await
            .expect("representative selection should succeed");
            assert_eq!(selected.len(), 1);
            assert!(skipped.is_empty());

            let mut group = sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                "key-00000",
                0,
                provider_config,
            );
            group.kind = LocalExecutionCandidateKind::PoolGroup;
            group.candidate = selected[0].clone();
            group.provider_api_format = "gemini:generate_content".to_string();
            let transport = Arc::make_mut(&mut group.transport);
            transport.provider.provider_type = "antigravity".to_string();
            transport.endpoint.api_format = "gemini:generate_content".to_string();
            transport.endpoint.api_family = Some("gemini".to_string());
            transport.endpoint.endpoint_kind = Some("generate_content".to_string());
            transport.endpoint.base_url = "https://antigravity.googleapis.com".to_string();
            transport.key.auth_type = "oauth".to_string();
            transport.key.api_formats = Some(vec!["gemini:generate_content".to_string()]);

            let mut sibling_group = group.clone();
            sibling_group.candidate.model_id = "model-sibling".to_string();
            sibling_group.candidate.global_model_id = "global-model-sibling".to_string();
            sibling_group.candidate.global_model_name = "claude-sonnet-4-6".to_string();
            sibling_group.candidate.selected_provider_model_name = "claude-sonnet-4-6".to_string();

            let pool_config = pool_config_for_candidate(&group).expect("pool config should parse");
            assert_eq!(
                pool_config.skip_exhausted_accounts,
                !ignore_exhausted_accounts
            );
            record_admin_provider_pool_success(
                app.runtime_state.as_ref(),
                "provider-pool",
                "key-00000",
                &pool_config,
                Some("session-antigravity-quota"),
                0,
                None,
            )
            .await;

            for session in [None, Some("session-antigravity-quota")] {
                let mut cursor = PoolKeyCursor::new(
                    PlannerAppState::new(&app),
                    group.clone(),
                    session,
                    Some("claude-sonnet-4-5"),
                    None,
                );
                let candidate = cursor
                    .next_key()
                    .await
                    .expect("available tail account should survive exhausted window");
                assert_eq!(candidate.candidate.key_id, "key-00017");
                assert_eq!(candidate.orchestration.pool_key_index, Some(0));
                assert!(cursor.next_key().await.is_none());
                let rejected = cursor.take_skipped_candidates();
                assert_eq!(rejected.len(), 17);
                assert!(rejected
                    .iter()
                    .all(|item| item.skip_reason == POOL_ACCOUNT_EXHAUSTED_SKIP_REASON));
                assert!(rejected
                    .iter()
                    .all(|item| item.skip_reason != POOL_KEY_QUOTA_EXHAUSTED_SKIP_REASON));
                assert!(rejected
                    .iter()
                    .any(|item| item.candidate.key_id == "key-00000"));

                let mut sibling_cursor = PoolKeyCursor::new(
                    PlannerAppState::new(&app),
                    sibling_group.clone(),
                    session,
                    Some("claude-sonnet-4-6"),
                    None,
                );
                let sibling_first = sibling_cursor
                    .next_key()
                    .await
                    .expect("sibling model with remaining quota must stay schedulable");
                assert_eq!(sibling_first.candidate.key_id, "key-00000");
            }

            refresh_antigravity_key_quota(
                &app,
                "key-00000",
                0.5,
                now_unix_secs.saturating_sub(20),
                future_reset_at,
            )
            .await;

            for session in [None, Some("session-antigravity-quota")] {
                let mut cursor = PoolKeyCursor::new(
                    PlannerAppState::new(&app),
                    group.clone(),
                    session,
                    Some("claude-sonnet-4-5"),
                    None,
                );
                let first = cursor
                    .next_key()
                    .await
                    .expect("refreshed account should immediately recover");
                assert_eq!(first.candidate.key_id, "key-00000");
                assert_eq!(first.orchestration.pool_key_index, Some(0));
                let second = cursor
                    .next_key()
                    .await
                    .expect("remaining eligible account should follow");
                assert_eq!(second.candidate.key_id, "key-00017");
                assert_eq!(second.orchestration.pool_key_index, Some(1));
                assert!(cursor.next_key().await.is_none());
                let rejected = cursor.take_skipped_candidates();
                assert_eq!(rejected.len(), 16);
                assert!(rejected
                    .iter()
                    .all(|item| item.skip_reason == POOL_ACCOUNT_EXHAUSTED_SKIP_REASON));
            }

            refresh_antigravity_key_quota(
                &app,
                "key-00017",
                0.0,
                now_unix_secs.saturating_sub(10),
                future_reset_at,
            )
            .await;
            refresh_antigravity_key_quota(&app, "key-00000", 0.0, now_unix_secs, future_reset_at)
                .await;

            for session in [None, Some("session-antigravity-quota")] {
                let mut cursor = PoolKeyCursor::new(
                    PlannerAppState::new(&app),
                    group.clone(),
                    session,
                    Some("claude-sonnet-4-5"),
                    None,
                );
                assert!(cursor.next_key().await.is_none());
                let rejected = cursor.take_skipped_candidates();
                assert_eq!(rejected.len(), 18);
                assert!(rejected
                    .iter()
                    .all(|item| item.skip_reason == POOL_ACCOUNT_EXHAUSTED_SKIP_REASON));
                assert!(rejected
                    .iter()
                    .all(|item| item.skip_reason != POOL_KEY_QUOTA_EXHAUSTED_SKIP_REASON));

                let mut sibling_cursor = PoolKeyCursor::new(
                    PlannerAppState::new(&app),
                    sibling_group.clone(),
                    session,
                    Some("claude-sonnet-4-6"),
                    None,
                );
                let sibling_first = sibling_cursor
                    .next_key()
                    .await
                    .expect("sibling model must remain available when target model is exhausted");
                assert_eq!(sibling_first.candidate.key_id, "key-00000");
            }
        }
    }

    #[tokio::test]
    async fn pool_member_thinking_model_resolves_against_strong_account_catalog() {
        // Pool 成员必须在强读自己的目录之后、额度/发现判定之前解析请求档位。
        let provider_config = Some(json!({
            "pool_advanced": { "skip_exhausted_accounts": true }
        }));
        let (provider, endpoint, mut keys, rows) = large_pool_fixture(1, provider_config.clone());
        let mut key = keys.pop().expect("fixture key should exist");
        key.upstream_metadata = Some(json!({
            "antigravity": {
                "quota_by_model": {
                    "gemini-3-flash-low": {},
                    "gemini-3-flash-high": {}
                }
            }
        }));
        let key_id = key.id.clone();
        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    vec![key],
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);

        let mut candidate = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            key_id.as_str(),
            10,
            provider_config,
        );
        candidate.candidate.provider_type = "antigravity".to_string();
        candidate.candidate.endpoint_api_format = "gemini:generate_content".to_string();
        candidate.candidate.global_model_name = "gemini-3-flash".to_string();
        candidate.candidate.selected_provider_model_name = "gemini-3-flash-high".to_string();
        Arc::make_mut(&mut candidate.transport)
            .provider
            .provider_type = "antigravity".to_string();

        let mut candidates = vec![candidate];
        let (contexts, keys_by_id, skipped) = read_pool_catalog_key_contexts_by_id(
            PlannerAppState::new(&app),
            &mut candidates,
            None,
            Some("gemini-3-flash"),
            Some(AntigravityThinkingLevel::Low),
        )
        .await;

        assert!(skipped.is_empty());
        assert_eq!(
            candidates[0].candidate.selected_provider_model_name,
            "gemini-3-flash-low"
        );
        assert!(keys_by_id.contains_key(key_id.as_str()));
        assert!(contexts.contains_key(key_id.as_str()));
    }

    #[tokio::test]
    async fn pool_key_cursor_continues_after_exhausted_window() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "skip_exhausted_accounts": true
            }
        }));
        let (provider, endpoint, mut keys, rows) = large_pool_fixture(3, provider_config.clone());
        for key in keys.iter_mut().take(2) {
            key.status_snapshot = Some(json!({
                "quota": {
                    "provider_type": "openai",
                    "exhausted": true,
                    "usage_ratio": 1.0,
                    "windows": [
                        {
                            "code": "daily",
                            "used_ratio": 1.0,
                            "remaining_ratio": 0.0
                        }
                    ]
                }
            }));
        }

        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config,
        );

        let mut cursor = PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None);
        cursor.window_size = 2;
        cursor.page_size = 2;
        cursor.max_scanned_keys = 4;

        let candidate = cursor
            .next_key()
            .await
            .expect("cursor should scan past an exhausted window");
        assert_eq!(candidate.candidate.key_id, "key-00002");
        assert_eq!(candidate.orchestration.pool_key_index, Some(0));
        assert!(candidate.orchestration.pool_key_lease.is_none());
        assert_eq!(
            cursor
                .skip_reason_counts
                .get(aether_pool_core::POOL_ACCOUNT_EXHAUSTED_SKIP_REASON),
            Some(&2)
        );

        let skipped = cursor.take_skipped_candidates();
        assert_eq!(skipped.len(), 2);
        assert!(skipped.iter().all(|candidate| {
            candidate.skip_reason == aether_pool_core::POOL_ACCOUNT_EXHAUSTED_SKIP_REASON
        }));
    }

    #[tokio::test]
    async fn pool_key_cursor_reserve_minimum_quota_filters_pages_and_sticky_hits() {
        for reserve_enabled in [false, true] {
            for sticky in [false, true] {
                for used_percent in [99.0, 98.0, 83.0] {
                    let provider_config = Some(json!({
                        "pool_advanced": {
                            "reserve_minimum_quota": reserve_enabled,
                            "skip_exhausted_accounts": false
                        }
                    }));
                    let provider =
                        sample_codex_pool_provider("provider-pool", 0, provider_config.clone());
                    let endpoint = sample_codex_pool_endpoint("provider-pool", "endpoint-1");
                    let mut reserved = sample_codex_pool_key("provider-pool", "key-low");
                    reserved.status_snapshot = Some(json!({
                        "quota": {
                            "provider_type": "codex",
                            "updated_at": 100,
                            "allowed": false,
                            "exhausted": true,
                            "code": "exhausted",
                            "windows": [{
                                "code": "weekly",
                                "scope": "account",
                                "used_ratio": 1.0,
                                "reset_at": 4_102_444_800u64
                            }]
                        }
                    }));
                    reserved.upstream_metadata = Some(json!({
                        "codex_model_discovery": {"model_ids": ["gpt-5"]},
                        "codex": {
                            "updated_at": 200,
                            "primary_used_percent": used_percent,
                            "primary_reset_at": 4_102_444_800u64
                        }
                    }));
                    let ready = sample_codex_pool_key("provider-pool", "key-ready");
                    let rows = vec![
                        sample_codex_pool_row("provider-pool", "endpoint-1", "key-low", 0),
                        sample_codex_pool_row("provider-pool", "endpoint-1", "key-ready", 0),
                    ];
                    let data_state = GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                        Arc::new(InMemoryProviderCatalogReadRepository::seed(
                            vec![provider], vec![endpoint], vec![reserved, ready],
                        )),
                        Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
                    )
                    .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
                    let app = AppState::new()
                        .expect("state should build")
                        .with_data_state_for_tests(data_state);
                    let group =
                        sample_codex_pool_group("provider-pool", "endpoint-1", 0, provider_config);
                    let pool_config =
                        pool_config_for_candidate(&group).expect("pool config should parse");
                    let sticky_token = sticky.then_some("reserve-session");
                    if sticky {
                        record_admin_provider_pool_success(
                            app.runtime_state.as_ref(),
                            "provider-pool",
                            "key-low",
                            &pool_config,
                            sticky_token,
                            0,
                            None,
                        )
                        .await;
                    }
                    let mut cursor = PoolKeyCursor::new(
                        PlannerAppState::new(&app),
                        group,
                        sticky_token,
                        None,
                        None,
                    );
                    cursor.window_size = 1;
                    cursor.page_size = 1;
                    let mut returned = Vec::new();
                    while let Some(candidate) = cursor.next_key().await {
                        returned.push(candidate.candidate.key_id);
                    }
                    let reserve_reached = reserve_enabled && used_percent >= 99.0;
                    assert_eq!(
                        returned.contains(&"key-low".to_string()),
                        !reserve_reached,
                        "reserve={reserve_enabled}, sticky={sticky}, used={used_percent}"
                    );
                    assert!(returned.contains(&"key-ready".to_string()));
                    if reserve_reached {
                        assert_eq!(
                            cursor
                                .skip_reason_counts
                                .get(POOL_ACCOUNT_EXHAUSTED_SKIP_REASON),
                            Some(&1)
                        );
                    } else if sticky {
                        assert_eq!(returned.first().map(String::as_str), Some("key-low"));
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn pool_key_cursor_does_not_spend_effective_scan_budget_on_exhausted_accounts() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "skip_exhausted_accounts": true
            }
        }));
        let (provider, endpoint, mut keys, rows) = large_pool_fixture(700, provider_config.clone());
        for key in keys.iter_mut().take(600) {
            key.status_snapshot = Some(json!({
                "quota": {
                    "provider_type": "openai",
                    "exhausted": true,
                    "usage_ratio": 1.0,
                    "windows": [
                        {
                            "code": "daily",
                            "used_ratio": 1.0,
                            "remaining_ratio": 0.0
                        }
                    ]
                }
            }));
        }

        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config,
        );

        let mut cursor = PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None);
        assert_eq!(
            cursor.max_scanned_keys,
            aether_dispatch_core::DEFAULT_POOL_MAX_SCAN
        );
        assert!(
            cursor.absolute_max_scanned_keys > cursor.max_scanned_keys,
            "pool config scan limit should be retained as the absolute cap"
        );

        let candidate = cursor
            .next_key()
            .await
            .expect("cursor should scan past exhausted accounts within the absolute cap");
        let key_index = candidate
            .candidate
            .key_id
            .strip_prefix("key-")
            .and_then(|value| value.parse::<usize>().ok())
            .expect("fixture key id should contain a numeric suffix");
        assert!(
            key_index >= 600,
            "cursor should not return one of the exhausted leading keys"
        );
        assert_eq!(candidate.orchestration.pool_key_index, Some(0));
        assert_eq!(cursor.scanned_keys, 640);
        assert_eq!(cursor.budget_scanned_keys, 40);
        assert_eq!(
            cursor
                .skip_reason_counts
                .get(aether_pool_core::POOL_ACCOUNT_EXHAUSTED_SKIP_REASON),
            Some(&600)
        );
    }

    #[tokio::test]
    async fn pool_key_cursor_does_not_spend_effective_scan_budget_on_blocked_accounts() {
        const BLOCKED_COUNT: usize = 1_600;
        let provider_config = Some(json!({ "pool_advanced": {} }));
        let (provider, endpoint, mut keys, rows) =
            large_pool_fixture(BLOCKED_COUNT + 100, provider_config.clone());
        for key in keys.iter_mut().take(BLOCKED_COUNT) {
            key.oauth_invalid_reason = Some("blocked account".to_string());
        }

        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config,
        );

        let mut cursor = PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None);
        assert_eq!(
            cursor.max_scanned_keys,
            aether_dispatch_core::DEFAULT_POOL_MAX_SCAN
        );
        assert!(
            cursor.absolute_max_scanned_keys >= u32::try_from(BLOCKED_COUNT + 1).unwrap(),
            "default absolute scan cap should allow scanning past a large blocked prefix"
        );

        let candidate = cursor
            .next_key()
            .await
            .expect("cursor should scan past blocked accounts within the absolute cap");
        let key_index = candidate
            .candidate
            .key_id
            .strip_prefix("key-")
            .and_then(|value| value.parse::<usize>().ok())
            .expect("fixture key id should contain a numeric suffix");
        assert!(
            key_index >= BLOCKED_COUNT,
            "cursor should not return one of the blocked leading keys"
        );
        assert_eq!(candidate.orchestration.pool_key_index, Some(0));
        assert!(
            cursor.budget_scanned_keys <= aether_dispatch_core::DEFAULT_POOL_PAGE_SIZE,
            "blocked accounts should not consume effective scan budget"
        );
        assert_eq!(
            cursor
                .skip_reason_counts
                .get(aether_pool_core::POOL_ACCOUNT_BLOCKED_SKIP_REASON),
            Some(&(BLOCKED_COUNT as u32))
        );
    }

    #[tokio::test]
    async fn pool_key_cursor_does_not_spend_scan_budget_on_missing_score_rows() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "score_top_n": 128
            }
        }));
        let (provider, endpoint, keys, rows) = large_pool_fixture(1, provider_config.clone());
        let scores = (0..128)
            .map(|index| {
                sample_provider_key_pool_score(
                    "provider-pool",
                    &format!("missing-key-{index:03}"),
                    1_000.0 - index as f64,
                )
            })
            .collect::<Vec<_>>();

        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_pool_score_repository_for_tests(Arc::new(
                InMemoryPoolMemberScoreRepository::seed(scores),
            ))
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config,
        );

        let mut cursor = PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None);
        let candidate = cursor
            .next_key()
            .await
            .expect("cursor should fall back to catalog rows after stale scores");

        assert_eq!(candidate.candidate.key_id, "key-00000");
        assert_eq!(cursor.scanned_keys, 1);
        assert_eq!(cursor.budget_scanned_keys, 1);
        assert_eq!(
            cursor.skip_reason_counts.get("pool_score_member_missing"),
            Some(&128)
        );
    }

    #[tokio::test]
    async fn inactive_pool_key_with_stale_score_does_not_exhaust_pool() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "score_top_n": 128,
                "scheduling_presets": [
                    {"preset": "single_account", "enabled": true},
                    {"preset": "priority_first", "enabled": true}
                ]
            }
        }));
        let (provider, endpoint, mut keys, mut rows) =
            large_pool_fixture(2, provider_config.clone());
        keys[1].is_active = false;
        rows.retain(|row| row.key_id != "key-00001");
        let scores = vec![
            sample_provider_key_pool_score("provider-pool", "key-00000", 5.0),
            sample_provider_key_pool_score("provider-pool", "key-00001", 20.0),
        ];
        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_pool_score_repository_for_tests(Arc::new(
                InMemoryPoolMemberScoreRepository::seed(scores),
            ))
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config,
        );
        let mut cursor = PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None);

        let candidate = cursor
            .next_key()
            .await
            .expect("active key must stay schedulable beside a stale inactive score");

        assert_eq!(candidate.candidate.key_id, "key-00000");
        assert_eq!(
            cursor.skip_reason_counts.get("pool_score_member_missing"),
            Some(&1)
        );
    }

    #[tokio::test]
    async fn stale_inactive_score_only_does_not_exhaust_pool() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "score_top_n": 128,
                "scheduling_presets": [
                    {"preset": "single_account", "enabled": true},
                    {"preset": "priority_first", "enabled": true}
                ]
            }
        }));
        let (provider, endpoint, mut keys, mut rows) =
            large_pool_fixture(2, provider_config.clone());
        keys[1].is_active = false;
        rows.retain(|row| row.key_id != "key-00001");
        let scores = vec![sample_provider_key_pool_score(
            "provider-pool",
            "key-00001",
            20.0,
        )];
        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_pool_score_repository_for_tests(Arc::new(
                InMemoryPoolMemberScoreRepository::seed(scores),
            ))
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config,
        );
        let mut cursor = PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None);

        let candidate = cursor
            .next_key()
            .await
            .expect("catalog rows must remain schedulable when the only score is stale");

        assert_eq!(candidate.candidate.key_id, "key-00000");
    }

    #[tokio::test]
    async fn score_candidates_continue_across_pool_windows() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "score_top_n": 128,
                "score_fallback_scan_limit": 128
            }
        }));
        let (provider, endpoint, keys, rows) = large_pool_fixture(128, provider_config.clone());
        let scores = (0..128)
            .map(|index| {
                sample_provider_key_pool_score(
                    "provider-pool",
                    &format!("key-{index:05}"),
                    1_000.0 - index as f64,
                )
            })
            .collect::<Vec<_>>();
        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_pool_score_repository_for_tests(Arc::new(
                InMemoryPoolMemberScoreRepository::seed(scores),
            ))
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config,
        );
        let mut cursor = PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None);

        let mut returned_key_ids = Vec::new();
        while let Some(candidate) = cursor.next_key().await {
            returned_key_ids.push(candidate.candidate.key_id);
        }

        assert_eq!(returned_key_ids.len(), 32);
        assert!(returned_key_ids.iter().any(|key_id| key_id == "key-00112"));
        assert!(returned_key_ids.iter().any(|key_id| key_id == "key-00127"));
        assert_eq!(cursor.score_next_offset, 128);
        assert!(cursor.score_phase_exhausted);
        assert_eq!(cursor.score_schedule_interest_count, 16);
        assert_eq!(cursor.scanned_keys, 128);
        assert_eq!(cursor.budget_scanned_keys, 128);
    }

    #[tokio::test]
    async fn score_and_fallback_duplicate_does_not_consume_scan_budget_twice() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "score_top_n": 1,
                "score_fallback_scan_limit": 2
            }
        }));
        let (provider, endpoint, keys, rows) = large_pool_fixture(2, provider_config.clone());
        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_pool_score_repository_for_tests(Arc::new(
                InMemoryPoolMemberScoreRepository::seed(vec![sample_provider_key_pool_score(
                    "provider-pool",
                    "key-00000",
                    1_000.0,
                )]),
            ))
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config,
        );
        let mut cursor = PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None);

        let mut returned_key_ids = vec![
            cursor
                .next_key()
                .await
                .expect("one candidate should schedule")
                .candidate
                .key_id,
            cursor
                .next_key()
                .await
                .expect("the other candidate should schedule")
                .candidate
                .key_id,
        ];
        returned_key_ids.sort();

        assert_eq!(returned_key_ids, ["key-00000", "key-00001"]);
        assert_eq!(cursor.scanned_keys, 3);
        assert_eq!(cursor.budget_scanned_keys, 2);
    }

    #[tokio::test]
    async fn pool_scheduler_skips_invalid_and_exhausted_high_priority_hot_pool_before_fallback_provider(
    ) {
        let provider_config = Some(json!({
            "pool_advanced": {
                "probing_enabled": true,
                "skip_exhausted_accounts": true,
                "scheduling_presets": [
                    {"preset": "single_account", "enabled": true}
                ]
            }
        }));
        let provider_a = sample_codex_pool_provider("provider-a", 0, provider_config.clone());
        let provider_b = sample_codex_pool_provider("provider-b", 10, provider_config.clone());
        let endpoint_a = sample_codex_pool_endpoint("provider-a", "endpoint-a");
        let endpoint_b = sample_codex_pool_endpoint("provider-b", "endpoint-b");

        let mut key_a_invalid = sample_codex_pool_key("provider-a", "key-a-invalid");
        key_a_invalid.oauth_invalid_at_unix_secs = Some(1_710_000_000);
        key_a_invalid.oauth_invalid_reason = Some("[OAUTH_EXPIRED] token invalidated".to_string());
        let exhausted_status_snapshot = json!({
            "quota": {
                "provider_type": "codex",
                "exhausted": true,
                "usage_ratio": 1.0,
                "windows": [
                    {
                        "code": "daily",
                        "used_ratio": 1.0,
                        "remaining_ratio": 0.0
                    }
                ]
            }
        });
        key_a_invalid.status_snapshot = Some(exhausted_status_snapshot.clone());
        let mut key_a_exhausted = sample_codex_pool_key("provider-a", "key-a-exhausted");
        key_a_exhausted.status_snapshot = Some(exhausted_status_snapshot);
        let key_b_ready = sample_codex_pool_key("provider-b", "key-b-ready");

        let rows = vec![
            sample_codex_pool_row("provider-a", "endpoint-a", "key-a-invalid", 0),
            sample_codex_pool_row("provider-a", "endpoint-a", "key-a-exhausted", 0),
            sample_codex_pool_row("provider-b", "endpoint-b", "key-b-ready", 10),
        ];

        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider_a, provider_b],
                    vec![endpoint_a, endpoint_b],
                    vec![key_a_invalid, key_a_exhausted, key_b_ready],
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        app.runtime_state
            .set_add(
                &admin_provider_pool_quota_probe_active_members_key("provider-a"),
                "key-a-invalid",
            )
            .await
            .expect("provider-a hot member should insert");
        app.runtime_state
            .set_add(
                &admin_provider_pool_quota_probe_active_members_key("provider-b"),
                "key-b-ready",
            )
            .await
            .expect("provider-b hot member should insert");

        let group_a =
            sample_codex_pool_group("provider-a", "endpoint-a", 0, provider_config.clone());
        let group_b = sample_codex_pool_group("provider-b", "endpoint-b", 10, provider_config);

        let (scheduled, skipped) = apply_local_execution_pool_scheduler(
            PlannerAppState::new(&app),
            vec![group_a, group_b],
            None,
            Some("gpt-5"),
            None,
            None,
            None,
        )
        .await;

        assert_eq!(
            scheduled
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-b-ready"]
        );
        let skipped_pairs = skipped
            .iter()
            .map(|item| (item.candidate.key_id.as_str(), item.skip_reason))
            .collect::<Vec<_>>();
        assert!(skipped_pairs.contains(&("key-a-invalid", "pool_account_blocked")));
        assert!(skipped_pairs.contains(&(
            "key-a-exhausted",
            aether_pool_core::POOL_ACCOUNT_EXHAUSTED_SKIP_REASON
        )));
    }

    #[tokio::test]
    async fn pool_scheduler_skips_invalid_high_priority_hot_pool_account_even_with_remaining_quota()
    {
        let provider_config = Some(json!({
            "pool_advanced": {
                "probing_enabled": true,
                "skip_exhausted_accounts": true,
                "scheduling_presets": [
                    {"preset": "single_account", "enabled": true}
                ]
            }
        }));
        let provider_a = sample_codex_pool_provider("provider-a", 0, provider_config.clone());
        let provider_b = sample_codex_pool_provider("provider-b", 10, provider_config.clone());
        let endpoint_a = sample_codex_pool_endpoint("provider-a", "endpoint-a");
        let endpoint_b = sample_codex_pool_endpoint("provider-b", "endpoint-b");

        let mut key_a_invalid = sample_codex_pool_key("provider-a", "key-a-invalid");
        key_a_invalid.oauth_invalid_at_unix_secs = Some(1_710_000_000);
        key_a_invalid.oauth_invalid_reason = Some("[OAUTH_EXPIRED] token invalidated".to_string());
        key_a_invalid.status_snapshot = Some(json!({
            "quota": {
                "provider_type": "codex",
                "exhausted": false,
                "usage_ratio": 0.25,
                "windows": [
                    {
                        "code": "daily",
                        "used_ratio": 0.25,
                        "remaining_ratio": 0.75
                    }
                ]
            }
        }));
        let key_b_ready = sample_codex_pool_key("provider-b", "key-b-ready");

        let rows = vec![
            sample_codex_pool_row("provider-a", "endpoint-a", "key-a-invalid", 0),
            sample_codex_pool_row("provider-b", "endpoint-b", "key-b-ready", 10),
        ];

        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider_a, provider_b],
                    vec![endpoint_a, endpoint_b],
                    vec![key_a_invalid, key_b_ready],
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        app.runtime_state
            .set_add(
                &admin_provider_pool_quota_probe_active_members_key("provider-a"),
                "key-a-invalid",
            )
            .await
            .expect("provider-a hot member should insert");
        app.runtime_state
            .set_add(
                &admin_provider_pool_quota_probe_active_members_key("provider-b"),
                "key-b-ready",
            )
            .await
            .expect("provider-b hot member should insert");

        let group_a =
            sample_codex_pool_group("provider-a", "endpoint-a", 0, provider_config.clone());
        let group_b = sample_codex_pool_group("provider-b", "endpoint-b", 10, provider_config);

        let (scheduled, skipped) = apply_local_execution_pool_scheduler(
            PlannerAppState::new(&app),
            vec![group_a, group_b],
            None,
            Some("gpt-5"),
            None,
            None,
            None,
        )
        .await;

        assert_eq!(
            scheduled
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-b-ready"]
        );
        let skipped_pairs = skipped
            .iter()
            .map(|item| (item.candidate.key_id.as_str(), item.skip_reason))
            .collect::<Vec<_>>();
        assert!(skipped_pairs.contains(&("key-a-invalid", "pool_account_blocked")));
    }

    #[test]
    fn pool_key_reauth_scheduling_keeps_recoverable_oauth_markers_usable() {
        let mut key = sample_codex_pool_key("provider-a", "key-refresh-failed");
        key.expires_at_unix_secs = Some(200);
        key.oauth_invalid_reason = Some(
            "[REFRESH_FAILED] Token 续期失败 (401): refresh_token 已被使用并轮换，请重新登录授权"
                .to_string(),
        );

        assert!(!pool_key_requires_reauth_for_scheduling(&key, 100));
        assert!(pool_key_requires_reauth_for_scheduling(&key, 200));

        key.oauth_invalid_reason = Some("[REQUEST_FAILED] 账号状态检查失败".to_string());
        key.oauth_invalid_at_unix_secs = Some(100);
        assert!(!pool_key_requires_reauth_for_scheduling(&key, 300));

        key.oauth_invalid_reason = Some("[OAUTH_EXPIRED] session expired".to_string());
        assert!(!pool_key_requires_reauth_for_scheduling(&key, 300));
    }

    #[test]
    fn pool_key_reauth_scheduling_blocks_invalid_oauth_markers_without_affecting_non_oauth_keys() {
        let mut key = sample_codex_pool_key("provider-a", "key-invalid");
        key.oauth_invalid_reason = Some("[ACCOUNT_BLOCK] account has been deactivated".to_string());
        assert!(pool_key_requires_reauth_for_scheduling(&key, 100));

        key.oauth_invalid_reason = Some("[OAUTH_EXPIRED] token invalidated".to_string());
        key.oauth_invalid_at_unix_secs = None;
        assert!(pool_key_requires_reauth_for_scheduling(&key, 100));

        key.oauth_invalid_reason = Some("Kiro Token 无效或已过期".to_string());
        key.oauth_invalid_at_unix_secs = None;
        assert!(pool_key_requires_reauth_for_scheduling(&key, 100));

        key.oauth_invalid_reason = None;
        key.oauth_invalid_at_unix_secs = Some(100);
        assert!(pool_key_requires_reauth_for_scheduling(&key, 100));

        key.auth_type = "api_key".to_string();
        assert!(!pool_key_requires_reauth_for_scheduling(&key, 100));
    }

    #[tokio::test]
    async fn pool_key_cursor_simulates_large_lru_pool_with_lazy_pages_and_dynamic_skips() {
        const KEY_COUNT: usize = 2048;
        let provider_config = Some(json!({
            "pool_advanced": {
                "lru_enabled": true,
                "rate_limit_cooldown_seconds": 300
            }
        }));
        let (provider, endpoint, keys, rows) =
            large_pool_fixture(KEY_COUNT, provider_config.clone());
        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);
        let group = sample_eligible_candidate(
            "provider-pool",
            "endpoint-1",
            "pool-group",
            10,
            provider_config.clone(),
        );
        let pool_config = pool_config_for_candidate(&group).expect("pool config should parse");
        for key_id in ["key-00000", "key-00001"] {
            record_admin_provider_pool_error(
                app.runtime_state.as_ref(),
                "provider-pool",
                key_id,
                &pool_config,
                429,
                None,
                None,
            )
            .await;
        }

        let mut cursor = PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None);
        assert_eq!(
            cursor.window_size,
            aether_dispatch_core::DEFAULT_POOL_WINDOW_SIZE
        );
        assert_eq!(
            cursor.page_size,
            aether_dispatch_core::DEFAULT_POOL_PAGE_SIZE
        );
        assert_eq!(
            cursor.max_scanned_keys,
            aether_dispatch_core::DEFAULT_POOL_MAX_SCAN
        );

        let mut returned_ids = Vec::new();
        for _ in 0..10 {
            let candidate = cursor
                .next_key()
                .await
                .expect("large pool should return first page candidates");
            returned_ids.push(candidate.candidate.key_id.clone());
            assert!(candidate.orchestration.pool_key_lease.is_none());
        }
        assert_eq!(returned_ids.first().map(String::as_str), Some("key-00002"));
        assert_eq!(returned_ids.last().map(String::as_str), Some("key-00011"));
        assert_eq!(cursor.scanned_keys, 64);
        assert!(
            cursor.queued_candidates.len() <= cursor.window_size as usize,
            "cursor should only retain the current page window"
        );

        record_admin_provider_pool_error(
            app.runtime_state.as_ref(),
            "provider-pool",
            "key-00014",
            &pool_config,
            429,
            None,
            None,
        )
        .await;

        let candidate = cursor
            .next_key()
            .await
            .expect("cursor should keep the frozen window despite later runtime changes");
        assert_eq!(candidate.candidate.key_id, "key-00012");
        returned_ids.push(candidate.candidate.key_id.clone());
        assert!(candidate.orchestration.pool_key_lease.is_none());

        while let Some(candidate) = cursor.next_key().await {
            returned_ids.push(candidate.candidate.key_id.clone());
            assert!(candidate.orchestration.pool_key_lease.is_none());
        }

        let max_returned_windows = aether_dispatch_core::DEFAULT_POOL_MAX_SCAN
            / aether_dispatch_core::DEFAULT_POOL_PAGE_SIZE;
        assert!(
            returned_ids.len()
                <= (max_returned_windows * aether_dispatch_core::DEFAULT_POOL_WINDOW_SIZE) as usize,
            "cursor should only return bounded frozen windows per request"
        );
        assert_eq!(returned_ids.len(), 127);
        assert_eq!(returned_ids.last().map(String::as_str), Some("key-00463"));
        assert_eq!(cursor.scanned_keys, 512);
        assert_eq!(cursor.skip_reason_counts.get("pool_cooldown"), Some(&3));
        assert!(!cursor
            .skip_reason_counts
            .contains_key("pool_key_lease_busy"));
        for skipped in ["key-00000", "key-00001", "key-00014"] {
            assert!(
                !returned_ids.iter().any(|key_id| key_id == skipped),
                "{skipped} should have been skipped"
            );
        }
        assert!(
            returned_ids.iter().any(|key_id| key_id == "key-00015"),
            "key-00015 should not be blocked by request-scoped leases"
        );
    }

    /// 验证目录快照、鉴权配置和使用统计共同投影为无模型限定的 Pool 信号。
    #[test]
    fn builds_pool_catalog_context_from_status_snapshot_and_auth_config() {
        let mut key = StoredProviderCatalogKey::new(
            "key-1".to_string(),
            "provider-1".to_string(),
            "key-1".to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("key should build")
        .with_transport_fields(
            None,
            "secret".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("transport fields should build");
        key.status_snapshot = Some(json!({
            "account": {"blocked": false},
            "quota": {
                "usage_ratio": 0.25,
                "reset_seconds": 3600,
                "exhausted": false,
                "plan_type": "team"
            }
        }));
        key.success_count = Some(4);
        key.total_response_time_ms = Some(200);
        key.last_used_at_unix_secs = Some(1_711_000_123);

        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(GatewayDataState::with_provider_catalog_reader_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    Vec::new(),
                    Vec::new(),
                    vec![key.clone()],
                )),
            ));

        let context = build_pool_catalog_key_context(
            PlannerAppState::new(&app),
            &ProviderPoolService::with_builtin_adapters(),
            &key,
            "codex",
            None,
        );

        assert_eq!(context.plan_tier.as_deref(), Some("team"));
        assert_eq!(context.quota_usage_ratio, Some(0.25));
        assert_eq!(context.quota_reset_seconds, Some(3600.0));
        assert_eq!(context.latency_avg_ms, Some(50.0));
        assert_eq!(context.catalog_lru_score, Some(1_711_000_123.0));
    }

    /// 验证未指定模型时仍沿用账号快照并忽略已恢复窗口上的陈旧耗尽标志。
    #[test]
    fn pool_catalog_context_ignores_stale_codex_exhausted_snapshot_when_windows_have_capacity() {
        let mut key = sample_catalog_oauth_key("key-stale-exhausted");
        key.upstream_metadata = Some(json!({
            "codex": {
                "primary_used_percent": 100.0
            }
        }));
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "code": "exhausted",
                "exhausted": true,
                "usage_ratio": 0.0,
                "windows": [
                    {
                        "code": "weekly",
                        "used_ratio": 0.0,
                        "remaining_ratio": 1.0
                    },
                    {
                        "code": "5h",
                        "used_ratio": 0.0,
                        "remaining_ratio": 1.0
                    }
                ]
            }
        }));

        let app = app_state_with_catalog_key(key.clone());
        let context = build_pool_catalog_key_context(
            PlannerAppState::new(&app),
            &ProviderPoolService::with_builtin_adapters(),
            &key,
            "codex",
            None,
        );

        assert!(!context.quota_exhausted);
    }

    /// 验证缺少模型窗口时仍可从 Codex 账号元数据识别耗尽状态。
    #[test]
    fn pool_catalog_context_marks_codex_metadata_exhausted() {
        let mut key = sample_catalog_oauth_key("key-metadata-exhausted");
        key.upstream_metadata = Some(json!({
            "codex": {
                "secondary_used_percent": 100.0
            }
        }));

        let app = app_state_with_catalog_key(key.clone());
        let context = build_pool_catalog_key_context(
            PlannerAppState::new(&app),
            &ProviderPoolService::with_builtin_adapters(),
            &key,
            "codex",
            None,
        );

        assert!(context.quota_exhausted);
    }

    /// 验证未指定模型时，Antigravity 等快照型 Provider 保留账号级耗尽回退。
    #[test]
    fn pool_catalog_context_preserves_snapshot_exhaustion_for_snapshot_only_providers() {
        let mut key = sample_catalog_oauth_key("key-antigravity-exhausted");
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "antigravity",
                "code": "exhausted",
                "exhausted": true,
                "windows": [
                    {
                        "code": "gemini-2.5-pro",
                        "used_ratio": 1.0,
                        "remaining_ratio": 0.0
                    }
                ]
            }
        }));

        let app = app_state_with_catalog_key(key.clone());
        let context = build_pool_catalog_key_context(
            PlannerAppState::new(&app),
            &ProviderPoolService::with_builtin_adapters(),
            &key,
            "antigravity",
            None,
        );

        assert!(context.quota_exhausted);
    }

    /// 验证目录上下文只用请求模型对应的 Antigravity 窗口判定可恢复额度耗尽。
    #[test]
    fn pool_catalog_context_scopes_antigravity_exhaustion_to_requested_model() {
        let mut key = sample_catalog_oauth_key("key-antigravity-model-quota");
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "antigravity",
                "exhausted": false,
                "windows": [
                    {
                        "code": "model:gemini-3.1-pro-high",
                        "scope": "model",
                        "model": "gemini-3.1-pro-high",
                        "used_ratio": 1.0,
                        "is_exhausted": true
                    },
                    {
                        "code": "model:gemini-3-flash-agent",
                        "scope": "model",
                        "model": "gemini-3-flash-agent",
                        "used_ratio": 0.1,
                        "is_exhausted": false
                    }
                ]
            }
        }));

        let app = app_state_with_catalog_key(key.clone());
        let exhausted = build_pool_catalog_key_context(
            PlannerAppState::new(&app),
            &ProviderPoolService::with_builtin_adapters(),
            &key,
            "antigravity",
            Some("gemini-3.1-pro-high"),
        );
        let available = build_pool_catalog_key_context(
            PlannerAppState::new(&app),
            &ProviderPoolService::with_builtin_adapters(),
            &key,
            "antigravity",
            Some("gemini-3-flash-agent"),
        );

        assert!(exhausted.quota_exhausted);
        assert!(!available.quota_exhausted);
    }

    /// 验证已知封禁元数据仍是与请求模型无关的账号级阻断。

    #[test]
    fn pool_catalog_context_marks_known_banned_account_from_metadata() {
        let mut key = sample_catalog_oauth_key("key-account-banned");
        key.upstream_metadata = Some(json!({
            "codex": {
                "account_disabled": true,
                "reason": "deactivated_workspace"
            }
        }));

        let app = app_state_with_catalog_key(key.clone());
        let context = build_pool_catalog_key_context(
            PlannerAppState::new(&app),
            &ProviderPoolService::with_builtin_adapters(),
            &key,
            "codex",
            None,
        );

        assert!(context.account_blocked);
    }

    fn sample_catalog_oauth_key(key_id: &str) -> StoredProviderCatalogKey {
        StoredProviderCatalogKey::new(
            key_id.to_string(),
            "provider-1".to_string(),
            key_id.to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("key should build")
        .with_transport_fields(
            None,
            "secret".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("transport fields should build")
    }

    fn app_state_with_catalog_key(key: StoredProviderCatalogKey) -> AppState {
        AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(GatewayDataState::with_provider_catalog_reader_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    Vec::new(),
                    Vec::new(),
                    vec![key],
                )),
            ))
    }

    /// 逐账号模型解析边界：无清单也走解析器并保留行默认与操作范围；显式空清单或
    /// 无交集拒绝；仅凭全局映射正则不得越过显式 Provider 映射替换请求模型，
    /// 显式作用域 Provider 映射与账号清单交集才解析出带后缀模型名；操作范围映射
    /// 只在请求操作命中时替换默认名。
    #[test]
    fn pool_member_model_resolution_tracks_account_scope_and_denials() {
        let group = sample_codex_pool_group("provider-pool", "endpoint-1", 0, None);
        let mut row = large_pool_fixture(1, None).3.remove(0);
        row.endpoint_api_format = "openai:responses".to_string();

        // 无账号模型清单：解析行自身的默认/映射模型名，不继承其他账号目标。
        row.key_allowed_models = None;
        assert_eq!(
            super::pool_member_model_resolution(&group, &row, "gpt-5", None),
            Some(("gpt-5".to_string(), None)),
        );

        // 显式空清单与无交集清单都必须整行拒绝。
        row.key_allowed_models = Some(Vec::new());
        assert_eq!(
            super::pool_member_model_resolution(&group, &row, "gpt-5", None),
            None
        );
        row.key_allowed_models = Some(vec!["other-model".to_string()]);
        assert_eq!(
            super::pool_member_model_resolution(&group, &row, "gpt-5", None),
            None
        );

        // 仅凭全局映射正则不得将未配置在 Provider 映射中的变体替换成上游模型；
        // 显式作用域 Provider 映射与账号清单产生交集后，才解析为该账号实际上游模型名。
        row.global_model_mappings = Some(vec!["gpt-5-(high|low|medium|tiered)".to_string()]);
        row.key_allowed_models = Some(vec![
            "gpt-5-high".to_string(),
            "gpt-5-low".to_string(),
            "gpt-5-medium".to_string(),
            "gpt-5-tiered".to_string(),
        ]);
        assert_eq!(
            super::pool_member_model_resolution(&group, &row, "gpt-5", None),
            None,
        );
        row.model_provider_model_mappings = Some(vec![
            aether_data_contracts::repository::candidate_selection::StoredProviderModelMapping {
                name: "gpt-5-high".to_string(),
                priority: 1,
                api_formats: Some(vec!["openai:responses".to_string()]),
                endpoint_ids: None,
                operations: None,
            },
        ]);
        assert_eq!(
            super::pool_member_model_resolution(&group, &row, "gpt-5", None),
            Some(("gpt-5-high".to_string(), Some("gpt-5-high".to_string()))),
        );

        // 操作范围映射只在请求操作命中时替换默认模型名，范围外保持默认。
        row.key_allowed_models = None;
        row.global_model_mappings = None;
        row.model_provider_model_mappings = Some(vec![
            aether_data_contracts::repository::candidate_selection::StoredProviderModelMapping {
                name: "gpt-5-compact".to_string(),
                priority: 0,
                api_formats: None,
                endpoint_ids: None,
                operations: Some(vec!["compact".to_string()]),
            },
        ]);
        assert_eq!(
            super::pool_member_model_resolution(&group, &row, "gpt-5", Some("compact")),
            Some(("gpt-5-compact".to_string(), None)),
        );
        assert_eq!(
            super::pool_member_model_resolution(&group, &row, "gpt-5", None),
            Some(("gpt-5".to_string(), None)),
        );
    }

    /// 路由模型（指令基座）是逐账号解析输入：基座命中账号清单时可用；
    /// 未收敛的指令原名不得自行剥离后缀而比枚举更宽松地命中。
    #[test]
    fn pool_member_model_resolution_uses_routing_model_without_suffix_guessing() {
        let group = sample_codex_pool_group("provider-pool", "endpoint-1", 0, None);
        let mut row = large_pool_fixture(1, None).3.remove(0);
        row.endpoint_api_format = "openai:responses".to_string();
        row.key_allowed_models = Some(vec!["gpt-5.6-sol".to_string()]);
        assert_eq!(
            super::pool_member_model_resolution(&group, &row, "gpt-5.6-sol", None),
            Some(("gpt-5".to_string(), None)),
        );
        assert_eq!(
            super::pool_member_model_resolution(&group, &row, "gpt-5.6-sol-high", None),
            None,
        );
    }

    /// 失败回归（真实 cursor 展开）：受限账号清单持有自定义后缀原始模型名时，raw 语义
    /// （指令路由关闭、路由模型为空）必须让该成员可调度；一旦路由模型被指令基座替换，
    /// 同一受限成员会被错误拒绝——这正是从全局策略推断指令语义导致的 P2 行为。
    #[tokio::test]
    async fn pool_cursor_raw_routing_model_keeps_restricted_custom_suffix_member_schedulable() {
        let provider_config = Some(json!({ "pool_advanced": {} }));
        let (provider, endpoint, mut keys, mut rows) =
            large_pool_fixture(2, provider_config.clone());
        // 噪声成员：清单与请求无交集，两种语义下都必须被拒绝。
        rows[0].key_allowed_models = Some(vec!["unrelated-model".to_string()]);
        keys[0].allowed_models = Some(json!(["unrelated-model"]));
        // 受限成员：账号清单只允许自定义后缀的原始模型名；目录 Key 与行保持一致。
        rows[1].key_allowed_models = Some(vec!["gpt-5-boost".to_string()]);
        keys[1].allowed_models = Some(json!(["gpt-5-boost"]));
        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    keys,
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);

        // raw 语义（image/video/files 等显式关闭指令的调用方传 None）：
        // 原始请求名命中受限清单，成员保持可调度。
        let mut raw_cursor = PoolKeyCursor::new(
            PlannerAppState::new(&app),
            sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                "pool-group",
                10,
                provider_config.clone(),
            ),
            None,
            Some("gpt-5-boost"),
            None,
        )
        .with_routing_model(None);
        let raw_member = raw_cursor
            .next_key()
            .await
            .expect("raw routing model must keep the restricted custom-suffix member schedulable");
        assert_eq!(raw_member.candidate.key_id, "key-00001");
        assert!(raw_cursor.next_key().await.is_none());

        // 冻结策略收敛出的指令基座路由模型：基座不再命中该受限清单，成员被拒绝。
        let mut base_cursor = PoolKeyCursor::new(
            PlannerAppState::new(&app),
            sample_eligible_candidate(
                "provider-pool",
                "endpoint-1",
                "pool-group",
                10,
                provider_config,
            ),
            None,
            Some("gpt-5-boost"),
            None,
        )
        .with_routing_model(Some("gpt-5"));
        assert!(
            base_cursor.next_key().await.is_none(),
            "suffix-stripped base must not silently schedule or leak the restricted member"
        );
        assert_eq!(
            base_cursor.skip_reason_counts.get("key_model_disabled"),
            Some(&2),
            "both restricted members must be rejected once the custom suffix is stripped"
        );
    }

    fn large_pool_fixture(
        key_count: usize,
        provider_config: Option<serde_json::Value>,
    ) -> (
        StoredProviderCatalogProvider,
        StoredProviderCatalogEndpoint,
        Vec<StoredProviderCatalogKey>,
        Vec<StoredMinimalCandidateSelectionRow>,
    ) {
        let provider = StoredProviderCatalogProvider::new(
            "provider-pool".to_string(),
            "provider-pool".to_string(),
            Some("https://example.com".to_string()),
            "openai".to_string(),
        )
        .expect("provider should build")
        .with_routing_fields(0)
        .with_transport_fields(
            true,
            false,
            false,
            None,
            None,
            None,
            None,
            None,
            provider_config,
        );
        let endpoint = StoredProviderCatalogEndpoint::new(
            "endpoint-1".to_string(),
            "provider-pool".to_string(),
            "openai:chat".to_string(),
            Some("openai".to_string()),
            Some("chat".to_string()),
            true,
        )
        .expect("endpoint should build")
        .with_health_score(1.0)
        .with_transport_fields(
            "https://example.com/v1/chat/completions".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("endpoint transport should build");

        // 这些用例只验证池扫描、跳过计数和游标预算，不会发起请求或读取凭据。
        // 留空凭据可跳过无关的 Fernet 加解密，同时避免复用绑定密文破坏 key_id AAD。
        let mut keys = Vec::with_capacity(key_count);
        let mut rows = Vec::with_capacity(key_count);
        for index in 0..key_count {
            let key_id = format!("key-{index:05}");
            let mut key = StoredProviderCatalogKey::new(
                key_id.clone(),
                "provider-pool".to_string(),
                key_id.clone(),
                "api_key".to_string(),
                None,
                true,
            )
            .expect("key should build")
            .with_transport_fields(
                Some(json!(["openai:chat"])),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .expect("key transport should build");
            key.internal_priority = 10;
            key.last_used_at_unix_secs = Some(index as u64);
            keys.push(key);
            rows.push(StoredMinimalCandidateSelectionRow {
                provider_id: "provider-pool".to_string(),
                provider_name: "provider-pool".to_string(),
                provider_type: "openai".to_string(),
                provider_priority: 0,
                provider_is_active: true,
                endpoint_id: "endpoint-1".to_string(),
                endpoint_api_format: "openai:chat".to_string(),
                endpoint_api_family: Some("openai".to_string()),
                endpoint_kind: Some("chat".to_string()),
                endpoint_is_active: true,
                key_id: key_id.clone(),
                key_name: key_id,
                key_auth_type: "api_key".to_string(),
                key_is_active: true,
                key_api_formats: Some(vec!["openai:chat".to_string()]),
                key_allowed_models: None,
                key_capabilities: None,
                key_internal_priority: 10,
                key_global_priority_by_format: None,
                routing_facts: Default::default(),
                model_id: "model-1".to_string(),
                global_model_id: "global-model-1".to_string(),
                global_model_name: "gpt-5".to_string(),
                global_model_mappings: None,
                global_model_supports_streaming: Some(true),
                model_provider_model_name: "gpt-5".to_string(),
                model_provider_model_mappings: None,
                model_supports_streaming: Some(true),
                model_is_active: true,
                model_is_available: true,
            });
        }
        (provider, endpoint, keys, rows)
    }

    fn sample_provider_key_pool_score(
        provider_id: &str,
        key_id: &str,
        score: f64,
    ) -> StoredPoolMemberScore {
        let identity = PoolMemberIdentity::provider_api_key(provider_id, key_id);
        let scope = provider_key_pool_score_scope();
        StoredPoolMemberScore {
            id: provider_key_pool_score_id(&identity, &scope),
            pool_kind: identity.pool_kind,
            pool_id: identity.pool_id,
            member_kind: identity.member_kind,
            member_id: identity.member_id,
            capability: scope.capability,
            scope_kind: scope.scope_kind,
            scope_id: scope.scope_id,
            score,
            hard_state: PoolMemberHardState::Available,
            score_version: 1,
            score_reason: json!({}),
            last_ranked_at: Some(1_000),
            last_scheduled_at: None,
            last_success_at: None,
            last_failure_at: None,
            failure_count: 0,
            last_probe_attempt_at: None,
            last_probe_success_at: None,
            last_probe_failure_at: None,
            probe_failure_count: 0,
            probe_status: PoolMemberProbeStatus::Ok,
            updated_at: 1_000,
        }
    }

    fn sample_codex_pool_provider(
        provider_id: &str,
        provider_priority: i32,
        provider_config: Option<serde_json::Value>,
    ) -> StoredProviderCatalogProvider {
        StoredProviderCatalogProvider::new(
            provider_id.to_string(),
            provider_id.to_string(),
            Some("https://example.com".to_string()),
            "codex".to_string(),
        )
        .expect("provider should build")
        .with_routing_fields(provider_priority)
        .with_transport_fields(
            true,
            false,
            false,
            None,
            None,
            None,
            None,
            None,
            provider_config,
        )
    }

    fn sample_codex_pool_endpoint(
        provider_id: &str,
        endpoint_id: &str,
    ) -> StoredProviderCatalogEndpoint {
        StoredProviderCatalogEndpoint::new(
            endpoint_id.to_string(),
            provider_id.to_string(),
            "openai:responses".to_string(),
            Some("openai".to_string()),
            Some("responses".to_string()),
            true,
        )
        .expect("endpoint should build")
        .with_health_score(1.0)
        .with_transport_fields(
            "https://example.com/v1/responses".to_string(),
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

    /// 这些测试只检查池调度状态，不涉及凭据解密，因此不构造无关的密文。
    fn sample_codex_pool_key(provider_id: &str, key_id: &str) -> StoredProviderCatalogKey {
        let mut key = StoredProviderCatalogKey::new(
            key_id.to_string(),
            provider_id.to_string(),
            key_id.to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("key should build")
        .with_transport_fields(
            Some(json!(["openai:responses"])),
            None,
            None,
            None,
            Some(json!({"openai:responses": 1})),
            None,
            Some(4_102_444_800),
            None,
            None,
        )
        .expect("key transport should build");
        key.internal_priority = 10;
        key.upstream_metadata = Some(json!({
            "codex_model_discovery": {"model_ids": ["gpt-5"]}
        }));
        key
    }

    fn sample_codex_pool_row(
        provider_id: &str,
        endpoint_id: &str,
        key_id: &str,
        provider_priority: i32,
    ) -> StoredMinimalCandidateSelectionRow {
        StoredMinimalCandidateSelectionRow {
            provider_id: provider_id.to_string(),
            provider_name: provider_id.to_string(),
            provider_type: "codex".to_string(),
            provider_priority,
            provider_is_active: true,
            endpoint_id: endpoint_id.to_string(),
            endpoint_api_format: "openai:responses".to_string(),
            endpoint_api_family: Some("openai".to_string()),
            endpoint_kind: Some("responses".to_string()),
            endpoint_is_active: true,
            key_id: key_id.to_string(),
            key_name: key_id.to_string(),
            key_auth_type: "oauth".to_string(),
            key_is_active: true,
            key_api_formats: Some(vec!["openai:responses".to_string()]),
            key_allowed_models: None,
            key_capabilities: None,
            key_internal_priority: 10,
            key_global_priority_by_format: Some(json!({"openai:responses": 1})),
            routing_facts: Default::default(),
            model_id: "model-1".to_string(),
            global_model_id: "global-model-1".to_string(),
            global_model_name: "gpt-5".to_string(),
            global_model_mappings: None,
            global_model_supports_streaming: Some(true),
            model_provider_model_name: "gpt-5".to_string(),
            model_provider_model_mappings: None,
            model_supports_streaming: Some(true),
            model_is_active: true,
            model_is_available: true,
        }
    }

    fn sample_codex_pool_group(
        provider_id: &str,
        endpoint_id: &str,
        provider_priority: i32,
        provider_config: Option<serde_json::Value>,
    ) -> EligibleLocalExecutionCandidate {
        EligibleLocalExecutionCandidate {
            kind: LocalExecutionCandidateKind::PoolGroup,
            candidate: SchedulerMinimalCandidateSelectionCandidate {
                provider_id: provider_id.to_string(),
                provider_name: provider_id.to_string(),
                provider_type: "codex".to_string(),
                provider_priority,
                endpoint_id: endpoint_id.to_string(),
                endpoint_api_format: "openai:responses".to_string(),
                key_id: format!("{provider_id}-pool-group"),
                key_name: format!("{provider_id}-pool-group"),
                key_auth_type: "oauth".to_string(),
                key_internal_priority: 10,
                key_global_priority_for_format: Some(1),
                key_capabilities: None,
                routing_facts: Default::default(),
                model_id: "model-1".to_string(),
                global_model_id: "global-model-1".to_string(),
                global_model_name: "gpt-5".to_string(),
                selected_provider_model_name: "gpt-5".to_string(),
                supports_streaming: true,
                mapping_matched_model: None,
            },
            provider_api_format: "openai:responses".to_string(),
            orchestration: LocalExecutionCandidateMetadata::default(),
            ranking: None,
            transport: Arc::new(crate::ai_serving::GatewayProviderTransportSnapshot {
                provider: GatewayProviderTransportProvider {
                    id: provider_id.to_string(),
                    name: provider_id.to_string(),
                    provider_type: "codex".to_string(),
                    website: None,
                    is_active: true,
                    keep_priority_on_conversion: false,
                    enable_format_conversion: false,
                    concurrent_limit: None,
                    max_retries: None,
                    proxy: None,
                    request_timeout_secs: None,
                    stream_first_byte_timeout_secs: None,
                    config: provider_config,
                },
                endpoint: GatewayProviderTransportEndpoint {
                    id: endpoint_id.to_string(),
                    provider_id: provider_id.to_string(),
                    api_format: "openai:responses".to_string(),
                    api_family: Some("openai".to_string()),
                    endpoint_kind: Some("responses".to_string()),
                    is_active: true,
                    base_url: "https://example.com/v1/responses".to_string(),
                    header_rules: None,
                    body_rules: None,
                    max_retries: None,
                    custom_path: None,
                    config: None,
                    format_acceptance_config: None,
                    proxy: None,
                },
                key: GatewayProviderTransportKey {
                    id: format!("{provider_id}-pool-group"),
                    provider_id: provider_id.to_string(),
                    name: format!("{provider_id}-pool-group"),
                    auth_type: "oauth".to_string(),
                    is_active: true,
                    api_formats: Some(vec!["openai:responses".to_string()]),
                    auth_type_by_format: None,
                    allow_auth_channel_mismatch_formats: None,
                    allowed_models: None,
                    capabilities: None,
                    rate_multipliers: None,
                    global_priority_by_format: None,
                    expires_at_unix_secs: None,
                    proxy: None,
                    fingerprint: None,
                    upstream_metadata: None,
                    decrypted_api_key: "secret".to_string(),
                    decrypted_auth_config: None,
                },
            }),
        }
    }

    /// 构造仅允许指定 Key 的路由策略，供 Pool 过滤与排序测试复用。
    fn routing_policy_with_allowed_keys<const N: usize>(
        key_ids: [&str; N],
    ) -> ResolvedRoutingPolicy {
        ResolvedRoutingPolicy {
            billing_multiplier: 1.0,
            group_name: None,
            group_id: Some("routing-group-1".to_string()),
            group_version: Some(1),
            selection_source: "test".to_string(),
            requested_model: "gpt-5".to_string(),
            resolved_model: "gpt-5".to_string(),
            priority_mode: RoutingSetPriorityMode::Provider,
            scheduling_mode: RoutingSchedulingMode::CacheAffinity,
            keep_priority_on_conversion: false,
            sticky_key_attempts: aether_routing_core::DEFAULT_STICKY_KEY_ATTEMPTS,
            execution_policy: Default::default(),
            ranking_overlay: RankingOverlay {
                allowed_keys: key_ids.into_iter().map(str::to_string).collect(),
                ..RankingOverlay::default()
            },
            mutation_plan: Default::default(),
            pool_policy_overrides: BTreeMap::new(),
            matched_rules: Vec::new(),
        }
    }

    fn sample_eligible_candidate(
        provider_id: &str,
        endpoint_id: &str,
        key_id: &str,
        internal_priority: i32,
        provider_config: Option<serde_json::Value>,
    ) -> EligibleLocalExecutionCandidate {
        EligibleLocalExecutionCandidate {
            kind: if provider_config.is_some() {
                LocalExecutionCandidateKind::PoolGroup
            } else {
                LocalExecutionCandidateKind::SingleKey
            },
            candidate: SchedulerMinimalCandidateSelectionCandidate {
                provider_id: provider_id.to_string(),
                provider_name: provider_id.to_string(),
                provider_type: "codex".to_string(),
                provider_priority: 10,
                endpoint_id: endpoint_id.to_string(),
                endpoint_api_format: "openai:chat".to_string(),
                key_id: key_id.to_string(),
                key_name: key_id.to_string(),
                key_auth_type: "api_key".to_string(),
                key_internal_priority: internal_priority,
                key_global_priority_for_format: Some(1),
                key_capabilities: None,
                routing_facts: Default::default(),
                model_id: "model-1".to_string(),
                global_model_id: "global-model-1".to_string(),
                global_model_name: "gpt-5".to_string(),
                selected_provider_model_name: "gpt-5".to_string(),
                supports_streaming: true,
                mapping_matched_model: None,
            },
            provider_api_format: "openai:chat".to_string(),
            orchestration: LocalExecutionCandidateMetadata::default(),
            ranking: None,
            transport: Arc::new(crate::ai_serving::GatewayProviderTransportSnapshot {
                provider: GatewayProviderTransportProvider {
                    id: provider_id.to_string(),
                    name: provider_id.to_string(),
                    provider_type: "codex".to_string(),
                    website: None,
                    is_active: true,
                    keep_priority_on_conversion: false,
                    enable_format_conversion: false,
                    concurrent_limit: None,
                    max_retries: None,
                    proxy: None,
                    request_timeout_secs: None,
                    stream_first_byte_timeout_secs: None,
                    config: provider_config,
                },
                endpoint: GatewayProviderTransportEndpoint {
                    id: endpoint_id.to_string(),
                    provider_id: provider_id.to_string(),
                    api_format: "openai:chat".to_string(),
                    api_family: Some("openai".to_string()),
                    endpoint_kind: Some("chat".to_string()),
                    is_active: true,
                    base_url: "https://example.com".to_string(),
                    header_rules: None,
                    body_rules: None,
                    max_retries: None,
                    custom_path: None,
                    config: None,
                    format_acceptance_config: None,
                    proxy: None,
                },
                key: GatewayProviderTransportKey {
                    id: key_id.to_string(),
                    provider_id: provider_id.to_string(),
                    name: key_id.to_string(),
                    auth_type: "api_key".to_string(),
                    is_active: true,
                    api_formats: Some(vec!["openai:chat".to_string()]),
                    auth_type_by_format: None,
                    allow_auth_channel_mismatch_formats: None,

                    allowed_models: None,
                    capabilities: None,
                    rate_multipliers: None,
                    global_priority_by_format: None,
                    expires_at_unix_secs: None,
                    proxy: None,
                    fingerprint: None,
                    upstream_metadata: None,
                    decrypted_api_key: "secret".to_string(),
                    decrypted_auth_config: None,
                },
            }),
        }
    }

    #[tokio::test]
    async fn codex_model_discovery_pool_key_cursor_filters_pages_sticky_and_frozen_window() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "probing_enabled": true,
                "skip_exhausted_accounts": true,
                "scheduling_presets": [{
                    "preset": "cache_affinity",
                    "enabled": true,
                    "mode": "single_account"
                }]
            }
        }));
        let provider = sample_codex_pool_provider("provider-pool", 0, provider_config.clone());
        let endpoint = sample_codex_pool_endpoint("provider-pool", "endpoint-1");

        let mut free_key = sample_codex_pool_key("provider-pool", "key-free");
        free_key.internal_priority = 0;
        free_key.allowed_models = Some(json!(["gpt-5", "gpt-basic-test"]));
        free_key.upstream_metadata = Some(json!({
            "codex_model_discovery": {
                "model_ids": ["gpt-basic-test"]
            }
        }));

        let mut missing_key = sample_codex_pool_key("provider-pool", "key-missing");
        missing_key.internal_priority = 5;
        missing_key.upstream_metadata = None;

        let mut paid_key = sample_codex_pool_key("provider-pool", "key-paid");
        paid_key.internal_priority = 10;
        paid_key.upstream_metadata = Some(json!({
            "codex_model_discovery": {
                "model_ids": ["gpt-basic-test", "gpt-5"]
            }
        }));

        let mut rows = Vec::new();
        for (key_id, priority) in [("key-free", 0), ("key-missing", 5), ("key-paid", 10)] {
            let mut adv_row = sample_codex_pool_row("provider-pool", "endpoint-1", key_id, 0);
            adv_row.key_internal_priority = priority;
            adv_row.routing_facts.provider_pool_enabled = true;
            rows.push(adv_row);

            let mut basic_row = sample_codex_pool_row("provider-pool", "endpoint-1", key_id, 0);
            basic_row.key_internal_priority = priority;
            basic_row.routing_facts.provider_pool_enabled = true;
            basic_row.model_id = "model-basic".to_string();
            basic_row.global_model_id = "global-model-basic".to_string();
            basic_row.global_model_name = "gpt-basic-test".to_string();
            basic_row.model_provider_model_name = "gpt-basic-test".to_string();
            rows.push(basic_row);
        }

        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    vec![free_key, missing_key, paid_key],
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
            )
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);

        let active_set_key = admin_provider_pool_quota_probe_active_members_key("provider-pool");
        app.runtime_state
            .set_add(&active_set_key, "key-free")
            .await
            .expect("active probe member should insert");

        let group = sample_codex_pool_group("provider-pool", "endpoint-1", 0, provider_config);
        let pool_config = pool_config_for_candidate(&group).expect("pool config should parse");
        record_admin_provider_pool_success(
            app.runtime_state.as_ref(),
            "provider-pool",
            "key-free",
            &pool_config,
            Some("codex-discovery-session"),
            0,
            None,
        )
        .await;

        for page_size in [1, 3] {
            for sticky in [false, true] {
                let sticky_token = sticky.then_some("codex-discovery-session");
                let mut cursor = PoolKeyCursor::new(
                    PlannerAppState::new(&app),
                    group.clone(),
                    sticky_token,
                    Some("gpt-5"),
                    None,
                );
                cursor.window_size = 1;
                cursor.page_size = page_size;
                let mut returned = Vec::new();
                while let Some(candidate) = cursor.next_key().await {
                    returned.push(candidate.candidate.key_id);
                }
                assert_eq!(
                    returned,
                    vec!["key-paid".to_string()],
                    "page_size={page_size}, sticky={sticky}"
                );
                assert!(
                    cursor
                        .skip_reason_counts
                        .get("key_model_disabled")
                        .copied()
                        .unwrap_or(0)
                        >= 2,
                    "page_size={page_size}, sticky={sticky}"
                );
            }
        }

        let active_members = app
            .runtime_state
            .set_members(&active_set_key)
            .await
            .expect("active probe set should read");
        assert!(active_members.contains(&"key-free".to_string()));

        let mut basic_group = group;
        basic_group.candidate.model_id = "model-basic".to_string();
        basic_group.candidate.global_model_id = "global-model-basic".to_string();
        basic_group.candidate.global_model_name = "gpt-basic-test".to_string();
        basic_group.candidate.selected_provider_model_name = "gpt-basic-test".to_string();
        let mut basic_cursor = PoolKeyCursor::new(
            PlannerAppState::new(&app),
            basic_group,
            Some("codex-discovery-session"),
            Some("gpt-basic-test"),
            None,
        );
        basic_cursor.window_size = 1;
        basic_cursor.page_size = 1;
        let first_basic = basic_cursor
            .next_key()
            .await
            .expect("free account should still serve basic model");
        assert_eq!(first_basic.candidate.key_id, "key-free");
    }

    #[tokio::test]
    async fn codex_model_discovery_pool_page_strong_read_falls_back_from_hot_and_keeps_priority() {
        let provider_config = Some(json!({
            "pool_advanced": {
                "probing_enabled": true,
                "skip_exhausted_accounts": true,
                "scheduling_presets": [
                    {"preset": "single_account", "enabled": true}
                ]
            }
        }));
        let provider = sample_codex_pool_provider("provider-pool", 0, provider_config.clone());
        let endpoint = sample_codex_pool_endpoint("provider-pool", "endpoint-1");

        let basic_only_discovery = json!({
            "codex_model_discovery": {
                "model_ids": ["gpt-basic-test"]
            }
        });
        let mut key_oauth_blocked = sample_codex_pool_key("provider-pool", "key-a-oauth-blocked");
        key_oauth_blocked.internal_priority = 0;
        key_oauth_blocked.oauth_invalid_at_unix_secs = Some(1_710_000_000);
        key_oauth_blocked.oauth_invalid_reason =
            Some("[OAUTH_EXPIRED] token invalidated".to_string());
        key_oauth_blocked.upstream_metadata = Some(basic_only_discovery.clone());

        let mut key_exhausted = sample_codex_pool_key("provider-pool", "key-b-exhausted");
        key_exhausted.internal_priority = 1;
        key_exhausted.status_snapshot = Some(json!({
            "quota": {
                "provider_type": "codex",
                "exhausted": true,
                "usage_ratio": 1.0,
                "windows": [{
                    "code": "daily",
                    "used_ratio": 1.0,
                    "remaining_ratio": 0.0
                }]
            }
        }));
        key_exhausted.upstream_metadata = Some(basic_only_discovery.clone());

        let mut key_hot_free = sample_codex_pool_key("provider-pool", "key-c-hot-free");
        key_hot_free.internal_priority = 2;
        key_hot_free.upstream_metadata = Some(basic_only_discovery);

        let mut key_cold_paid = sample_codex_pool_key("provider-pool", "key-d-cold-paid");
        key_cold_paid.internal_priority = 3;
        key_cold_paid.upstream_metadata = Some(json!({
            "codex_model_discovery": {
                "model_ids": ["gpt-basic-test", "gpt-5"]
            }
        }));

        let data_state =
            GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
                Arc::new(InMemoryProviderCatalogReadRepository::seed(
                    vec![provider],
                    vec![endpoint],
                    vec![
                        key_oauth_blocked,
                        key_exhausted,
                        key_hot_free,
                        key_cold_paid,
                    ],
                )),
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(
                    Vec::new(),
                )),
            )
            .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(data_state);

        let active_set_key = admin_provider_pool_quota_probe_active_members_key("provider-pool");
        app.runtime_state
            .set_add(&active_set_key, "key-c-hot-free")
            .await
            .expect("hot free member should insert");

        let page_candidates = [
            ("key-a-oauth-blocked", 0),
            ("key-b-exhausted", 1),
            ("key-c-hot-free", 2),
            ("key-d-cold-paid", 3),
        ]
        .into_iter()
        .map(|(key_id, priority)| {
            let mut candidate =
                sample_codex_pool_group("provider-pool", "endpoint-1", 0, provider_config.clone());
            candidate.kind = LocalExecutionCandidateKind::SingleKey;
            candidate.candidate.key_id = key_id.to_string();
            candidate.candidate.key_name = key_id.to_string();
            candidate.candidate.key_internal_priority = priority;
            let transport = Arc::make_mut(&mut candidate.transport);
            transport.key.id = key_id.to_string();
            transport.key.name = key_id.to_string();
            transport.key.upstream_metadata = Some(json!({
                "codex_model_discovery": {
                    "model_ids": ["gpt-basic-test", "gpt-5"]
                }
            }));
            candidate
        })
        .collect::<Vec<_>>();
        let pool_config =
            pool_config_for_candidate(&page_candidates[0]).expect("pool config should parse");

        let (scheduled, skipped) = super::schedule_pool_page_candidates(
            PlannerAppState::new(&app),
            page_candidates,
            None,
            Some(&pool_config),
            Some("gpt-5"),
            None,
        )
        .await;

        assert_eq!(
            scheduled
                .iter()
                .map(|item| item.candidate.key_id.as_str())
                .collect::<Vec<_>>(),
            vec!["key-d-cold-paid"]
        );
        let skipped_pairs = skipped
            .iter()
            .map(|item| (item.candidate.key_id.as_str(), item.skip_reason))
            .collect::<Vec<_>>();
        assert!(skipped_pairs.contains(&("key-a-oauth-blocked", "pool_account_blocked")));
        assert!(skipped_pairs.contains(&("key-b-exhausted", POOL_ACCOUNT_EXHAUSTED_SKIP_REASON)));
        assert!(skipped_pairs.contains(&("key-c-hot-free", "key_model_disabled")));

        let active_members = app
            .runtime_state
            .set_members(&active_set_key)
            .await
            .expect("active probe set should read");
        assert!(active_members.contains(&"key-c-hot-free".to_string()));
    }
}
