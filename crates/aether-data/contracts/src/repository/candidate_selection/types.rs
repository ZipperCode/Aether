use async_trait::async_trait;

#[derive(
    Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
/// 候选排序所需的单层代理亲和事实；仅保留节点与所有者标识，不携带代理 URL 或凭据。
pub struct StoredCandidateProxyAffinitySource {
    /// 配置引用的代理节点；空值表示仅配置了内联代理 URL。
    pub node_id: Option<String>,
    /// 是否存在非空内联 URL；仅用于判定本层配置是否阻止继续回退到低优先级代理。
    pub has_inline_url: bool,
    /// 配置快照中已知的隧道所有者实例，用于避免不必要的跨节点查询。
    pub tunnel_owner_instance_id: Option<String>,
}

impl StoredCandidateProxyAffinitySource {
    /// 从独立的安全 SQL 标量构造代理亲和事实；无有效节点和 URL 时视为未配置。
    pub fn new(
        node_id: Option<String>,
        has_inline_url: bool,
        tunnel_owner_instance_id: Option<String>,
    ) -> Option<Self> {
        let node_id = node_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if node_id.is_none() && !has_inline_url {
            return None;
        }
        Some(Self {
            node_id,
            has_inline_url,
            tunnel_owner_instance_id: tunnel_owner_instance_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// 最小候选跨层携带的非敏感路由事实；完整 Provider、Key 和 transport 仍在真正尝试时读取。
pub struct StoredMinimalCandidateRoutingFacts {
    /// Provider 是否要求跨格式转换后保留原优先级。
    pub provider_keep_priority_on_conversion: bool,
    /// Provider 是否声明 Pool 配置；用于选择 Pool 组排序槽与后续按 Key 展开。
    pub provider_pool_enabled: bool,
    /// 按当前 Endpoint 格式解析后的 Key 认证类型，已应用格式级覆盖。
    pub key_auth_type_for_endpoint_format: String,
    /// 当前 Endpoint 格式是否允许认证通道不一致；缺失配置沿用旧合同默认允许。
    pub key_allows_auth_channel_mismatch_for_endpoint_format: bool,
    /// Key 层代理亲和事实，优先级高于 Endpoint 和 Provider。
    pub key_proxy: Option<StoredCandidateProxyAffinitySource>,
    /// Endpoint 层代理亲和事实，Key 层不可用时参与回退。
    pub endpoint_proxy: Option<StoredCandidateProxyAffinitySource>,
    /// Provider 层代理亲和事实，前两层不可用时参与回退。
    pub provider_proxy: Option<StoredCandidateProxyAffinitySource>,
}

impl Default for StoredMinimalCandidateRoutingFacts {
    /// 缺失投影沿用旧认证合同：未显式配置 mismatch allowlist 时默认放行。
    fn default() -> Self {
        Self {
            provider_keep_priority_on_conversion: false,
            provider_pool_enabled: false,
            key_auth_type_for_endpoint_format: String::new(),
            key_allows_auth_channel_mismatch_for_endpoint_format: true,
            key_proxy: None,
            endpoint_proxy: None,
            provider_proxy: None,
        }
    }
}

impl StoredMinimalCandidateRoutingFacts {
    #[allow(clippy::too_many_arguments)]
    /// 归一化数据库候选投影；格式级认证策略在此收敛为标量，代理只接收无凭据投影。
    pub fn from_safe_projection(
        provider_keep_priority_on_conversion: bool,
        provider_pool_enabled: bool,
        default_key_auth_type: &str,
        endpoint_api_format: &str,
        key_auth_type_by_format: Option<&serde_json::Value>,
        key_allow_auth_channel_mismatch_formats: Option<&serde_json::Value>,
        key_proxy: Option<StoredCandidateProxyAffinitySource>,
        endpoint_proxy: Option<StoredCandidateProxyAffinitySource>,
        provider_proxy: Option<StoredCandidateProxyAffinitySource>,
    ) -> Self {
        let normalized_api_format =
            aether_ai_formats::normalize_api_format_alias(endpoint_api_format);
        let key_auth_type_for_endpoint_format = key_auth_type_by_format
            .and_then(serde_json::Value::as_object)
            .and_then(|overrides| {
                overrides
                    .get(&normalized_api_format)
                    .or_else(|| overrides.get(endpoint_api_format.trim()))
            })
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .filter(|value| matches!(value.as_str(), "api_key" | "bearer"))
            .unwrap_or_else(|| default_key_auth_type.trim().to_ascii_lowercase());
        let key_allows_auth_channel_mismatch_for_endpoint_format =
            match key_allow_auth_channel_mismatch_formats.and_then(serde_json::Value::as_array) {
                Some(formats) => {
                    formats
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .any(|format| {
                            aether_ai_formats::normalize_api_format_alias(format)
                                == normalized_api_format
                        })
                }
                // 旧合同只在显式数组排除当前格式时阻断；缺失或非数组配置继续默认放行。
                None => true,
            };

        Self {
            provider_keep_priority_on_conversion,
            provider_pool_enabled,
            key_auth_type_for_endpoint_format,
            key_allows_auth_channel_mismatch_for_endpoint_format,
            key_proxy,
            endpoint_proxy,
            provider_proxy,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredProviderModelMapping {
    pub name: String,
    pub priority: i32,
    pub api_formats: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_ids: Option<Vec<String>>,
    /// Optional request-operation scope. An omitted scope applies to every
    /// operation supported by the selected API format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operations: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StoredMinimalCandidateSelectionRow {
    pub provider_id: String,
    pub provider_name: String,
    pub provider_type: String,
    pub provider_priority: i32,
    pub provider_is_active: bool,
    pub endpoint_id: String,
    pub endpoint_api_format: String,
    pub endpoint_api_family: Option<String>,
    pub endpoint_kind: Option<String>,
    pub endpoint_is_active: bool,
    pub key_id: String,
    pub key_name: String,
    pub key_auth_type: String,
    pub key_is_active: bool,
    pub key_api_formats: Option<Vec<String>>,
    pub key_allowed_models: Option<Vec<String>>,
    pub key_capabilities: Option<serde_json::Value>,
    pub key_internal_priority: i32,
    pub key_global_priority_by_format: Option<serde_json::Value>,
    /// 排序和认证通道预判所需的紧凑事实，不包含 transport、凭据或原始代理配置。
    #[serde(default)]
    pub routing_facts: StoredMinimalCandidateRoutingFacts,
    pub model_id: String,
    pub global_model_id: String,
    pub global_model_name: String,
    pub global_model_mappings: Option<Vec<String>>,
    pub global_model_supports_streaming: Option<bool>,
    pub model_provider_model_name: String,
    pub model_provider_model_mappings: Option<Vec<StoredProviderModelMapping>>,
    pub model_supports_streaming: Option<bool>,
    pub model_is_active: bool,
    pub model_is_available: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StoredPoolKeyCandidateOrder {
    #[default]
    InternalPriority,
    Lru,
    CacheAffinity,
    SingleAccount,
    LoadBalance {
        seed: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredPoolKeyCandidateRowsQuery {
    pub api_format: String,
    pub provider_id: String,
    pub endpoint_id: String,
    pub model_id: String,
    pub selected_provider_model_name: String,
    #[serde(default)]
    pub order: StoredPoolKeyCandidateOrder,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredPoolKeyCandidateRowsByKeyIdsQuery {
    pub api_format: String,
    pub provider_id: String,
    pub endpoint_id: String,
    pub model_id: String,
    pub selected_provider_model_name: String,
    pub key_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredRequestedModelCandidateRowsQuery {
    pub api_format: String,
    pub requested_model_name: String,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredApiFormatCandidateRowsQuery {
    pub api_format: String,
    pub offset: u32,
    pub limit: u32,
}

impl StoredMinimalCandidateSelectionRow {
    pub fn supports_streaming(&self) -> bool {
        self.model_supports_streaming
            .or(self.global_model_supports_streaming)
            .unwrap_or(true)
    }

    pub fn key_supports_api_format(&self, api_format: &str) -> bool {
        match self.key_api_formats.as_deref() {
            None => true,
            Some(formats) => formats
                .iter()
                .any(|value| api_format_permission_covers(value, api_format)),
        }
    }
}

/// Evaluates the API-format scope on a provider-model mapping.
///
/// Codex Live was introduced after existing Codex model associations had
/// already stored their source-model scope as `openai:responses`. Preserve
/// those associations for the same Codex provider without treating the two
/// formats as globally interchangeable. Endpoint and key permissions remain
/// independently scoped to `codex:live`.
pub fn provider_model_mapping_api_format_covers(
    provider_type: &str,
    mapping_api_format: &str,
    requested_api_format: &str,
) -> bool {
    if aether_ai_formats::api_format_permission_covers(mapping_api_format, requested_api_format) {
        return true;
    }

    provider_type.trim().eq_ignore_ascii_case("codex")
        && aether_ai_formats::normalize_api_format_alias(requested_api_format) == "codex:live"
        && aether_ai_formats::normalize_api_format_alias(mapping_api_format) == "openai:responses"
}

fn api_format_permission_covers(allowed: &str, requested: &str) -> bool {
    aether_ai_formats::api_format_permission_covers(allowed, requested)
}

#[async_trait]
pub trait MinimalCandidateSelectionReadRepository: Send + Sync {
    fn clear_local_cache(&self) {}

    async fn list_for_exact_api_format(
        &self,
        api_format: &str,
    ) -> Result<Vec<StoredMinimalCandidateSelectionRow>, crate::DataLayerError>;

    async fn list_for_exact_api_format_page(
        &self,
        query: &StoredApiFormatCandidateRowsQuery,
    ) -> Result<Vec<StoredMinimalCandidateSelectionRow>, crate::DataLayerError> {
        Ok(self
            .list_for_exact_api_format(&query.api_format)
            .await?
            .into_iter()
            .skip(query.offset as usize)
            .take(query.limit as usize)
            .collect())
    }

    async fn list_for_exact_api_format_and_global_model(
        &self,
        api_format: &str,
        global_model_name: &str,
    ) -> Result<Vec<StoredMinimalCandidateSelectionRow>, crate::DataLayerError>;

    async fn list_for_exact_api_format_and_requested_model(
        &self,
        api_format: &str,
        requested_model_name: &str,
    ) -> Result<Vec<StoredMinimalCandidateSelectionRow>, crate::DataLayerError>;

    async fn list_for_exact_api_format_and_requested_model_page(
        &self,
        query: &StoredRequestedModelCandidateRowsQuery,
    ) -> Result<Vec<StoredMinimalCandidateSelectionRow>, crate::DataLayerError>;

    async fn list_pool_key_rows_for_group(
        &self,
        query: &StoredPoolKeyCandidateRowsQuery,
    ) -> Result<Vec<StoredMinimalCandidateSelectionRow>, crate::DataLayerError>;

    async fn list_pool_key_rows_for_group_key_ids(
        &self,
        query: &StoredPoolKeyCandidateRowsByKeyIdsQuery,
    ) -> Result<Vec<StoredMinimalCandidateSelectionRow>, crate::DataLayerError>;
}

pub trait MinimalCandidateSelectionRepository:
    MinimalCandidateSelectionReadRepository + Send + Sync
{
}

impl<T> MinimalCandidateSelectionRepository for T where
    T: MinimalCandidateSelectionReadRepository + Send + Sync
{
}

#[cfg(test)]
mod tests {
    use super::{provider_model_mapping_api_format_covers, StoredMinimalCandidateRoutingFacts};
    use serde_json::json;

    /// 验证紧凑认证投影保留格式覆盖与“未配置即允许 mismatch”的既有合同。
    #[test]
    fn routing_facts_preserve_effective_auth_channel_policy() {
        assert!(
            StoredMinimalCandidateRoutingFacts::default()
                .key_allows_auth_channel_mismatch_for_endpoint_format
        );
        let unspecified = StoredMinimalCandidateRoutingFacts::from_safe_projection(
            false,
            false,
            "api_key",
            "openai:chat",
            Some(&json!({"openai:chat": "bearer"})),
            None,
            None,
            None,
            None,
        );
        assert_eq!(unspecified.key_auth_type_for_endpoint_format, "bearer");
        assert!(unspecified.key_allows_auth_channel_mismatch_for_endpoint_format);

        let explicitly_limited = StoredMinimalCandidateRoutingFacts::from_safe_projection(
            false,
            false,
            "bearer",
            "/v1/chat/completions",
            None,
            Some(&json!(["claude:messages"])),
            None,
            None,
            None,
        );
        assert!(!explicitly_limited.key_allows_auth_channel_mismatch_for_endpoint_format);

        let explicitly_allowed = StoredMinimalCandidateRoutingFacts::from_safe_projection(
            false,
            false,
            "bearer",
            "/v1/chat/completions",
            None,
            Some(&json!(["openai:chat"])),
            None,
            None,
            None,
        );
        assert!(explicitly_allowed.key_allows_auth_channel_mismatch_for_endpoint_format);
    }

    #[test]
    fn legacy_responses_mapping_is_only_compatible_with_codex_live() {
        assert!(provider_model_mapping_api_format_covers(
            "codex",
            "openai:responses",
            "codex:live"
        ));
        assert!(provider_model_mapping_api_format_covers(
            " CoDeX ",
            "/v1/responses",
            "codex:live"
        ));

        for provider_type in ["openai", "custom", "chatgpt_web"] {
            assert!(!provider_model_mapping_api_format_covers(
                provider_type,
                "openai:responses",
                "codex:live"
            ));
        }
        for requested_api_format in ["openai:chat", "claude:messages", "openai:image"] {
            assert!(!provider_model_mapping_api_format_covers(
                "codex",
                "openai:responses",
                requested_api_format
            ));
        }
    }
}
