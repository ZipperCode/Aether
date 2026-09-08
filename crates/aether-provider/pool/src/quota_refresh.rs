use std::collections::BTreeMap;
use std::fmt;

use aether_contracts::redact_url_for_debug;
use serde_json::Value;

/// 官方余额探测失败后的最小重试间隔，单位秒。
pub const OFFICIAL_BALANCE_MIN_BACKOFF_SECS: u64 = 30;
/// 官方余额探测指数退避的上限，单位秒。
pub const OFFICIAL_BALANCE_MAX_BACKOFF_SECS: u64 = 600;

/// 根据连续失败次数计算有上限的退避秒数，保持后台观察探测可自动恢复。
pub fn official_balance_backoff_secs(failures: u32) -> u64 {
    (OFFICIAL_BALANCE_MIN_BACKOFF_SECS * (1u64 << failures.min(5)))
        .min(OFFICIAL_BALANCE_MAX_BACKOFF_SECS)
}

/// 在退避秒数上施加确定性 ±10% 抖动；种子只影响分散时刻，不代表 Unix 时间。
pub fn official_balance_backoff_with_jitter_secs(failures: u32, seed: u64) -> u64 {
    let base = official_balance_backoff_secs(failures);
    let span = (base / 10).max(1);
    let offset = seed % (span.saturating_mul(2).saturating_add(1));
    base.saturating_sub(span).saturating_add(offset).clamp(
        OFFICIAL_BALANCE_MIN_BACKOFF_SECS,
        OFFICIAL_BALANCE_MAX_BACKOFF_SECS,
    )
}

#[derive(Clone, PartialEq)]
/// 额度探测的完整出站请求；沿上游统一 TLS 策略，不再携带忽略证书开关。
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
}

impl fmt::Debug for ProviderPoolQuotaRequestSpec {
    /// 调试输出只展示请求形状，不展开凭据头、正文或含敏感查询的 URL。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderPoolQuotaRequestSpec")
            .field("request_id", &self.request_id)
            .field("provider_name", &self.provider_name)
            .field("quota_kind", &self.quota_kind)
            .field("method", &self.method)
            .field("url", &redact_url_for_debug(&self.url))
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .field("content_type", &self.content_type)
            .field("has_json_body", &self.json_body.is_some())
            .field(
                "json_body_bytes",
                &self
                    .json_body
                    .as_ref()
                    .and_then(|body| serde_json::to_vec(body).ok().map(|bytes| bytes.len())),
            )
            .field("client_api_format", &self.client_api_format)
            .field("provider_api_format", &self.provider_api_format)
            .field("model_name", &self.model_name)
            .finish()
    }
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
    /// 验证沿上游统一 TLS 策略后的请求字面量仍保留余额探测需要的字段。
    fn quota_request_spec_preserves_balance_probe_fields() {
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
        };

        // 请求元数据不因 TLS 策略字段移除而丢失。
        assert_eq!(spec.request_id, "quota:test");
    }
}
