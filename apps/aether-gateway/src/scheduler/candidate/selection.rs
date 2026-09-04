use crate::data::auth::GatewayAuthApiKeySnapshot;
use crate::data::candidate_selection::MinimalCandidateSelectionRowSource;
use crate::scheduler::affinity::SCHEDULER_AFFINITY_TTL;
use crate::scheduler::config::{SchedulerOrderingConfig, SchedulerSchedulingMode};
use crate::GatewayError;
use aether_scheduler_core::ClientSessionAffinity;

use super::affinity::{
    build_scheduler_affinity_cache_key, has_explicit_session_affinity, remember_scheduler_affinity,
};
use super::enumeration::enumerate_scheduler_candidates;
use super::ranking::rank_scheduler_candidates;
use super::resolution::resolve_scheduler_candidate_selectability;
use super::runtime::{
    auth_snapshot_concurrency_limit_reached, read_candidate_runtime_selection_snapshot,
};
use super::{
    CandidateSchedulingContext, SchedulerMinimalCandidateSelectionCandidate, SchedulerRuntimeState,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SchedulerSkippedCandidate {
    pub(crate) candidate: SchedulerMinimalCandidateSelectionCandidate,
    pub(crate) skip_reason: &'static str,
}

pub(crate) const AUTH_API_KEY_CONCURRENCY_LIMIT_SKIP_REASON: &str =
    "auth_api_key_concurrency_limit_reached";
pub(crate) const LEGACY_API_KEY_CONCURRENCY_LIMIT_SKIP_REASON: &str =
    "api_key_concurrency_limit_reached";
pub(crate) const API_KEY_CONCURRENCY_LIMIT_SKIP_REASON: &str =
    AUTH_API_KEY_CONCURRENCY_LIMIT_SKIP_REASON;

pub(crate) fn is_auth_api_key_concurrency_limit_skip_reason(reason: &str) -> bool {
    matches!(
        reason.trim(),
        AUTH_API_KEY_CONCURRENCY_LIMIT_SKIP_REASON | LEGACY_API_KEY_CONCURRENCY_LIMIT_SKIP_REASON
    )
}

pub(super) fn is_exact_all_skipped_by_auth_limit(
    selected: &[SchedulerMinimalCandidateSelectionCandidate],
    skipped: &[SchedulerSkippedCandidate],
) -> bool {
    selected.is_empty()
        && !skipped.is_empty()
        && skipped
            .iter()
            .all(|candidate| is_auth_api_key_concurrency_limit_skip_reason(candidate.skip_reason))
}

#[cfg_attr(not(test), allow(dead_code))]
/// 使用同一具名上下文过滤运行态并排序一个模型的候选，避免时间与分布种子错位。
pub(super) async fn select_minimal_candidate(
    selection_row_source: &(impl MinimalCandidateSelectionRowSource + Sync),
    runtime_state: &impl SchedulerRuntimeState,
    api_format: &str,
    global_model_name: &str,
    require_streaming: bool,
    required_capabilities: Option<&serde_json::Value>,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    scheduling_context: CandidateSchedulingContext,
    enable_model_directives: bool,
) -> Result<Option<SchedulerMinimalCandidateSelectionCandidate>, GatewayError> {
    let affinity_epoch = runtime_state.scheduler_affinity_epoch();
    let ordering_config = runtime_state.read_scheduler_ordering_config().await?;
    let affinity_cache_key = build_scheduler_affinity_cache_key(
        auth_snapshot,
        api_format,
        global_model_name,
        client_session_affinity,
    );
    let priority_affinity_key = scheduling_priority_affinity_key(
        auth_snapshot,
        client_session_affinity,
        ordering_config.scheduling_mode,
    );
    let candidates = enumerate_scheduler_candidates(
        selection_row_source,
        api_format,
        global_model_name,
        require_streaming,
        required_capabilities,
        auth_snapshot,
        enable_model_directives,
        None,
    )
    .await?;
    let selected = collect_selectable_enumerated_candidates_with_skip_reasons(
        runtime_state,
        api_format,
        global_model_name,
        candidates,
        required_capabilities,
        auth_snapshot,
        client_session_affinity,
        scheduling_context,
        ordering_config,
        priority_affinity_key,
    )
    .await?
    .0
    .into_iter()
    .next();
    if ordering_config.scheduling_mode == SchedulerSchedulingMode::CacheAffinity
        && has_explicit_session_affinity(client_session_affinity)
    {
        if let Some(candidate) = selected.as_ref() {
            remember_scheduler_affinity(
                affinity_cache_key.as_deref(),
                runtime_state,
                candidate,
                Some(affinity_epoch),
            );
        }
    }
    Ok(selected)
}

/// 使用具名时间/种子上下文收集并排序指定模型的可用候选，不返回跳过明细。
#[allow(clippy::too_many_arguments)]
pub(super) async fn collect_selectable_candidates(
    selection_row_source: &(impl MinimalCandidateSelectionRowSource + Sync),
    runtime_state: &impl SchedulerRuntimeState,
    api_format: &str,
    global_model_name: &str,
    require_streaming: bool,
    required_capabilities: Option<&serde_json::Value>,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    scheduling_context: CandidateSchedulingContext,
    enable_model_directives: bool,
    ordering_config: Option<SchedulerOrderingConfig>,
) -> Result<Vec<SchedulerMinimalCandidateSelectionCandidate>, GatewayError> {
    Ok(
        collect_selectable_candidates_with_skip_reasons_and_ordering(
            selection_row_source,
            runtime_state,
            api_format,
            global_model_name,
            require_streaming,
            required_capabilities,
            auth_snapshot,
            client_session_affinity,
            scheduling_context,
            enable_model_directives,
            None,
            ordering_config,
        )
        .await?
        .0,
    )
}

/// 使用具名调度上下文收集跳过原因；未显式提供请求策略时读取运行态默认配置。
#[allow(clippy::too_many_arguments)]
pub(super) async fn collect_selectable_candidates_with_skip_reasons(
    selection_row_source: &(impl MinimalCandidateSelectionRowSource + Sync),
    runtime_state: &impl SchedulerRuntimeState,
    api_format: &str,
    global_model_name: &str,
    require_streaming: bool,
    required_capabilities: Option<&serde_json::Value>,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    scheduling_context: CandidateSchedulingContext,
    enable_model_directives: bool,
    request_operation: Option<&str>,
) -> Result<
    (
        Vec<SchedulerMinimalCandidateSelectionCandidate>,
        Vec<SchedulerSkippedCandidate>,
    ),
    GatewayError,
> {
    collect_selectable_candidates_with_skip_reasons_and_ordering(
        selection_row_source,
        runtime_state,
        api_format,
        global_model_name,
        require_streaming,
        required_capabilities,
        auth_snapshot,
        client_session_affinity,
        scheduling_context,
        enable_model_directives,
        request_operation,
        None,
    )
    .await
}

/// 解析预选排序配置：请求级路由策略优先，否则依次使用系统默认组和旧系统配置。
pub(super) async fn resolve_preselection_ordering_config(
    runtime_state: &impl SchedulerRuntimeState,
    ordering_config: Option<SchedulerOrderingConfig>,
) -> Result<SchedulerOrderingConfig, GatewayError> {
    match ordering_config {
        Some(config) => Ok(config),
        None => runtime_state.read_scheduler_ordering_config().await,
    }
}

/// 按显式或运行态排序配置及具名调度上下文选择候选，并返回每个跳过原因。
#[allow(clippy::too_many_arguments)]
pub(super) async fn collect_selectable_candidates_with_skip_reasons_and_ordering(
    selection_row_source: &(impl MinimalCandidateSelectionRowSource + Sync),
    runtime_state: &impl SchedulerRuntimeState,
    api_format: &str,
    global_model_name: &str,
    require_streaming: bool,
    required_capabilities: Option<&serde_json::Value>,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    scheduling_context: CandidateSchedulingContext,
    enable_model_directives: bool,
    request_operation: Option<&str>,
    ordering_config: Option<SchedulerOrderingConfig>,
) -> Result<
    (
        Vec<SchedulerMinimalCandidateSelectionCandidate>,
        Vec<SchedulerSkippedCandidate>,
    ),
    GatewayError,
> {
    let ordering_config =
        resolve_preselection_ordering_config(runtime_state, ordering_config).await?;
    let priority_affinity_key = scheduling_priority_affinity_key(
        auth_snapshot,
        client_session_affinity,
        ordering_config.scheduling_mode,
    );
    let candidates = enumerate_scheduler_candidates(
        selection_row_source,
        api_format,
        global_model_name,
        require_streaming,
        required_capabilities,
        auth_snapshot,
        enable_model_directives,
        request_operation,
    )
    .await?;
    collect_selectable_enumerated_candidates_with_skip_reasons(
        runtime_state,
        api_format,
        global_model_name,
        candidates,
        required_capabilities,
        auth_snapshot,
        client_session_affinity,
        scheduling_context,
        ordering_config,
        priority_affinity_key,
    )
    .await
}

/// 对已枚举候选以真实时间过滤、以独立种子排序并记录亲和，最后返回跳过明细。
#[allow(clippy::too_many_arguments)]
pub(super) async fn collect_selectable_enumerated_candidates_with_skip_reasons(
    runtime_state: &impl SchedulerRuntimeState,
    api_format: &str,
    global_model_name: &str,
    mut candidates: Vec<SchedulerMinimalCandidateSelectionCandidate>,
    required_capabilities: Option<&serde_json::Value>,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    scheduling_context: CandidateSchedulingContext,
    ordering_config: SchedulerOrderingConfig,
    priority_affinity_key: Option<&str>,
) -> Result<
    (
        Vec<SchedulerMinimalCandidateSelectionCandidate>,
        Vec<SchedulerSkippedCandidate>,
    ),
    GatewayError,
> {
    let runtime_snapshot = read_candidate_runtime_selection_snapshot(
        runtime_state,
        &candidates,
        auth_snapshot,
        scheduling_context.now_unix_secs,
    )
    .await?;
    let affinity_cache_key = build_scheduler_affinity_cache_key(
        auth_snapshot,
        api_format,
        global_model_name,
        client_session_affinity,
    );
    let cached_affinity_target = if ordering_config.scheduling_mode
        == SchedulerSchedulingMode::CacheAffinity
        && has_explicit_session_affinity(client_session_affinity)
    {
        affinity_cache_key.as_deref().and_then(|cache_key| {
            runtime_state.read_cached_scheduler_affinity_target(cache_key, SCHEDULER_AFFINITY_TTL)
        })
    } else {
        None
    };

    if auth_snapshot_concurrency_limit_reached(
        auth_snapshot,
        &runtime_snapshot,
        scheduling_context.now_unix_secs,
    ) {
        rank_scheduler_candidates(
            &mut candidates,
            &runtime_snapshot,
            ordering_config,
            required_capabilities,
            priority_affinity_key,
            cached_affinity_target.as_ref(),
            scheduling_context.load_balance_seed,
        );
        return Ok((
            Vec::new(),
            candidates
                .into_iter()
                .map(|candidate| SchedulerSkippedCandidate {
                    candidate,
                    skip_reason: API_KEY_CONCURRENCY_LIMIT_SKIP_REASON,
                })
                .collect(),
        ));
    }

    let (mut selected, skipped) = resolve_scheduler_candidate_selectability(
        candidates,
        &runtime_snapshot,
        scheduling_context.now_unix_secs,
    );
    rank_scheduler_candidates(
        &mut selected,
        &runtime_snapshot,
        ordering_config,
        required_capabilities,
        priority_affinity_key,
        cached_affinity_target.as_ref(),
        scheduling_context.load_balance_seed,
    );

    Ok((selected, skipped))
}

pub(super) fn scheduling_priority_affinity_key<'a>(
    auth_snapshot: Option<&'a GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    scheduling_mode: SchedulerSchedulingMode,
) -> Option<&'a str> {
    if scheduling_mode == SchedulerSchedulingMode::FixedOrder {
        return None;
    }
    if scheduling_mode == SchedulerSchedulingMode::CacheAffinity
        && !has_explicit_session_affinity(client_session_affinity)
    {
        return None;
    }

    auth_snapshot
        .map(|snapshot| snapshot.api_key_id.trim())
        .filter(|value| !value.is_empty())
}
