use std::collections::BTreeMap;

use serde_json::Value;

pub const OFFICIAL_BALANCE_MIN_BACKOFF_SECS: u64 = 30;
pub const OFFICIAL_BALANCE_MAX_BACKOFF_SECS: u64 = 600;

pub fn official_balance_backoff_secs(failures: u32) -> u64 {
    (OFFICIAL_BALANCE_MIN_BACKOFF_SECS * (1u64 << failures.min(5)))
        .min(OFFICIAL_BALANCE_MAX_BACKOFF_SECS)
}

/// Applies a deterministic, bounded +/-10% jitter suitable for persisted retry schedules.
pub fn official_balance_backoff_with_jitter_secs(failures: u32, seed: u64) -> u64 {
    let base = official_balance_backoff_secs(failures);
    let span = (base / 10).max(1);
    let offset = seed % (span.saturating_mul(2).saturating_add(1));
    base.saturating_sub(span).saturating_add(offset).clamp(
        OFFICIAL_BALANCE_MIN_BACKOFF_SECS,
        OFFICIAL_BALANCE_MAX_BACKOFF_SECS,
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderPoolQuotaRequestSpec {
    pub request_id: String,
    pub provider_name: String,
    pub quota_kind: String,
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub content_type: Option<String>,
    pub json_body: Option<Value>,
    pub client_api_format: String,
    pub provider_api_format: String,
    pub model_name: Option<String>,
    pub accept_invalid_certs: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_balance_backoff_reaches_and_stays_at_cap() {
        assert_eq!(official_balance_backoff_secs(0), 30);
        assert_eq!(official_balance_backoff_secs(4), 480);
        assert_eq!(official_balance_backoff_secs(5), 600);
        assert_eq!(official_balance_backoff_secs(u32::MAX), 600);
    }

    #[test]
    fn official_balance_jitter_is_deterministic_and_bounded() {
        assert_eq!(official_balance_backoff_with_jitter_secs(2, 7), 115);
        for seed in 0..100 {
            let delay = official_balance_backoff_with_jitter_secs(3, seed);
            assert!((216..=264).contains(&delay));
        }
        assert!((540..=600).contains(&official_balance_backoff_with_jitter_secs(99, 999)));
    }

    #[test]
    fn quota_request_spec_preserves_pre_redirect_control_struct_literal_compatibility() {
        // Given / When: downstream code constructs the original public literal.
        let spec = ProviderPoolQuotaRequestSpec {
            request_id: "quota:test".to_string(),
            provider_name: "test".to_string(),
            quota_kind: "test".to_string(),
            method: "GET".to_string(),
            url: "https://example.com/quota".to_string(),
            headers: BTreeMap::new(),
            content_type: None,
            json_body: None,
            client_api_format: "openai:responses".to_string(),
            provider_api_format: "openai:responses".to_string(),
            model_name: None,
            accept_invalid_certs: false,
        };

        // Then: the unchanged literal compiles and retains its existing fields.
        assert_eq!(spec.request_id, "quota:test");
    }
}
