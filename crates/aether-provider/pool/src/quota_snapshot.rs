use serde::{Deserialize, Serialize};

pub const PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
pub const ZHIPU_TOKEN_PLAN_STATUS_FIELD: &str = "token_plan_status";
pub const ZHIPU_TOKEN_PLAN_SCHEDULING_BLOCKED_FIELD: &str = "token_plan_scheduling_blocked";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderQuotaSnapshotKind {
    Balance,
    Subscription,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderQuotaBalance {
    pub unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topped_up: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProviderQuotaValue {
    Decimal(String),
    Number(serde_json::Number),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderQuotaWindow {
    pub code: String,
    pub label: String,
    pub scope: String,
    pub unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_value: Option<ProviderQuotaValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_value: Option<ProviderQuotaValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_value: Option<ProviderQuotaValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_ratio: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_ratio: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_minutes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_at_text: Option<String>,
    pub is_exhausted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderQuotaRefreshState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_attempt_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_eligible_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderQuotaSnapshotContract {
    pub schema_version: u32,
    pub kind: ProviderQuotaSnapshotKind,
    pub provider_type: String,
    #[serde(default)]
    pub exhausted: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub balances: Vec<ProviderQuotaBalance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub windows: Vec<ProviderQuotaWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<serde_json::Value>,
    #[serde(default)]
    pub refresh_state: ProviderQuotaRefreshState,
    #[serde(flatten)]
    pub extensions: serde_json::Map<String, serde_json::Value>,
}

impl ProviderQuotaSnapshotContract {
    pub fn balance(provider_type: impl Into<String>, balances: Vec<ProviderQuotaBalance>) -> Self {
        Self {
            schema_version: PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION,
            kind: ProviderQuotaSnapshotKind::Balance,
            provider_type: provider_type.into(),
            exhausted: false,
            balances,
            windows: Vec::new(),
            rate_limit: None,
            refresh_state: ProviderQuotaRefreshState::default(),
            extensions: serde_json::Map::new(),
        }
    }

    pub fn subscription(
        provider_type: impl Into<String>,
        windows: Vec<ProviderQuotaWindow>,
        now_unix_secs: u64,
    ) -> Self {
        let exhausted = windows.iter().any(|window| {
            window.is_exhausted
                && window
                    .reset_at
                    .is_some_and(|reset_at| reset_at > now_unix_secs)
        });
        Self {
            schema_version: PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION,
            kind: ProviderQuotaSnapshotKind::Subscription,
            provider_type: provider_type.into(),
            exhausted,
            balances: Vec::new(),
            windows,
            rate_limit: None,
            refresh_state: ProviderQuotaRefreshState::default(),
            extensions: serde_json::Map::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn balance_contract_round_trips_decimal_strings_and_unknown_fields() {
        let fixture = json!({
            "schema_version": 1,
            "kind": "balance",
            "provider_type": "deepseek",
            "balances": [{
                "unit": "CNY",
                "available": "12.3400",
                "granted": "10.0000",
                "topped_up": "2.3400"
            }],
            "windows": [],
            "rate_limit": {"remaining": 59},
            "refresh_state": {"last_attempt_at": 10, "last_success_at": 9},
            "future_field": {"safe": true}
        });

        let parsed: ProviderQuotaSnapshotContract =
            serde_json::from_value(fixture).expect("fixture should deserialize");
        assert_eq!(parsed.kind, ProviderQuotaSnapshotKind::Balance);
        assert_eq!(parsed.balances[0].available.as_deref(), Some("12.3400"));
        assert!(parsed.extensions.contains_key("future_field"));
        assert_eq!(
            serde_json::to_value(parsed).expect("serialize")["future_field"]["safe"],
            true
        );
    }

    #[test]
    fn legacy_subscription_fixture_remains_readable_as_untyped_json() {
        let legacy = json!({
            "version": 2,
            "provider_type": "codex",
            "code": "ok",
            "exhausted": false,
            "windows": [{"code": "5h", "used_ratio": 0.25}]
        });
        assert_eq!(legacy["windows"][0]["code"], "5h");
        assert!(serde_json::from_value::<ProviderQuotaSnapshotContract>(legacy).is_err());
    }
}
