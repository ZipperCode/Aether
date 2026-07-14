use aether_contracts::ExecutionTimeouts;
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogEndpoint;
use std::net::IpAddr;
use url::Url;

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
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) => value.to_string(),
        _ => return None,
    };
    let unsigned = raw.strip_prefix('+').unwrap_or(&raw);
    let unsigned = unsigned.strip_prefix('-').unwrap_or(unsigned);
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
    Some(raw)
}

pub(crate) fn subtract_decimal_clamped(total: &str, used: &str) -> Option<String> {
    fn parts(value: &str) -> Option<(u128, usize)> {
        if value.starts_with('-') {
            return None;
        }
        let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
        let scale = fraction.len();
        let digits = format!("{whole}{fraction}").parse().ok()?;
        Some((digits, scale))
    }
    let (mut total, total_scale) = parts(total)?;
    let (mut used, used_scale) = parts(used)?;
    let scale = total_scale.max(used_scale);
    total = total.checked_mul(10u128.checked_pow((scale - total_scale) as u32)?)?;
    used = used.checked_mul(10u128.checked_pow((scale - used_scale) as u32)?)?;
    let remaining = total.saturating_sub(used);
    if scale == 0 {
        return Some(remaining.to_string());
    }
    let factor = 10u128.checked_pow(scale as u32)?;
    let mut fraction = format!("{:0width$}", remaining % factor, width = scale);
    while fraction.ends_with('0') {
        fraction.pop();
    }
    if fraction.is_empty() {
        Some((remaining / factor).to_string())
    } else {
        Some(format!("{}.{}", remaining / factor, fraction))
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
