use super::*;

fn subscription(exhausted: bool) -> ProviderQuotaSnapshotContract {
    let mut snapshot = ProviderQuotaSnapshotContract::subscription("kimi_coding", Vec::new(), 100);
    snapshot.exhausted = exhausted;
    snapshot
}

#[test]
fn balance_cache_scope_is_catalog_only() {
    // Given
    let key = key("key-1", "Balance", None);
    let attempt = AttemptResult::Success {
        snapshot: ProviderQuotaSnapshotContract::balance("deepseek", Vec::new()),
        status_code: 200,
        quota_kind: QuotaKind::Balance,
    };

    // When
    let scope = quota_cache_invalidation_scope(&SnapshotUpdate {
        key: &key,
        provider_type: "deepseek",
        attempt: &attempt,
        now_unix_secs: 100,
    });

    // Then
    assert_eq!(scope, QuotaCacheInvalidationScope::CatalogOnly);
}

#[test]
fn subscription_cache_scope_tracks_only_active_exhausted_transitions() {
    // Given
    let active = subscription(false);
    let key = key(
        "key-1",
        "Subscription",
        Some(serde_json::to_value(active.clone()).expect("active snapshot")),
    );
    let unchanged = AttemptResult::Success {
        snapshot: active,
        status_code: 200,
        quota_kind: QuotaKind::Subscription,
    };
    let transitioned = AttemptResult::Success {
        snapshot: subscription(true),
        status_code: 200,
        quota_kind: QuotaKind::Subscription,
    };

    // When
    let unchanged_scope = quota_cache_invalidation_scope(&SnapshotUpdate {
        key: &key,
        provider_type: "kimi_coding",
        attempt: &unchanged,
        now_unix_secs: 100,
    });
    let transitioned_scope = quota_cache_invalidation_scope(&SnapshotUpdate {
        key: &key,
        provider_type: "kimi_coding",
        attempt: &transitioned,
        now_unix_secs: 100,
    });

    // Then
    assert_eq!(unchanged_scope, QuotaCacheInvalidationScope::CatalogOnly);
    assert_eq!(
        transitioned_scope,
        QuotaCacheInvalidationScope::CandidateRouting
    );
}

#[test]
fn malformed_legacy_subscription_state_fails_safe_to_candidate_invalidation() {
    // Given
    let key = key(
        "key-1",
        "Subscription",
        Some(json!({"version": 0, "exhausted": false})),
    );
    let attempt = AttemptResult::Success {
        snapshot: subscription(false),
        status_code: 200,
        quota_kind: QuotaKind::Subscription,
    };

    // When
    let scope = quota_cache_invalidation_scope(&SnapshotUpdate {
        key: &key,
        provider_type: "kimi_coding",
        attempt: &attempt,
        now_unix_secs: 100,
    });

    // Then
    assert_eq!(scope, QuotaCacheInvalidationScope::CandidateRouting);
}

#[test]
fn management_response_has_stable_totals_and_item_fields() {
    // Given
    let snapshot = json!({"schema_version":1,"kind":"balance"});
    let items = vec![
        item("success", ItemStatus::Success, Some(snapshot.clone())),
        item("error", ItemStatus::Error, Some(snapshot.clone())),
        item("backoff", ItemStatus::Backoff, Some(snapshot)),
    ];

    // When
    let payload = management_response(items);

    // Then
    assert_eq!(payload["total"], 3);
    assert_eq!(payload["success"], 1);
    assert_eq!(payload["failed"], 1);
    assert_eq!(payload["skipped"], 1);
    assert_eq!(
        payload["total"].as_u64(),
        payload["success"]
            .as_u64()
            .zip(payload["failed"].as_u64())
            .and_then(|(success, failed)| payload["skipped"]
                .as_u64()
                .map(|skipped| success + failed + skipped))
    );
    for result in payload["results"].as_array().expect("result list") {
        for field in [
            "key_id",
            "key_name",
            "status",
            "quota_snapshot",
            "refresh_state",
        ] {
            assert!(result.get(field).is_some(), "missing {field}");
        }
    }
}

#[test]
fn http_failure_uses_execution_headers_for_bounded_delta_and_date_retry_after() {
    // Given
    let delta = ExecutionResult {
        request_id: "delta".into(),
        candidate_id: None,
        status_code: 429,
        headers: BTreeMap::from([("Retry-After".into(), "60".into())]),
        response_observation: None,
        body: None,
        telemetry: None,
        error: None,
    };
    let date = ExecutionResult {
        request_id: "date".into(),
        headers: BTreeMap::from([("retry-after".into(), "Thu, 01 Jan 1970 00:03:20 GMT".into())]),
        ..delta.clone()
    };

    // When
    let delta_attempt = execution_result_to_attempt(delta, QuotaKind::Balance, "deepseek");
    let date_attempt = execution_result_to_attempt(date, QuotaKind::Balance, "deepseek");

    // Then
    for (attempt, expected) in [(delta_attempt, 160), (date_attempt, 200)] {
        let AttemptResult::HttpFailure {
            status_code,
            headers,
            class,
            ..
        } = attempt
        else {
            panic!("expected typed HTTP failure");
        };
        assert_eq!(status_code, 429);
        assert_eq!(class, StableErrorClass::HttpRateLimited);
        assert_eq!(retry_after_eligibility(&headers, 100), Some(expected));
    }
    assert_eq!(
        retry_after_eligibility(&BTreeMap::from([("retry-after".into(), "1".into())]), 100),
        Some(130)
    );
    assert_eq!(
        retry_after_eligibility(
            &BTreeMap::from([("retry-after".into(), "9999".into())]),
            100
        ),
        Some(700)
    );
    assert_eq!(
        retry_after_eligibility(
            &BTreeMap::from([("retry-after".into(), "bad\r\nvalue".into())]),
            100
        ),
        None
    );
}

#[test]
fn http_200_business_error_preserves_zhipu_reason_instead_of_parse_failed() {
    let result = ExecutionResult {
        request_id: "zhipu-business-error".into(),
        candidate_id: None,
        status_code: 200,
        headers: BTreeMap::new(),
        response_observation: None,
        body: Some(ResponseBody {
            json_body: Some(json!({
                "code": 1315,
                "msg": "该 API Key 仅限企业编程套餐场景使用",
                "success": false
            })),
            body_bytes_b64: None,
        }),
        telemetry: None,
        error: None,
    };

    let attempt = execution_result_to_attempt(result, QuotaKind::Subscription, "zhipu");
    let AttemptResult::BusinessFailure {
        status_code,
        class,
        upstream_code,
        detail,
        ..
    } = &attempt
    else {
        panic!("expected typed business failure, got {attempt:?}");
    };
    assert_eq!(*status_code, 200);
    assert_eq!(*class, StableErrorClass::HttpClient);
    assert_eq!(*upstream_code, Some(1315));
    assert!(should_retry_zhipu_team_quota(&attempt));
    assert!(should_fallback_to_zhipu_balance(&attempt));
    assert_eq!(
        detail,
        "upstream business code 1315: API key product type does not match the selected endpoint"
    );

    let persisted = build_persisted_snapshot(SnapshotUpdate {
        key: &key("key-1", "Zhipu", None),
        provider_type: "zhipu",
        attempt: &attempt,
        now_unix_secs: 100,
    })
    .expect("business failure snapshot");
    assert_eq!(persisted.snapshot["code"], "http_client_error");
    assert_eq!(
        persisted.refresh_state.error.as_deref(),
        Some(
            "http_client_error: upstream business code 1315: API key product type does not match the selected endpoint"
        )
    );
}

#[test]
fn http_200_zhipu_business_auth_error_maps_to_unauthorized() {
    let result = ExecutionResult {
        request_id: "zhipu-business-auth".into(),
        candidate_id: None,
        status_code: 200,
        headers: BTreeMap::new(),
        response_observation: None,
        body: Some(ResponseBody {
            json_body: Some(json!({
                "code": 1001,
                "msg": "must not be copied into the persisted diagnostic",
                "success": false
            })),
            body_bytes_b64: None,
        }),
        telemetry: None,
        error: None,
    };

    let attempt = execution_result_to_attempt(result, QuotaKind::Balance, "zhipu");
    let AttemptResult::BusinessFailure {
        class,
        upstream_code,
        detail,
        ..
    } = &attempt
    else {
        panic!("expected typed business failure");
    };
    assert_eq!(*class, StableErrorClass::HttpUnauthorized);
    assert_eq!(*upstream_code, Some(1001));
    assert_eq!(
        detail,
        "upstream business code 1001: authentication rejected"
    );
    assert!(!should_fallback_to_zhipu_balance(&attempt));
}

#[test]
fn expired_or_unavailable_zhipu_token_plan_falls_back_but_rate_limits_do_not() {
    for code in [1220, 1309, 1315] {
        let attempt = execution_result_to_attempt(
            ExecutionResult {
                request_id: format!("zhipu-no-plan-{code}"),
                candidate_id: None,
                status_code: 200,
                headers: BTreeMap::new(),
                response_observation: None,
                body: Some(ResponseBody {
                    json_body: Some(json!({"code": code, "success": false})),
                    body_bytes_b64: None,
                }),
                telemetry: None,
                error: None,
            },
            QuotaKind::Subscription,
            "zhipu",
        );
        assert!(
            should_fallback_to_zhipu_balance(&attempt),
            "code {code} should use account balance"
        );
    }

    let limited = execution_result_to_attempt(
        ExecutionResult {
            request_id: "zhipu-rate-limited".into(),
            candidate_id: None,
            status_code: 200,
            headers: BTreeMap::new(),
            response_observation: None,
            body: Some(ResponseBody {
                json_body: Some(json!({"code": 1305, "success": false})),
                body_bytes_b64: None,
            }),
            telemetry: None,
            error: None,
        },
        QuotaKind::Subscription,
        "zhipu",
    );
    assert!(!should_fallback_to_zhipu_balance(&limited));

    let empty_plan = execution_result_to_attempt(
        ExecutionResult {
            request_id: "zhipu-empty-plan".into(),
            candidate_id: None,
            status_code: 200,
            headers: BTreeMap::new(),
            response_observation: None,
            body: Some(ResponseBody {
                json_body: Some(json!({"success": true, "data": {"limits": []}})),
                body_bytes_b64: None,
            }),
            telemetry: None,
            error: None,
        },
        QuotaKind::Subscription,
        "zhipu",
    );
    assert!(matches!(empty_plan, AttemptResult::ParseFailure { .. }));
    assert!(should_fallback_to_zhipu_balance(&empty_plan));

    let unknown_business_error = execution_result_to_attempt(
        ExecutionResult {
            request_id: "zhipu-unknown-business-error".into(),
            candidate_id: None,
            status_code: 200,
            headers: BTreeMap::new(),
            response_observation: None,
            body: Some(ResponseBody {
                json_body: Some(json!({"success": false})),
                body_bytes_b64: None,
            }),
            telemetry: None,
            error: None,
        },
        QuotaKind::Subscription,
        "zhipu",
    );
    assert!(matches!(
        unknown_business_error,
        AttemptResult::BusinessFailure {
            class: StableErrorClass::HttpClient,
            upstream_code: None,
            ..
        }
    ));
    assert!(should_fallback_to_zhipu_balance(&unknown_business_error));

    let decorated = apply_zhipu_token_plan_fallback_policy(
        &unknown_business_error,
        AttemptResult::Success {
            snapshot: ProviderQuotaSnapshotContract::balance("zhipu", Vec::new()),
            status_code: 200,
            quota_kind: QuotaKind::Balance,
        },
    );
    let AttemptResult::Success { snapshot, .. } = decorated else {
        panic!("expected decorated balance fallback");
    };
    assert!(!snapshot.exhausted);
    assert_eq!(snapshot.extensions["token_plan_status"], "business_error");
    assert_eq!(snapshot.extensions["token_plan_scheduling_blocked"], false);
}

#[test]
fn zhipu_business_500_retries_team_quota_then_uses_informational_balance() {
    let primary = execution_result_to_attempt(
        ExecutionResult {
            request_id: "zhipu-personal-plan-500".into(),
            candidate_id: None,
            status_code: 200,
            headers: BTreeMap::new(),
            response_observation: None,
            body: Some(ResponseBody {
                json_body: Some(json!({"code": 500, "success": false})),
                body_bytes_b64: None,
            }),
            telemetry: None,
            error: None,
        },
        QuotaKind::Subscription,
        "zhipu",
    );
    assert!(should_retry_zhipu_team_quota(&primary));
    assert!(should_fallback_to_zhipu_balance(&primary));

    let decorated = apply_zhipu_token_plan_fallback_policy(
        &primary,
        AttemptResult::Success {
            snapshot: ProviderQuotaSnapshotContract::balance("zhipu", Vec::new()),
            status_code: 200,
            quota_kind: QuotaKind::Balance,
        },
    );
    let AttemptResult::Success { snapshot, .. } = decorated else {
        panic!("expected informational balance fallback");
    };
    assert!(!snapshot.exhausted);
    assert_eq!(snapshot.extensions["token_plan_status"], "query_failed");
    assert_eq!(snapshot.extensions["token_plan_scheduling_blocked"], false);
    assert_eq!(
        snapshot.extensions["token_plan_error"],
        "upstream business code 500: quota upstream returned a business error"
    );
}

#[test]
fn zhipu_team_quota_success_records_scope() {
    let attempt = apply_zhipu_plan_scope(
        AttemptResult::Success {
            snapshot: ProviderQuotaSnapshotContract::subscription("zhipu", Vec::new(), 100),
            status_code: 200,
            quota_kind: QuotaKind::Subscription,
        },
        "team",
    );
    let AttemptResult::Success { snapshot, .. } = attempt else {
        panic!("expected team quota success");
    };
    assert_eq!(snapshot.extensions["token_plan_scope"], "team");
}

#[test]
fn zhipu_balance_kind_uses_standard_account_parser() {
    let result = ExecutionResult {
        request_id: "zhipu-standard-balance".into(),
        candidate_id: None,
        status_code: 200,
        headers: BTreeMap::new(),
        response_observation: None,
        body: Some(ResponseBody {
            json_body: Some(json!({
                "success": true,
                "data": {
                    "availableBalance": "12.50",
                    "rechargeAmount": "20",
                    "giveAmount": "5",
                    "totalSpendAmount": "12.50"
                }
            })),
            body_bytes_b64: None,
        }),
        telemetry: None,
        error: None,
    };

    let attempt = execution_result_to_attempt(result, QuotaKind::Balance, "zhipu");
    let AttemptResult::Success {
        snapshot,
        quota_kind,
        ..
    } = attempt
    else {
        panic!("expected standard balance success");
    };
    assert_eq!(quota_kind, QuotaKind::Balance);
    assert_eq!(snapshot.kind, ProviderQuotaSnapshotKind::Balance);
    assert_eq!(snapshot.balances[0].available.as_deref(), Some("12.50"));
    assert_eq!(snapshot.extensions["balance_source"], "standard_api");
    assert_eq!(snapshot.extensions["balance_insufficient"], false);
}

#[test]
fn zhipu_balance_business_1113_becomes_a_structured_insufficient_balance() {
    let attempt = execution_result_to_attempt(
        ExecutionResult {
            request_id: "zhipu-balance-insufficient".into(),
            candidate_id: None,
            status_code: 200,
            headers: BTreeMap::new(),
            response_observation: None,
            body: Some(ResponseBody {
                json_body: Some(json!({"code": 1113, "success": false})),
                body_bytes_b64: None,
            }),
            telemetry: None,
            error: None,
        },
        QuotaKind::Balance,
        "zhipu",
    );

    let AttemptResult::Success { snapshot, .. } = attempt else {
        panic!("expected structured insufficient balance");
    };
    assert_eq!(snapshot.balances[0].available.as_deref(), Some("0"));
    assert_eq!(snapshot.extensions["balance_insufficient"], true);
    assert_eq!(snapshot.extensions["balance_status"], "insufficient");
}

#[test]
fn zhipu_balance_fallback_keeps_balance_but_marks_missing_plan_as_exhausted() {
    let primary = execution_result_to_attempt(
        ExecutionResult {
            request_id: "zhipu-expired-plan".into(),
            candidate_id: None,
            status_code: 200,
            headers: BTreeMap::new(),
            response_observation: None,
            body: Some(ResponseBody {
                json_body: Some(json!({"code": 1309, "success": false})),
                body_bytes_b64: None,
            }),
            telemetry: None,
            error: None,
        },
        QuotaKind::Subscription,
        "zhipu",
    );
    let fallback = execution_result_to_attempt(
        ExecutionResult {
            request_id: "zhipu-balance-fallback".into(),
            candidate_id: None,
            status_code: 200,
            headers: BTreeMap::new(),
            response_observation: None,
            body: Some(ResponseBody {
                json_body: Some(json!({
                    "success": true,
                    "data": {"availableBalance": "12.50"}
                })),
                body_bytes_b64: None,
            }),
            telemetry: None,
            error: None,
        },
        QuotaKind::Balance,
        "zhipu",
    );

    let attempt = apply_zhipu_token_plan_fallback_policy(&primary, fallback);
    let AttemptResult::Success {
        snapshot,
        quota_kind,
        ..
    } = &attempt
    else {
        panic!("expected successful balance fallback");
    };
    assert_eq!(*quota_kind, QuotaKind::Balance);
    assert_eq!(snapshot.kind, ProviderQuotaSnapshotKind::Balance);
    assert_eq!(snapshot.balances[0].available.as_deref(), Some("12.50"));
    assert!(snapshot.exhausted);
    assert_eq!(snapshot.extensions["token_plan_status"], "expired");
    assert_eq!(snapshot.extensions["token_plan_scheduling_blocked"], true);

    let scope = quota_cache_invalidation_scope(&SnapshotUpdate {
        key: &key("key-1", "Zhipu expired", None),
        provider_type: "zhipu",
        attempt: &attempt,
        now_unix_secs: 100,
    });
    assert_eq!(scope, QuotaCacheInvalidationScope::CandidateRouting);

    let blocked_key = key(
        "key-1",
        "Zhipu expired",
        Some(serde_json::to_value(snapshot).expect("blocked balance snapshot")),
    );
    let recovered = AttemptResult::Success {
        snapshot: ProviderQuotaSnapshotContract::subscription("zhipu", Vec::new(), 100),
        status_code: 200,
        quota_kind: QuotaKind::Subscription,
    };
    assert_eq!(
        quota_cache_invalidation_scope(&SnapshotUpdate {
            key: &blocked_key,
            provider_type: "zhipu",
            attempt: &recovered,
            now_unix_secs: 100,
        }),
        QuotaCacheInvalidationScope::CandidateRouting
    );
}

#[test]
fn first_subscription_failure_builds_schema_v1_subscription_snapshot() {
    // Given
    let key = key("key-1", "Subscription", None);
    let attempt = AttemptResult::TransportFailure {
        quota_kind: Some(QuotaKind::Subscription),
        class: StableErrorClass::TransportFailed,
    };

    // When
    let persisted = build_persisted_snapshot(SnapshotUpdate {
        key: &key,
        provider_type: "kimi_coding",
        attempt: &attempt,
        now_unix_secs: 100,
    })
    .expect("typed subscription failure snapshot");

    // Then
    assert_eq!(persisted.snapshot["schema_version"], 1);
    assert_eq!(persisted.snapshot["kind"], "subscription");
    assert_eq!(persisted.snapshot["provider_type"], "kimi_coding");
    assert_eq!(persisted.snapshot["code"], "transport_failed");
}

#[test]
fn success_decoration_preserves_parser_exhaustion() {
    // Given
    let key = key("key-1", "Subscription", None);
    let snapshot = ProviderQuotaSnapshotContract::subscription(
        "kimi_coding",
        vec![ProviderQuotaWindow {
            code: "cycle".into(),
            label: "cycle".into(),
            scope: "account".into(),
            unit: "count".into(),
            used_value: Some(ProviderQuotaValue::Number(10.into())),
            remaining_value: Some(ProviderQuotaValue::Number(0.into())),
            limit_value: Some(ProviderQuotaValue::Number(10.into())),
            used_ratio: Some(1.0),
            remaining_ratio: Some(0.0),
            window_minutes: Some(60),
            reset_at: Some(200),
            reset_at_text: None,
            is_exhausted: true,
        }],
        100,
    );
    assert!(snapshot.exhausted);
    let attempt = AttemptResult::Success {
        snapshot,
        status_code: 200,
        quota_kind: QuotaKind::Subscription,
    };

    // When
    let persisted = build_persisted_snapshot(SnapshotUpdate {
        key: &key,
        provider_type: "kimi_coding",
        attempt: &attempt,
        now_unix_secs: 100,
    })
    .expect("typed success snapshot");

    // Then
    assert_eq!(persisted.snapshot["exhausted"], true);
    assert_eq!(persisted.snapshot["kind"], "subscription");
}

#[test]
fn only_manual_refresh_bypasses_persisted_backoff() {
    // Given
    let sources = [
        QuotaRefreshSource::Manual,
        QuotaRefreshSource::PoolBackground,
        QuotaRefreshSource::AccountSelfCheck,
        QuotaRefreshSource::OAuthPostUpdate,
    ];

    // When
    let bypasses = sources.map(QuotaRefreshSource::bypasses_persisted_backoff);

    // Then
    assert_eq!(bypasses, [true, false, false, false]);
}

#[test]
fn error_and_backoff_items_return_the_latest_snapshot_and_refresh_state() {
    // Given
    let mut retained = ProviderQuotaSnapshotContract::balance("deepseek", Vec::new());
    retained.refresh_state = ProviderQuotaRefreshState {
        last_attempt_at: Some(100),
        last_success_at: Some(90),
        error: Some("http_rate_limited: quota upstream rate limited the request".into()),
        next_eligible_at: Some(160),
        failure_count: Some(1),
    };
    let retained = serde_json::to_value(retained).expect("retained snapshot");
    let key = key("key-1", "DeepSeek", Some(retained.clone()));
    let attempt = AttemptResult::HttpFailure {
        status_code: 429,
        headers: BTreeMap::from([("retry-after".into(), "60".into())]),
        class: StableErrorClass::HttpRateLimited,
        quota_kind: QuotaKind::Balance,
    };
    let persisted = build_persisted_snapshot(SnapshotUpdate {
        key: &key,
        provider_type: "deepseek",
        attempt: &attempt,
        now_unix_secs: 110,
    })
    .expect("error snapshot");

    // When
    let error = persisted_item(&key, &attempt, persisted);
    let backoff = backoff_item(&key);

    // Then
    assert_eq!(error.status, ItemStatus::Error);
    assert_eq!(
        error
            .quota_snapshot
            .as_ref()
            .and_then(|value| value.get("code")),
        Some(&json!("http_rate_limited"))
    );
    assert_eq!(error.refresh_state.last_attempt_at, Some(110));
    assert_eq!(backoff.status, ItemStatus::Backoff);
    assert_eq!(backoff.quota_snapshot, Some(retained));
    assert_eq!(backoff.refresh_state.next_eligible_at, Some(160));
}

#[test]
fn persisted_backoff_is_enforced_for_background_and_bypassed_for_manual() {
    // Given
    let mut retained = ProviderQuotaSnapshotContract::balance("deepseek", Vec::new());
    retained.refresh_state.next_eligible_at = Some(160);
    let key = key(
        "key-1",
        "DeepSeek",
        Some(serde_json::to_value(retained).expect("backoff snapshot")),
    );

    // When
    let background = persisted_backoff_applies(&key, QuotaRefreshSource::PoolBackground, 100);
    let manual = persisted_backoff_applies(&key, QuotaRefreshSource::Manual, 100);

    // Then
    assert!(background);
    assert!(!manual);
}
