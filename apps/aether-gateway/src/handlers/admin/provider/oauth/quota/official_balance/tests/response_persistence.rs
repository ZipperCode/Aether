use super::*;

fn subscription(exhausted: bool) -> ProviderQuotaSnapshotContract {
    let mut snapshot = ProviderQuotaSnapshotContract::subscription(
        "kimi_coding",
        vec![ProviderQuotaWindow {
            source_id: Some("subscription".into()),
            code: "cycle".into(),
            scope: "account".into(),
            // 与实际解析器的比例单位一致，才能验证有效额度的调度转换。
            unit: "percent".into(),
            reset_at: Some(4_102_444_800),
            is_exhausted: exhausted,
            ..Default::default()
        }],
        100,
    );
    snapshot.sources.push(ProviderQuotaSource {
        id: "subscription".into(),
        product: "coding_plan".into(),
        scope: "account".into(),
        query_status: ProviderQuotaQueryStatus::Ok,
        freshness: "fresh".into(),
        ..Default::default()
    });
    snapshot.exhausted = exhausted;
    snapshot
}

/// 构造带 freshness 的标准余额快照，供缓存 eligibility 转换测试复用。
fn balance(available: &str, freshness: &str) -> ProviderQuotaSnapshotContract {
    let mut snapshot = ProviderQuotaSnapshotContract::balance(
        "deepseek",
        vec![aether_provider_pool::ProviderQuotaBalance {
            unit: "CNY".to_string(),
            available: Some(available.to_string()),
            total: None,
            granted: None,
            topped_up: None,
            used: None,
            ..Default::default()
        }],
    );
    snapshot
        .extensions
        .insert("freshness".to_string(), json!(freshness));
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
fn balance_cache_scope_invalidates_fresh_low_to_fresh_active() {
    // 验证余额恢复后立即清除候选缓存，使 Key 自动重新加入调度。
    let key = key(
        "key-1",
        "Low balance",
        Some(serde_json::to_value(balance("1", "fresh")).expect("low snapshot")),
    );
    let attempt = AttemptResult::Success {
        snapshot: balance("1.0001", "fresh"),
        status_code: 200,
        quota_kind: QuotaKind::Balance,
    };

    assert_eq!(
        quota_cache_invalidation_scope(&SnapshotUpdate {
            key: &key,
            provider_type: "deepseek",
            attempt: &attempt,
            now_unix_secs: 100,
        }),
        QuotaCacheInvalidationScope::CandidateRouting
    );
}

#[test]
fn balance_cache_scope_invalidates_fresh_low_to_stale() {
    // 验证刷新失败后的 stale 快照 fail-open，并立即清除旧低余额候选缓存。
    let key = key(
        "key-1",
        "Low balance",
        Some(serde_json::to_value(balance("1", "fresh")).expect("low snapshot")),
    );
    let attempt = AttemptResult::TransportFailure {
        class: StableErrorClass::TransportFailed,
        quota_kind: Some(QuotaKind::Balance),
    };

    assert_eq!(
        quota_cache_invalidation_scope(&SnapshotUpdate {
            key: &key,
            provider_type: "deepseek",
            attempt: &attempt,
            now_unix_secs: 100,
        }),
        QuotaCacheInvalidationScope::CandidateRouting
    );
}

#[test]
fn balance_cache_scope_keeps_unchanged_low_state_catalog_only() {
    // 验证 low→low 不重复失效候选缓存，仅刷新目录展示数据。
    let key = key(
        "key-1",
        "Low balance",
        Some(serde_json::to_value(balance("1", "fresh")).expect("low snapshot")),
    );
    let attempt = AttemptResult::Success {
        snapshot: balance("0.5", "fresh"),
        status_code: 200,
        quota_kind: QuotaKind::Balance,
    };

    assert_eq!(
        quota_cache_invalidation_scope(&SnapshotUpdate {
            key: &key,
            provider_type: "deepseek",
            attempt: &attempt,
            now_unix_secs: 100,
        }),
        QuotaCacheInvalidationScope::CatalogOnly
    );
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
fn malformed_legacy_subscription_state_does_not_invent_an_eligibility_transition() {
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
    assert_eq!(scope, QuotaCacheInvalidationScope::CatalogOnly);
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
    assert_eq!(
        attempt.query_status(),
        ProviderQuotaQueryStatus::NotApplicable
    );
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
    assert_eq!(
        attempt.query_status(),
        ProviderQuotaQueryStatus::PermissionDenied
    );
}

#[test]
fn unavailable_zhipu_plan_keeps_query_status_separate_from_rate_limits() {
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
        assert_eq!(
            attempt.query_status(),
            if code == 1220 {
                ProviderQuotaQueryStatus::PermissionDenied
            } else {
                ProviderQuotaQueryStatus::NotApplicable
            }
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
    assert_eq!(limited.query_status(), ProviderQuotaQueryStatus::Error);

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
    assert_eq!(empty_plan.query_status(), ProviderQuotaQueryStatus::Error);

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
    assert_eq!(
        unknown_business_error.query_status(),
        ProviderQuotaQueryStatus::Error
    );
}

#[test]
fn zhipu_business_500_is_persisted_as_an_independent_source_failure() {
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
    let attempt = AttemptResult::Sources {
        attempts: vec![zhipu_source_attempt("personal", primary)],
        quota_kind: QuotaKind::Subscription,
    };
    let persisted = build_persisted_snapshot(SnapshotUpdate {
        key: &key("key-1", "Zhipu", None),
        provider_type: "zhipu",
        attempt: &attempt,
        now_unix_secs: 100,
    })
    .expect("source failure snapshot");
    assert_eq!(persisted.snapshot["exhausted"], false);
    assert_eq!(persisted.snapshot["sources"][0]["id"], "personal");
    assert_eq!(persisted.snapshot["sources"][0]["query_status"], "error");
    assert_eq!(
        persisted.snapshot["sources"][0]["refresh_state"]["error"],
        "http_server_error: upstream business code 500: quota upstream returned a business error"
    );
}

#[test]
fn zhipu_team_quota_success_records_scope() {
    let mut snapshot = ProviderQuotaSnapshotContract::subscription("zhipu", Vec::new(), 100);
    snapshot.sources.push(ProviderQuotaSource {
        id: "subscription".into(),
        product: "coding_plan".into(),
        ..Default::default()
    });
    let attempt = apply_zhipu_plan_scope(
        AttemptResult::Success {
            snapshot,
            status_code: 200,
            quota_kind: QuotaKind::Subscription,
        },
        "team",
    );
    let AttemptResult::Success { snapshot, .. } = attempt else {
        panic!("expected team quota success");
    };
    assert_eq!(snapshot.sources[0].id, "team");
    assert_eq!(snapshot.sources[0].scope, "team");
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
    assert_eq!(snapshot.sources[0].product, "account_balance");
}

#[test]
fn zhipu_balance_business_1113_does_not_synthesize_zero_balance() {
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

    assert!(matches!(
        attempt,
        AttemptResult::BusinessFailure {
            upstream_code: Some(1113),
            ..
        }
    ));
    assert_eq!(attempt.query_status(), ProviderQuotaQueryStatus::Error);
}

#[test]
fn zhipu_balance_success_and_missing_plan_are_kept_without_global_exhaustion() {
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

    let attempt = AttemptResult::Sources {
        attempts: vec![
            zhipu_source_attempt("personal", primary),
            zhipu_source_attempt("balance", fallback),
        ],
        quota_kind: QuotaKind::Subscription,
    };
    let persisted = build_persisted_snapshot(SnapshotUpdate {
        key: &key("key-1", "Zhipu", None),
        provider_type: "zhipu",
        attempt: &attempt,
        now_unix_secs: 100,
    })
    .expect("partial quota snapshot");
    assert_eq!(persisted.snapshot["balances"][0]["available"], "12.50");
    assert_eq!(persisted.snapshot["exhausted"], false);
    assert_eq!(persisted.snapshot["code"], "ok");
    assert_eq!(
        persisted.snapshot["sources"][0]["query_status"],
        "not_applicable"
    );
    assert_eq!(persisted.snapshot["sources"][1]["query_status"], "ok");

    let scope = quota_cache_invalidation_scope(&SnapshotUpdate {
        key: &key("key-1", "Zhipu expired", None),
        provider_type: "zhipu",
        attempt: &attempt,
        now_unix_secs: 100,
    });
    assert_eq!(scope, QuotaCacheInvalidationScope::CatalogOnly);
}

#[test]
fn minimax_business_codes_keep_2049_denied_and_2062_plan_not_applicable() {
    let denied = execution_result_to_attempt(
        ExecutionResult {
            request_id: "minimax-2049".into(),
            candidate_id: None,
            status_code: 200,
            headers: BTreeMap::new(),
            response_observation: None,
            body: Some(ResponseBody {
                json_body: Some(json!({"base_resp": {"status_code": 2049}})),
                body_bytes_b64: None,
            }),
            telemetry: None,
            error: None,
        },
        QuotaKind::Subscription,
        "minimax",
    );
    assert_eq!(
        denied.query_status(),
        ProviderQuotaQueryStatus::PermissionDenied
    );

    let no_plan = execution_result_to_attempt(
        ExecutionResult {
            request_id: "minimax-2062".into(),
            candidate_id: None,
            status_code: 200,
            headers: BTreeMap::new(),
            response_observation: None,
            body: Some(ResponseBody {
                json_body: Some(json!({"base_resp": {"status_code": 2062}})),
                body_bytes_b64: None,
            }),
            telemetry: None,
            error: None,
        },
        QuotaKind::Subscription,
        "minimax",
    );
    assert_eq!(
        no_plan.query_status(),
        ProviderQuotaQueryStatus::NotApplicable
    );

    // 2062 只对套餐查询声明不适用；同一码到达余额查询时不能被猜测为不适用。
    let balance_channel = execution_result_to_attempt(
        ExecutionResult {
            request_id: "minimax-2062-balance".into(),
            candidate_id: None,
            status_code: 200,
            headers: BTreeMap::new(),
            response_observation: None,
            body: Some(ResponseBody {
                json_body: Some(json!({"base_resp": {"status_code": 2062}})),
                body_bytes_b64: None,
            }),
            telemetry: None,
            error: None,
        },
        QuotaKind::Balance,
        "minimax",
    );
    assert_eq!(
        balance_channel.query_status(),
        ProviderQuotaQueryStatus::Error
    );
}

#[test]
fn minimax_balance_success_and_missing_token_plan_merge_independently() {
    // 旧格式 Key 套餐 2062 而余额成功：两来源独立保留，互不覆盖。
    let balance = execution_result_to_attempt(
        ExecutionResult {
            request_id: "minimax-balance".into(),
            candidate_id: None,
            status_code: 200,
            headers: BTreeMap::new(),
            response_observation: None,
            body: Some(ResponseBody {
                json_body: Some(json!({
                    "available_amount": "12.34",
                    "cash_balance": "10.00",
                    "voucher_balance": "2.34",
                    "currency": "CNY"
                })),
                body_bytes_b64: None,
            }),
            telemetry: None,
            error: None,
        },
        QuotaKind::Balance,
        "minimax",
    );
    let no_plan = execution_result_to_attempt(
        ExecutionResult {
            request_id: "minimax-token-plan-2062".into(),
            candidate_id: None,
            status_code: 200,
            headers: BTreeMap::new(),
            response_observation: None,
            body: Some(ResponseBody {
                json_body: Some(json!({"base_resp": {"status_code": 2062}})),
                body_bytes_b64: None,
            }),
            telemetry: None,
            error: None,
        },
        QuotaKind::Subscription,
        "minimax",
    );
    let attempt = AttemptResult::Sources {
        attempts: vec![
            minimax_source_attempt("balance", balance),
            minimax_source_attempt("subscription", no_plan),
        ],
        quota_kind: QuotaKind::Balance,
    };
    let persisted = build_persisted_snapshot(SnapshotUpdate {
        key: &key("key-1", "MiniMax", None),
        provider_type: "minimax",
        attempt: &attempt,
        now_unix_secs: 100,
    })
    .expect("minimax dual source snapshot");

    assert_eq!(persisted.snapshot["code"], "ok");
    assert_eq!(persisted.snapshot["exhausted"], false);
    assert_eq!(persisted.snapshot["balances"][0]["available"], "12.34");
    assert_eq!(persisted.snapshot["sources"][0]["id"], "balance");
    assert_eq!(persisted.snapshot["sources"][0]["query_status"], "ok");
    assert_eq!(persisted.snapshot["sources"][1]["id"], "subscription");
    assert_eq!(
        persisted.snapshot["sources"][1]["query_status"],
        "not_applicable"
    );
    assert_eq!(persisted.snapshot["sources"][1]["freshness"], "fresh");
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
            ..Default::default()
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

fn openrouter_endpoint() -> StoredProviderCatalogEndpoint {
    endpoint(EndpointFixture {
        id: "endpoint-1",
        provider_id: "provider-1",
        base_url: "https://openrouter.ai/api/v1",
        active: true,
    })
}

fn openrouter_history_snapshot() -> Value {
    json!({
        "schema_version": 1,
        "provider_type": "openrouter",
        "kind": "balance",
        "sources": [
            {
                "id": "key_limit",
                "label": "Key 消费限额",
                "product": "key_spending_limit",
                "scope": "key",
                "region": "global",
                "query_status": "ok",
                "freshness": "fresh",
                "refresh_state": {"last_attempt_at": 90, "last_success_at": 90}
            },
            {
                "id": "free_requests",
                "label": "免费模型每日请求",
                "product": "model_requests",
                "scope": "model",
                "region": "global",
                "query_status": "ok",
                "freshness": "fresh",
                "refresh_state": {"last_attempt_at": 90, "last_success_at": 90}
            }
        ],
        "balances": [
            {"source_id": "key_limit", "unit": "USD", "available": "5"}
        ],
        "windows": [{
            "source_id": "free_requests",
            "code": "free_model_daily_requests",
            "label": "免费模型每日请求",
            "scope": "model",
            "unit": "requests",
            "limit_value": "50",
            "used_value": "12",
            "remaining_value": "38",
            "is_exhausted": false
        }],
        "exhausted": false
    })
}

#[test]
fn openrouter_query_failure_retains_free_requests_history_as_stale() {
    // 免费来源已在请求前声明身份；查询失败时旧来源与窗口保留为过期数据。
    let key = key("key-1", "OpenRouter", Some(openrouter_history_snapshot()));
    let attempt = AttemptResult::Sources {
        attempts: vec![SourceAttempt {
            sources: official_api_key_quota_sources(
                "openrouter",
                &openrouter_endpoint(),
                "balance",
            ),
            result: AttemptResult::HttpFailure {
                status_code: 503,
                headers: BTreeMap::new(),
                class: StableErrorClass::HttpServer,
                quota_kind: QuotaKind::Balance,
            },
        }],
        quota_kind: QuotaKind::Balance,
    };
    let persisted = build_persisted_snapshot(SnapshotUpdate {
        key: &key,
        provider_type: "openrouter",
        attempt: &attempt,
        now_unix_secs: 200,
    })
    .expect("openrouter failure snapshot");

    let sources = persisted.snapshot["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 2);
    let free = sources
        .iter()
        .find(|source| source["id"] == "free_requests")
        .expect("free source retained");
    assert_eq!(free["freshness"], "stale");
    assert_eq!(free["query_status"], "error");
    assert_eq!(
        persisted.snapshot["windows"][0]["source_id"],
        "free_requests"
    );
    assert_eq!(persisted.snapshot["windows"][0]["remaining_value"], "38");
    let key_limit = sources
        .iter()
        .find(|source| source["id"] == "key_limit")
        .expect("key limit source retained");
    assert_eq!(key_limit["freshness"], "stale");
    assert_eq!(persisted.snapshot["balances"][0]["available"], "5");
}

#[test]
fn openrouter_success_without_free_field_expires_free_requests_history() {
    // 成功响应未携带免费字段：不是失败，来源与旧窗口随之过期消失，也不计错误退避。
    let key = key("key-1", "OpenRouter", Some(openrouter_history_snapshot()));
    let parsed = parse_openrouter_credits(&json!({
        "data": {"limit": 10, "limit_remaining": 5}
    }))
    .unwrap();
    let attempt = AttemptResult::Sources {
        attempts: vec![SourceAttempt {
            sources: official_api_key_quota_sources(
                "openrouter",
                &openrouter_endpoint(),
                "balance",
            ),
            result: AttemptResult::Success {
                snapshot: parsed,
                status_code: 200,
                quota_kind: QuotaKind::Balance,
            },
        }],
        quota_kind: QuotaKind::Balance,
    };
    let persisted = build_persisted_snapshot(SnapshotUpdate {
        key: &key,
        provider_type: "openrouter",
        attempt: &attempt,
        now_unix_secs: 200,
    })
    .expect("openrouter success snapshot");

    let sources = persisted.snapshot["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["id"], "key_limit");
    assert_eq!(sources[0]["query_status"], "ok");
    assert_eq!(sources[0]["freshness"], "fresh");
    assert!(persisted.snapshot.get("windows").is_none());
    assert_eq!(persisted.snapshot["balances"][0]["available"], "5");
    assert_eq!(persisted.snapshot["code"], "ok");
}
