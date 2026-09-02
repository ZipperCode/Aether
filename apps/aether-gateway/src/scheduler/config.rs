use aether_data_contracts::repository::routing_profiles::RoutingGroupLookupKey;
use aether_routing_core::{
    ResolvedRoutingPolicy, RoutingDefaultPolicy, RoutingSchedulingMode, RoutingSetPriorityMode,
    DEFAULT_STICKY_KEY_ATTEMPTS,
};
use aether_scheduler_core::SchedulerPriorityMode;
use tracing::warn;

use crate::{AppState, GatewayError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// 候选在同一优先级内的调度方式，序列化名称与路由策略保持一致。
pub(crate) enum SchedulerSchedulingMode {
    /// 严格按既定顺序选择。
    FixedOrder,
    #[default]
    /// 优先复用客户端会话命中的缓存 Key。
    CacheAffinity,
    /// 按运行时负载评分选择。
    LoadBalance,
}

impl SchedulerSchedulingMode {
    /// 返回配置和报告使用的稳定蛇形名称。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::FixedOrder => "fixed_order",
            Self::CacheAffinity => "cache_affinity",
            Self::LoadBalance => "load_balance",
        }
    }
}

/// 返回优先级模式的稳定配置名称。
pub(crate) fn scheduler_priority_mode_as_str(mode: SchedulerPriorityMode) -> &'static str {
    match mode {
        SchedulerPriorityMode::Provider => "provider",
        SchedulerPriorityMode::GlobalKey => "global_key",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// 单次请求已解析出的调度排序配置；请求级策略存在时不得再混入旧全局配置。
pub(crate) struct SchedulerOrderingConfig {
    /// 先按 Provider 还是全局 Key 优先级排序。
    pub(crate) priority_mode: SchedulerPriorityMode,
    /// 同优先级候选的固定、缓存亲和或负载均衡模式。
    pub(crate) scheduling_mode: SchedulerSchedulingMode,
    /// 跨格式转换时是否继续保留原优先级。
    pub(crate) keep_priority_on_conversion: bool,
    /// 首位粘性候选在故障转移前的总尝试次数。
    pub(crate) sticky_key_attempts: u32,
}

impl Default for SchedulerOrderingConfig {
    /// 使用路由层的稳定默认值构造无显式配置时的排序行为。
    fn default() -> Self {
        Self {
            priority_mode: SchedulerPriorityMode::Provider,
            scheduling_mode: SchedulerSchedulingMode::CacheAffinity,
            keep_priority_on_conversion: false,
            sticky_key_attempts: DEFAULT_STICKY_KEY_ATTEMPTS,
        }
    }
}

impl SchedulerOrderingConfig {
    /// 从已解析路由策略构造排序配置；该策略是唯一来源，不合并旧系统值。
    pub(crate) fn from_routing_policy(policy: &ResolvedRoutingPolicy) -> Self {
        Self {
            priority_mode: scheduler_priority_mode_from_routing(policy.priority_mode),
            scheduling_mode: scheduler_scheduling_mode_from_routing(policy.scheduling_mode),
            keep_priority_on_conversion: policy.keep_priority_on_conversion,
            sticky_key_attempts: policy.sticky_key_attempts,
        }
    }

    /// 从系统默认组的默认策略构造排序配置。
    pub(crate) fn from_routing_default_policy(policy: &RoutingDefaultPolicy) -> Self {
        Self {
            priority_mode: scheduler_priority_mode_from_routing(policy.priority_mode),
            scheduling_mode: scheduler_scheduling_mode_from_routing(policy.scheduling_mode),
            keep_priority_on_conversion: policy.keep_priority_on_conversion,
            sticky_key_attempts: policy.sticky_key_attempts,
        }
    }

    /// 将旧调度配置投影为可持久化的默认路由策略，供一次性引导创建默认组。
    pub(crate) fn to_routing_default_policy(self) -> RoutingDefaultPolicy {
        RoutingDefaultPolicy {
            priority_mode: match self.priority_mode {
                SchedulerPriorityMode::Provider => RoutingSetPriorityMode::Provider,
                SchedulerPriorityMode::GlobalKey => RoutingSetPriorityMode::GlobalKey,
            },
            scheduling_mode: match self.scheduling_mode {
                SchedulerSchedulingMode::FixedOrder => RoutingSchedulingMode::FixedOrder,
                SchedulerSchedulingMode::CacheAffinity => RoutingSchedulingMode::CacheAffinity,
                SchedulerSchedulingMode::LoadBalance => RoutingSchedulingMode::LoadBalance,
            },
            keep_priority_on_conversion: self.keep_priority_on_conversion,
            sticky_key_attempts: self.sticky_key_attempts,
        }
    }

    /// 返回当前优先级模式的报告字段值。
    pub(crate) fn priority_mode_str(self) -> &'static str {
        scheduler_priority_mode_as_str(self.priority_mode)
    }

    /// 返回当前调度模式的报告字段值。
    pub(crate) fn scheduling_mode_str(self) -> &'static str {
        self.scheduling_mode.as_str()
    }
}

/// 将路由核心优先级枚举映射到调度器枚举。
fn scheduler_priority_mode_from_routing(mode: RoutingSetPriorityMode) -> SchedulerPriorityMode {
    match mode {
        RoutingSetPriorityMode::Provider => SchedulerPriorityMode::Provider,
        RoutingSetPriorityMode::GlobalKey => SchedulerPriorityMode::GlobalKey,
    }
}

/// 将路由核心调度枚举映射到网关调度器枚举。
fn scheduler_scheduling_mode_from_routing(mode: RoutingSchedulingMode) -> SchedulerSchedulingMode {
    match mode {
        RoutingSchedulingMode::FixedOrder => SchedulerSchedulingMode::FixedOrder,
        RoutingSchedulingMode::CacheAffinity => SchedulerSchedulingMode::CacheAffinity,
        RoutingSchedulingMode::LoadBalance => SchedulerSchedulingMode::LoadBalance,
    }
}

pub(crate) fn parse_scheduler_priority_mode(
    value: Option<&serde_json::Value>,
) -> SchedulerPriorityMode {
    match value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("global_key") => SchedulerPriorityMode::GlobalKey,
        _ => SchedulerPriorityMode::Provider,
    }
}

pub(crate) fn parse_keep_priority_on_conversion(value: Option<&serde_json::Value>) -> bool {
    value.and_then(serde_json::Value::as_bool).unwrap_or(false)
}

pub(crate) fn parse_scheduler_scheduling_mode(
    value: Option<&serde_json::Value>,
) -> SchedulerSchedulingMode {
    match value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("fixed_order") => SchedulerSchedulingMode::FixedOrder,
        Some("load_balance") => SchedulerSchedulingMode::LoadBalance,
        _ => SchedulerSchedulingMode::CacheAffinity,
    }
}

/// 读取无请求级策略时的有效排序配置：先取启用的系统默认路由组，再回退旧系统键。
/// 旧键只承担迁移兜底，缺失时使用稳定默认值。
pub(crate) async fn read_scheduler_ordering_config(
    state: &AppState,
) -> Result<SchedulerOrderingConfig, GatewayError> {
    if let Some(config) = read_system_default_routing_ordering_config(state).await? {
        return Ok(config);
    }
    read_legacy_scheduler_ordering_config(state).await
}

/// 从启用的系统默认路由组读取排序配置；组缺失、禁用或配置无效时返回空。
pub(crate) async fn read_system_default_routing_ordering_config(
    state: &AppState,
) -> Result<Option<SchedulerOrderingConfig>, GatewayError> {
    let Some(group) = state
        .find_routing_group(RoutingGroupLookupKey::SystemDefault)
        .await?
        .filter(|group| group.enabled)
    else {
        return Ok(None);
    };
    let default_policy = match group.config_json.get("default_policy") {
        None | Some(serde_json::Value::Null) => RoutingDefaultPolicy::default(),
        Some(value) => match serde_json::from_value::<RoutingDefaultPolicy>(value.clone()) {
            Ok(policy) => policy,
            Err(error) => {
                warn!(
                    event_name = "scheduler_system_default_routing_policy_invalid",
                    log_type = "event",
                    group_id = %group.id,
                    error = %error,
                    "system default routing group has an invalid default_policy; ignoring it"
                );
                return Ok(None);
            }
        },
    };
    Ok(Some(SchedulerOrderingConfig::from_routing_default_policy(
        &default_policy,
    )))
}

/// 从旧系统配置键读取排序配置，仅作为尚未引导出默认路由组时的迁移兜底。
pub(crate) async fn read_legacy_scheduler_ordering_config(
    state: &AppState,
) -> Result<SchedulerOrderingConfig, GatewayError> {
    let priority_mode = parse_scheduler_priority_mode(
        state
            .read_system_config_json_value("provider_priority_mode")
            .await?
            .as_ref(),
    );
    let scheduling_mode = parse_scheduler_scheduling_mode(
        state
            .read_system_config_json_value("scheduling_mode")
            .await?
            .as_ref(),
    );
    let keep_priority_on_conversion = parse_keep_priority_on_conversion(
        state
            .read_system_config_json_value("keep_priority_on_conversion")
            .await?
            .as_ref(),
    );
    Ok(SchedulerOrderingConfig {
        priority_mode,
        scheduling_mode,
        keep_priority_on_conversion,
        // 旧配置没有粘性尝试字段，统一采用路由默认值。
        sticky_key_attempts: DEFAULT_STICKY_KEY_ATTEMPTS,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aether_data::repository::routing_profiles::InMemoryRoutingGroupRepository;
    use aether_data_contracts::repository::routing_profiles::{
        CreateRoutingGroupRecord, RoutingGroupLookupKey, RoutingGroupReadRepository,
        RoutingGroupWriteRepository,
    };
    use serde_json::json;

    use super::*;
    use crate::data::GatewayDataState;

    /// 构造与新默认策略明显不同的旧配置，便于验证来源优先级。
    fn legacy_values() -> [(String, serde_json::Value); 3] {
        [
            ("provider_priority_mode".to_string(), json!("global_key")),
            ("scheduling_mode".to_string(), json!("load_balance")),
            ("keep_priority_on_conversion".to_string(), json!(true)),
        ]
    }

    /// 在内存仓库写入测试用系统默认组，并允许控制启用状态和策略内容。
    async fn create_system_default(
        repository: &InMemoryRoutingGroupRepository,
        enabled: bool,
        config_json: serde_json::Value,
    ) {
        repository
            .create_routing_group(CreateRoutingGroupRecord {
                id: "system-default".to_string(),
                name: "system-default".to_string(),
                description: None,
                enabled,
                is_system_default: true,
                config_json,
                version: 1,
                created_at: 1,
                updated_at: 1,
                published_at: None,
            })
            .await
            .unwrap();
    }

    /// 验证启用的系统默认路由组覆盖旧系统配置键。
    #[tokio::test]
    async fn system_default_routing_group_overrides_legacy_keys() {
        let repository = Arc::new(InMemoryRoutingGroupRepository::default());
        create_system_default(
            &repository,
            true,
            json!({
                "default_policy": {
                    "priority_mode": "provider",
                    "scheduling_mode": "fixed_order",
                    "keep_priority_on_conversion": false
                }
            }),
        )
        .await;
        let state = AppState::new().unwrap().with_data_state_for_tests(
            GatewayDataState::disabled()
                .with_system_config_values_for_tests(legacy_values())
                .with_routing_group_repository_for_tests(repository),
        );

        let config = read_scheduler_ordering_config(&state).await.unwrap();

        assert_eq!(config.priority_mode, SchedulerPriorityMode::Provider);
        assert_eq!(config.scheduling_mode, SchedulerSchedulingMode::FixedOrder);
        assert!(!config.keep_priority_on_conversion);
    }

    /// 验证默认组缺少 `default_policy` 时采用路由核心默认值。
    #[tokio::test]
    async fn missing_default_policy_in_system_default_group_uses_routing_defaults() {
        let repository = Arc::new(InMemoryRoutingGroupRepository::default());
        create_system_default(&repository, true, json!({})).await;
        let state = AppState::new().unwrap().with_data_state_for_tests(
            GatewayDataState::disabled()
                .with_system_config_values_for_tests(legacy_values())
                .with_routing_group_repository_for_tests(repository),
        );

        let config = read_scheduler_ordering_config(&state).await.unwrap();

        assert_eq!(config, SchedulerOrderingConfig::default());
    }

    /// 验证默认组禁用或仓库缺失时仍可回退旧配置。
    #[tokio::test]
    async fn disabled_or_missing_system_default_group_falls_back_to_legacy_keys() {
        let repository = Arc::new(InMemoryRoutingGroupRepository::default());
        create_system_default(
            &repository,
            false,
            json!({"default_policy": {"scheduling_mode": "fixed_order"}}),
        )
        .await;
        let with_disabled_group = AppState::new().unwrap().with_data_state_for_tests(
            GatewayDataState::disabled()
                .with_system_config_values_for_tests(legacy_values())
                .with_routing_group_repository_for_tests(repository),
        );
        let without_repository = AppState::new().unwrap().with_data_state_for_tests(
            GatewayDataState::disabled().with_system_config_values_for_tests(legacy_values()),
        );

        for state in [with_disabled_group, without_repository] {
            let config = read_scheduler_ordering_config(&state).await.unwrap();
            assert_eq!(config.priority_mode, SchedulerPriorityMode::GlobalKey);
            assert_eq!(config.scheduling_mode, SchedulerSchedulingMode::LoadBalance);
            assert!(config.keep_priority_on_conversion);
        }
    }

    /// 验证引导过程只创建一次默认组，并完整继承旧排序行为。
    #[tokio::test]
    async fn bootstrap_creates_system_default_group_from_legacy_keys_once() {
        let repository = Arc::new(InMemoryRoutingGroupRepository::default());
        let state = AppState::new().unwrap().with_data_state_for_tests(
            GatewayDataState::disabled()
                .with_system_config_values_for_tests(legacy_values())
                .with_routing_group_repository_for_tests(repository.clone()),
        );

        let created = state
            .ensure_system_default_routing_group_inner()
            .await
            .unwrap()
            .expect("first bootstrap should create the system default group");
        assert!(created.enabled);
        assert!(created.is_system_default);
        assert_eq!(
            created.config_json["default_policy"],
            json!({
                "priority_mode": "global_key",
                "scheduling_mode": "load_balance",
                "keep_priority_on_conversion": true,
                "sticky_key_attempts": DEFAULT_STICKY_KEY_ATTEMPTS
            })
        );

        let second = state
            .ensure_system_default_routing_group_inner()
            .await
            .unwrap();
        assert!(second.is_none(), "bootstrap must be idempotent");
        assert_eq!(
            repository
                .find_routing_group(RoutingGroupLookupKey::SystemDefault)
                .await
                .unwrap()
                .map(|group| group.id),
            Some(created.id)
        );

        let config = read_scheduler_ordering_config(&state).await.unwrap();
        assert_eq!(config.priority_mode, SchedulerPriorityMode::GlobalKey);
        assert_eq!(config.scheduling_mode, SchedulerSchedulingMode::LoadBalance);
        assert!(config.keep_priority_on_conversion);
    }
}
