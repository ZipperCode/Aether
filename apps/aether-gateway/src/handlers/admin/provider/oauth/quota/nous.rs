use super::shared::{
    build_provider_quota_execution_plan, build_quota_snapshot_payload, execute_provider_quota_plan,
    persist_provider_quota_refresh_state, quota_refresh_success_invalid_state,
    resolve_provider_quota_execution_timeouts, ProviderQuotaExecutionOutcome,
};
use crate::handlers::admin::request::{AdminAppState, AdminGatewayProviderTransportSnapshot};
use crate::GatewayError;
use aether_contracts::{ExecutionResult, ProxySnapshot};
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use aether_provider_pool::{
    build_nous_account_quota_request, build_nous_billing_quota_request,
    ProviderPoolQuotaRequestSpec,
};
use base64::Engine as _;
use serde_json::{json, Map, Value};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BillingFailureKind {
    Http(u16),
    InvalidJson,
    TransportFailure,
    TransportError,
}

fn apply_billing_failure(
    mut snapshot: Value,
    previous: Option<&Value>,
    kind: BillingFailureKind,
) -> Value {
    snapshot["billing_stale"] = Value::Bool(true);
    snapshot["billing_source"] = Value::String("preserved_snapshot".into());
    snapshot["billing_error"] = Value::String(match kind {
        BillingFailureKind::Http(status) => format!("http_{status}"),
        BillingFailureKind::InvalidJson => "invalid_json".into(),
        BillingFailureKind::TransportFailure => "transport_failure".into(),
        BillingFailureKind::TransportError => "transport_error".into(),
    });
    if let Some(previous) = previous {
        if let Some(value) = previous.get("balance_usd") {
            snapshot["balance_usd"] = value.clone();
        }
        if let Some(window) = previous
            .get("windows")
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|v| v.get("code") == Some(&json!("monthly_spend")))
            })
        {
            snapshot["windows"]
                .as_array_mut()
                .expect("windows")
                .push(window.clone());
        }
    }
    snapshot
}

async fn coordinate_one_auth_retry<
    T,
    E,
    Auth,
    Execute,
    ExecuteFuture,
    Refresh,
    RefreshFuture,
    Status,
>(
    auth: Auth,
    mut execute: Execute,
    refresh: Refresh,
    status: Status,
) -> Result<T, E>
where
    Execute: FnMut(Auth) -> ExecuteFuture,
    ExecuteFuture: std::future::Future<Output = Result<T, E>>,
    Refresh: FnOnce() -> RefreshFuture,
    RefreshFuture: std::future::Future<Output = Result<Option<Auth>, E>>,
    Status: Fn(&T) -> u16,
{
    let first = execute(auth).await?;
    if status(&first) != 401 {
        return Ok(first);
    }
    match refresh().await? {
        Some(refreshed) => execute(refreshed).await,
        None => Ok(first),
    }
}

fn string_decimal(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(v) if valid_decimal(v.trim()) => Some(v.trim().to_string()),
        Value::Number(v) => Some(v.to_string()),
        _ => None,
    }
}
fn valid_decimal(value: &str) -> bool {
    let value = value.strip_prefix(['+', '-']).unwrap_or(value);
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    parts.next().is_none()
        && (!whole.is_empty() || fraction.is_some_and(|v| !v.is_empty()))
        && whole.chars().all(|c| c.is_ascii_digit())
        && fraction.is_none_or(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()))
}
fn bool_value(value: Option<&Value>) -> Option<bool> {
    value.and_then(Value::as_bool)
}
fn decimal_is_non_positive(value: &str) -> bool {
    let value = value.trim();
    let negative = value.starts_with('-');
    negative
        || value
            .trim_start_matches(['+', '-'])
            .chars()
            .all(|c| c == '0' || c == '.')
}
fn decimal_parts(value: &str) -> Option<(i128, u32)> {
    if !valid_decimal(value) {
        return None;
    }
    let negative = value.starts_with('-');
    let raw = value.trim_start_matches(['+', '-']);
    let (whole, fraction) = raw.split_once('.').unwrap_or((raw, ""));
    let scale = fraction.len() as u32;
    let mut number = format!("{}{}", if whole.is_empty() { "0" } else { whole }, fraction)
        .parse::<i128>()
        .ok()?;
    if negative {
        number = -number;
    }
    Some((number, scale))
}
fn decimal_subtract(left: &str, right: &str) -> Option<String> {
    let (a, sa) = decimal_parts(left)?;
    let (b, sb) = decimal_parts(right)?;
    let scale = sa.max(sb);
    let n = a
        .checked_mul(10i128.checked_pow(scale - sa)?)?
        .checked_sub(b.checked_mul(10i128.checked_pow(scale - sb)?)?)?;
    if scale == 0 {
        return Some(n.to_string());
    }
    let digits = n.abs().to_string();
    let padded = format!("{:0>width$}", digits, width = scale as usize + 1);
    let split = padded.len() - scale as usize;
    Some(format!(
        "{}{}.{}",
        if n < 0 { "-" } else { "" },
        &padded[..split],
        &padded[split..]
    ))
}
fn decimal_ratio(numerator: &str, denominator: &str) -> Option<String> {
    let (n, sn) = decimal_parts(numerator)?;
    let (d, sd) = decimal_parts(denominator)?;
    if d <= 0 {
        return None;
    }
    let precision = 8;
    let scaled = n.checked_mul(10i128.checked_pow(sd + precision)?)?
        / d.checked_mul(10i128.checked_pow(sn)?)?;
    let digits = scaled.max(0).to_string();
    let padded = format!("{:0>width$}", digits, width = precision as usize + 1);
    let split = padded.len() - precision as usize;
    Some(format!("{}.{}", &padded[..split], &padded[split..]))
}
fn field<'a>(root: &'a Value, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|n| root.get(*n))
}

fn configured_rate_limits_from_auth_config(auth_config: Option<&str>) -> Option<Value> {
    let auth: Value = serde_json::from_str(auth_config?).ok()?;
    let token = field(&auth, &["access_token", "accessToken"]).and_then(Value::as_str)?;
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .ok()?;
    let claims: Value = serde_json::from_slice(&decoded).ok()?;
    let limits = [
        ("rpm", "rate_limit_rpm"),
        ("tpm", "rate_limit_tpm"),
        ("rph", "rate_limit_rph"),
        ("tph", "rate_limit_tph"),
    ];
    let mut output = Map::from_iter([(
        "kind".to_string(),
        Value::String("configured_limits".to_string()),
    )]);
    for (target, claim) in limits {
        if let Some(value) = claims.get(claim).and_then(Value::as_u64) {
            output.insert(target.to_string(), Value::from(value));
        }
    }
    (output.len() > 1).then_some(Value::Object(output))
}

pub(super) fn build_nous_snapshot(
    account: &Value,
    billing: Option<&Value>,
    now: u64,
) -> Result<Value, String> {
    let subscription = field(account, &["subscription"]).unwrap_or(&Value::Null);
    let access =
        field(account, &["paid_service_access", "paidServiceAccess"]).unwrap_or(&Value::Null);
    let monthly = string_decimal(field(subscription, &["monthly_credits", "monthlyCredits"]));
    let remaining = string_decimal(field(
        subscription,
        &["credits_remaining", "creditsRemaining"],
    ));
    let purchased = string_decimal(field(
        access,
        &["purchased_credits_remaining", "purchasedCreditsRemaining"],
    ));
    let total = string_decimal(field(
        access,
        &["total_usable_credits", "totalUsableCredits"],
    ));
    if monthly.is_none() && remaining.is_none() && total.is_none() {
        return Err("Nous account response missing credit fields".into());
    }
    let allowed = bool_value(field(access, &["allowed"]));
    let reason = field(access, &["reason"])
        .and_then(Value::as_str)
        .unwrap_or_default();
    let exhausted = allowed == Some(false)
        || total.as_deref().is_some_and(decimal_is_non_positive)
        || reason.eq_ignore_ascii_case("no_usable_credits");
    let period_end = field(subscription, &["current_period_end", "currentPeriodEnd"]).cloned();
    let used = monthly
        .as_deref()
        .zip(remaining.as_deref())
        .and_then(|(l, r)| decimal_subtract(l, r));
    let ratio = remaining
        .as_deref()
        .zip(monthly.as_deref())
        .and_then(|(r, l)| decimal_ratio(r, l));
    let windows = vec![
        json!({"code":"subscription_credits", "scope":"account", "limit_value":monthly, "used_value":used, "remaining_value":remaining, "remaining_ratio":ratio, "reset_at":period_end, "is_exhausted":exhausted}),
    ];
    let mut snapshot = json!({"provider_type":"nous", "observed_at":now, "updated_at":now, "plan_type":field(subscription, &["plan"]).or_else(|| field(subscription, &["tier"])), "exhausted":exhausted, "exhausted_reason":if reason.is_empty(){Value::Null}else{Value::String(reason.to_string())}, "windows":windows, "purchased_credits_remaining":purchased, "total_usable_credits":total, "current_period_end":period_end, "billing_available":false});
    if let Some(billing) = billing {
        let cap = field(billing, &["monthlyCap", "monthly_cap"]).unwrap_or(&Value::Null);
        let limit = string_decimal(field(cap, &["limitUsd", "limit_usd"]));
        let used = string_decimal(field(cap, &["spentThisMonthUsd", "spent_this_month_usd"]));
        let billing_remaining = limit
            .as_deref()
            .zip(used.as_deref())
            .and_then(|(l, u)| decimal_subtract(l, u));
        let billing_ratio = billing_remaining
            .as_deref()
            .zip(limit.as_deref())
            .and_then(|(r, l)| decimal_ratio(r, l));
        snapshot["balance_usd"] = string_decimal(field(billing, &["balanceUsd", "balance_usd"]))
            .map(Value::String)
            .unwrap_or(Value::Null);
        snapshot["billing_available"] = Value::Bool(true);
        snapshot["billing_stale"] = Value::Bool(false);
        snapshot["billing_source"] = Value::String("billing_api".into());
        snapshot["windows"].as_array_mut().expect("windows array").push(
            json!({"code":"monthly_spend", "scope":"billing", "limit_value":limit, "used_value":used, "remaining_value":billing_remaining, "remaining_ratio":billing_ratio, "reset_at":Value::Null, "is_exhausted":false}));
    }
    Ok(snapshot)
}

async fn execute(
    state: &AdminAppState<'_>,
    transport: &AdminGatewayProviderTransportSnapshot,
    spec: ProviderPoolQuotaRequestSpec,
    proxy_override: Option<&ProxySnapshot>,
) -> Result<ProviderQuotaExecutionOutcome, GatewayError> {
    let proxy = match proxy_override {
        Some(v) => Some(v.clone()),
        None => {
            state
                .resolve_transport_proxy_snapshot_with_tunnel_affinity(transport)
                .await
        }
    };
    let timeouts = Some(resolve_provider_quota_execution_timeouts(
        state.resolve_transport_execution_timeouts(transport),
        proxy.as_ref(),
    ));
    let plan = build_provider_quota_execution_plan(
        transport,
        spec,
        proxy,
        state.resolve_transport_profile(transport),
        timeouts,
    );
    execute_provider_quota_plan(state, transport, plan, "nous").await
}
fn json_body(result: &ExecutionResult) -> Option<&Value> {
    result.body.as_ref()?.json_body.as_ref()
}
fn retry_after_reset_at(result: &ExecutionResult, now: u64) -> Option<u64> {
    let value = result
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("retry-after"))?
        .1
        .trim();
    value
        .parse::<u64>()
        .ok()
        .map(|seconds| now.saturating_add(seconds))
        .or_else(|| {
            httpdate::parse_http_date(value)
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|v| v.as_secs())
        })
}

fn temporary_rate_limit_snapshot(now: u64, reset_at: Option<u64>) -> Value {
    json!({"provider_type":"nous","observed_at":now,"updated_at":now,"exhausted":false,"code":"rate_limited","windows":[{"code":"rate_limit","scope":"account","limit_value":Value::Null,"used_value":Value::Null,"remaining_value":Value::Null,"remaining_ratio":Value::Null,"reset_at":reset_at,"reset_seconds":reset_at.map(|v|v.saturating_sub(now)),"is_exhausted":true}]})
}
fn preserve_previous_snapshot(mut snapshot: Value, previous: Option<&Value>) -> Value {
    let Some(previous) = previous.and_then(Value::as_object) else {
        return snapshot;
    };
    for name in [
        "plan_type",
        "total_usable_credits",
        "purchased_credits_remaining",
        "balance_usd",
        "billing_available",
        "billing_stale",
        "billing_source",
        "current_period_end",
        "rate_limits",
        "exhausted_reason",
    ] {
        if let Some(value) = previous.get(name) {
            snapshot[name] = value.clone();
        }
    }
    if let Some(windows) = previous.get("windows").and_then(Value::as_array) {
        for window in windows
            .iter()
            .filter(|v| v.get("code") != Some(&json!("rate_limit")))
        {
            snapshot["windows"]
                .as_array_mut()
                .expect("windows")
                .push(window.clone());
        }
    }
    snapshot
}

async fn execute_with_one_auth_retry<F>(
    state: &AdminAppState<'_>,
    transport: &AdminGatewayProviderTransportSnapshot,
    proxy: Option<&ProxySnapshot>,
    auth: (String, String),
    build: F,
) -> Result<ProviderQuotaExecutionOutcome, GatewayError>
where
    F: Fn((String, String)) -> ProviderPoolQuotaRequestSpec,
{
    coordinate_one_auth_retry(
        auth,
        |auth| execute(state, transport, build(auth), proxy),
        || async {
            state
                .force_local_oauth_refresh_entry(transport)
                .await
                .map(|entry| entry.map(|v| (v.auth_header_name, v.auth_header_value)))
                .map_err(|error| {
                    GatewayError::Internal(format!("Nous OAuth refresh failed: {error}"))
                })
        },
        |outcome| match outcome {
            ProviderQuotaExecutionOutcome::Response(result) => result.status_code,
            ProviderQuotaExecutionOutcome::Failure(_) => 0,
        },
    )
    .await
}
fn should_retry_unauthorized(status: u16, retries: u8) -> bool {
    status == 401 && retries == 0
}

pub(crate) async fn refresh_nous_provider_quota_locally(
    state: &AdminAppState<'_>,
    provider: &StoredProviderCatalogProvider,
    endpoint: &StoredProviderCatalogEndpoint,
    keys: Vec<StoredProviderCatalogKey>,
    proxy_override: Option<ProxySnapshot>,
) -> Result<Option<Value>, GatewayError> {
    let mut results = Vec::new();
    let mut success_count = 0usize;
    let mut failed_count = 0usize;
    for key in keys {
        let Some(transport) = state
            .read_provider_transport_snapshot(&provider.id, &endpoint.id, &key.id)
            .await?
        else {
            failed_count += 1;
            results.push(json!({"key_id":key.id,"status":"error","message":"Provider transport snapshot unavailable"}));
            continue;
        };
        let Some(auth) = state.resolve_local_oauth_header_auth(&transport).await? else {
            failed_count += 1;
            results.push(
                json!({"key_id":key.id,"status":"error","message":"缺少 Nous OAuth 认证信息"}),
            );
            continue;
        };
        let account = execute_with_one_auth_retry(
            state,
            &transport,
            proxy_override.as_ref(),
            auth.clone(),
            |auth| build_nous_account_quota_request(&key.id, auth),
        )
        .await?;
        let ProviderQuotaExecutionOutcome::Response(account) = account else {
            failed_count += 1;
            results.push(
                json!({"key_id":key.id,"status":"error","message":"Nous account 请求执行失败"}),
            );
            continue;
        };
        if account.status_code == 429 {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|v| v.as_secs())
                .unwrap_or(0);
            let snapshot = temporary_rate_limit_snapshot(now, retry_after_reset_at(&account, now));
            let snapshot = preserve_previous_snapshot(
                snapshot,
                key.status_snapshot.as_ref().and_then(|v| v.get("quota")),
            );
            let metadata = json!({"nous":snapshot});
            let (invalid_at, invalid_reason) = quota_refresh_success_invalid_state(&key);
            if persist_provider_quota_refresh_state(
                state,
                &key.id,
                Some(&metadata),
                invalid_at,
                invalid_reason,
                None,
            )
            .await?
            {
                success_count += 1;
                results.push(json!({"key_id":key.id,"status":"rate_limited","quota_snapshot":build_quota_snapshot_payload("nous",key.status_snapshot.as_ref(),Some(&metadata))}));
            } else {
                failed_count += 1;
            }
            continue;
        }
        if account.status_code != 200 {
            failed_count += 1;
            results.push(json!({"key_id":key.id,"status":"error","message":format!("Nous account 返回状态码 {}",account.status_code)}));
            continue;
        }
        let Some(account_json) = json_body(&account) else {
            failed_count += 1;
            results.push(
                json!({"key_id":key.id,"status":"error","message":"Nous account 响应不是 JSON"}),
            );
            continue;
        };
        let billing_auth = state
            .resolve_local_oauth_header_auth(&transport)
            .await?
            .unwrap_or(auth);
        let billing = execute_with_one_auth_retry(
            state,
            &transport,
            proxy_override.as_ref(),
            billing_auth,
            |auth| build_nous_billing_quota_request(&key.id, auth),
        )
        .await;
        let billing_json = match billing.as_ref().ok() {
            Some(ProviderQuotaExecutionOutcome::Response(v)) if v.status_code == 200 => {
                json_body(v)
            }
            _ => None,
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|v| v.as_secs())
            .unwrap_or(0);
        let snapshot = match build_nous_snapshot(account_json, billing_json, now) {
            Ok(v) => v,
            Err(message) => {
                failed_count += 1;
                results.push(json!({"key_id":key.id,"status":"error","message":message}));
                continue;
            }
        };
        let mut snapshot = snapshot;
        if billing_json.is_none() {
            let kind = match billing.as_ref().ok() {
                Some(ProviderQuotaExecutionOutcome::Response(v)) if v.status_code == 200 => {
                    BillingFailureKind::InvalidJson
                }
                Some(ProviderQuotaExecutionOutcome::Response(v)) => {
                    BillingFailureKind::Http(v.status_code)
                }
                Some(ProviderQuotaExecutionOutcome::Failure(_)) => {
                    BillingFailureKind::TransportFailure
                }
                None => BillingFailureKind::TransportError,
            };
            snapshot = apply_billing_failure(
                snapshot,
                key.status_snapshot.as_ref().and_then(|v| v.get("quota")),
                kind,
            );
        }
        if let Some(ProviderQuotaExecutionOutcome::Response(limited)) = billing.as_ref().ok().filter(|outcome| matches!(outcome, ProviderQuotaExecutionOutcome::Response(v) if v.status_code == 429)) {
            let reset_at = retry_after_reset_at(limited, now);
            let reset_seconds=reset_at.map(|at|at.saturating_sub(now));
            snapshot["windows"].as_array_mut().expect("windows").push(json!({"code":"rate_limit","scope":"account","limit_value":Value::Null,"used_value":Value::Null,"remaining_value":Value::Null,"remaining_ratio":Value::Null,"reset_at":reset_at,"reset_seconds":reset_seconds,"is_exhausted":true}));
        }
        if let Some(rate_limits) =
            configured_rate_limits_from_auth_config(transport.key.decrypted_auth_config.as_deref())
        {
            snapshot["rate_limits"] = rate_limits;
        }
        let metadata = json!({"nous":snapshot});
        let (invalid_at, invalid_reason) = quota_refresh_success_invalid_state(&key);
        if persist_provider_quota_refresh_state(
            state,
            &key.id,
            Some(&metadata),
            invalid_at,
            invalid_reason,
            None,
        )
        .await?
        {
            success_count += 1;
            results.push(json!({"key_id":key.id,"status":"success","quota_snapshot":build_quota_snapshot_payload("nous",key.status_snapshot.as_ref(),Some(&metadata))}));
        } else {
            failed_count += 1;
        }
    }
    Ok(Some(
        json!({"success":failed_count==0,"success_count":success_count,"failed_count":failed_count,"results":results}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builds_full_snapshot_and_exhaustion() {
        let a = json!({"subscription":{"plan":"free","monthly_credits":"10.00","credits_remaining":"0.00","current_period_end":123},"paid_service_access":{"allowed":false,"total_usable_credits":"0.00","purchased_credits_remaining":"0"}});
        let b = json!({"balanceUsd":"2.50","monthlyCap":{"limitUsd":"25.00","spentThisMonthUsd":"3.10"}});
        let s = build_nous_snapshot(&a, Some(&b), 1).unwrap();
        assert_eq!(s["exhausted"], true);
        assert_eq!(s["balance_usd"], "2.50");
        assert_eq!(s["windows"][1]["used_value"], "3.10");
    }
    #[test]
    fn billing_is_optional() {
        let a = json!({"subscription":{"monthlyCredits":"10","creditsRemaining":"5"},"paidServiceAccess":{"allowed":true,"totalUsableCredits":"5"}});
        let s = build_nous_snapshot(&a, None, 1).unwrap();
        assert_eq!(s["billing_available"], false);
    }

    #[test]
    fn jwt_rate_limits_are_display_only_configured_limits() {
        let claims = json!({
            "rate_limit_rpm": 50,
            "rate_limit_tpm": 500000,
            "rate_limit_rph": 2100,
            "rate_limit_tph": 6000000
        });
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).unwrap());
        let auth = json!({"access_token":format!("ignored.{payload}.unsigned")}).to_string();
        let limits = configured_rate_limits_from_auth_config(Some(&auth)).unwrap();
        assert_eq!(limits["kind"], "configured_limits");
        assert_eq!(limits["rpm"], 50);
        assert_eq!(limits["tph"], 6_000_000);
    }

    #[test]
    fn decimal_precision_is_preserved_as_text() {
        assert_eq!(
            string_decimal(Some(&json!("9007199254740993.00000001"))).as_deref(),
            Some("9007199254740993.00000001")
        );
        assert!(!decimal_is_non_positive("0.00000000000000000001"));
        assert!(decimal_is_non_positive("-0.00000000000000000001"));
        assert_eq!(
            decimal_subtract("9007199254740993.00000001", "0.00000001").as_deref(),
            Some("9007199254740993.00000000")
        );
        assert_eq!(decimal_ratio("5", "10").as_deref(), Some("0.50000000"));
    }
    #[test]
    fn unauthorized_retries_exactly_once() {
        assert!(should_retry_unauthorized(401, 0));
        assert!(!should_retry_unauthorized(401, 1));
        assert!(!should_retry_unauthorized(403, 0));
    }
    #[tokio::test]
    async fn retry_coordinator_executes_twice_refreshes_once_and_uses_refreshed_auth() {
        use std::sync::{Arc, Mutex};
        let executed = Arc::new(Mutex::new(Vec::new()));
        let refreshes = Arc::new(Mutex::new(0));
        let result = coordinate_one_auth_retry(
            "old".to_string(),
            {
                let executed = Arc::clone(&executed);
                move |auth| {
                    let executed = Arc::clone(&executed);
                    async move {
                        executed.lock().unwrap().push(auth.clone());
                        Ok::<_, ()>(if auth == "old" { 401 } else { 200 })
                    }
                }
            },
            {
                let refreshes = Arc::clone(&refreshes);
                move || async move {
                    *refreshes.lock().unwrap() += 1;
                    Ok::<_, ()>(Some("fresh".to_string()))
                }
            },
            |status| *status,
        )
        .await
        .unwrap();
        assert_eq!(result, 200);
        assert_eq!(*executed.lock().unwrap(), ["old", "fresh"]);
        assert_eq!(*refreshes.lock().unwrap(), 1);
    }

    #[test]
    fn billing_failure_reducer_preserves_prior_values_and_marks_stale() {
        let account = json!({"subscription":{"monthly_credits":"10","credits_remaining":"5"},"paid_service_access":{"allowed":true,"total_usable_credits":"5"}});
        let previous = json!({"balance_usd":"9.25","windows":[{"code":"monthly_spend","limit_value":"20","used_value":"4"}]});
        for (kind, expected) in [
            (BillingFailureKind::Http(403), "http_403"),
            (BillingFailureKind::Http(404), "http_404"),
            (BillingFailureKind::Http(503), "http_503"),
            (BillingFailureKind::InvalidJson, "invalid_json"),
            (BillingFailureKind::TransportFailure, "transport_failure"),
            (BillingFailureKind::TransportError, "transport_error"),
        ] {
            let snapshot = apply_billing_failure(
                build_nous_snapshot(&account, None, 1).unwrap(),
                Some(&previous),
                kind,
            );
            assert_eq!(snapshot["billing_stale"], true);
            assert_eq!(snapshot["billing_source"], "preserved_snapshot");
            assert_eq!(snapshot["billing_error"], expected);
            assert_eq!(snapshot["balance_usd"], "9.25");
            assert_eq!(snapshot["windows"][1]["code"], "monthly_spend");
        }
    }
    #[test]
    fn retry_after_supports_delta_and_http_date() {
        let make = |v: &str| ExecutionResult {
            request_id: "x".into(),
            candidate_id: None,
            status_code: 429,
            headers: std::collections::BTreeMap::from([("Retry-After".into(), v.into())]),
            body: None,
            telemetry: None,
            error: None,
        };
        assert_eq!(retry_after_reset_at(&make("60"), 100), Some(160));
        assert_eq!(
            retry_after_reset_at(&make("Thu, 01 Jan 1970 00:03:20 GMT"), 100),
            Some(200)
        );
    }
    #[test]
    fn temporary_rate_limit_does_not_exhaust_credits() {
        let s = temporary_rate_limit_snapshot(100, Some(160));
        assert_eq!(s["exhausted"], false);
        assert_eq!(s["windows"][0]["is_exhausted"], true);
        assert_eq!(s["windows"][0]["reset_seconds"], 60);
    }
    #[test]
    fn account_rate_limit_preserves_display_and_billing_state() {
        let previous = json!({"plan_type":"free","total_usable_credits":"7.5","billing_available":true,"billing_stale":false,"billing_source":"billing_api","current_period_end":999,"rate_limits":{"kind":"configured_limits","rpm":50},"windows":[{"code":"subscription_credits","scope":"account","limit_value":"10","used_value":"2.5","remaining_value":"7.5","remaining_ratio":"0.75","reset_at":999},{"code":"monthly_spend","scope":"billing","limit_value":"20","used_value":"3","remaining_value":"17","remaining_ratio":"0.85","reset_at":null}]});
        let snapshot = preserve_previous_snapshot(
            temporary_rate_limit_snapshot(100, Some(160)),
            Some(&previous),
        );
        assert_eq!(snapshot["billing_available"], true);
        assert_eq!(snapshot["current_period_end"], 999);
        assert_eq!(snapshot["rate_limits"]["rpm"], 50);
        assert_eq!(snapshot["windows"].as_array().unwrap().len(), 3);
    }
    #[test]
    fn catalog_normalizes_nous_array_snapshot() {
        let account = json!({"subscription":{"monthly_credits":"10","credits_remaining":"5"},"paid_service_access":{"allowed":true,"total_usable_credits":"5"}});
        let snapshot = build_nous_snapshot(&account, None, 123).unwrap();
        let status = crate::handlers::shared::sync_provider_key_quota_status_snapshot(
            None,
            "nous",
            Some(&json!({"nous":snapshot})),
            "test",
        )
        .expect("nous snapshot");
        assert_eq!(status["quota"]["provider_type"], "nous");
        assert!(status["quota"]["windows"].is_array());
    }
}
