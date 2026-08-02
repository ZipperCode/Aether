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
fn zhipu_supports_personal_team_and_account_balance_queries() {
    let standard = build_official_api_key_quota_request(
        "zhipu",
        "standard-key",
        &endpoint("https://open.bigmodel.cn/api/paas/v4"),
        || "standard-secret".into(),
    )
    .unwrap();
    assert_eq!(standard.url, ZHIPU_QUOTA_URL);
    assert_eq!(standard.quota_kind, "subscription");
    assert_eq!(standard.headers["authorization"], "standard-secret");

    let balance = build_zhipu_account_balance_request(
        "standard-key",
        &endpoint("https://open.bigmodel.cn/api/paas/v4"),
        || "standard-secret".into(),
    )
    .unwrap();
    assert_eq!(balance.url, ZHIPU_ACCOUNT_REPORT_URL);
    assert_eq!(balance.quota_kind, "balance");
    assert_eq!(balance.headers["authorization"], "standard-secret");

    let coding = build_official_api_key_quota_request(
        "zhipu",
        "coding-key",
        &endpoint("https://open.bigmodel.cn/api/coding/paas/v4"),
        || "coding-secret".into(),
    )
    .unwrap();
    assert_eq!(coding.url, ZHIPU_QUOTA_URL);
    assert_eq!(coding.quota_kind, "subscription");
    assert_eq!(coding.headers["authorization"], "coding-secret");

    let team = build_zhipu_team_quota_request(
        "team-key",
        &endpoint("https://open.bigmodel.cn/api/coding/paas/v4"),
        || "team-secret".into(),
    )
    .unwrap();
    assert_eq!(team.url, ZHIPU_TEAM_QUOTA_URL);
    assert_eq!(team.quota_kind, "subscription");
    assert_eq!(team.headers["authorization"], "team-secret");

    let mut custom_path_standard = endpoint("https://open.bigmodel.cn");
    custom_path_standard.custom_path = Some("/api/paas/v4".into());
    let custom_path_request = build_official_api_key_quota_request(
        "zhipu",
        "custom-path-key",
        &custom_path_standard,
        || "custom-path-secret".into(),
    )
    .unwrap();
    assert_eq!(custom_path_request.url, ZHIPU_QUOTA_URL);
    assert_eq!(custom_path_request.quota_kind, "subscription");
    assert!(
        build_zhipu_account_balance_request("custom-path-key", &custom_path_standard, || {
            "custom-path-secret".into()
        },)
        .is_ok()
    );

    let coding_balance = build_zhipu_account_balance_request(
        "coding-key",
        &endpoint("https://open.bigmodel.cn/api/coding/paas/v4"),
        || "coding-secret".into(),
    )
    .unwrap();
    assert_eq!(coding_balance.url, ZHIPU_ACCOUNT_REPORT_URL);
    assert_eq!(coding_balance.headers["authorization"], "coding-secret");
}

#[test]
fn parses_zhipu_standard_account_balance_without_subscription_limits() {
    let parsed = parse_zhipu_standard_balance(&json!({
        "success": true,
        "data": {
            "balance": 42.5,
            "availableBalance": 40.25,
            "rechargeAmount": 100,
            "giveAmount": 10,
            "totalSpendAmount": 69.75,
            "frozenBalance": 2.25
        }
    }))
    .unwrap();

    assert_eq!(parsed.kind, ProviderQuotaSnapshotKind::Balance);
    assert_eq!(parsed.provider_type, "zhipu");
    assert_eq!(parsed.balances[0].unit, "CNY");
    assert_eq!(parsed.balances[0].available.as_deref(), Some("40.25"));
    assert_eq!(parsed.balances[0].granted.as_deref(), Some("10"));
    assert_eq!(parsed.balances[0].topped_up.as_deref(), Some("100"));
    assert_eq!(parsed.balances[0].used.as_deref(), Some("69.75"));
    assert_eq!(parsed.extensions["balance_source"], "standard_api");
    assert_eq!(parsed.extensions["balance_status"], "available");
    assert_eq!(parsed.extensions["balance_insufficient"], false);
    assert_eq!(parsed.extensions["frozen_balance"], "2.25");
}

#[test]
fn parses_zhipu_zero_and_business_1113_as_insufficient_balance() {
    for fixture in [
        json!({"success": true, "data": {"availableBalance": "0.00"}}),
        json!({"success": false, "code": 1113, "msg": "余额不足"}),
    ] {
        let parsed = parse_zhipu_standard_balance(&fixture).unwrap();
        assert_eq!(parsed.kind, ProviderQuotaSnapshotKind::Balance);
        assert!(matches!(
            parsed.balances[0].available.as_deref(),
            Some("0.00" | "0")
        ));
        assert_eq!(parsed.extensions["balance_status"], "insufficient");
        assert_eq!(parsed.extensions["balance_insufficient"], true);
    }
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
fn parses_zhipu_credit_limits_for_personal_and_team_plans() {
    let parsed = parse_at(
        "zhipu",
        json!({"success":true,"data":{"level":"TEAM_PRO","limits":[
            {"type":"CREDIT_LIMIT","unit":3,"currentValue":250,"usage":2000,"percentage":12.5,"nextResetTime":1_900_000_000_000u64},
            {"type":"CREDIT_LIMIT","unit":6,"currentValue":1000,"usage":10000,"percentage":10,"nextResetTime":1_900_100_000_000u64}
        ]}}),
    );

    assert_eq!(parsed.windows[0].code, "credits_5h");
    assert_eq!(parsed.windows[0].label, "5小时积分");
    assert_eq!(parsed.windows[0].unit, "credits");
    assert_eq!(parsed.windows[0].window_minutes, Some(300));
    assert_eq!(parsed.windows[1].code, "credits_weekly");
    assert_eq!(parsed.windows[1].label, "每周积分");
    assert_eq!(parsed.windows[1].unit, "credits");
    assert_eq!(parsed.windows[1].window_minutes, Some(7 * 24 * 60));
    assert_eq!(parsed.extensions["plan_type"], "team_pro");
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
