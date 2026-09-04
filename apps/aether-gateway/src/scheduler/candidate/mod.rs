use self::selection::{
    collect_selectable_candidates, collect_selectable_candidates_with_skip_reasons_and_ordering,
    collect_selectable_enumerated_candidates_with_skip_reasons,
    resolve_preselection_ordering_config,
};
use super::config::SchedulerOrderingConfig;
use super::state::SchedulerRuntimeState;

mod affinity;
mod enumeration;
mod ranking;
mod resolution;
mod runtime;
mod selection;

#[cfg(test)]
mod tests;

use aether_data_contracts::repository::candidate_selection::{
    StoredMinimalCandidateSelectionRow, StoredProviderModelMapping,
};
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use aether_data_contracts::repository::quota::StoredProviderQuotaSnapshot;
use aether_scheduler_core::{
    candidate_model_names, candidate_supports_required_capability, matches_model_mapping,
    normalize_api_format, resolve_provider_model_name, select_provider_model_name,
    ClientSessionAffinity, SchedulerMinimalCandidateSelectionCandidate,
};
use aether_wallet::{ProviderBillingType, ProviderQuotaSnapshot};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(crate) use self::selection::{
    is_auth_api_key_concurrency_limit_skip_reason, SchedulerSkippedCandidate,
    API_KEY_CONCURRENCY_LIMIT_SKIP_REASON, AUTH_API_KEY_CONCURRENCY_LIMIT_SKIP_REASON,
    LEGACY_API_KEY_CONCURRENCY_LIMIT_SKIP_REASON,
};

use crate::data::auth::GatewayAuthApiKeySnapshot;
use crate::data::candidate_selection::{
    read_global_model_names_for_api_format, read_global_model_names_for_required_capability,
    MinimalCandidateSelectionRowSource,
};
use crate::GatewayError;

#[cfg_attr(not(test), allow(dead_code))]
const SCHEDULER_AFFINITY_MAX_ENTRIES: usize = 10_000;

/// 候选调度一次排序批次的时间与分布上下文，避免两个同型整数在跨层调用时互换语义。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CandidateSchedulingContext {
    /// 运行态资格判断使用的真实 Unix 秒；熔断、RPM、并发和凭据时效只读取该值。
    pub(crate) now_unix_secs: u64,
    /// 当前排序批次固定使用的分布种子；不得作为任何运行态时间来源。
    pub(crate) load_balance_seed: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequiredCapabilityMatchMode {
    Compatible,
    Exclusive,
}

/// 使用具名时间/种子上下文列出可用候选；请求排序配置为空时读取运行态默认策略。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn list_selectable_candidates(
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
    collect_selectable_candidates(
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
        ordering_config,
    )
    .await
}

pub(crate) fn is_exact_all_skipped_by_auth_limit(
    selected: &[SchedulerMinimalCandidateSelectionCandidate],
    skipped: &[SchedulerSkippedCandidate],
) -> bool {
    selection::is_exact_all_skipped_by_auth_limit(selected, skipped)
}

/// 使用具名时间/种子上下文列出候选及跳过原因，并沿用请求级或系统默认排序配置。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn list_selectable_candidates_with_skip_reasons(
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
        None,
        ordering_config,
    )
    .await
}

/// 按具体请求操作和具名调度上下文列出候选及跳过原因，保留 Endpoint 能力隔离。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn list_selectable_candidates_with_skip_reasons_for_request_operation(
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
        ordering_config,
    )
    .await
}

/// 对已枚举候选统一应用权限、亲和和具名时间/种子上下文。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn list_selectable_enumerated_candidates_with_skip_reasons(
    runtime_state: &impl SchedulerRuntimeState,
    api_format: &str,
    global_model_name: &str,
    candidates: Vec<SchedulerMinimalCandidateSelectionCandidate>,
    required_capabilities: Option<&serde_json::Value>,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    scheduling_context: CandidateSchedulingContext,
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
    let priority_affinity_key = selection::scheduling_priority_affinity_key(
        auth_snapshot,
        client_session_affinity,
        ordering_config.scheduling_mode,
    );
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

/// 在无请求模型名时按必需能力和具名调度上下文选择候选，并返回可用列表。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn list_selectable_candidates_for_required_capability_without_requested_model(
    selection_row_source: &(impl MinimalCandidateSelectionRowSource + Sync),
    runtime_state: &impl SchedulerRuntimeState,
    candidate_api_format: &str,
    required_capability: &str,
    require_streaming: bool,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    scheduling_context: CandidateSchedulingContext,
    ordering_config: Option<SchedulerOrderingConfig>,
) -> Result<Vec<SchedulerMinimalCandidateSelectionCandidate>, GatewayError> {
    Ok(
        list_selectable_candidates_for_required_capability_without_requested_model_with_auth_limit_signal(
            selection_row_source,
            runtime_state,
            candidate_api_format,
            required_capability,
            require_streaming,
            auth_snapshot,
            client_session_affinity,
            scheduling_context,
            ordering_config,
        )
        .await?
        .0,
    )
}

/// 按必需能力和具名调度上下文选择候选，并额外报告是否全部受 API Key 并发额度阻断。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn list_selectable_candidates_for_required_capability_without_requested_model_with_auth_limit_signal(
    selection_row_source: &(impl MinimalCandidateSelectionRowSource + Sync),
    runtime_state: &impl SchedulerRuntimeState,
    candidate_api_format: &str,
    required_capability: &str,
    require_streaming: bool,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    scheduling_context: CandidateSchedulingContext,
    ordering_config: Option<SchedulerOrderingConfig>,
) -> Result<(Vec<SchedulerMinimalCandidateSelectionCandidate>, bool), GatewayError> {
    let normalized_api_format = normalize_api_format(candidate_api_format);
    if normalized_api_format.is_empty() {
        return Ok((Vec::new(), false));
    }

    let capability_mode = required_capability_match_mode(required_capability);
    let model_names = match capability_mode {
        RequiredCapabilityMatchMode::Exclusive => {
            read_global_model_names_for_required_capability(
                selection_row_source,
                &normalized_api_format,
                required_capability,
                require_streaming,
                auth_snapshot,
            )
            .await
        }
        RequiredCapabilityMatchMode::Compatible => {
            read_global_model_names_for_api_format(
                selection_row_source,
                &normalized_api_format,
                require_streaming,
                auth_snapshot,
            )
            .await
        }
    }
    .map_err(|err| GatewayError::Internal(err.to_string()))?;
    let required_capabilities = build_required_capabilities_object(required_capability);
    let mut all_attempts_blocked_by_auth_limit = !model_names.is_empty();

    for global_model_name in model_names {
        let (candidates, skipped_candidates) =
            collect_selectable_candidates_with_skip_reasons_and_ordering(
                selection_row_source,
                runtime_state,
                &normalized_api_format,
                &global_model_name,
                require_streaming,
                required_capabilities.as_ref(),
                auth_snapshot,
                client_session_affinity,
                scheduling_context,
                false,
                None,
                ordering_config,
            )
            .await?;
        all_attempts_blocked_by_auth_limit &=
            is_exact_all_skipped_by_auth_limit(&candidates, &skipped_candidates);
        match capability_mode {
            RequiredCapabilityMatchMode::Exclusive => {
                let filtered = candidates
                    .into_iter()
                    .filter(|candidate| {
                        candidate_supports_required_capability(candidate, required_capability)
                    })
                    .collect::<Vec<_>>();
                if !filtered.is_empty() {
                    return Ok((filtered, false));
                }
            }
            RequiredCapabilityMatchMode::Compatible => {
                if candidates.is_empty() {
                    continue;
                }
                return Ok((candidates, false));
            }
        }
    }

    Ok((Vec::new(), all_attempts_blocked_by_auth_limit))
}

fn required_capability_match_mode(required_capability: &str) -> RequiredCapabilityMatchMode {
    match required_capability.trim().to_ascii_lowercase().as_str() {
        "cache_1h" | "context_1m" => RequiredCapabilityMatchMode::Compatible,
        _ => RequiredCapabilityMatchMode::Exclusive,
    }
}

fn build_required_capabilities_object(required_capability: &str) -> Option<serde_json::Value> {
    let required_capability = required_capability.trim();
    if required_capability.is_empty() {
        return None;
    }

    let mut capabilities = serde_json::Map::new();
    capabilities.insert(
        required_capability.to_string(),
        serde_json::Value::Bool(true),
    );
    Some(serde_json::Value::Object(capabilities))
}
