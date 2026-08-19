use std::collections::BTreeMap;

use aether_data_contracts::repository::candidates::StoredRequestCandidate;
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;

use super::types::SchedulerMinimalCandidateSelectionCandidate;

pub fn auth_api_key_concurrency_limit_reached(
    recent_candidates: &[StoredRequestCandidate],
    now_unix_secs: u64,
    api_key_id: &str,
    concurrent_limit: usize,
) -> bool {
    if api_key_id.trim().is_empty() || concurrent_limit == 0 {
        return false;
    }

    crate::count_recent_active_requests_for_api_key(recent_candidates, api_key_id, now_unix_secs)
        >= concurrent_limit
}

#[derive(Clone, Copy, Debug)]
pub struct CandidateRuntimeSelectabilityInput<'a> {
    pub candidate: &'a SchedulerMinimalCandidateSelectionCandidate,
    pub recent_candidates: &'a [StoredRequestCandidate],
    pub provider_concurrent_limits: &'a BTreeMap<String, usize>,
    pub provider_key_rpm_states: &'a BTreeMap<String, StoredProviderCatalogKey>,
    pub now_unix_secs: u64,
    pub provider_quota_blocks_requests: bool,
    pub account_quota_exhausted: bool,
    /// 标准余额快照明确低于内部调度下限；该事实不受订阅额度开关控制。
    pub balance_below_minimum: bool,
    pub oauth_invalid: bool,
    pub enforce_key_circuit_breaker: bool,
    pub rpm_reset_at: Option<u64>,
}

pub fn candidate_is_selectable_with_runtime_state(
    input: CandidateRuntimeSelectabilityInput<'_>,
) -> bool {
    candidate_runtime_skip_reason_with_state(input).is_none()
}

/// 按统一优先级返回候选运行态阻断原因，余额不足与订阅耗尽保持独立诊断。
pub fn candidate_runtime_skip_reason_with_state(
    input: CandidateRuntimeSelectabilityInput<'_>,
) -> Option<&'static str> {
    let CandidateRuntimeSelectabilityInput {
        candidate,
        recent_candidates,
        provider_concurrent_limits,
        provider_key_rpm_states,
        now_unix_secs,
        provider_quota_blocks_requests,
        account_quota_exhausted,
        balance_below_minimum,
        oauth_invalid,
        enforce_key_circuit_breaker,
        rpm_reset_at,
    } = input;

    if provider_quota_blocks_requests {
        return Some("provider_quota_blocked");
    }
    if account_quota_exhausted {
        return Some("account_quota_exhausted");
    }
    // 余额事实独立于订阅 exhaustion，确保关闭旧开关时仍能保护低余额 Key。
    if balance_below_minimum {
        return Some("key_balance_below_minimum");
    }
    if oauth_invalid {
        return Some("oauth_invalid");
    }
    if provider_concurrent_limits
        .get(&candidate.provider_id)
        .is_some_and(|limit| {
            crate::count_recent_active_requests_for_provider(
                recent_candidates,
                candidate.provider_id.as_str(),
                now_unix_secs,
            ) >= *limit
        })
    {
        return Some("provider_concurrency_limit_reached");
    }

    let provider_key = provider_key_rpm_states.get(&candidate.key_id);
    if let Some(provider_key) = provider_key {
        if let Some(limit) = provider_key
            .concurrent_limit
            .filter(|limit| *limit > 0)
            .and_then(|limit| usize::try_from(limit).ok())
        {
            if crate::count_recent_active_requests_for_provider_key(
                recent_candidates,
                candidate.key_id.as_str(),
                now_unix_secs,
            ) >= limit
            {
                return Some("provider_key_concurrency_limit_reached");
            }
        }
    }

    if let Some(provider_key) = provider_key {
        if enforce_key_circuit_breaker
            && crate::is_provider_key_circuit_open_at(
                provider_key,
                candidate.endpoint_api_format.as_str(),
                now_unix_secs,
            )
        {
            return Some("key_circuit_open");
        }
        if crate::provider_key_health_score(provider_key, candidate.endpoint_api_format.as_str())
            .is_some_and(|score| score <= 0.0)
        {
            return Some("key_health_score_zero");
        }
        if !crate::provider_key_rpm_allows_request_since(
            provider_key,
            recent_candidates,
            now_unix_secs,
            false,
            rpm_reset_at,
        ) {
            return Some("key_rpm_exhausted");
        }
    }

    None
}
