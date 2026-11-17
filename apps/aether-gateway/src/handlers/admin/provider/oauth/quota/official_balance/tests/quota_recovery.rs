use super::*;
use crate::{
    ai_serving::build_provider_key_pool_score_upsert, data::GatewayDataState,
    handlers::admin::request::AdminAppState, AppState,
};
use aether_data::repository::{
    pool_scores::InMemoryPoolMemberScoreRepository,
    provider_catalog::InMemoryProviderCatalogReadRepository,
};
use aether_data_contracts::repository::{
    pool_scores::{GetPoolMemberScoresByIdsQuery, PoolMemberHardState, PoolScoreReadRepository},
    provider_catalog::{ProviderCatalogReadRepository, StoredProviderCatalogProvider},
};

fn quota_attempt(weekly_exhausted: bool, personal_failed: bool) -> AttemptResult {
    let mut plan = ProviderQuotaSnapshotContract::subscription(
        "zhipu",
        vec![
            ProviderQuotaWindow {
                source_id: Some("personal".into()),
                code: "credits_5h".into(),
                scope: "account".into(),
                unit: "credits".into(),
                remaining_value: Some(ProviderQuotaValue::Decimal("27930".into())),
                used_ratio: Some(0.01),
                ..Default::default()
            },
            ProviderQuotaWindow {
                source_id: Some("personal".into()),
                code: "credits_weekly".into(),
                scope: "account".into(),
                unit: "credits".into(),
                remaining_value: Some(ProviderQuotaValue::Decimal(
                    if weekly_exhausted { "0" } else { "136191" }.into(),
                )),
                used_ratio: Some(if weekly_exhausted { 1.0 } else { 0.02 }),
                is_exhausted: weekly_exhausted,
                reset_at: Some(4_102_444_800),
                ..Default::default()
            },
        ],
        100,
    );
    plan.sources = quota_sources("personal");
    let mut balance = ProviderQuotaSnapshotContract::balance(
        "zhipu",
        vec![aether_provider_pool::ProviderQuotaBalance {
            source_id: Some("balance".into()),
            unit: "CNY".into(),
            available: Some("0".into()),
            ..Default::default()
        }],
    );
    balance.sources = quota_sources("balance");
    AttemptResult::Sources {
        attempts: vec![
            zhipu_source_attempt(
                "personal",
                if personal_failed {
                    AttemptResult::TransportFailure {
                        class: StableErrorClass::TransportFailed,
                        quota_kind: Some(QuotaKind::Subscription),
                    }
                } else {
                    AttemptResult::Success {
                        snapshot: plan,
                        status_code: 200,
                        quota_kind: QuotaKind::Subscription,
                    }
                },
            ),
            zhipu_source_attempt(
                "team",
                AttemptResult::TransportFailure {
                    class: StableErrorClass::TransportFailed,
                    quota_kind: Some(QuotaKind::Subscription),
                },
            ),
            zhipu_source_attempt(
                "balance",
                AttemptResult::Success {
                    snapshot: balance,
                    status_code: 200,
                    quota_kind: QuotaKind::Balance,
                },
            ),
        ],
        quota_kind: QuotaKind::Subscription,
    }
}

fn quota_sources(scope: &str) -> Vec<ProviderQuotaSource> {
    let mut sources = zhipu_source_attempt(
        scope,
        AttemptResult::TransportFailure {
            class: StableErrorClass::TransportFailed,
            quota_kind: Some(QuotaKind::Subscription),
        },
    )
    .sources;
    sources[0].query_status = ProviderQuotaQueryStatus::Ok;
    sources
}

#[tokio::test]
async fn quota_refresh_recovers_old_runtime_block_and_pool_score() {
    let mut provider =
        StoredProviderCatalogProvider::new("provider-1".into(), "GLM".into(), None, "zhipu".into())
            .unwrap();
    provider.config = Some(json!({"pool_advanced": {}}));
    for (weekly_exhausted, personal_failed, block_time, active, score_state, recovered) in [
        (
            false,
            false,
            90,
            true,
            PoolMemberHardState::QuotaExhausted,
            true,
        ),
        (
            true,
            false,
            90,
            true,
            PoolMemberHardState::QuotaExhausted,
            false,
        ),
        (
            false,
            true,
            90,
            true,
            PoolMemberHardState::QuotaExhausted,
            false,
        ),
        (
            false,
            false,
            100,
            true,
            PoolMemberHardState::QuotaExhausted,
            false,
        ),
        (
            false,
            false,
            101,
            true,
            PoolMemberHardState::QuotaExhausted,
            false,
        ),
        (
            false,
            false,
            90,
            false,
            PoolMemberHardState::AuthInvalid,
            true,
        ),
    ] {
        let mut key = key("key-1", "GLM", None);
        key.is_active = active;
        key.status_snapshot = Some(json!({
            "scheduling": {"code": "quota_exhausted", "blocked": true, "last_observed_at": block_time, "requires_manual_recovery": true},
            "health": {"status": "degraded"}
        }));
        key.oauth_invalid_reason = (!active).then(|| "existing authentication state".into());
        let mut score = build_provider_key_pool_score_upsert(
            &key,
            "zhipu",
            None,
            90,
            aether_pool_core::PoolMemberScoreRules::default(),
        )
        .into_stored();
        score.hard_state = score_state;
        let score_id = score.id.clone();
        let scores = Arc::new(InMemoryPoolMemberScoreRepository::seed(vec![score]));
        let catalog = Arc::new(InMemoryProviderCatalogReadRepository::seed(
            vec![provider.clone()],
            vec![],
            vec![key.clone()],
        ));
        let state = AppState::new().unwrap().with_data_state_for_tests(
            GatewayDataState::with_provider_catalog_repository_for_tests(catalog.clone())
                .with_pool_score_repository_for_tests(scores.clone()),
        );
        let attempt = quota_attempt(weekly_exhausted, personal_failed);
        super::super::persistence::persist_attempt(
            &AdminAppState::new(&state),
            SnapshotUpdate {
                key: &key,
                provider_type: "zhipu",
                attempt: &attempt,
                now_unix_secs: 100,
            },
        )
        .await
        .expect("quota refresh should persist");
        let stored = catalog
            .list_keys_by_ids(&[key.id.clone()])
            .await
            .unwrap()
            .remove(0);
        assert_eq!(
            stored
                .status_snapshot
                .as_ref()
                .unwrap()
                .get("scheduling")
                .is_none_or(Value::is_null),
            recovered,
            "weekly={weekly_exhausted}, failed={personal_failed}, block={block_time}"
        );
        assert_eq!(stored.is_active, active);
        assert_eq!(stored.oauth_invalid_reason, key.oauth_invalid_reason);
        assert_eq!(
            stored.status_snapshot.unwrap()["health"],
            json!({"status":"degraded"})
        );
        let score = scores
            .get_pool_member_scores_by_ids(&GetPoolMemberScoresByIdsQuery {
                ids: vec![score_id],
            })
            .await
            .unwrap()
            .remove(0);
        if score_state == PoolMemberHardState::AuthInvalid || !recovered {
            assert_eq!(score.hard_state, score_state);
        } else {
            // 未执行健康探测时为 Unknown；调度器与 Available 一样接纳该状态。
            assert_eq!(score.hard_state, PoolMemberHardState::Unknown);
        }
    }
}

#[test]
fn positive_quota_evidence_rejects_unknown_or_stale_sources() {
    use aether_provider_pool::provider_quota_snapshot_has_available_quota as available;
    let key = key("key-1", "GLM", None);
    let attempt = quota_attempt(false, false);
    let snapshot = build_persisted_snapshot(SnapshotUpdate {
        key: &key,
        provider_type: "zhipu",
        attempt: &attempt,
        now_unix_secs: 100,
    })
    .unwrap()
    .snapshot;
    assert!(available(&snapshot, 100));
    assert!(!available(&snapshot, 101));
    let mut stale = snapshot.clone();
    stale["sources"][0]["freshness"] = json!("stale");
    assert!(!available(&stale, 100));
    let mut unknown = snapshot.clone();
    unknown["windows"] = json!([]);
    assert!(!available(&unknown, 100));
    let mut money = snapshot;
    money["windows"] = json!([]);
    money["balances"][0]["available"] = json!("29.18133021");
    assert!(available(&money, 100));
    money["balances"][0]["available"] = json!("1");
    assert!(!available(&money, 100));
    money["balances"][0]["available"] = json!("1.000000000000000000001");
    assert!(available(&money, 100));
}
