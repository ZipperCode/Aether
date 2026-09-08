use std::sync::Arc;

use aether_ai_serving::{
    run_ai_candidate_resolution, AiCandidateResolutionMode, AiCandidateResolutionPort,
    AiCandidateResolutionRequest,
};
use aether_data_contracts::repository::candidate_selection::{
    StoredCandidateProxyAffinitySource, StoredMinimalCandidateRoutingFacts,
};
use aether_routing_core::ResolvedRoutingPolicy;
use async_trait::async_trait;
use std::convert::Infallible;
use std::time::Instant;
use tracing::warn;

use aether_scheduler_core::{
    ClientSessionAffinity, SchedulerMinimalCandidateSelectionCandidate, SchedulerRankingOutcome,
};

use crate::ai_serving::transport::provider_types::provider_runtime_policy;
use crate::ai_serving::{
    candidate_common_transport_skip_reason, candidate_transport_pair_skip_reason,
    CandidateTransportPolicyFacts, GatewayAuthApiKeySnapshot, GatewayProviderTransportSnapshot,
    PlannerAppState,
};
use crate::orchestration::LocalExecutionCandidateMetadata;
use crate::stage_metrics::observe_gateway_stage_ms;

use super::candidate_ranking::{
    rank_eligible_local_execution_candidates, rank_local_candidate_snapshots,
    scheduler_ordering_config_for_routing_policy,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EligibleLocalExecutionCandidate {
    pub(crate) kind: LocalExecutionCandidateKind,
    pub(crate) candidate: SchedulerMinimalCandidateSelectionCandidate,
    pub(crate) transport: Arc<GatewayProviderTransportSnapshot>,
    pub(crate) provider_api_format: String,
    pub(crate) orchestration: LocalExecutionCandidateMetadata,
    pub(crate) ranking: Option<SchedulerRankingOutcome>,
}

#[derive(Debug, Clone, PartialEq)]
/// 尚未读取 transport 的全局排序候选；分页缓存与路由聚合只能保存此类型。
pub(crate) struct RankedLocalExecutionCandidate {
    /// 由轻量 Provider Pool 标志确定的逻辑候选类型。
    pub(crate) kind: LocalExecutionCandidateKind,
    /// 数据层输出的最小候选及无凭据排序事实。
    pub(crate) candidate: SchedulerMinimalCandidateSelectionCandidate,
    /// 标准化后的 Endpoint 格式，供跨格式排序使用。
    pub(crate) provider_api_format: String,
    /// 排序后附加的编排元数据；不包含请求体或 transport。
    pub(crate) orchestration: LocalExecutionCandidateMetadata,
    /// 调度核心生成的全局排名诊断。
    pub(crate) ranking: Option<SchedulerRankingOutcome>,
}

#[derive(Debug, Clone, PartialEq)]
/// transport-free 的解析跳过结果，允许 resolved-page 缓存安全保存诊断。
pub(crate) struct UnmaterializedSkippedLocalExecutionCandidate {
    /// 被轻量策略拒绝的候选。
    pub(crate) candidate: SchedulerMinimalCandidateSelectionCandidate,
    /// 与完整解析路径一致的稳定跳过原因。
    pub(crate) skip_reason: &'static str,
    /// 可选全局排序结果；预排序拒绝通常为空。
    pub(crate) ranking: Option<SchedulerRankingOutcome>,
    /// 不含 transport 的附加诊断。
    pub(crate) extra_data: Option<serde_json::Value>,
}

impl UnmaterializedSkippedLocalExecutionCandidate {
    /// 在离开缓存边界后转换为通用跳过类型，transport 固定为空。
    pub(crate) fn into_skipped(self) -> SkippedLocalExecutionCandidate {
        SkippedLocalExecutionCandidate {
            candidate: self.candidate,
            skip_reason: self.skip_reason,
            transport: None,
            ranking: self.ranking,
            extra_data: self.extra_data,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum LocalExecutionCandidateKind {
    #[default]
    SingleKey,
    PoolGroup,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SkippedLocalExecutionCandidate {
    pub(crate) candidate: SchedulerMinimalCandidateSelectionCandidate,
    pub(crate) skip_reason: &'static str,
    pub(crate) transport: Option<Arc<GatewayProviderTransportSnapshot>>,
    pub(crate) ranking: Option<SchedulerRankingOutcome>,
    pub(crate) extra_data: Option<serde_json::Value>,
}

impl SkippedLocalExecutionCandidate {
    pub(crate) fn transport_ref(&self) -> Option<&GatewayProviderTransportSnapshot> {
        self.transport.as_deref()
    }
}

struct GatewayLocalCandidateResolutionPort<'a> {
    state: PlannerAppState<'a>,
    requested_model: Option<&'a str>,
    auth_snapshot: Option<&'a GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&'a ClientSessionAffinity>,
    required_capabilities: Option<&'a serde_json::Value>,
    routing_policy: Option<&'a ResolvedRoutingPolicy>,
    request_auth_channel: Option<&'a str>,
}

#[async_trait]
impl AiCandidateResolutionPort for GatewayLocalCandidateResolutionPort<'_> {
    type Candidate = SchedulerMinimalCandidateSelectionCandidate;
    type Transport = Arc<GatewayProviderTransportSnapshot>;
    type Eligible = EligibleLocalExecutionCandidate;
    type Skipped = SkippedLocalExecutionCandidate;
    type Error = Infallible;

    async fn read_candidate_transport(
        &self,
        candidate: &Self::Candidate,
    ) -> Result<Option<Self::Transport>, Self::Error> {
        let started_at = Instant::now();
        let transport = read_candidate_transport_snapshot_arc(self.state, candidate).await;
        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        observe_gateway_stage_ms("candidate_transport_snapshot", elapsed_ms);
        observe_gateway_stage_ms("candidate_resolution_transport_read", elapsed_ms);
        Ok(transport)
    }

    fn build_missing_transport_skipped_candidate(
        &self,
        candidate: Self::Candidate,
    ) -> Self::Skipped {
        SkippedLocalExecutionCandidate {
            candidate,
            skip_reason: "transport_snapshot_missing",
            transport: None,
            ranking: None,
            extra_data: None,
        }
    }

    fn candidate_common_skip_reason(
        &self,
        candidate: &Self::Candidate,
        transport: &Self::Transport,
        requested_model: Option<&str>,
    ) -> Option<&'static str> {
        if let Some(skip_reason) =
            routing_policy_candidate_skip_reason(self.routing_policy, candidate, transport)
        {
            return Some(skip_reason);
        }
        if provider_transport_uses_pool(transport) {
            return pool_group_common_transport_skip_reason(candidate, transport);
        }
        if let Some(skip_reason) =
            candidate_auth_channel_skip_reason(transport, self.request_auth_channel)
        {
            return Some(skip_reason);
        }
        candidate_common_transport_skip_reason(
            transport,
            candidate_transport_policy_facts(candidate),
            requested_model,
        )
    }

    fn candidate_transport_pair_skip_reason(
        &self,
        candidate: &Self::Candidate,
        transport: &Self::Transport,
        normalized_client_api_format: &str,
        requested_model: &str,
    ) -> Option<&'static str> {
        let _ = (candidate, requested_model);
        candidate_transport_pair_skip_reason(transport, normalized_client_api_format)
    }

    fn build_skipped_candidate(
        &self,
        candidate: Self::Candidate,
        transport: Self::Transport,
        skip_reason: &'static str,
    ) -> Self::Skipped {
        SkippedLocalExecutionCandidate {
            candidate,
            skip_reason,
            transport: Some(transport),
            ranking: None,
            extra_data: None,
        }
    }

    fn build_eligible_candidate(
        &self,
        mut candidate: Self::Candidate,
        transport: Self::Transport,
    ) -> Self::Eligible {
        if candidate.routing_facts == StoredMinimalCandidateRoutingFacts::default() {
            candidate.routing_facts = routing_facts_from_hydrated_transport(&transport);
        }
        let provider_api_format = transport.endpoint.api_format.trim().to_ascii_lowercase();
        let kind = if provider_transport_uses_pool(&transport) {
            LocalExecutionCandidateKind::PoolGroup
        } else {
            LocalExecutionCandidateKind::SingleKey
        };
        EligibleLocalExecutionCandidate {
            kind,
            candidate,
            transport,
            provider_api_format,
            orchestration: LocalExecutionCandidateMetadata::default(),
            ranking: None,
        }
    }

    async fn rank_eligible_candidates(
        &self,
        candidates: Vec<Self::Eligible>,
        normalized_client_api_format: &str,
    ) -> Result<Vec<Self::Eligible>, Self::Error> {
        let started_at = Instant::now();
        let ranked = rank_eligible_local_execution_candidates(
            self.state,
            candidates,
            normalized_client_api_format,
            self.requested_model,
            self.auth_snapshot,
            self.client_session_affinity,
            self.required_capabilities,
            self.routing_policy,
        )
        .await;
        observe_gateway_stage_ms(
            "candidate_resolution_rank",
            started_at.elapsed().as_millis() as u64,
        );
        Ok(ranked)
    }

    async fn apply_pool_scheduler(
        &self,
        candidates: Vec<Self::Eligible>,
    ) -> Result<(Vec<Self::Eligible>, Vec<Self::Skipped>), Self::Error> {
        Ok((candidates, Vec::new()))
    }
}

pub(crate) async fn resolve_and_rank_local_execution_candidates(
    state: PlannerAppState<'_>,
    candidates: Vec<SchedulerMinimalCandidateSelectionCandidate>,
    client_api_format: &str,
    requested_model: &str,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    required_capabilities: Option<&serde_json::Value>,
    routing_policy: Option<&ResolvedRoutingPolicy>,
    _sticky_session_token: Option<&str>,
    request_auth_channel: Option<&str>,
) -> (
    Vec<EligibleLocalExecutionCandidate>,
    Vec<SkippedLocalExecutionCandidate>,
) {
    let requested_model = requested_model.trim();
    resolve_and_rank_local_execution_candidates_with_mode(
        state,
        candidates,
        client_api_format,
        Some(requested_model),
        auth_snapshot,
        client_session_affinity,
        required_capabilities,
        routing_policy,
        None,
        request_auth_channel,
        AiCandidateResolutionMode::Standard,
    )
    .await
}

pub(crate) async fn resolve_and_rank_local_execution_candidates_without_transport_pair_gate(
    state: PlannerAppState<'_>,
    candidates: Vec<SchedulerMinimalCandidateSelectionCandidate>,
    client_api_format: &str,
    requested_model: Option<&str>,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    required_capabilities: Option<&serde_json::Value>,
    routing_policy: Option<&ResolvedRoutingPolicy>,
    _sticky_session_token: Option<&str>,
    request_auth_channel: Option<&str>,
) -> (
    Vec<EligibleLocalExecutionCandidate>,
    Vec<SkippedLocalExecutionCandidate>,
) {
    let requested_model = requested_model.map(str::trim);
    resolve_and_rank_local_execution_candidates_with_mode(
        state,
        candidates,
        client_api_format,
        requested_model,
        auth_snapshot,
        client_session_affinity,
        required_capabilities,
        routing_policy,
        None,
        request_auth_channel,
        AiCandidateResolutionMode::WithoutTransportPairGate,
    )
    .await
}

pub(crate) async fn resolve_and_rank_logical_local_execution_candidates(
    state: PlannerAppState<'_>,
    candidates: Vec<SchedulerMinimalCandidateSelectionCandidate>,
    client_api_format: &str,
    requested_model: Option<&str>,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    required_capabilities: Option<&serde_json::Value>,
    routing_policy: Option<&ResolvedRoutingPolicy>,
    _sticky_session_token: Option<&str>,
    request_auth_channel: Option<&str>,
    mode: AiCandidateResolutionMode,
) -> (
    Vec<EligibleLocalExecutionCandidate>,
    Vec<SkippedLocalExecutionCandidate>,
) {
    resolve_and_rank_local_execution_candidates_with_pool_expansion(
        state,
        candidates,
        client_api_format,
        requested_model,
        auth_snapshot,
        client_session_affinity,
        required_capabilities,
        routing_policy,
        None,
        request_auth_channel,
        mode,
        false,
    )
    .await
}

/// 为旧静态调用方从已存在的 transport 补紧凑事实；不会触发额外读取或延长 transport 生命周期。
pub(super) fn routing_facts_from_hydrated_transport(
    transport: &GatewayProviderTransportSnapshot,
) -> StoredMinimalCandidateRoutingFacts {
    StoredMinimalCandidateRoutingFacts::from_safe_projection(
        transport.provider.keep_priority_on_conversion,
        provider_transport_uses_pool(transport),
        &transport.key.auth_type,
        &transport.endpoint.api_format,
        transport.key.auth_type_by_format.as_ref(),
        transport.key.allow_auth_channel_mismatch_formats.as_ref(),
        compact_proxy_affinity_source(transport.key.proxy.as_ref()),
        compact_proxy_affinity_source(transport.endpoint.proxy.as_ref()),
        compact_proxy_affinity_source(transport.provider.proxy.as_ref()),
    )
}

/// 从当前已 hydration 的代理配置提取安全标量，丢弃 URL 内容和任意 extra。
fn compact_proxy_affinity_source(
    raw: Option<&serde_json::Value>,
) -> Option<StoredCandidateProxyAffinitySource> {
    let object = raw?.as_object()?;
    if object.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
        return None;
    }
    let compact_string = |key: &str| {
        object
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    };
    StoredCandidateProxyAffinitySource::new(
        compact_string("node_id"),
        compact_string("url")
            .or_else(|| compact_string("proxy_url"))
            .is_some(),
        compact_string("tunnel_owner_instance_id"),
    )
}

/// 对分页候选执行 transport-free 资格预判与全局排序；完整校验推迟到选中 attempt。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve_and_rank_logical_local_candidate_snapshots(
    state: PlannerAppState<'_>,
    candidates: Vec<SchedulerMinimalCandidateSelectionCandidate>,
    client_api_format: &str,
    requested_model: Option<&str>,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    required_capabilities: Option<&serde_json::Value>,
    routing_policy: Option<&ResolvedRoutingPolicy>,
    request_auth_channel: Option<&str>,
) -> (
    Vec<RankedLocalExecutionCandidate>,
    Vec<UnmaterializedSkippedLocalExecutionCandidate>,
) {
    let scheduler_affinity_epoch = state.app().scheduler_affinity_epoch();
    let mut rankable = Vec::with_capacity(candidates.len());
    let mut skipped = Vec::new();
    for candidate in candidates {
        let kind = projected_candidate_kind(&candidate);
        let skip_reason =
            routing_policy_candidate_skip_reason_for_kind(routing_policy, &candidate, kind)
                .or_else(|| {
                    projected_candidate_auth_channel_skip_reason(&candidate, request_auth_channel)
                });
        if let Some(skip_reason) = skip_reason {
            skipped.push(UnmaterializedSkippedLocalExecutionCandidate {
                candidate,
                skip_reason,
                ranking: None,
                extra_data: None,
            });
            continue;
        }
        let provider_api_format = candidate.endpoint_api_format.trim().to_ascii_lowercase();
        rankable.push(RankedLocalExecutionCandidate {
            kind,
            candidate,
            provider_api_format,
            orchestration: LocalExecutionCandidateMetadata::default(),
            ranking: None,
        });
    }

    let mut ranked = rank_local_candidate_snapshots(
        state,
        rankable,
        client_api_format,
        requested_model,
        auth_snapshot,
        client_session_affinity,
        required_capabilities,
        routing_policy,
    )
    .await;
    let sticky_key_attempts = if ranked.is_empty() {
        None
    } else {
        Some(
            scheduler_ordering_config_for_routing_policy(state, routing_policy)
                .await
                .sticky_key_attempts,
        )
    };
    for candidate in &mut ranked {
        candidate.orchestration.scheduler_affinity_epoch = Some(scheduler_affinity_epoch);
        candidate.orchestration.sticky_key_attempts = sticky_key_attempts;
    }
    (ranked, skipped)
}

/// 为一个已排好序的候选读取唯一完整 transport，并重跑现有最终资格门禁。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn hydrate_ranked_local_execution_candidate(
    state: PlannerAppState<'_>,
    ranked: RankedLocalExecutionCandidate,
    client_api_format: &str,
    requested_model: Option<&str>,
    routing_policy: Option<&ResolvedRoutingPolicy>,
    request_auth_channel: Option<&str>,
    mode: AiCandidateResolutionMode,
) -> Result<EligibleLocalExecutionCandidate, SkippedLocalExecutionCandidate> {
    let port = GatewayLocalCandidateResolutionPort {
        state,
        requested_model,
        auth_snapshot: None,
        client_session_affinity: None,
        required_capabilities: None,
        routing_policy,
        request_auth_channel,
    };
    let candidate = ranked.candidate;
    let transport = match port.read_candidate_transport(&candidate).await {
        Ok(Some(transport)) => transport,
        Ok(None) => {
            let mut skipped = port.build_missing_transport_skipped_candidate(candidate);
            skipped.ranking = ranked.ranking;
            return Err(skipped);
        }
        Err(error) => match error {},
    };
    let normalized_client_api_format = client_api_format.trim().to_ascii_lowercase();
    let skip_reason = port
        .candidate_common_skip_reason(&candidate, &transport, requested_model)
        .or_else(|| match mode {
            AiCandidateResolutionMode::Standard => port.candidate_transport_pair_skip_reason(
                &candidate,
                &transport,
                &normalized_client_api_format,
                requested_model.unwrap_or_default(),
            ),
            AiCandidateResolutionMode::WithoutTransportPairGate => None,
        });
    if let Some(skip_reason) = skip_reason {
        let mut skipped = port.build_skipped_candidate(candidate, transport, skip_reason);
        skipped.ranking = ranked.ranking;
        return Err(skipped);
    }

    let mut eligible = port.build_eligible_candidate(candidate, transport);
    eligible.orchestration = ranked.orchestration;
    eligible.ranking = ranked.ranking;
    Ok(eligible)
}

async fn resolve_and_rank_local_execution_candidates_with_mode(
    state: PlannerAppState<'_>,
    candidates: Vec<SchedulerMinimalCandidateSelectionCandidate>,
    client_api_format: &str,
    requested_model: Option<&str>,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    required_capabilities: Option<&serde_json::Value>,
    routing_policy: Option<&ResolvedRoutingPolicy>,
    _sticky_session_token: Option<&str>,
    request_auth_channel: Option<&str>,
    mode: AiCandidateResolutionMode,
) -> (
    Vec<EligibleLocalExecutionCandidate>,
    Vec<SkippedLocalExecutionCandidate>,
) {
    resolve_and_rank_local_execution_candidates_with_pool_expansion(
        state,
        candidates,
        client_api_format,
        requested_model,
        auth_snapshot,
        client_session_affinity,
        required_capabilities,
        routing_policy,
        None,
        request_auth_channel,
        mode,
        false,
    )
    .await
}

/// 展开 Pool 后按请求路由策略统一重排候选，并把粘性尝试预算附加到结果。
#[allow(clippy::too_many_arguments)]
async fn resolve_and_rank_local_execution_candidates_with_pool_expansion(
    state: PlannerAppState<'_>,
    candidates: Vec<SchedulerMinimalCandidateSelectionCandidate>,
    client_api_format: &str,
    requested_model: Option<&str>,
    auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
    client_session_affinity: Option<&ClientSessionAffinity>,
    required_capabilities: Option<&serde_json::Value>,
    routing_policy: Option<&ResolvedRoutingPolicy>,
    _sticky_session_token: Option<&str>,
    request_auth_channel: Option<&str>,
    mode: AiCandidateResolutionMode,
    expand_pool_groups: bool,
) -> (
    Vec<EligibleLocalExecutionCandidate>,
    Vec<SkippedLocalExecutionCandidate>,
) {
    let scheduler_affinity_epoch = state.app().scheduler_affinity_epoch();
    let port = GatewayLocalCandidateResolutionPort {
        state,
        requested_model,
        auth_snapshot,
        client_session_affinity,
        required_capabilities,
        routing_policy,
        request_auth_channel,
    };

    let request = AiCandidateResolutionRequest {
        client_api_format,
        requested_model,
        mode,
        expand_pool_groups,
    };

    let started_at = Instant::now();
    match run_ai_candidate_resolution(&port, candidates, request).await {
        Ok(mut outcome) => {
            observe_gateway_stage_ms(
                "candidate_resolution_core",
                started_at.elapsed().as_millis() as u64,
            );
            let sticky_key_attempts = if outcome.eligible_candidates.is_empty() {
                None
            } else {
                Some(
                    scheduler_ordering_config_for_routing_policy(state, routing_policy)
                        .await
                        .sticky_key_attempts,
                )
            };
            for candidate in &mut outcome.eligible_candidates {
                candidate.orchestration.scheduler_affinity_epoch = Some(scheduler_affinity_epoch);
                candidate.orchestration.sticky_key_attempts = sticky_key_attempts;
            }
            (outcome.eligible_candidates, outcome.skipped_candidates)
        }
        Err(error) => match error {},
    }
}

fn candidate_transport_policy_facts(
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
) -> CandidateTransportPolicyFacts<'_> {
    CandidateTransportPolicyFacts {
        endpoint_api_format: candidate.endpoint_api_format.as_str(),
        global_model_name: candidate.global_model_name.as_str(),
        selected_provider_model_name: candidate.selected_provider_model_name.as_str(),
        mapping_matched_model: candidate.mapping_matched_model.as_deref(),
    }
}

fn provider_transport_uses_pool(transport: &GatewayProviderTransportSnapshot) -> bool {
    crate::handlers::shared::provider_pool::admin_provider_pool_config_from_config_value(
        transport.provider.config.as_ref(),
    )
    .is_some()
}

/// 从最小投影判定候选逻辑类型，避免仅为识别 Pool 组读取 Provider 完整配置。
fn projected_candidate_kind(
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
) -> LocalExecutionCandidateKind {
    if candidate.routing_facts.provider_pool_enabled {
        LocalExecutionCandidateKind::PoolGroup
    } else {
        LocalExecutionCandidateKind::SingleKey
    }
}

fn routing_policy_candidate_skip_reason(
    routing_policy: Option<&ResolvedRoutingPolicy>,
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
    transport: &GatewayProviderTransportSnapshot,
) -> Option<&'static str> {
    let kind = if provider_transport_uses_pool(transport) {
        LocalExecutionCandidateKind::PoolGroup
    } else {
        LocalExecutionCandidateKind::SingleKey
    };
    routing_policy_candidate_skip_reason_for_kind(routing_policy, candidate, kind)
}

/// 使用已知候选类型应用路由 allowlist；Pool 组不把代表 Key 当作真实成员过滤。
fn routing_policy_candidate_skip_reason_for_kind(
    routing_policy: Option<&ResolvedRoutingPolicy>,
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
    kind: LocalExecutionCandidateKind,
) -> Option<&'static str> {
    let policy = routing_policy?;
    if !policy
        .ranking_overlay
        .provider_allowed(candidate.provider_id.as_str())
    {
        return Some("routing_profile_disallowed_provider");
    }
    if kind == LocalExecutionCandidateKind::SingleKey
        && !policy
            .ranking_overlay
            .key_allowed(candidate.key_id.as_str())
    {
        return Some("routing_profile_disallowed_key");
    }
    None
}

/// 用紧凑认证策略预判请求与上游通道是否兼容，最终 hydration 仍会重跑完整门禁。
fn projected_candidate_auth_channel_skip_reason(
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
    request_auth_channel: Option<&str>,
) -> Option<&'static str> {
    let request_auth_channel = normalize_request_auth_channel(request_auth_channel?)?;
    let projected_auth_type = candidate
        .routing_facts
        .key_auth_type_for_endpoint_format
        .trim();
    let auth_type = if projected_auth_type.is_empty() {
        candidate.key_auth_type.trim()
    } else {
        projected_auth_type
    }
    .to_ascii_lowercase();
    let provider_policy = provider_runtime_policy(&candidate.provider_type);
    let upstream_auth_channel = match auth_type.as_str() {
        "api_key" => Some("api_key"),
        "bearer" => Some("bearer_like"),
        "oauth" if provider_policy.oauth_is_bearer_like => Some("bearer_like"),
        _ => None,
    }?;
    if request_auth_channel == upstream_auth_channel
        || candidate
            .routing_facts
            .key_allows_auth_channel_mismatch_for_endpoint_format
    {
        None
    } else {
        Some("auth_channel_mismatch")
    }
}

fn pool_group_common_transport_skip_reason(
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
    transport: &GatewayProviderTransportSnapshot,
) -> Option<&'static str> {
    if !transport.provider.is_active {
        return Some("provider_inactive");
    }
    if !transport.endpoint.is_active {
        return Some("endpoint_inactive");
    }
    if !crate::ai_serving::api_format_alias_matches(
        candidate.endpoint_api_format.as_str(),
        transport.endpoint.api_format.trim(),
    ) {
        return Some("endpoint_api_format_changed");
    }
    None
}

pub(crate) fn candidate_auth_channel_skip_reason(
    transport: &GatewayProviderTransportSnapshot,
    request_auth_channel: Option<&str>,
) -> Option<&'static str> {
    let request_auth_channel = normalize_request_auth_channel(request_auth_channel?)?;
    let upstream_auth_channel = resolve_transport_request_auth_channel(transport)?;
    if request_auth_channel == upstream_auth_channel {
        return None;
    }
    auth_channel_mismatch_is_explicitly_disabled_for_format(transport)
        .then_some("auth_channel_mismatch")
}

fn normalize_request_auth_channel(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "api_key" | "api-key" | "apikey" => Some("api_key"),
        "bearer_like" | "bearer-like" | "bearer" | "oauth" => Some("bearer_like"),
        _ => None,
    }
}

fn resolve_transport_request_auth_channel(
    transport: &GatewayProviderTransportSnapshot,
) -> Option<&'static str> {
    let auth_type = resolve_transport_auth_type_for_endpoint_format(transport);
    let provider_policy = provider_runtime_policy(&transport.provider.provider_type);
    match auth_type.as_str() {
        "api_key" => Some("api_key"),
        "bearer" => Some("bearer_like"),
        "oauth" if provider_policy.oauth_is_bearer_like => Some("bearer_like"),
        _ => None,
    }
}

fn resolve_transport_auth_type_for_endpoint_format(
    transport: &GatewayProviderTransportSnapshot,
) -> String {
    let default_auth_type = transport.key.auth_type.trim().to_ascii_lowercase();
    let api_format = crate::ai_serving::normalize_api_format_alias(&transport.endpoint.api_format);
    transport
        .key
        .auth_type_by_format
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .and_then(|overrides| {
            overrides
                .get(&api_format)
                .or_else(|| overrides.get(transport.endpoint.api_format.trim()))
        })
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .filter(|value| matches!(value.as_str(), "api_key" | "bearer"))
        .unwrap_or(default_auth_type)
}

fn auth_channel_mismatch_is_explicitly_disabled_for_format(
    transport: &GatewayProviderTransportSnapshot,
) -> bool {
    let api_format = crate::ai_serving::normalize_api_format_alias(&transport.endpoint.api_format);
    let Some(items) = transport
        .key
        .allow_auth_channel_mismatch_formats
        .as_ref()
        .and_then(serde_json::Value::as_array)
    else {
        return false;
    };
    !items
        .iter()
        .filter_map(serde_json::Value::as_str)
        .any(|item| crate::ai_serving::normalize_api_format_alias(item) == api_format)
}

pub(crate) async fn read_candidate_transport_snapshot(
    state: PlannerAppState<'_>,
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
) -> Option<GatewayProviderTransportSnapshot> {
    read_candidate_transport_snapshot_arc(state, candidate)
        .await
        .map(|transport| (*transport).clone())
}

pub(crate) async fn read_candidate_transport_snapshot_arc(
    state: PlannerAppState<'_>,
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
) -> Option<Arc<GatewayProviderTransportSnapshot>> {
    match state
        .read_provider_transport_snapshot_arc(
            &candidate.provider_id,
            &candidate.endpoint_id,
            &candidate.key_id,
        )
        .await
    {
        Ok(Some(transport)) => Some(transport),
        Ok(None) => None,
        Err(error) => {
            warn!(
                event_name = "candidate_resolution_transport_load_failed",
                log_type = "event",
                provider_id = %candidate.provider_id,
                endpoint_id = %candidate.endpoint_id,
                key_id = %candidate.key_id,
                error = ?error,
                "failed to load provider transport while evaluating local candidate eligibility"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        candidate_auth_channel_skip_reason, pool_group_common_transport_skip_reason,
        projected_candidate_auth_channel_skip_reason,
    };
    use crate::ai_serving::GatewayProviderTransportSnapshot;
    use aether_provider_transport::snapshot::{
        GatewayProviderTransportEndpoint, GatewayProviderTransportKey,
        GatewayProviderTransportProvider,
    };
    use aether_scheduler_core::SchedulerMinimalCandidateSelectionCandidate;
    use serde_json::json;

    fn sample_transport(auth_type: &str) -> GatewayProviderTransportSnapshot {
        GatewayProviderTransportSnapshot {
            provider: GatewayProviderTransportProvider {
                id: "provider-1".to_string(),
                name: "provider".to_string(),
                provider_type: "custom".to_string(),
                website: None,
                is_active: true,
                keep_priority_on_conversion: false,
                enable_format_conversion: false,
                concurrent_limit: None,
                max_retries: None,
                proxy: None,
                request_timeout_secs: None,
                stream_first_byte_timeout_secs: None,
                config: None,
            },
            endpoint: GatewayProviderTransportEndpoint {
                id: "endpoint-1".to_string(),
                provider_id: "provider-1".to_string(),
                api_format: "claude:messages".to_string(),
                api_family: Some("claude".to_string()),
                endpoint_kind: Some("messages".to_string()),
                is_active: true,
                base_url: "https://example.test".to_string(),
                header_rules: None,
                body_rules: None,
                max_retries: None,
                custom_path: None,
                config: None,
                format_acceptance_config: None,
                proxy: None,
            },
            key: GatewayProviderTransportKey {
                id: "key-1".to_string(),
                provider_id: "provider-1".to_string(),
                name: "key".to_string(),
                auth_type: auth_type.to_string(),
                is_active: true,
                api_formats: Some(vec!["claude:messages".to_string()]),
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
        }
    }

    fn sample_candidate() -> SchedulerMinimalCandidateSelectionCandidate {
        SchedulerMinimalCandidateSelectionCandidate {
            provider_id: "provider-1".to_string(),
            provider_name: "provider".to_string(),
            provider_type: "custom".to_string(),
            provider_priority: 10,
            endpoint_id: "endpoint-1".to_string(),
            endpoint_api_format: "claude:messages".to_string(),
            key_id: "key-1".to_string(),
            key_name: "key".to_string(),
            key_auth_type: "bearer".to_string(),
            key_internal_priority: 10,
            key_global_priority_for_format: None,
            key_capabilities: None,
            routing_facts: Default::default(),
            model_id: "model-1".to_string(),
            global_model_id: "global-model-1".to_string(),
            global_model_name: "claude-sonnet".to_string(),
            selected_provider_model_name: "claude-sonnet".to_string(),
            supports_streaming: true,
            mapping_matched_model: None,
        }
    }

    /// 验证轻量预判与完整门禁一致：缺失 allowlist 默认放行，显式排除才阻断。
    #[test]
    fn projected_auth_channel_gate_preserves_default_allow_semantics() {
        let mut candidate = sample_candidate();
        candidate.routing_facts.key_auth_type_for_endpoint_format = "bearer".to_string();
        assert_eq!(
            projected_candidate_auth_channel_skip_reason(&candidate, Some("api_key")),
            None
        );

        candidate
            .routing_facts
            .key_allows_auth_channel_mismatch_for_endpoint_format = false;
        assert_eq!(
            projected_candidate_auth_channel_skip_reason(&candidate, Some("api_key")),
            Some("auth_channel_mismatch")
        );
    }

    #[test]
    fn auth_channel_gate_allows_mismatched_raw_secret_auth_by_default() {
        let transport = sample_transport("bearer");
        assert_eq!(
            candidate_auth_channel_skip_reason(&transport, Some("api_key")),
            None
        );
    }

    #[test]
    fn auth_channel_gate_allows_explicit_mismatch_format() {
        let mut transport = sample_transport("bearer");
        transport.key.allow_auth_channel_mismatch_formats = Some(json!(["claude:messages"]));
        assert_eq!(
            candidate_auth_channel_skip_reason(&transport, Some("api_key")),
            None
        );
    }

    #[test]
    fn auth_channel_gate_blocks_explicitly_disabled_mismatch_format() {
        let mut transport = sample_transport("bearer");
        transport.key.allow_auth_channel_mismatch_formats = Some(json!(["openai:responses"]));
        assert_eq!(
            candidate_auth_channel_skip_reason(&transport, Some("api_key")),
            Some("auth_channel_mismatch")
        );
    }

    #[test]
    fn auth_channel_gate_treats_cli_oauth_provider_as_bearer_like() {
        let mut transport = sample_transport("oauth");
        transport.provider.provider_type = "claude_code".to_string();
        assert_eq!(
            candidate_auth_channel_skip_reason(&transport, Some("bearer_like")),
            None
        );
        assert_eq!(
            candidate_auth_channel_skip_reason(&transport, Some("api_key")),
            None
        );
    }

    #[test]
    fn auth_channel_gate_allows_kiro_provider_mismatch_by_default() {
        let mut transport = sample_transport("oauth");
        transport.provider.provider_type = "kiro".to_string();
        assert_eq!(
            candidate_auth_channel_skip_reason(&transport, Some("api_key")),
            None
        );
    }

    #[test]
    fn pool_group_common_gate_ignores_representative_key_model_policy() {
        let candidate = sample_candidate();
        let mut transport = sample_transport("bearer");
        transport.key.allowed_models = Some(vec!["different-model".to_string()]);
        transport.key.api_formats = Some(vec!["different:format".to_string()]);

        assert_eq!(
            pool_group_common_transport_skip_reason(&candidate, &transport),
            None
        );
    }
}
