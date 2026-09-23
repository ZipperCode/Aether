use std::collections::BTreeMap;

use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogEndpoint;
use serde_json::Value;

use super::minimax::parse_minimax_quota;
use super::official_balance::{
    decimal_string, endpoint_has_official_origin, official_quota_source,
};
use super::official_subscription::{parse_kimi_coding_subscription, parse_zhipu_subscription};
use crate::capability::{ProviderPoolCapabilities, ProviderQuotaServingPolicy};
use crate::provider::{provider_pool_matching_endpoint, ProviderPoolAdapter};
use crate::quota::provider_pool_current_unix_secs;
use crate::quota_refresh::ProviderPoolQuotaRequestSpec;
use crate::quota_snapshot::{
    ProviderQuotaBalance, ProviderQuotaQueryStatus, ProviderQuotaSnapshotContract,
    ProviderQuotaSource,
};

pub const MOONSHOT_BALANCE_URL: &str = "https://api.moonshot.cn/v1/users/me/balance";
pub const MOONSHOT_GLOBAL_BALANCE_URL: &str = "https://api.moonshot.ai/v1/users/me/balance";
pub const KIMI_CODING_USAGE_URL: &str = "https://api.kimi.com/coding/v1/usages";
pub const KIMI_CODING_GLOBAL_USAGE_URL: &str = "https://api.kimi.ai/coding/v1/usages";
pub const SILICONFLOW_GLOBAL_BALANCE_URL: &str = "https://api.siliconflow.com/v1/user/info";
pub const ZHIPU_QUOTA_URL: &str = "https://open.bigmodel.cn/api/monitor/usage/quota/limit";
pub const ZHIPU_TEAM_QUOTA_URL: &str =
    "https://open.bigmodel.cn/api/monitor/usage/quota/limit?type=2";
pub const ZHIPU_ACCOUNT_REPORT_URL: &str =
    "https://open.bigmodel.cn/api/biz/account/query-customer-account-report";
pub const ZAI_QUOTA_URL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";

/// 官方 API Key 供商的额度查询域名；端点匹配与出站 URL origin 校验共用，避免两套白名单漂移。
const OFFICIAL_API_KEY_QUOTA_HOSTS: &[(&str, &[&str])] = &[
    ("moonshot", &["api.moonshot.cn", "api.moonshot.ai"]),
    ("kimi_coding", &["api.kimi.com", "api.kimi.ai"]),
    (
        "siliconflow",
        &["api.siliconflow.cn", "api.siliconflow.com"],
    ),
    ("zhipu", &["bigmodel.cn", "open.bigmodel.cn"]),
    ("zai", &["api.z.ai"]),
    ("minimax", &["api.minimaxi.com", "api.minimax.io"]),
];

fn official_api_key_quota_hosts(provider_type: &str) -> Option<&'static [&'static str]> {
    OFFICIAL_API_KEY_QUOTA_HOSTS
        .iter()
        .find(|(name, _)| *name == provider_type)
        .map(|(_, hosts)| *hosts)
}

fn official_host(provider_type: &str, endpoint: &StoredProviderCatalogEndpoint) -> bool {
    official_api_key_quota_hosts(provider_type).is_some_and(|hosts| {
        hosts
            .iter()
            .any(|host| endpoint_has_official_origin(endpoint, host))
    })
}

/// 额度请求出站 URL 的 host 校验复用与端点匹配相同的官方域名表。
pub fn official_api_key_quota_url_host_is_allowed(provider_type: &str, host: &str) -> bool {
    official_api_key_quota_hosts(provider_type).is_some_and(|hosts| {
        hosts
            .iter()
            .any(|allowed| host.eq_ignore_ascii_case(allowed))
    })
}

pub fn is_official_api_key_quota_endpoint(
    provider_type: &str,
    endpoint: &StoredProviderCatalogEndpoint,
) -> bool {
    official_host(provider_type, endpoint)
}

/// 国内站余额接口已于 2026-08-14 退役；调用方可在读取凭据或传输配置前识别。
pub fn is_retired_official_api_key_quota_endpoint(
    provider_type: &str,
    endpoint: &StoredProviderCatalogEndpoint,
) -> bool {
    provider_type == "siliconflow" && endpoint_has_official_origin(endpoint, "api.siliconflow.cn")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficialApiKeyQuotaProvider {
    Moonshot,
    KimiCoding,
    SiliconFlow,
    Zhipu,
    Zai,
    MiniMax,
}

impl OfficialApiKeyQuotaProvider {
    pub const fn provider_type(self) -> &'static str {
        match self {
            Self::Moonshot => "moonshot",
            Self::KimiCoding => "kimi_coding",
            Self::SiliconFlow => "siliconflow",
            Self::Zhipu => "zhipu",
            Self::Zai => "zai",
            Self::MiniMax => "minimax",
        }
    }

    pub const fn quota_serving_policy(self) -> ProviderQuotaServingPolicy {
        match self {
            Self::Moonshot | Self::SiliconFlow | Self::MiniMax => {
                ProviderQuotaServingPolicy::ObservationOnly
            }
            Self::KimiCoding | Self::Zhipu | Self::Zai => {
                ProviderQuotaServingPolicy::SubscriptionExhaustionOnly
            }
        }
    }
}

fn official_quota_region(
    provider_type: &str,
    endpoint: &StoredProviderCatalogEndpoint,
) -> &'static str {
    let global_host = match provider_type {
        "moonshot" => "api.moonshot.ai",
        "kimi_coding" => "api.kimi.ai",
        "siliconflow" => "api.siliconflow.com",
        "minimax" => "api.minimax.io",
        "zai" => "api.z.ai",
        "openrouter" => "openrouter.ai",
        _ => return "cn",
    };
    if endpoint_has_official_origin(endpoint, global_host) {
        "global"
    } else {
        "cn"
    }
}

/// 请求失败前也能确定查询的产品来源，网关据此仅保留同一来源的历史值。
pub fn official_api_key_quota_sources(
    provider_type: &str,
    endpoint: &StoredProviderCatalogEndpoint,
    quota_kind: &str,
) -> Vec<ProviderQuotaSource> {
    let mut sources = match (provider_type, quota_kind) {
        ("openrouter", _) => vec![
            official_quota_source("key_limit", "Key 消费限额", "key_spending_limit", "key"),
            // 免费请求数与金额限额同接口返回；请求前声明来源身份，
            // 查询失败时网关才能按同一来源保留历史值为过期数据。
            official_quota_source(
                "free_requests",
                "免费模型每日请求",
                "model_requests",
                "model",
            ),
        ],
        ("kimi_coding", _) => vec![
            official_quota_source("subscription", "Kimi Coding 套餐", "coding_plan", "account"),
            official_quota_source("extra_usage", "额外用量", "extra_usage", "account"),
        ],
        ("minimax", "subscription") => vec![official_quota_source(
            "subscription",
            "Token Plan",
            "token_plan",
            "account",
        )],
        ("zhipu" | "zai", "subscription") => vec![official_quota_source(
            "subscription",
            "Coding Plan",
            "coding_plan",
            "account",
        )],
        ("deepseek" | "moonshot" | "siliconflow" | "zhipu" | "zai" | "minimax", _) => {
            vec![official_quota_source(
                "balance",
                "账户余额",
                "account_balance",
                "account",
            )]
        }
        _ => Vec::new(),
    };
    for source in &mut sources {
        source.region = if provider_type == "deepseek" {
            None
        } else {
            Some(official_quota_region(provider_type, endpoint).into())
        };
        source.query_status = ProviderQuotaQueryStatus::NotQueried;
        source.freshness = "unknown".into();
    }
    sources
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

/// 校验官方端点后构造对应余额或套餐请求，不携带可绕过统一 TLS 的局部配置。
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
    if is_retired_official_api_key_quota_endpoint(provider_type, endpoint) {
        return Err("quota unsupported: SiliconFlow CN retired its balance API");
    }
    let secret = read_secret();
    let secret = secret.trim();
    let secret = secret
        .strip_prefix("Bearer ")
        .or_else(|| secret.strip_prefix("bearer "))
        .unwrap_or(secret)
        .trim();
    let region = official_quota_region(provider_type, endpoint);
    let url = match provider_type {
        "moonshot" if region == "global" => MOONSHOT_GLOBAL_BALANCE_URL,
        "moonshot" => MOONSHOT_BALANCE_URL,
        "kimi_coding" if region == "global" => KIMI_CODING_GLOBAL_USAGE_URL,
        "kimi_coding" => KIMI_CODING_USAGE_URL,
        "siliconflow" => SILICONFLOW_GLOBAL_BALANCE_URL,
        "zhipu" => ZHIPU_QUOTA_URL,
        "zai" => ZAI_QUOTA_URL,
        // MiniMax 余额与 Token Plan 由专用构造器分别发起，不再凭 Key 前缀排他选择接口。
        "minimax" => {
            return Err("minimax quota requires dedicated balance and token plan requests")
        }
        _ => return Err("unsupported official API key quota provider"),
    };
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
    })
}

/// 为官方 MiniMax 端点构造同区域账户余额查询；官方 Key 前缀不代表产品身份。
pub fn build_minimax_balance_request<F>(
    key_id: &str,
    endpoint: &StoredProviderCatalogEndpoint,
    read_secret: F,
) -> Result<ProviderPoolQuotaRequestSpec, &'static str>
where
    F: FnOnce() -> String,
{
    build_minimax_quota_request("balance", key_id, endpoint, read_secret)
}

/// 为官方 MiniMax 端点构造同区域 Token Plan 套餐查询，与余额查询相互独立。
pub fn build_minimax_token_plan_request<F>(
    key_id: &str,
    endpoint: &StoredProviderCatalogEndpoint,
    read_secret: F,
) -> Result<ProviderPoolQuotaRequestSpec, &'static str>
where
    F: FnOnce() -> String,
{
    build_minimax_quota_request("subscription", key_id, endpoint, read_secret)
}

fn build_minimax_quota_request<F>(
    quota_kind: &str,
    key_id: &str,
    endpoint: &StoredProviderCatalogEndpoint,
    read_secret: F,
) -> Result<ProviderPoolQuotaRequestSpec, &'static str>
where
    F: FnOnce() -> String,
{
    if !official_host("minimax", endpoint) {
        return Err("quota unsupported for non-official MiniMax endpoint");
    }
    let secret = read_secret();
    let secret = secret.trim();
    let secret = secret
        .strip_prefix("Bearer ")
        .or_else(|| secret.strip_prefix("bearer "))
        .unwrap_or(secret)
        .trim();
    // 两个接口固定在同一区域内查询，不做跨区探测。
    let host = if official_quota_region("minimax", endpoint) == "global" {
        "api.minimax.io"
    } else {
        "api.minimaxi.com"
    };
    let path = if quota_kind == "subscription" {
        "/v1/token_plan/remains"
    } else {
        "/account/query_balance"
    };
    Ok(ProviderPoolQuotaRequestSpec {
        request_id: format!("minimax-{quota_kind}-{key_id}"),
        provider_name: "minimax".into(),
        quota_kind: quota_kind.into(),
        method: "GET".into(),
        url: format!("https://{host}{path}"),
        headers: BTreeMap::from([
            ("accept".into(), "application/json".into()),
            ("authorization".into(), format!("Bearer {secret}")),
        ]),
        content_type: None,
        json_body: None,
        client_api_format: "openai:chat".into(),
        provider_api_format: "openai:chat".into(),
        model_name: None,
    })
}

pub fn build_zhipu_team_quota_request<F>(
    key_id: &str,
    endpoint: &StoredProviderCatalogEndpoint,
    read_secret: F,
) -> Result<ProviderPoolQuotaRequestSpec, &'static str>
where
    F: FnOnce() -> String,
{
    let mut request = build_official_api_key_quota_request("zhipu", key_id, endpoint, read_secret)?;
    request.request_id = format!("zhipu-team-quota-{key_id}");
    request.url = ZHIPU_TEAM_QUOTA_URL.into();
    Ok(request)
}

/// 为官方智谱端点构造账户余额补充查询，认证与 TLS 分别由现有边界负责。
pub fn build_zhipu_account_balance_request<F>(
    key_id: &str,
    endpoint: &StoredProviderCatalogEndpoint,
    read_secret: F,
) -> Result<ProviderPoolQuotaRequestSpec, &'static str>
where
    F: FnOnce() -> String,
{
    if !official_host("zhipu", endpoint) {
        return Err("account balance query requires an official Zhipu endpoint");
    }
    let authorization = read_secret().trim().to_owned();
    Ok(ProviderPoolQuotaRequestSpec {
        request_id: format!("zhipu-balance-{key_id}"),
        provider_name: "zhipu".into(),
        quota_kind: "balance".into(),
        method: "GET".into(),
        url: ZHIPU_ACCOUNT_REPORT_URL.into(),
        headers: BTreeMap::from([
            ("accept".into(), "application/json".into()),
            ("authorization".into(), authorization),
        ]),
        content_type: None,
        json_body: None,
        client_api_format: "openai:chat".into(),
        provider_api_format: "openai:chat".into(),
        model_name: None,
    })
}

pub fn parse_zhipu_standard_balance(
    value: &Value,
) -> Result<ProviderQuotaSnapshotContract, &'static str> {
    if value.get("success").and_then(Value::as_bool) == Some(false)
        || value.get("error").is_some_and(|error| !error.is_null())
        || value
            .get("code")
            .filter(|code| !code.is_null())
            .is_some_and(|code| !matches!(decimal_string(code).as_deref(), Some("0" | "200")))
    {
        return Err("upstream balance response was unsuccessful");
    }
    let data = value.get("data");
    let available = data
        .and_then(|data| data.get("availableBalance"))
        .and_then(decimal_string)
        .or_else(|| {
            data.and_then(|data| data.get("balance"))
                .and_then(decimal_string)
        });
    let granted = data
        .and_then(|data| data.get("giveAmount"))
        .and_then(decimal_string);
    let topped_up = data
        .and_then(|data| data.get("rechargeAmount"))
        .and_then(decimal_string);
    let used = data
        .and_then(|data| data.get("totalSpendAmount"))
        .and_then(decimal_string);
    if available.is_none() && granted.is_none() && topped_up.is_none() && used.is_none() {
        return Err("no valid account balance fields");
    }

    let balance_insufficient = available
        .as_deref()
        .and_then(|value| value.parse::<f64>().ok())
        .is_some_and(|value| value <= 0.0);

    let mut snapshot = ProviderQuotaSnapshotContract::balance(
        "zhipu",
        vec![ProviderQuotaBalance {
            source_id: Some("balance".into()),
            unit: "CNY".into(),
            available,
            total: None,
            granted,
            topped_up,
            used,
        }],
    );
    let mut source = official_quota_source("balance", "账户余额", "account_balance", "account");
    source.region = Some("cn".into());
    source.currency_source = Some("region_mapping".into());
    snapshot.sources.push(source);
    snapshot.extensions.insert(
        "balance_source".into(),
        Value::String("standard_api".into()),
    );
    snapshot.extensions.insert(
        "balance_status".into(),
        Value::String(
            if balance_insufficient {
                "insufficient"
            } else if snapshot.balances[0].available.is_some() {
                "available"
            } else {
                "unknown"
            }
            .into(),
        ),
    );
    snapshot.extensions.insert(
        "balance_insufficient".into(),
        Value::Bool(balance_insufficient),
    );
    if let Some(frozen) = data
        .and_then(|data| data.get("frozenBalance"))
        .and_then(decimal_string)
    {
        snapshot
            .extensions
            .insert("frozen_balance".into(), Value::String(frozen));
    }
    Ok(snapshot)
}

pub fn parse_official_api_key_quota(
    provider_type: &str,
    value: &Value,
) -> Result<ProviderQuotaSnapshotContract, &'static str> {
    let now_unix_secs = if matches!(provider_type, "kimi_coding" | "zhipu" | "zai" | "minimax") {
        provider_pool_current_unix_secs().ok_or("system clock unavailable")?
    } else {
        0
    };
    parse_official_api_key_quota_at(provider_type, value, now_unix_secs)
}

/// 从已匹配的官方端点确定区域，避免在解析阶段把国际站金额标为人民币。
pub fn parse_official_api_key_quota_for_endpoint(
    provider_type: &str,
    value: &Value,
    endpoint: &StoredProviderCatalogEndpoint,
) -> Result<ProviderQuotaSnapshotContract, &'static str> {
    if !official_host(provider_type, endpoint) {
        return Err("quota unsupported for non-official endpoint");
    }
    let now_unix_secs = provider_pool_current_unix_secs().ok_or("system clock unavailable")?;
    parse_official_api_key_quota_in_region(
        provider_type,
        value,
        now_unix_secs,
        official_quota_region(provider_type, endpoint),
    )
}

fn parse_official_api_key_quota_at(
    provider_type: &str,
    value: &Value,
    now_unix_secs: u64,
) -> Result<ProviderQuotaSnapshotContract, &'static str> {
    parse_official_api_key_quota_in_region(
        provider_type,
        value,
        now_unix_secs,
        if provider_type == "zai" {
            "global"
        } else {
            "cn"
        },
    )
}

fn parse_official_api_key_quota_in_region(
    provider_type: &str,
    value: &Value,
    now_unix_secs: u64,
    region: &str,
) -> Result<ProviderQuotaSnapshotContract, &'static str> {
    let mut snapshot = match provider_type {
        "moonshot" => {
            if value.get("error").is_some_and(|error| !error.is_null())
                || value.get("success").and_then(Value::as_bool) == Some(false)
                || value
                    .get("code")
                    .filter(|code| !code.is_null())
                    .is_some_and(|code| {
                        !matches!(decimal_string(code).as_deref(), Some("0" | "200"))
                    })
            {
                return Err("upstream balance response was unsuccessful");
            }
            let data = value.get("data").ok_or("missing data")?;
            let available = data.get("available_balance").and_then(decimal_string);
            let cash = data.get("cash_balance").and_then(decimal_string);
            let voucher = data.get("voucher_balance").and_then(decimal_string);
            if available.is_none() {
                return Err("missing available_balance");
            }
            let mut snapshot = ProviderQuotaSnapshotContract::balance(
                "moonshot",
                vec![ProviderQuotaBalance {
                    source_id: Some("balance".into()),
                    unit: if region == "global" { "USD" } else { "CNY" }.into(),
                    available,
                    total: None,
                    granted: voucher,
                    topped_up: cash,
                    used: None,
                }],
            );
            let mut source =
                official_quota_source("balance", "账户余额", "account_balance", "account");
            source.currency_source = Some("region_mapping".into());
            snapshot.sources.push(source);
            snapshot
        }
        "kimi_coding" => {
            let parsed = parse_kimi_coding_subscription(value)?;
            let mut snapshot = ProviderQuotaSnapshotContract::subscription(
                "kimi_coding",
                parsed.windows,
                now_unix_secs,
            );
            snapshot.extensions = parsed.extensions;
            snapshot.balances = parsed.balances;
            snapshot.sources = parsed.sources;
            snapshot
        }
        "siliconflow" => {
            // HTTP 200 不代表业务成功；错误或缺失业务码不能生成余额快照。
            let code = value.get("code").and_then(decimal_string);
            if value.get("status").and_then(Value::as_bool) != Some(true)
                || code.as_deref() != Some("20000")
            {
                return Err("upstream balance response was unsuccessful");
            }
            let data = value.get("data").ok_or("missing data")?;
            let available = data.get("totalBalance").and_then(decimal_string);
            if available.is_none() {
                return Err("missing totalBalance");
            }
            let mut snapshot = ProviderQuotaSnapshotContract::balance(
                "siliconflow",
                vec![ProviderQuotaBalance {
                    source_id: Some("balance".into()),
                    unit: if region == "global" { "USD" } else { "CNY" }.into(),
                    available,
                    total: None,
                    granted: data.get("balance").and_then(decimal_string),
                    topped_up: data.get("chargeBalance").and_then(decimal_string),
                    used: None,
                }],
            );
            let mut source =
                official_quota_source("balance", "账户余额", "account_balance", "account");
            source.currency_source = Some("region_mapping".into());
            snapshot.sources.push(source);
            if let Some(status) = data.get("status").filter(|v| !v.is_null()) {
                snapshot
                    .extensions
                    .insert("account_status".into(), status.clone());
            }
            snapshot
        }
        "zhipu" | "zai" => {
            let parsed = parse_zhipu_subscription(value)?;
            let mut snapshot = ProviderQuotaSnapshotContract::subscription(
                provider_type,
                parsed.windows,
                now_unix_secs,
            );
            snapshot.extensions = parsed.extensions;
            snapshot.sources = parsed.sources;
            snapshot
        }
        "minimax" => parse_minimax_quota(value, now_unix_secs)?,
        _ => return Err("unsupported official API key quota provider"),
    };
    for source in &mut snapshot.sources {
        source.region = Some(region.into());
    }
    Ok(snapshot)
}

#[cfg(test)]
#[path = "official_api_key_tests.rs"]
mod tests;
