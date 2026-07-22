use std::collections::BTreeMap;

use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogEndpoint;
use serde_json::Value;

use crate::capability::{ProviderPoolCapabilities, ProviderQuotaServingPolicy};
use crate::provider::{provider_pool_matching_endpoint, ProviderPoolAdapter};
use crate::quota::provider_pool_current_unix_secs;
use crate::quota_refresh::ProviderPoolQuotaRequestSpec;
use crate::quota_snapshot::{ProviderQuotaBalance, ProviderQuotaSnapshotContract};

use super::official_balance::{decimal_string, endpoint_has_official_origin};
use super::official_subscription::{parse_kimi_coding_subscription, parse_zhipu_subscription};

pub const MOONSHOT_BALANCE_URL: &str = "https://api.moonshot.cn/v1/users/me/balance";
pub const KIMI_CODING_USAGE_URL: &str = "https://api.kimi.com/coding/v1/usages";
pub const SILICONFLOW_CN_BALANCE_URL: &str = "https://api.siliconflow.cn/v1/user/info";
pub const SILICONFLOW_GLOBAL_BALANCE_URL: &str = "https://api.siliconflow.com/v1/user/info";
pub const ZHIPU_QUOTA_URL: &str = "https://bigmodel.cn/api/monitor/usage/quota/limit";
pub const ZAI_QUOTA_URL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";

fn official_host(provider_type: &str, endpoint: &StoredProviderCatalogEndpoint) -> bool {
    match provider_type {
        "moonshot" => endpoint_has_official_origin(endpoint, "api.moonshot.cn"),
        "kimi_coding" => endpoint_has_official_origin(endpoint, "api.kimi.com"),
        "siliconflow" => {
            endpoint_has_official_origin(endpoint, "api.siliconflow.cn")
                || endpoint_has_official_origin(endpoint, "api.siliconflow.com")
        }
        "zhipu" => {
            endpoint_has_official_origin(endpoint, "bigmodel.cn")
                || endpoint_has_official_origin(endpoint, "open.bigmodel.cn")
        }
        "zai" => endpoint_has_official_origin(endpoint, "api.z.ai"),
        _ => false,
    }
}

pub fn is_official_api_key_quota_endpoint(
    provider_type: &str,
    endpoint: &StoredProviderCatalogEndpoint,
) -> bool {
    official_host(provider_type, endpoint)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficialApiKeyQuotaProvider {
    Moonshot,
    KimiCoding,
    SiliconFlow,
    Zhipu,
    Zai,
}

impl OfficialApiKeyQuotaProvider {
    pub const fn provider_type(self) -> &'static str {
        match self {
            Self::Moonshot => "moonshot",
            Self::KimiCoding => "kimi_coding",
            Self::SiliconFlow => "siliconflow",
            Self::Zhipu => "zhipu",
            Self::Zai => "zai",
        }
    }

    pub const fn quota_serving_policy(self) -> ProviderQuotaServingPolicy {
        match self {
            Self::Moonshot | Self::SiliconFlow => ProviderQuotaServingPolicy::ObservationOnly,
            Self::KimiCoding | Self::Zhipu | Self::Zai => {
                ProviderQuotaServingPolicy::SubscriptionExhaustionOnly
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct OfficialApiKeyQuotaProviderPoolAdapter(pub OfficialApiKeyQuotaProvider);

impl ProviderPoolAdapter for OfficialApiKeyQuotaProviderPoolAdapter {
    fn provider_type(&self) -> &'static str {
        self.0.provider_type()
    }

    fn quota_serving_policy(&self) -> Option<ProviderQuotaServingPolicy> {
        Some(self.0.quota_serving_policy())
    }

    fn capabilities(&self) -> ProviderPoolCapabilities {
        ProviderPoolCapabilities {
            quota_refresh: true,
            ..Default::default()
        }
    }

    fn quota_refresh_endpoint(
        &self,
        endpoints: &[StoredProviderCatalogEndpoint],
        include_inactive: bool,
    ) -> Option<StoredProviderCatalogEndpoint> {
        provider_pool_matching_endpoint(endpoints, include_inactive, |endpoint| {
            official_host(self.0.provider_type(), endpoint)
        })
    }
}

pub fn build_official_api_key_quota_request<F>(
    provider_type: &str,
    key_id: &str,
    endpoint: &StoredProviderCatalogEndpoint,
    read_secret: F,
) -> Result<ProviderPoolQuotaRequestSpec, &'static str>
where
    F: FnOnce() -> String,
{
    if !official_host(provider_type, endpoint) {
        return Err("quota unsupported for non-official endpoint");
    }
    let url = match provider_type {
        "moonshot" => MOONSHOT_BALANCE_URL,
        "kimi_coding" => KIMI_CODING_USAGE_URL,
        "siliconflow" if endpoint_has_official_origin(endpoint, "api.siliconflow.cn") => {
            SILICONFLOW_CN_BALANCE_URL
        }
        "siliconflow" => SILICONFLOW_GLOBAL_BALANCE_URL,
        "zhipu" => ZHIPU_QUOTA_URL,
        "zai" => ZAI_QUOTA_URL,
        _ => return Err("unsupported official API key quota provider"),
    };
    let secret = read_secret();
    let authorization = if matches!(provider_type, "zhipu" | "zai") {
        secret.trim().to_string()
    } else {
        format!("Bearer {}", secret.trim())
    };
    Ok(ProviderPoolQuotaRequestSpec {
        request_id: format!("{provider_type}-quota-{key_id}"),
        provider_name: provider_type.into(),
        quota_kind: if matches!(provider_type, "kimi_coding" | "zhipu" | "zai") {
            "subscription"
        } else {
            "balance"
        }
        .into(),
        method: "GET".into(),
        url: url.into(),
        headers: BTreeMap::from([
            ("accept".into(), "application/json".into()),
            ("authorization".into(), authorization),
        ]),
        content_type: None,
        json_body: None,
        client_api_format: "openai:chat".into(),
        provider_api_format: "openai:chat".into(),
        model_name: None,
        accept_invalid_certs: false,
    })
}

pub fn parse_official_api_key_quota(
    provider_type: &str,
    value: &Value,
) -> Result<ProviderQuotaSnapshotContract, &'static str> {
    let now_unix_secs = if matches!(provider_type, "kimi_coding" | "zhipu" | "zai") {
        provider_pool_current_unix_secs().ok_or("system clock unavailable")?
    } else {
        0
    };
    parse_official_api_key_quota_at(provider_type, value, now_unix_secs)
}

fn parse_official_api_key_quota_at(
    provider_type: &str,
    value: &Value,
    now_unix_secs: u64,
) -> Result<ProviderQuotaSnapshotContract, &'static str> {
    match provider_type {
        "moonshot" => {
            let data = value.get("data").ok_or("missing data")?;
            let available = data.get("available_balance").and_then(decimal_string);
            let cash = data.get("cash_balance").and_then(decimal_string);
            let voucher = data.get("voucher_balance").and_then(decimal_string);
            if available.is_none() {
                return Err("missing available_balance");
            }
            Ok(ProviderQuotaSnapshotContract::balance(
                "moonshot",
                vec![ProviderQuotaBalance {
                    unit: "CNY".into(),
                    available,
                    total: None,
                    granted: voucher,
                    topped_up: cash,
                    used: None,
                }],
            ))
        }
        "kimi_coding" => {
            let parsed = parse_kimi_coding_subscription(value)?;
            let mut snapshot = ProviderQuotaSnapshotContract::subscription(
                "kimi_coding",
                parsed.windows,
                now_unix_secs,
            );
            snapshot.extensions = parsed.extensions;
            Ok(snapshot)
        }
        "siliconflow" => {
            let data = value.get("data").ok_or("missing data")?;
            let available = data.get("totalBalance").and_then(decimal_string);
            if available.is_none() {
                return Err("missing totalBalance");
            }
            let mut snapshot = ProviderQuotaSnapshotContract::balance(
                "siliconflow",
                vec![ProviderQuotaBalance {
                    unit: "CNY".into(),
                    available,
                    total: None,
                    granted: data.get("balance").and_then(decimal_string),
                    topped_up: data.get("chargeBalance").and_then(decimal_string),
                    used: None,
                }],
            );
            if let Some(status) = data.get("status").filter(|v| !v.is_null()) {
                snapshot
                    .extensions
                    .insert("account_status".into(), status.clone());
            }
            Ok(snapshot)
        }
        "zhipu" | "zai" => {
            let parsed = parse_zhipu_subscription(value)?;
            let mut snapshot = ProviderQuotaSnapshotContract::subscription(
                provider_type,
                parsed.windows,
                now_unix_secs,
            );
            snapshot.extensions = parsed.extensions;
            Ok(snapshot)
        }
        _ => Err("unsupported official API key quota provider"),
    }
}

#[cfg(test)]
#[path = "official_api_key_tests.rs"]
mod tests;
