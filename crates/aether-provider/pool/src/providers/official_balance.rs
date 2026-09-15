use aether_contracts::ExecutionTimeouts;
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogEndpoint;
use std::net::IpAddr;
use url::Url;

use crate::quota_snapshot::{ProviderQuotaQueryStatus, ProviderQuotaSource};

pub const OFFICIAL_BALANCE_DIRECT_TIMEOUT_CAP_MS: u64 = 30_000;
pub const OFFICIAL_BALANCE_PROXY_TIMEOUT_CAP_MS: u64 = 60_000;

pub fn clamp_official_balance_execution_timeouts(
    mut timeouts: ExecutionTimeouts,
    proxy_active: bool,
) -> ExecutionTimeouts {
    let cap = if proxy_active {
        OFFICIAL_BALANCE_PROXY_TIMEOUT_CAP_MS
    } else {
        OFFICIAL_BALANCE_DIRECT_TIMEOUT_CAP_MS
    };
    timeouts.connect_ms = Some(timeouts.connect_ms.unwrap_or(cap).min(cap));
    timeouts.read_ms = Some(timeouts.read_ms.unwrap_or(cap).min(cap));
    timeouts.first_byte_ms = Some(timeouts.first_byte_ms.unwrap_or(cap).min(cap));
    timeouts.total_ms = Some(timeouts.total_ms.unwrap_or(cap).min(cap));
    timeouts
}

pub(crate) fn endpoint_has_official_origin(
    endpoint: &StoredProviderCatalogEndpoint,
    official_host: &str,
) -> bool {
    let Ok(url) = Url::parse(endpoint.base_url.trim()) else {
        return false;
    };
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case(official_host) && host.parse::<IpAddr>().is_err()
}

pub(crate) fn decimal_string(value: &serde_json::Value) -> Option<String> {
    let raw = match value {
        serde_json::Value::String(value) => value.trim().to_owned(),
        serde_json::Value::Number(value) => value.to_string(),
        _ => return None,
    };
    let (mantissa, exponent) = match raw.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().ok()?),
        None => (raw.as_str(), 0),
    };
    let unsigned = mantissa.strip_prefix(['+', '-']).unwrap_or(mantissa);
    let mut parts = unsigned.split('.');
    let whole = parts.next()?;
    let fraction = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.is_some_and(|v| v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    if exponent == 0 {
        return Some(mantissa.to_owned());
    }
    // JSON 数字可能使用科学计数法；仅移动小数点，绝不先经过浮点数。
    if exponent.unsigned_abs() > 10_000 {
        return None;
    }
    let digits = format!("{whole}{}", fraction.unwrap_or_default());
    let point = i64::try_from(whole.len()).ok()? + i64::from(exponent);
    let expanded = if point <= 0 {
        format!("0.{}{digits}", "0".repeat(usize::try_from(-point).ok()?))
    } else if usize::try_from(point).ok()? >= digits.len() {
        format!(
            "{digits}{}",
            "0".repeat(usize::try_from(point).ok()? - digits.len())
        )
    } else {
        let point = usize::try_from(point).ok()?;
        format!("{}.{}", &digits[..point], &digits[point..])
    };
    Some(if mantissa.starts_with('-') {
        format!("-{expanded}")
    } else {
        expanded
    })
}

/// 将上游固定小数位金额精确换算为主单位，不限制有效数字为 u128 或 f64。
pub(crate) fn scale_decimal(value: &serde_json::Value, places: usize) -> Option<String> {
    let raw = decimal_string(value)?;
    let unsigned = raw.strip_prefix(['+', '-']).unwrap_or(&raw);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let digits = format!("{whole}{fraction}");
    let scale = fraction.len().checked_add(places)?;
    let padded = format!(
        "{}{digits}",
        "0".repeat(scale.saturating_add(1).saturating_sub(digits.len()))
    );
    let point = padded.len().checked_sub(scale)?;
    let whole = padded[..point].trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    let fraction = padded[point..].trim_end_matches('0');
    let sign = if raw.starts_with('-') && (whole != "0" || !fraction.is_empty()) {
        "-"
    } else {
        ""
    };
    Some(if fraction.is_empty() {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{fraction}")
    })
}

pub(crate) fn subtract_decimal_clamped(total: &str, used: &str) -> Option<String> {
    fn parts(value: &str) -> Option<(String, usize)> {
        let value = decimal_string(&serde_json::Value::String(value.into()))?;
        if value.starts_with('-') {
            return None;
        }
        let value = value.strip_prefix('+').unwrap_or(&value);
        let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
        Some((format!("{whole}{fraction}"), fraction.len()))
    }
    let (mut total, total_scale) = parts(total)?;
    let (mut used, used_scale) = parts(used)?;
    let scale = total_scale.max(used_scale);
    total.push_str(&"0".repeat(scale - total_scale));
    used.push_str(&"0".repeat(scale - used_scale));
    let total = total.trim_start_matches('0');
    let used = used.trim_start_matches('0');
    if (total.len(), total) <= (used.len(), used) {
        return Some("0".into());
    }
    let mut used_digits = used.bytes().rev();
    let mut borrow = 0i16;
    let mut remaining = Vec::with_capacity(total.len());
    // 按十进制逐位相减，保留超过浮点及固定宽度整数范围的额度。
    for digit in total.bytes().rev() {
        let other = used_digits
            .next()
            .map_or(0, |digit| i16::from(digit - b'0'));
        let value = i16::from(digit - b'0') - other - borrow;
        borrow = i16::from(value < 0);
        remaining.push(b'0' + value.rem_euclid(10) as u8);
    }
    remaining.reverse();
    scale_decimal(
        &serde_json::Value::String(String::from_utf8(remaining).ok()?),
        scale,
    )
}

pub(crate) fn official_quota_source(
    id: &str,
    label: &str,
    product: &str,
    scope: &str,
) -> ProviderQuotaSource {
    ProviderQuotaSource {
        id: id.into(),
        label: label.into(),
        product: product.into(),
        scope: scope.into(),
        query_status: ProviderQuotaQueryStatus::Ok,
        freshness: "fresh".into(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogEndpoint;

    fn oversized_timeouts() -> ExecutionTimeouts {
        ExecutionTimeouts {
            connect_ms: Some(90_000),
            read_ms: Some(91_000),
            first_byte_ms: Some(92_000),
            total_ms: Some(93_000),
            write_ms: Some(94_000),
            pool_ms: Some(95_000),
        }
    }

    #[test]
    fn official_balance_clamps_direct_timeouts_without_touching_unrelated_fields() {
        let timeouts = clamp_official_balance_execution_timeouts(oversized_timeouts(), false);
        assert_eq!(
            (
                timeouts.connect_ms,
                timeouts.read_ms,
                timeouts.first_byte_ms,
                timeouts.total_ms
            ),
            (Some(30_000), Some(30_000), Some(30_000), Some(30_000))
        );
        assert_eq!(
            (timeouts.write_ms, timeouts.pool_ms),
            (Some(94_000), Some(95_000))
        );
    }

    #[test]
    fn official_balance_preserves_smaller_timeouts() {
        let configured = ExecutionTimeouts {
            connect_ms: Some(1_000),
            read_ms: Some(2_000),
            first_byte_ms: Some(3_000),
            total_ms: Some(4_000),
            ..ExecutionTimeouts::default()
        };
        let timeouts = clamp_official_balance_execution_timeouts(configured.clone(), false);
        assert_eq!(
            (
                timeouts.connect_ms,
                timeouts.read_ms,
                timeouts.first_byte_ms,
                timeouts.total_ms
            ),
            (
                configured.connect_ms,
                configured.read_ms,
                configured.first_byte_ms,
                configured.total_ms
            )
        );
    }

    #[test]
    fn official_balance_uses_proxy_cap() {
        let timeouts = clamp_official_balance_execution_timeouts(oversized_timeouts(), true);
        assert_eq!(
            (
                timeouts.connect_ms,
                timeouts.read_ms,
                timeouts.first_byte_ms,
                timeouts.total_ms
            ),
            (Some(60_000), Some(60_000), Some(60_000), Some(60_000))
        );
    }

    fn endpoint(base_url: &str) -> StoredProviderCatalogEndpoint {
        StoredProviderCatalogEndpoint::new(
            "endpoint".into(),
            "provider".into(),
            "openai:chat".into(),
            None,
            None,
            true,
        )
        .unwrap()
        .with_transport_fields(base_url.into(), None, None, None, None, None, None, None)
        .unwrap()
    }

    #[test]
    fn official_origin_requires_exact_https_dns_origin_and_default_port() {
        for accepted in [
            "https://api.deepseek.com",
            "https://API.DEEPSEEK.COM/v1",
            "https://api.deepseek.com:443/custom/path?ignored=true",
        ] {
            assert!(
                endpoint_has_official_origin(&endpoint(accepted), "api.deepseek.com"),
                "expected official origin: {accepted}"
            );
        }

        for rejected in [
            "http://api.deepseek.com",
            "https://user@api.deepseek.com",
            "https://user:password@api.deepseek.com",
            "https://api.deepseek.com:444",
            "https://api.deepseek.com.evil.test",
            "https://deepseek.com",
            "https://127.0.0.1",
            "https://[::1]",
            "https://xn--api-deepseek-9k1n.example",
            "not a url",
        ] {
            assert!(
                !endpoint_has_official_origin(&endpoint(rejected), "api.deepseek.com"),
                "expected hostile origin rejection: {rejected}"
            );
        }
    }

    #[test]
    fn mixed_endpoints_are_evaluated_independently() {
        let official = endpoint("https://openrouter.ai/api/v1");
        let custom = endpoint("https://openrouter.example/api/v1");
        assert!(endpoint_has_official_origin(&official, "openrouter.ai"));
        assert!(!endpoint_has_official_origin(&custom, "openrouter.ai"));
    }
}
