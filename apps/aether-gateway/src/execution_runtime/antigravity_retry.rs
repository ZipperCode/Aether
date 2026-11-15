use std::time::{Duration, Instant};

use aether_contracts::ExecutionPlan;
use serde_json::Value;
use tracing::debug;

use super::transport::{
    resolve_non_stream_total_timeout_for_request, resolve_stream_first_byte_timeout,
};
use crate::log_ids::short_request_id;
use crate::orchestration::{
    antigravity_tos_violation_response, attempt_identity_from_report_context,
    local_execution_candidate_metadata_from_report_context, next_same_key_retry_index,
};
use crate::AppState;

const DEFAULT_SHORT_RETRY_MAX_DELAY_MS: u64 = 2_000;
const MAX_SHORT_RETRY_MAX_DELAY_MS: u64 = 7_000;
const GOOGLE_RETRY_INFO_TYPE: &str = "type.googleapis.com/google.rpc.RetryInfo";

/// 短重试复用既有同 Key 尝试，但不能重新获得完整的超时预算。
pub(crate) async fn plan_with_remaining_budget(
    state: &AppState,
    plan: &ExecutionPlan,
    context: Option<&Value>,
    elapsed: Duration,
) -> Option<ExecutionPlan> {
    let mut adjusted = plan.clone();
    let Some(identity) = attempt_identity_from_report_context(context) else {
        return Some(adjusted);
    };
    if identity.candidate_index != 0
        || identity.retry_index == 0
        || identity.pool_key_index.is_some_and(|index| index != 0)
    {
        return Some(adjusted);
    }
    let transport = state
        .read_provider_transport_snapshot(&plan.provider_id, &plan.endpoint_id, &plan.key_id)
        .await
        .ok()
        .flatten();
    let enabled = transport.as_ref().is_some_and(|transport| {
        transport
            .provider
            .provider_type
            .eq_ignore_ascii_case("antigravity")
            && transport
                .provider
                .config
                .as_ref()
                .and_then(|config| config.pointer("/pool_advanced/antigravity/short_retry_enabled"))
                .and_then(Value::as_bool)
                == Some(true)
    });
    if !enabled {
        return Some(adjusted);
    }
    let timeout = if plan.stream {
        resolve_stream_first_byte_timeout(plan)
    } else {
        resolve_non_stream_total_timeout_for_request(
            plan.stream,
            &plan.provider_api_format,
            plan.timeouts.as_ref(),
        )
    };
    let Some(timeout) = timeout else {
        return Some(adjusted);
    };
    let remaining_ms = u64::try_from(timeout.checked_sub(elapsed)?.as_millis()).ok()?;
    if remaining_ms == 0 {
        return None;
    }
    let timeouts = adjusted.timeouts.get_or_insert_with(Default::default);
    if plan.stream {
        timeouts.first_byte_ms = Some(remaining_ms);
    } else {
        timeouts.total_ms = Some(remaining_ms);
    }
    Some(adjusted)
}
/// 只在首个客户端候选尚有原同 Key 预算时消费一次 RetryInfo 短等待。
/// 返回 true 后调用方必须走现有 Candidate retry，不得自行增加发送次数。
pub(crate) async fn wait_for_short_retry(
    state: &AppState,
    plan: &ExecutionPlan,
    report_context: Option<&Value>,
    status_code: u16,
    body: Option<&Value>,
    response_text: Option<&str>,
    attempt_started_at: Instant,
    output_committed: bool,
) -> bool {
    if output_committed || status_code != 429 {
        return false;
    }

    let Some(identity) = attempt_identity_from_report_context(report_context) else {
        return false;
    };
    // 只有最初尝试可进入短等待；派生重试及故障转移候选不能再次等待。
    if identity.retry_index != 0 {
        return false;
    }
    let metadata = local_execution_candidate_metadata_from_report_context(report_context);
    if next_same_key_retry_index(identity, metadata.sticky_key_attempts).is_none() {
        return false;
    }
    let Some(delay) = google_retry_info_delay(body) else {
        return false;
    };
    let Some(transport) = state
        .read_provider_transport_snapshot(&plan.provider_id, &plan.endpoint_id, &plan.key_id)
        .await
        .ok()
        .flatten()
    else {
        return false;
    };
    let provider_type = transport.provider.provider_type.as_str();
    if !provider_type.eq_ignore_ascii_case("antigravity")
        || antigravity_tos_violation_response(provider_type, status_code, response_text)
    {
        return false;
    }
    let Some(config) = transport
        .provider
        .config
        .as_ref()
        .and_then(|config| config.get("pool_advanced"))
        .and_then(|config| config.get("antigravity"))
        .and_then(Value::as_object)
    else {
        return false;
    };
    if !config
        .get("short_retry_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return false;
    }
    let policy =
        crate::orchestration::resolve_local_failover_policy(state, plan, report_context).await;
    let analysis = crate::orchestration::analyze_local_failover(
        &policy,
        crate::orchestration::LocalFailoverInput::new(status_code, response_text),
    );
    if policy.max_retries == Some(0)
        || matches!(
            analysis.decision,
            crate::orchestration::LocalFailoverDecision::StopLocalFailover
        )
        || analysis.quota_exhaustion.is_some()
        || analysis.antigravity_model_quota_exhaustion.is_some()
    {
        return false;
    }
    let max_delay_ms = config
        .get("short_retry_max_delay_ms")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_SHORT_RETRY_MAX_DELAY_MS);
    if !(1..=MAX_SHORT_RETRY_MAX_DELAY_MS).contains(&max_delay_ms)
        || delay > Duration::from_millis(max_delay_ms)
    {
        return false;
    }

    // 等待与首次发送共用当前尝试的绝对期限；不足以再发送时保持原冷却路径。
    let timeout = if plan.stream {
        resolve_stream_first_byte_timeout(plan)
    } else {
        resolve_non_stream_total_timeout_for_request(
            plan.stream,
            &plan.provider_api_format,
            plan.timeouts.as_ref(),
        )
    };
    let Some(remaining) =
        timeout.and_then(|timeout| timeout.checked_sub(attempt_started_at.elapsed()))
    else {
        return false;
    };
    if delay >= remaining {
        return false;
    }
    tokio::time::sleep(delay).await;
    if timeout.is_some_and(|timeout| attempt_started_at.elapsed() >= timeout) {
        return false;
    }

    debug!(
        event_name = "antigravity_short_retry_delay_elapsed",
        log_type = "event",
        request_id = %short_request_id(plan.request_id.as_str()),
        retry_delay_ms = delay.as_millis(),
        "gateway consumed bounded Antigravity RetryInfo delay before same-key retry"
    );
    true
}

/// 仅接受 google.rpc.RetryInfo 的 protobuf Duration JSON；消息、Retry-After
/// 和其他 quota 字段均不得触发短等待。
fn google_retry_info_delay(body: Option<&Value>) -> Option<Duration> {
    let details = body?
        .get("error")?
        .as_object()?
        .get("details")?
        .as_array()?;
    let mut retry_info = details.iter().filter(|detail| {
        detail.get("@type").and_then(Value::as_str) == Some(GOOGLE_RETRY_INFO_TYPE)
    });
    let detail = retry_info.next()?;
    // 重复或冲突的 RetryInfo 不做猜测，沿用现有冷却与故障转移。
    if retry_info.next().is_some() {
        return None;
    }
    parse_protobuf_duration(detail.get("retryDelay")?.as_str()?)
}

fn parse_protobuf_duration(raw: &str) -> Option<Duration> {
    let raw = raw.trim();
    let value = raw.strip_suffix('s')?;
    let mut parts = value.split('.');
    let seconds = parts.next()?;
    if seconds.is_empty() || !seconds.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let seconds = seconds.parse::<u64>().ok()?;
    let nanos = match parts.next() {
        Some(fraction)
            if !fraction.is_empty()
                && fraction.len() <= 9
                && fraction.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            let mut nanos = fraction.parse::<u32>().ok()?;
            for _ in fraction.len()..9 {
                nanos = nanos.checked_mul(10)?;
            }
            nanos
        }
        Some(_) => return None,
        None => 0,
    };
    if parts.next().is_some() {
        return None;
    }
    Some(Duration::new(seconds, nanos))
}

#[cfg(test)]
#[path = "antigravity_retry_http_tests.rs"]
mod http_tests;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{google_retry_info_delay, parse_protobuf_duration};

    #[test]
    fn retry_info_duration_is_explicit_and_bounded_by_proto_shape() {
        for (raw, expected_nanos) in [
            ("0s", 0),
            ("2s", 2_000_000_000),
            ("0.001s", 1_000_000),
            ("1.000000001s", 1_000_000_001),
        ] {
            let duration = parse_protobuf_duration(raw).expect("valid protobuf duration");
            assert_eq!(duration.as_nanos(), expected_nanos);
        }
        for raw in [
            "",
            "1",
            "1ms",
            "-1s",
            "+1s",
            ".5s",
            "1.s",
            "1.0000000000s",
            "1.2.3s",
            "NaNs",
            "18446744073709551616s",
        ] {
            assert_eq!(parse_protobuf_duration(raw), None, "malformed {raw}");
        }
    }

    #[test]
    fn retry_info_ignores_unstructured_or_conflicting_delay_hints() {
        let retry_info = json!({
            "@type": "type.googleapis.com/google.rpc.RetryInfo",
            "retryDelay": "0.025s"
        });
        let body = json!({"error": {"code": 429, "details": [
            {"@type": "type.googleapis.com/google.rpc.ErrorInfo", "reason": "RATE_LIMIT_EXCEEDED"},
            retry_info,
        ]}});
        assert_eq!(
            google_retry_info_delay(Some(&body)),
            Some(std::time::Duration::from_millis(25))
        );
        assert_eq!(
            google_retry_info_delay(Some(&json!({"error": {"message": "reset after 1s"}}))),
            None
        );
        assert_eq!(
            google_retry_info_delay(Some(&json!({"error": {"details": [
                {"@type": "type.googleapis.com/google.rpc.RetryInfo", "retryDelay": 0.025}
            ]}}))),
            None
        );
        assert_eq!(
            google_retry_info_delay(Some(&json!({"error": {"details": [
                {"@type": "type.googleapis.com/google.rpc.RetryInfo", "retryDelay": "0.025s"},
                {"@type": "type.googleapis.com/google.rpc.RetryInfo", "retryDelay": "0.010s"}
            ]}}))),
            None
        );
    }
}
