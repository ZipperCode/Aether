use super::*;
use crate::quota_snapshot::ProviderQuotaSnapshotKind;
use serde_json::json;

const FIXED_NOW_UNIX_SECS: u64 = 1_800_000_000;

fn endpoint(base_url: &str) -> StoredProviderCatalogEndpoint {
    StoredProviderCatalogEndpoint::new(
        "e".into(),
        "p".into(),
        "openai:chat".into(),
        None,
        None,
        true,
    )
    .unwrap()
    .with_transport_fields(base_url.into(), None, None, None, None, None, None, None)
    .unwrap()
}

fn parse_at(provider_type: &str, fixture: Value) -> ProviderQuotaSnapshotContract {
    parse_official_api_key_quota_at(provider_type, &fixture, FIXED_NOW_UNIX_SECS).unwrap()
}

#[test]
fn kimi_coding_endpoint_selects_fixed_usage_request() {
    let endpoint = endpoint("https://api.kimi.com/coding/v1");
    assert!(is_official_api_key_quota_endpoint("kimi_coding", &endpoint));
    assert!(!is_official_api_key_quota_endpoint("moonshot", &endpoint));
    let request =
        build_official_api_key_quota_request("kimi_coding", "key", &endpoint, || "secret".into())
            .unwrap();
    assert_eq!(request.url, KIMI_CODING_USAGE_URL);
    assert_eq!(request.quota_kind, "subscription");
}

#[test]
fn balance_and_subscription_are_not_conflated() {
    let moonshot = parse_official_api_key_quota(
        "moonshot",
        &json!({"data":{
            "available_balance": 12.5, "cash_balance": 10, "voucher_balance": 2.5
        }}),
    )
    .unwrap();
    assert_eq!(moonshot.balances[0].available.as_deref(), Some("12.5"));
    assert!(moonshot.windows.is_empty());

    let glm = parse_official_api_key_quota("zhipu", &json!({"success":true,"data":{"limits":[{
        "type":"TOKENS_LIMIT","currentValue":120,"usage":500,"percentage":24,"resetAt":"2030-01-01T00:00:00Z"
    }]}})).unwrap();
    assert!(glm.balances.is_empty());
    assert_eq!(
        serde_json::to_value(&glm.windows[0]).unwrap()["remaining_value"],
        380.0
    );
}

#[test]
fn parses_kimi_coding_cycle_and_five_hour_windows() {
    let parsed = parse_at(
        "kimi_coding",
        json!({
            "user":{"membership":{"level":"LEVEL_ADVANCED"}},
            "usage":{"limit":"100","used":"1","remaining":"99","resetTime":"2030-01-02T00:27:18Z"},
            "limits":[{"window":{"duration":300,"timeUnit":"TIME_UNIT_MINUTE"},"detail":{
                "limit":"100","used":"1","remaining":"99","resetTime":"2030-01-01T16:27:18Z"
            }}],
            "parallel":{"limit":"30"},
            "subType":"TYPE_PURCHASE"
        }),
    );
    assert_eq!(parsed.kind, ProviderQuotaSnapshotKind::Subscription);
    assert_eq!(parsed.provider_type, "kimi_coding");
    assert_eq!(parsed.windows.len(), 2);
    assert_eq!(parsed.windows[0].remaining_ratio, Some(0.99));
    assert_eq!(parsed.windows[1].label, "5小时配额");
    assert_eq!(parsed.windows[1].reset_at, Some(1_893_515_238u64));
    assert_eq!(parsed.extensions["membership_level"], "LEVEL_ADVANCED");
    assert_eq!(parsed.extensions["parallel_limit"], "30");
}

#[test]
fn kimi_subscription_exhaustion_requires_an_active_exhausted_window() {
    let active = parse_at(
        "kimi_coding",
        json!({
            "usage":{"limit":"100","used":"100","remaining":"0","resetTime":"2030-01-01T00:00:00Z"},
            "limits":[{"window":{"duration":300,"timeUnit":"TIME_UNIT_MINUTE"},"detail":{
                "limit":"100","used":"100","remaining":"0","resetTime":"2020-01-01T00:00:00Z"
            }}]
        }),
    );
    assert!(active.windows[0].is_exhausted);
    assert!(active.windows[1].is_exhausted);
    assert!(active.exhausted);

    let reset = parse_at(
        "kimi_coding",
        json!({
            "usage":{"limit":"100","used":"100","remaining":"0","resetTime":"2020-01-01T00:00:00Z"}
        }),
    );
    assert!(reset.windows[0].is_exhausted);
    assert!(!reset.exhausted);
}

#[test]
fn kimi_subscription_boundaries_do_not_infer_exhaustion_from_invalid_values() {
    let parsed = parse_at(
        "kimi_coding",
        json!({
            "usage":{"limit":"100","used":"99.999999","remaining":"0.000001","resetTime":"2030-01-01T00:00:00Z"},
            "limits":[
                {"window":{"duration":300,"timeUnit":"TIME_UNIT_MINUTE"},"detail":{
                    "limit":"100","used":"100","remaining":"invalid","resetTime":"2030-01-01T00:00:00Z"
                }},
                {"window":{"duration":60,"timeUnit":"TIME_UNIT_MINUTE"},"detail":{
                    "limit":"100","used":"100","resetTime":"2030-01-01T00:00:00Z"
                }}
            ]
        }),
    );
    assert!(!parsed.windows[0].is_exhausted);
    assert!(!parsed.windows[1].is_exhausted);
    assert!(!parsed.windows[2].is_exhausted);
    assert!(!parsed.exhausted);
}

#[test]
fn zhipu_and_zai_subscription_exhaustion_respects_reset_timing() {
    for provider_type in ["zhipu", "zai"] {
        let active = parse_at(
            provider_type,
            json!({"success":true,"data":{"limits":[
                {"type":"TOKENS_LIMIT","currentValue":500,"usage":500,"percentage":100,"resetAt":"2030-01-01T00:00:00Z"},
                {"type":"TIME_LIMIT","currentValue":10,"usage":10,"percentage":100,"resetAt":"2020-01-01T00:00:00Z"}
            ]}}),
        );
        assert_eq!(active.kind, ProviderQuotaSnapshotKind::Subscription);
        assert_eq!(active.provider_type, provider_type);
        assert!(active.windows[0].is_exhausted);
        assert!(active.windows[1].is_exhausted);
        assert!(active.exhausted);

        let reset = parse_at(
            provider_type,
            json!({"success":true,"data":{"limits":[{
                "type":"TOKENS_LIMIT","currentValue":500,"usage":500,"percentage":100,"resetAt":"2020-01-01T00:00:00Z"
            }]}}),
        );
        assert!(reset.windows[0].is_exhausted);
        assert!(!reset.exhausted);
    }
}

#[test]
fn zhipu_subscription_boundaries_fail_closed_for_invalid_ratio_and_reset() {
    let parsed = parse_at(
        "zhipu",
        json!({"success":true,"data":{"limits":[
            {"type":"TOKENS_LIMIT","currentValue":499.999999,"usage":500,"percentage":99.9999998,"resetAt":"2030-01-01T00:00:00Z"},
            {"type":"TIME_LIMIT","currentValue":10,"usage":10,"percentage":"invalid","resetAt":"2030-01-01T00:00:00Z"},
            {"type":"REQUESTS_LIMIT","currentValue":10,"usage":10,"percentage":100,"resetAt":"invalid"}
        ]}}),
    );
    assert!(!parsed.windows[0].is_exhausted);
    assert!(!parsed.windows[1].is_exhausted);
    assert!(parsed.windows[2].is_exhausted);
    assert!(!parsed.exhausted);
}

#[test]
fn subscription_parsing_does_not_retain_previous_exhaustion_state() {
    let active = parse_at(
        "zai",
        json!({"success":true,"data":{"limits":[{
            "type":"TOKENS_LIMIT","currentValue":500,"usage":500,"percentage":100,"resetAt":"2030-01-01T00:00:00Z"
        }]}}),
    );
    let reset = parse_at(
        "zai",
        json!({"success":true,"data":{"limits":[{
            "type":"TOKENS_LIMIT","currentValue":10,"usage":500,"percentage":2,"resetAt":"2030-01-01T00:00:00Z"
        }]}}),
    );
    assert!(active.exhausted);
    assert!(!reset.exhausted);
}
