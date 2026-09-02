use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingRulePhase {
    #[default]
    ClientRequest,
    ProviderRequest,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingSetPriorityMode {
    #[default]
    Provider,
    GlobalKey,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingSchedulingMode {
    #[default]
    CacheAffinity,
    LoadBalance,
    FixedOrder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum RoutingJsonPatchOperation {
    Add { path: String, value: Value },
    Replace { path: String, value: Value },
    Remove { path: String },
}

impl RoutingJsonPatchOperation {
    pub fn path(&self) -> &str {
        match self {
            Self::Add { path, .. } | Self::Replace { path, .. } | Self::Remove { path } => path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum RoutingHeaderPatch {
    Set { name: String, value: String },
    Remove { name: String },
}

impl RoutingHeaderPatch {
    pub fn name(&self) -> &str {
        match self {
            Self::Set { name, .. } | Self::Remove { name } => name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
/// 路由规则可执行的排序、调度和请求变更动作集合。
pub enum RoutingAction {
    RestrictModels {
        models: Vec<String>,
    },
    RestrictProviders {
        provider_ids: Vec<String>,
    },
    RestrictKeys {
        key_ids: Vec<String>,
    },
    SetScheduling {
        priority_mode: Option<RoutingSetPriorityMode>,
        scheduling_mode: Option<RoutingSchedulingMode>,
        keep_priority_on_conversion: Option<bool>,
        /// 首位粘性候选的总尝试次数；为空时不覆盖当前策略预算。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sticky_key_attempts: Option<u32>,
    },
    SetProviderPriority {
        provider_id: String,
        priority: i32,
    },
    SetKeyPriority {
        key_id: String,
        priority: i32,
        /// 指定时只覆盖该 API 格式下的 Key 优先级；为空时对该 Key 的所有格式生效。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        api_format: Option<String>,
    },
    JsonPatchBody {
        patch: Vec<RoutingJsonPatchOperation>,
    },
    PatchHeaders {
        patch: Vec<RoutingHeaderPatch>,
    },
}
