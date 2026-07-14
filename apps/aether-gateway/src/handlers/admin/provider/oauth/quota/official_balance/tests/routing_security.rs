use super::*;

#[test]
fn selected_endpoint_is_valid_with_multiple_other_active_endpoints() {
    // Given
    let selected = endpoint(EndpointFixture {
        id: "selected",
        provider_id: "provider-1",
        base_url: "https://api.deepseek.com",
        active: true,
    });
    let _other = endpoint(EndpointFixture {
        id: "other",
        provider_id: "provider-1",
        base_url: "https://api.deepseek.com/v1",
        active: true,
    });

    // When
    let result = validate_selected_endpoint("provider-1", "deepseek", &selected);

    // Then
    assert_eq!(result, Ok(()));
}

#[test]
fn selected_endpoint_rejects_inactive_foreign_and_nonofficial_origins() {
    // Given
    let cases = [
        EndpointFixture {
            id: "inactive",
            provider_id: "provider-1",
            base_url: "https://api.deepseek.com",
            active: false,
        },
        EndpointFixture {
            id: "foreign",
            provider_id: "provider-2",
            base_url: "https://api.deepseek.com",
            active: true,
        },
        EndpointFixture {
            id: "hostile",
            provider_id: "provider-1",
            base_url: "https://api.deepseek.com.evil.test",
            active: true,
        },
    ];

    // When
    let results = cases
        .map(|fixture| validate_selected_endpoint("provider-1", "deepseek", &endpoint(fixture)));

    // Then
    for result in results {
        assert!(result.is_err());
    }
}

#[tokio::test]
async fn configured_tunnel_is_used_and_explicit_override_skips_resolver() {
    // Given
    let calls = Arc::new(AtomicUsize::new(0));
    let configured_calls = Arc::clone(&calls);
    let tunnel = ProxySnapshot {
        enabled: Some(true),
        mode: Some("tunnel".into()),
        node_id: Some("node-1".into()),
        ..ProxySnapshot::default()
    };

    // When
    let configured = resolve_execution_route(None, move || {
        configured_calls.fetch_add(1, Ordering::SeqCst);
        let tunnel = tunnel.clone();
        async move { (Some(tunnel), Some("key")) }
    })
    .await;
    let explicit = ProxySnapshot {
        enabled: Some(true),
        url: Some("http://user:secret@proxy.test/?token=hidden".into()),
        ..ProxySnapshot::default()
    };
    let selected = resolve_execution_route(Some(explicit.clone()), || async {
        panic!("configured resolver must not run for an explicit override")
    })
    .await;

    // Then
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(configured.source, RouteSource::Key);
    assert_eq!(
        configured
            .proxy
            .as_ref()
            .and_then(|proxy| proxy.mode.as_deref()),
        Some("tunnel")
    );
    assert_eq!(selected.source, RouteSource::ExplicitOverride);
    assert_eq!(selected.proxy, Some(explicit));
    assert_eq!(
        official_balance_execution_timeouts(None, configured.proxy.as_ref()).total_ms,
        Some(60_000)
    );
}

#[test]
fn singleflight_identity_isolated_by_endpoint_and_route_without_secrets() {
    // Given
    let scope = FlightScope {
        provider_id: "provider-1",
        key_id: "key-1",
        endpoint_id: "endpoint-1",
    };
    let raw_secret = "http://user:password@proxy.test/?token=Bearer-secret";
    let explicit = ExecutionRoute {
        proxy: Some(ProxySnapshot {
            enabled: Some(true),
            url: Some(raw_secret.into()),
            ..ProxySnapshot::default()
        }),
        source: RouteSource::ExplicitOverride,
    };
    let configured = ExecutionRoute {
        proxy: explicit.proxy.clone(),
        source: RouteSource::Key,
    };
    let default_route = ExecutionRoute {
        proxy: explicit.proxy.clone(),
        source: RouteSource::Direct,
    };

    // When
    let explicit_id = singleflight_identity(scope, &explicit);
    let configured_id = singleflight_identity(scope, &configured);
    let default_id = singleflight_identity(scope, &default_route);
    let other_endpoint_id = singleflight_identity(
        FlightScope {
            endpoint_id: "endpoint-2",
            ..scope
        },
        &explicit,
    );

    // Then
    assert_ne!(explicit_id, configured_id);
    assert_ne!(explicit_id, default_id);
    assert_ne!(explicit_id, other_endpoint_id);
    assert!(!explicit_id.contains(raw_secret));
    assert!(!explicit_id.contains("password"));
    assert!(!explicit_id.contains("Bearer-secret"));
}

#[test]
fn singleflight_identity_distinguishes_delimiter_and_control_character_scopes() {
    // Given
    let route = ExecutionRoute {
        proxy: None,
        source: RouteSource::Direct,
    };
    let delimiter_scope = FlightScope {
        provider_id: "a:b",
        key_id: "c",
        endpoint_id: "d",
    };
    let shifted_delimiter_scope = FlightScope {
        provider_id: "a",
        key_id: "b:c",
        endpoint_id: "d",
    };
    let control_scope = FlightScope {
        provider_id: "a\0b",
        key_id: "c",
        endpoint_id: "d",
    };

    // When
    let delimiter_id = singleflight_identity(delimiter_scope, &route);
    let shifted_delimiter_id = singleflight_identity(shifted_delimiter_scope, &route);
    let control_id = singleflight_identity(control_scope, &route);

    // Then
    assert_ne!(delimiter_id, shifted_delimiter_id);
    assert_ne!(delimiter_id, control_id);
    for identity in [&delimiter_id, &shifted_delimiter_id, &control_id] {
        assert_eq!(identity.len(), 64);
        assert!(identity.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(identity.chars().all(|character| !character.is_control()));
        for raw_id in ["a:b", "b:c", "a\0b"] {
            assert!(!identity.contains(raw_id));
        }
    }
    eprintln!("assertion: delimiter-distinct singleflight scopes produced different identities");
}

#[test]
#[ignore = "manual terminal/data-surface verification"]
fn manual_singleflight_identity_surface() {
    // Given
    let identity = |provider_id, key_id, endpoint_id, source| {
        singleflight_identity(
            FlightScope {
                provider_id,
                key_id,
                endpoint_id,
            },
            &ExecutionRoute {
                proxy: Some(ProxySnapshot::default()),
                source,
            },
        )
    };

    // When
    let identities = [
        identity("a:b", "c", "d", RouteSource::ExplicitOverride),
        identity("a", "b:c", "d", RouteSource::ExplicitOverride),
        identity("a\0b", "c", "d", RouteSource::ExplicitOverride),
        identity("a:b", "c", "d-2", RouteSource::ExplicitOverride),
        identity("a:b", "c", "d", RouteSource::Direct),
    ];

    // Then
    assert!(identities[1..].iter().all(|id| id != &identities[0]));
    for identity in &identities {
        assert_eq!(identity.len(), 64);
        assert!(identity.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
    eprintln!("manual surface PASS {}", identities.join(","));
}

#[test]
fn malformed_success_is_typed_parse_failure_and_secrets_are_never_exposed() {
    // Given
    let malicious = "Bearer top-secret\r\nX-Api-Key: stolen http://user:pass@proxy/?token=x";
    let result = ExecutionResult {
        request_id: "malformed".into(),
        candidate_id: None,
        status_code: 200,
        headers: BTreeMap::new(),
        body: Some(ResponseBody {
            json_body: Some(json!({"error":{"message":malicious}})),
            body_bytes_b64: None,
        }),
        telemetry: None,
        error: None,
    };

    // When
    let attempt = execution_result_to_attempt(result, QuotaKind::Balance, "deepseek");
    let AttemptResult::ParseFailure { class, .. } = attempt else {
        panic!("expected parse failure");
    };
    let persisted = build_persisted_snapshot(SnapshotUpdate {
        key: &key("key-1", "Secret", None),
        provider_type: "deepseek",
        attempt: &AttemptResult::ParseFailure {
            quota_kind: QuotaKind::Balance,
            class,
        },
        now_unix_secs: 100,
    })
    .expect("sanitized parse failure snapshot");
    let response = management_response(vec![OfficialQuotaItem {
        key_id: "key-1".into(),
        key_name: "Secret".into(),
        status: ItemStatus::Error,
        status_code: None,
        error_class: Some(class),
        message: Some(class.message().into()),
        quota_snapshot: Some(persisted.snapshot),
        refresh_state: persisted.refresh_state,
    }]);
    let serialized = response.to_string();

    // Then
    assert_eq!(class, StableErrorClass::ParseFailed);
    for secret in [malicious, "top-secret", "X-Api-Key", "user:pass", "token=x"] {
        assert!(!serialized.contains(secret));
    }
    assert_eq!(
        serde_json::from_value::<ProviderQuotaSnapshotContract>(
            response["results"][0]["quota_snapshot"].clone()
        )
        .expect("schema v1 snapshot")
        .kind,
        ProviderQuotaSnapshotKind::Balance
    );
}
