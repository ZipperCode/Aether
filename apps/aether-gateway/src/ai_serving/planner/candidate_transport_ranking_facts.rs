use std::collections::BTreeMap;

use aether_contracts::ProxySnapshot;
use aether_data_contracts::repository::candidate_selection::StoredCandidateProxyAffinitySource;
use aether_scheduler_core::{
    SchedulerMinimalCandidateSelectionCandidate, SchedulerTunnelAffinityBucket,
};
use serde_json::Value;
use tracing::warn;

use crate::ai_serving::PlannerAppState;
use crate::scheduler::config::SchedulerOrderingConfig;

const TUNNEL_OWNER_INSTANCE_ID_EXTRA_KEY: &str = "tunnel_owner_instance_id";

pub(super) type CandidateTransportIdentity = (String, String, String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// 全局排序消费的紧凑事实；不拥有 transport、代理 URL 或任何解密凭据。
pub(super) struct CandidateTransportRankingFacts {
    /// 候选代理隧道相对当前网关的所有者位置。
    pub(super) tunnel_bucket: SchedulerTunnelAffinityBucket,
    /// 跨格式候选是否保留原优先级。
    pub(super) keep_priority_on_conversion: bool,
}

#[derive(Debug, Default)]
/// 单次排序批次的无凭据事实缓存；相同代理节点只解析一次所有者位置。
pub(super) struct CandidateTransportRankingFactsCache {
    candidate_facts: BTreeMap<CandidateTransportIdentity, CandidateTransportRankingFacts>,
    configured_proxy_buckets:
        BTreeMap<StoredCandidateProxyAffinitySource, Option<SchedulerTunnelAffinityBucket>>,
    system_proxy_bucket: Option<SchedulerTunnelAffinityBucket>,
    tunnel_buckets_by_node_id: BTreeMap<String, SchedulerTunnelAffinityBucket>,
}

/// 按候选身份复用紧凑排序事实，整个过程不触发 Provider transport 读取。
pub(super) async fn resolve_cached_candidate_transport_ranking_facts(
    state: PlannerAppState<'_>,
    cache: &mut CandidateTransportRankingFactsCache,
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
    ordering_config: SchedulerOrderingConfig,
) -> CandidateTransportRankingFacts {
    let identity = candidate_transport_identity(candidate);
    if let Some(facts) = cache.candidate_facts.get(&identity).copied() {
        return facts;
    }

    let facts =
        resolve_candidate_transport_ranking_facts(state, cache, candidate, ordering_config).await;
    cache.candidate_facts.insert(identity, facts);
    facts
}

/// 兼容已解析候选的排序入口；实现仍只读取候选上的紧凑事实，不接收完整 transport。
pub(super) async fn resolve_cached_transport_ranking_facts(
    state: PlannerAppState<'_>,
    cache: &mut CandidateTransportRankingFactsCache,
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
    ordering_config: SchedulerOrderingConfig,
) -> CandidateTransportRankingFacts {
    resolve_cached_candidate_transport_ranking_facts(state, cache, candidate, ordering_config).await
}

/// 判断跨格式候选是否留在优先页；只消费 Provider 标量投影，禁止为此解密 Key。
pub(super) async fn candidate_keeps_priority_on_conversion(
    _state: PlannerAppState<'_>,
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
    ordering_config: SchedulerOrderingConfig,
) -> bool {
    ordering_config.keep_priority_on_conversion
        || candidate.routing_facts.provider_keep_priority_on_conversion
}

/// 从候选轻量投影计算排序事实；代理解析只保留最终 bucket。
async fn resolve_candidate_transport_ranking_facts(
    state: PlannerAppState<'_>,
    cache: &mut CandidateTransportRankingFactsCache,
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
    ordering_config: SchedulerOrderingConfig,
) -> CandidateTransportRankingFacts {
    CandidateTransportRankingFacts {
        tunnel_bucket: resolve_transport_proxy_snapshot_with_tunnel_affinity(
            state, cache, candidate,
        )
        .await,
        keep_priority_on_conversion: ordering_config.keep_priority_on_conversion
            || candidate.routing_facts.provider_keep_priority_on_conversion,
    }
}

/// 按 Key → Endpoint → Provider → system 顺序解析代理亲和；内联 URL 只产生中性 bucket。
async fn resolve_transport_proxy_snapshot_with_tunnel_affinity(
    state: PlannerAppState<'_>,
    cache: &mut CandidateTransportRankingFactsCache,
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
) -> SchedulerTunnelAffinityBucket {
    for source in [
        candidate.routing_facts.key_proxy.as_ref(),
        candidate.routing_facts.endpoint_proxy.as_ref(),
        candidate.routing_facts.provider_proxy.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(bucket) = resolve_configured_proxy_bucket(state, cache, source).await {
            return bucket;
        }
    }

    if let Some(bucket) = cache.system_proxy_bucket {
        return bucket;
    }
    let bucket = match state.app().resolve_system_proxy_snapshot().await {
        Some(proxy) => resolve_tunnel_owner_affinity_from_proxy(state, cache, &proxy).await,
        None => SchedulerTunnelAffinityBucket::Neutral,
    };
    cache.system_proxy_bucket = Some(bucket);
    bucket
}

/// 解析一层代理配置；None 表示该层节点失效且没有内联 URL，应继续低优先级回退。
async fn resolve_configured_proxy_bucket(
    state: PlannerAppState<'_>,
    cache: &mut CandidateTransportRankingFactsCache,
    source: &StoredCandidateProxyAffinitySource,
) -> Option<SchedulerTunnelAffinityBucket> {
    if let Some(cached) = cache.configured_proxy_buckets.get(source).copied() {
        return cached;
    }

    let bucket = if let Some(node_id) = source.node_id.as_deref() {
        if let Some(proxy) = state.app().resolve_proxy_node_snapshot(Some(node_id)).await {
            Some(resolve_tunnel_owner_affinity_from_proxy(state, cache, &proxy).await)
        } else if !source.has_inline_url
            && state
                .app()
                .find_proxy_node(node_id)
                .await
                .ok()
                .flatten()
                .is_some()
        {
            None
        } else {
            Some(
                resolve_tunnel_owner_affinity_from_compact_source(state, cache, source, node_id)
                    .await,
            )
        }
    } else if source.has_inline_url {
        Some(SchedulerTunnelAffinityBucket::Neutral)
    } else {
        None
    };
    cache
        .configured_proxy_buckets
        .insert(source.clone(), bucket);
    bucket
}

/// 使用投影自带所有者或共享 attachment 记录解析未知节点，不需要代理 URL。
async fn resolve_tunnel_owner_affinity_from_compact_source(
    state: PlannerAppState<'_>,
    cache: &mut CandidateTransportRankingFactsCache,
    source: &StoredCandidateProxyAffinitySource,
    node_id: &str,
) -> SchedulerTunnelAffinityBucket {
    if let Some(bucket) = cache.tunnel_buckets_by_node_id.get(node_id).copied() {
        return bucket;
    }
    let bucket = if state.app().tunnel.has_local_proxy(node_id) {
        SchedulerTunnelAffinityBucket::LocalTunnel
    } else if let Some(owner_instance_id) = source.tunnel_owner_instance_id.as_deref() {
        tunnel_bucket_for_owner(state, owner_instance_id)
    } else {
        lookup_tunnel_owner_affinity(state, node_id).await
    };
    cache
        .tunnel_buckets_by_node_id
        .insert(node_id.to_string(), bucket);
    bucket
}

/// 从短生命周期 ProxySnapshot 提取 owner 后立即丢弃其 URL 与认证字段。
async fn resolve_tunnel_owner_affinity_from_proxy(
    state: PlannerAppState<'_>,
    cache: &mut CandidateTransportRankingFactsCache,
    proxy: &ProxySnapshot,
) -> SchedulerTunnelAffinityBucket {
    let Some(node_id) = proxy
        .node_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return SchedulerTunnelAffinityBucket::Neutral;
    };
    if let Some(bucket) = cache.tunnel_buckets_by_node_id.get(node_id).copied() {
        return bucket;
    }

    let bucket = if state.app().tunnel.has_local_proxy(node_id) {
        SchedulerTunnelAffinityBucket::LocalTunnel
    } else if let Some(owner_instance_id) = proxy_tunnel_owner_instance_id(proxy) {
        tunnel_bucket_for_owner(state, owner_instance_id)
    } else {
        lookup_tunnel_owner_affinity(state, node_id).await
    };
    cache
        .tunnel_buckets_by_node_id
        .insert(node_id.to_string(), bucket);
    bucket
}

/// 将明确的隧道所有者映射为本地或远端 bucket。
fn tunnel_bucket_for_owner(
    state: PlannerAppState<'_>,
    owner_instance_id: &str,
) -> SchedulerTunnelAffinityBucket {
    if owner_instance_id == state.app().tunnel.local_instance_id() {
        SchedulerTunnelAffinityBucket::LocalTunnel
    } else {
        SchedulerTunnelAffinityBucket::RemoteTunnel
    }
}

/// 查询共享隧道 attachment；查询失败保持中性，不阻断候选。
async fn lookup_tunnel_owner_affinity(
    state: PlannerAppState<'_>,
    node_id: &str,
) -> SchedulerTunnelAffinityBucket {
    match state
        .app()
        .tunnel
        .lookup_attachment_owner(state.app().data.as_ref(), node_id)
        .await
    {
        Ok(Some(owner)) => tunnel_bucket_for_owner(state, &owner.gateway_instance_id),
        Ok(None) => SchedulerTunnelAffinityBucket::Neutral,
        Err(error) => {
            warn!(
                event_name = "candidate_transport_ranking_facts_tunnel_owner_lookup_failed",
                log_type = "event",
                node_id = node_id,
                error = %error,
                "failed to load tunnel attachment owner while evaluating candidate transport ranking facts"
            );
            SchedulerTunnelAffinityBucket::Neutral
        }
    }
}

/// 生成候选事实缓存键；同一 Provider/Endpoint/Key 在一批排序中共享解析结果。
fn candidate_transport_identity(
    candidate: &SchedulerMinimalCandidateSelectionCandidate,
) -> CandidateTransportIdentity {
    (
        candidate.provider_id.clone(),
        candidate.endpoint_id.clone(),
        candidate.key_id.clone(),
    )
}

/// 从代理快照的 extra 中读取已知隧道 owner 标识。
fn proxy_tunnel_owner_instance_id(proxy: &ProxySnapshot) -> Option<&str> {
    proxy
        .extra
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|extra| extra.get(TUNNEL_OWNER_INSTANCE_ID_EXTRA_KEY))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_data_contracts::repository::candidate_selection::StoredMinimalCandidateRoutingFacts;
    use aether_scheduler_core::SchedulerMinimalCandidateSelectionCandidate;

    /// 构造只含排序标量的候选，测试不得借助完整 Provider transport。
    fn candidate_with_routing_facts(
        routing_facts: StoredMinimalCandidateRoutingFacts,
    ) -> SchedulerMinimalCandidateSelectionCandidate {
        SchedulerMinimalCandidateSelectionCandidate {
            provider_id: "provider-1".to_string(),
            provider_name: "provider-1".to_string(),
            provider_type: "custom".to_string(),
            provider_priority: 0,
            endpoint_id: "endpoint-1".to_string(),
            endpoint_api_format: "openai:chat".to_string(),
            key_id: "key-1".to_string(),
            key_name: "key-1".to_string(),
            key_auth_type: "api_key".to_string(),
            key_internal_priority: 0,
            key_global_priority_for_format: None,
            key_capabilities: None,
            routing_facts,
            model_id: "model-1".to_string(),
            global_model_id: "global-model-1".to_string(),
            global_model_name: "gpt-5".to_string(),
            selected_provider_model_name: "gpt-5".to_string(),
            supports_streaming: true,
            mapping_matched_model: None,
        }
    }

    /// 验证 Key 优先于 Endpoint，且内联代理产生中性 bucket 后不会继续向低层回退。
    #[tokio::test]
    async fn compact_proxy_facts_preserve_precedence_and_inline_semantics() {
        let app = crate::AppState::new()
            .expect("state should build")
            .with_tunnel_identity_for_tests("gateway-local", None);
        let state = PlannerAppState::new(&app);
        let endpoint_remote = StoredCandidateProxyAffinitySource::new(
            Some("endpoint-node".to_string()),
            false,
            Some("gateway-remote".to_string()),
        );
        let key_local = StoredCandidateProxyAffinitySource::new(
            Some("key-node".to_string()),
            false,
            Some("gateway-local".to_string()),
        );
        let candidate = candidate_with_routing_facts(StoredMinimalCandidateRoutingFacts {
            key_proxy: key_local,
            endpoint_proxy: endpoint_remote.clone(),
            ..StoredMinimalCandidateRoutingFacts::default()
        });

        let facts = resolve_candidate_transport_ranking_facts(
            state,
            &mut CandidateTransportRankingFactsCache::default(),
            &candidate,
            SchedulerOrderingConfig::default(),
        )
        .await;
        assert_eq!(
            facts.tunnel_bucket,
            SchedulerTunnelAffinityBucket::LocalTunnel
        );

        let inline_candidate = candidate_with_routing_facts(StoredMinimalCandidateRoutingFacts {
            key_proxy: StoredCandidateProxyAffinitySource::new(None, true, None),
            endpoint_proxy: endpoint_remote,
            ..StoredMinimalCandidateRoutingFacts::default()
        });
        let inline_facts = resolve_candidate_transport_ranking_facts(
            state,
            &mut CandidateTransportRankingFactsCache::default(),
            &inline_candidate,
            SchedulerOrderingConfig::default(),
        )
        .await;
        assert_eq!(
            inline_facts.tunnel_bucket,
            SchedulerTunnelAffinityBucket::Neutral
        );
    }
}
