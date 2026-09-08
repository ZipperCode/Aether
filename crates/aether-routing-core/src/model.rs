use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::actions::{
    RoutingAction, RoutingRulePhase, RoutingSchedulingMode, RoutingSetPriorityMode,
};
use crate::conditions::RoutingCondition;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingSchedulingPreset {
    pub preset: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
/// 单个 Pool 的调度预设覆盖集合。
pub struct RoutingPoolPolicyOverride {
    #[serde(default)]
    pub scheduling_presets: Vec<RoutingSchedulingPreset>,
}

/// Default number of attempts on the first-ranked (sticky) candidate before
/// failing over: one retry on the same key.
pub const DEFAULT_STICKY_KEY_ATTEMPTS: u32 = 2;

/// Request-independent execution behaviours selected by a routing strategy.
///
/// These flags deliberately live beside scheduling rather than in provider
/// transport configuration. A resolved policy is snapshotted for the request
/// and can therefore be consumed by execution without rereading mutable
/// system settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct RoutingExecutionPolicy {
    #[serde(default, skip_serializing_if = "is_false")]
    pub enable_cf_heartbeat: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub cyber_continue_failover: bool,
}

impl<'de> Deserialize<'de> for RoutingExecutionPolicy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize, Default)]
        struct LegacyCompatibleExecutionPolicy {
            #[serde(default)]
            enable_cf_heartbeat: bool,
            #[serde(default)]
            enable_openai_image_sync_heartbeat: bool,
            #[serde(default)]
            enable_standard_text_sync_heartbeat: bool,
            #[serde(default)]
            cyber_continue_failover: bool,
        }

        let value = LegacyCompatibleExecutionPolicy::deserialize(deserializer)?;
        Ok(Self {
            enable_cf_heartbeat: value.enable_cf_heartbeat
                || value.enable_openai_image_sync_heartbeat
                || value.enable_standard_text_sync_heartbeat,
            cyber_continue_failover: value.cyber_continue_failover,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingDefaultPolicy {
    /// 候选按 Provider 或全局 Key 优先级排序。
    #[serde(default)]
    pub priority_mode: RoutingSetPriorityMode,
    /// 同优先级候选的调度方式。
    #[serde(default)]
    pub scheduling_mode: RoutingSchedulingMode,
    /// 跨格式转换时是否继续保留原优先级。
    #[serde(default)]
    pub keep_priority_on_conversion: bool,
    /// 首位候选在切换前的总尝试数；后续候选各一次，`0` 和 `1` 均不重试。
    #[serde(default = "default_sticky_key_attempts")]
    pub sticky_key_attempts: u32,
    /// 路由策略冻结的执行行为开关，与排序配置共同作用于当前请求。
    #[serde(flatten)]
    pub execution_policy: RoutingExecutionPolicy,
}

impl Default for RoutingDefaultPolicy {
    /// 构造兼容旧配置的默认策略，并补入默认粘性尝试预算。
    fn default() -> Self {
        Self {
            priority_mode: RoutingSetPriorityMode::default(),
            scheduling_mode: RoutingSchedulingMode::default(),
            keep_priority_on_conversion: false,
            sticky_key_attempts: DEFAULT_STICKY_KEY_ATTEMPTS,
            execution_policy: RoutingExecutionPolicy::default(),
        }
    }
}

/// 为缺少新字段的 JSON 提供稳定默认粘性尝试数。
fn default_sticky_key_attempts() -> u32 {
    DEFAULT_STICKY_KEY_ATTEMPTS
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
/// 单个模型的可用 Provider/Key 及其优先级覆盖配置。
pub struct RoutingModelPolicy {
    pub model: String,
    #[serde(default)]
    pub allowed_providers: Vec<String>,
    #[serde(default)]
    pub allowed_keys: Vec<String>,
    #[serde(default)]
    pub provider_priority_overrides: BTreeMap<String, i32>,
    #[serde(default)]
    pub key_priority_overrides: BTreeMap<String, i32>,
    /// 按 API 格式隔离的 Key 优先级：`api_format -> key_id -> priority`。
    /// 格式匹配时优先于不区分格式的 `key_priority_overrides`。
    #[serde(default)]
    pub key_priority_overrides_by_format: BTreeMap<String, BTreeMap<String, i32>>,
    #[serde(default)]
    pub pool_priority_overrides: BTreeMap<String, i32>,
    #[serde(default)]
    pub pool_policy_overrides: BTreeMap<String, RoutingPoolPolicyOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub phase: RoutingRulePhase,
    #[serde(default)]
    pub conditions: RoutingCondition,
    #[serde(default)]
    pub actions: Vec<RoutingAction>,
    #[serde(default)]
    pub stop_processing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RoutingGroupConfig {
    /// The default policy is global for the selected strategy group. Model
    /// differences are expressed through `model_policies` and `rules`.
    #[serde(default)]
    pub default_policy: RoutingDefaultPolicy,
    #[serde(default)]
    pub model_policies: Vec<RoutingModelPolicy>,
    #[serde(default)]
    pub rules: Vec<RoutingRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingGroupRecord {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub enabled: bool,
    pub is_system_default: bool,
    pub config_json: Value,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingGroupBindingSubject {
    User,
    ApiKey,
    UserGroup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingGroupBinding {
    pub id: String,
    pub group_id: String,
    pub subject_type: RoutingGroupBindingSubject,
    pub subject_id: String,
    pub is_default: bool,
    pub allow_explicit_select: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingGroupVersionRecord {
    pub id: String,
    pub group_id: String,
    pub version: i64,
    pub config_json: Value,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

fn default_true() -> bool {
    true
}
