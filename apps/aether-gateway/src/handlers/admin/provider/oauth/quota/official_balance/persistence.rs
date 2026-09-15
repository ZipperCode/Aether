use super::domain::{AttemptResult, PersistedSnapshot, QuotaKind, SourceAttempt, StableErrorClass};
use super::routing::retry_after_eligibility;
use crate::{handlers::admin::request::AdminAppState, GatewayError};
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
use aether_provider_pool::{
    official_balance_backoff_with_jitter_secs, provider_pool_key_account_quota_exhausted,
    provider_pool_key_balance_below_minimum, provider_quota_snapshot_exhausted,
    ProviderQuotaQueryStatus, ProviderQuotaRefreshState, ProviderQuotaSnapshotContract,
    ProviderQuotaSource, PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION,
};
use serde_json::{json, Value};

pub(super) struct SnapshotUpdate<'a> {
    pub(super) key: &'a StoredProviderCatalogKey,
    pub(super) provider_type: &'a str,
    pub(super) attempt: &'a AttemptResult,
    pub(super) now_unix_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum QuotaCacheInvalidationScope {
    CatalogOnly,
    CandidateRouting,
}

/// 持久化额度尝试，并仅在调度 eligibility 发生变化时失效候选缓存。
pub(super) async fn persist_attempt(
    state: &AdminAppState<'_>,
    update: SnapshotUpdate<'_>,
) -> Result<PersistedSnapshot, GatewayError> {
    let key_id = update.key.id.clone();
    let now_unix_secs = update.now_unix_secs;
    let persisted = build_persisted_snapshot(SnapshotUpdate {
        key: update.key,
        provider_type: update.provider_type,
        attempt: update.attempt,
        now_unix_secs: update.now_unix_secs,
    })
    .map_err(|class| GatewayError::Internal(class.persisted_error()))?;
    let invalidation_scope =
        quota_cache_invalidation_scope_for_snapshot(&update, &persisted.snapshot);
    let updated = state
        .mutate_provider_catalog_key_quota_snapshot(
            &key_id,
            &persisted.snapshot,
            Some(now_unix_secs),
        )
        .await?;
    if !updated {
        return Err(GatewayError::Internal(
            StableErrorClass::PersistenceFailed.persisted_error(),
        ));
    }
    if invalidation_scope == QuotaCacheInvalidationScope::CandidateRouting {
        state.app().invalidate_provider_quota_candidate_caches();
    }
    Ok(persisted)
}

/// 计算额度写入的缓存失效范围；测试与非持久化调用复用实际快照装饰结果。
pub(super) fn quota_cache_invalidation_scope(
    update: &SnapshotUpdate<'_>,
) -> QuotaCacheInvalidationScope {
    let Ok(persisted) = build_persisted_snapshot(SnapshotUpdate {
        key: update.key,
        provider_type: update.provider_type,
        attempt: update.attempt,
        now_unix_secs: update.now_unix_secs,
    }) else {
        return QuotaCacheInvalidationScope::CatalogOnly;
    };
    quota_cache_invalidation_scope_for_snapshot(update, &persisted.snapshot)
}

/// 同时比较余额和套餐调度事实，包含失败后变旧、独立来源恢复及混合资金来源变化。
fn quota_cache_invalidation_scope_for_snapshot(
    update: &SnapshotUpdate<'_>,
    next_snapshot: &Value,
) -> QuotaCacheInvalidationScope {
    let mut next_key = update.key.clone();
    let mut status = next_key
        .status_snapshot
        .take()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    status.insert("quota".into(), next_snapshot.clone());
    next_key.status_snapshot = Some(Value::Object(status));
    let balance_changed = provider_pool_key_balance_below_minimum(update.key, update.provider_type)
        != provider_pool_key_balance_below_minimum(&next_key, update.provider_type);
    let previous_exhausted =
        provider_pool_key_account_quota_exhausted(update.key, update.provider_type);
    let next_exhausted = provider_pool_key_account_quota_exhausted(&next_key, update.provider_type);
    if balance_changed || previous_exhausted != next_exhausted {
        QuotaCacheInvalidationScope::CandidateRouting
    } else {
        QuotaCacheInvalidationScope::CatalogOnly
    }
}

pub(super) fn build_persisted_snapshot(
    update: SnapshotUpdate<'_>,
) -> Result<PersistedSnapshot, StableErrorClass> {
    let (mut snapshot, code, freshness) = match update.attempt {
        AttemptResult::Sources {
            attempts,
            quota_kind,
        } => {
            let snapshot = merge_source_attempts(&update, attempts, *quota_kind)?;
            let successful = snapshot.sources.iter().any(|source| {
                source.query_status == ProviderQuotaQueryStatus::Ok && source.freshness == "fresh"
            });
            let failed = snapshot
                .sources
                .iter()
                .any(|source| source.refresh_state.error.is_some());
            let code =
                if successful {
                    if failed {
                        "partial"
                    } else {
                        "ok"
                    }
                } else if !failed {
                    if snapshot.sources.iter().any(|source| {
                        source.query_status == ProviderQuotaQueryStatus::NotApplicable
                    }) {
                        StableErrorClass::QueryNotApplicable.code()
                    } else {
                        StableErrorClass::QueryUnsupported.code()
                    }
                } else {
                    update
                        .attempt
                        .failure_class()
                        .unwrap_or(StableErrorClass::ParseFailed)
                        .code()
                };
            let freshness = if !failed {
                "fresh"
            } else if successful {
                "partial"
            } else if snapshot
                .sources
                .iter()
                .any(|source| source.freshness == "stale")
            {
                "stale"
            } else {
                "unknown"
            };
            (snapshot, code, freshness)
        }
        AttemptResult::Success { snapshot, .. } => {
            let mut snapshot = snapshot.clone();
            snapshot.refresh_state = success_refresh_state(update.now_unix_secs);
            (snapshot, "ok", "fresh")
        }
        _ => {
            let quota_kind = update
                .attempt
                .quota_kind()
                .ok_or(StableErrorClass::RequestInvalid)?;
            let mut retained = typed_snapshot(update.key)
                .filter(|snapshot| snapshot.kind == quota_kind.snapshot_kind())
                .unwrap_or_else(|| {
                    quota_kind.empty_snapshot(update.provider_type, update.now_unix_secs)
                });
            let class = update
                .attempt
                .failure_class()
                .unwrap_or(StableErrorClass::ParseFailed);
            retained.refresh_state = failure_state_for_attempt(
                &update.key.id,
                &latest_refresh_state(update.key),
                update.attempt,
                class,
                update.attempt.failure_message(),
                update.now_unix_secs,
            );
            // 旧的无来源快照仍可读取，但失败不能把其历史值重新标记为有效证据。
            for source in &mut retained.sources {
                source.query_status = update.attempt.query_status();
                source.freshness = if source.refresh_state.last_success_at.is_some() {
                    "stale"
                } else {
                    "unknown"
                }
                .into();
                source.refresh_state = failure_state_for_attempt(
                    &format!("{}:{}", update.key.id, source.id),
                    &source.refresh_state,
                    update.attempt,
                    class,
                    update.attempt.failure_message(),
                    update.now_unix_secs,
                );
            }
            if !retained.sources.is_empty() {
                retained.refresh_state.next_eligible_at = retained
                    .sources
                    .iter()
                    .filter_map(|source| source.refresh_state.next_eligible_at)
                    .max();
                retained.refresh_state.failure_count = retained
                    .sources
                    .iter()
                    .filter_map(|source| source.refresh_state.failure_count)
                    .max();
            }
            (retained, class.code(), "stale")
        }
    };
    snapshot.schema_version = PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION;
    snapshot.provider_type = update.provider_type.to_owned();
    snapshot.extensions.insert("code".into(), json!(code));
    snapshot
        .extensions
        .insert("freshness".into(), json!(freshness));
    snapshot
        .extensions
        .entry("observed_at")
        .or_insert_with(|| json!(update.now_unix_secs));
    snapshot
        .extensions
        .insert("updated_at".into(), json!(update.now_unix_secs));
    // 套餐查询失败已由来源状态表达，清除旧的错误码到全 Key 阻断投影。
    for field in [
        "token_plan_status",
        "token_plan_scheduling_blocked",
        "token_plan_error",
        "token_plan_scope",
        "scheduling_block_reason",
    ] {
        snapshot.extensions.remove(field);
    }
    let refresh_state = snapshot.refresh_state.clone();
    let mut snapshot =
        serde_json::to_value(snapshot).map_err(|_| StableErrorClass::PersistenceFailed)?;
    snapshot["exhausted"] = json!(provider_quota_snapshot_exhausted(
        &snapshot,
        update.now_unix_secs
    ));
    Ok(PersistedSnapshot {
        snapshot,
        refresh_state,
    })
}

/// 只把匹配来源的旧值保留为过期数据；不同产品或区域之间不迁移余额和套餐。
fn merge_source_attempts(
    update: &SnapshotUpdate<'_>,
    attempts: &[SourceAttempt],
    quota_kind: QuotaKind,
) -> Result<ProviderQuotaSnapshotContract, StableErrorClass> {
    let previous = typed_snapshot(update.key).filter(|snapshot| {
        snapshot
            .provider_type
            .eq_ignore_ascii_case(update.provider_type)
    });
    let mut snapshot = quota_kind.empty_snapshot(update.provider_type, update.now_unix_secs);
    for attempt in attempts {
        let parsed = match &attempt.result {
            AttemptResult::Success { snapshot, .. } => Some(snapshot),
            _ => None,
        };
        let mut sources = attempt.sources.clone();
        if let Some(parsed) = parsed {
            for source in &parsed.sources {
                if let Some(expected) = sources.iter_mut().find(|expected| expected.id == source.id)
                {
                    *expected = source.clone();
                } else {
                    sources.push(source.clone());
                }
            }
            for (name, value) in &parsed.extensions {
                snapshot
                    .extensions
                    .entry(name.clone())
                    .or_insert_with(|| value.clone());
            }
            if snapshot.rate_limit.is_none() {
                snapshot.rate_limit = parsed.rate_limit.clone();
            }
        }
        for mut source in sources {
            let current =
                parsed.and_then(|parsed| parsed.sources.iter().find(|item| item.id == source.id));
            let historical = previous.as_ref().and_then(|previous| {
                previous
                    .sources
                    .iter()
                    .find(|item| same_source(item, &source))
            });
            source.query_status = if parsed.is_some() {
                current
                    .map(|item| item.query_status)
                    .unwrap_or(ProviderQuotaQueryStatus::Error)
            } else {
                attempt.result.query_status()
            };
            let confirmed_absent = matches!(
                source.query_status,
                ProviderQuotaQueryStatus::NotApplicable | ProviderQuotaQueryStatus::Unsupported
            );
            if source.query_status == ProviderQuotaQueryStatus::Ok || confirmed_absent {
                // 明确不支持或不适用的来源不计失败；清除旧值，避免把历史钱包当成当前资金。
                source.freshness = "fresh".into();
                source.refresh_state = success_refresh_state(update.now_unix_secs);
                if let Some(parsed) = parsed.filter(|_| !confirmed_absent) {
                    append_source_values(&mut snapshot, parsed, &source.id);
                }
            } else {
                let previous_refresh = historical
                    .map(|source| source.refresh_state.clone())
                    .unwrap_or_default();
                let class = attempt
                    .result
                    .failure_class()
                    .unwrap_or_else(|| StableErrorClass::from_query_status(source.query_status));
                let detail = current
                    .and_then(|source| source.refresh_state.error.as_deref())
                    .or_else(|| attempt.result.failure_message());
                source.refresh_state = failure_state_for_attempt(
                    &format!("{}:{}", update.key.id, source.id),
                    &previous_refresh,
                    &attempt.result,
                    class,
                    detail,
                    update.now_unix_secs,
                );
                source.freshness = if historical
                    .is_some_and(|source| source.refresh_state.last_success_at.is_some())
                {
                    "stale"
                } else {
                    "unknown"
                }
                .into();
                if let (Some(previous), Some(historical)) = (previous.as_ref(), historical) {
                    source.plan_id = source.plan_id.or_else(|| historical.plan_id.clone());
                    source.plan_name = source.plan_name.or_else(|| historical.plan_name.clone());
                    source.plan_tier = source.plan_tier.or_else(|| historical.plan_tier.clone());
                    source.currency_source = source
                        .currency_source
                        .or_else(|| historical.currency_source.clone());
                    append_source_values(&mut snapshot, previous, &source.id);
                }
            }
            snapshot.sources.push(source);
        }
    }
    if snapshot.sources.is_empty() {
        return Err(StableErrorClass::RequestInvalid);
    }
    if snapshot
        .sources
        .iter()
        .filter(|source| {
            matches!(source.product.as_str(), "coding_plan" | "token_plan")
                && !matches!(
                    source.query_status,
                    ProviderQuotaQueryStatus::NotApplicable | ProviderQuotaQueryStatus::Unsupported
                )
        })
        .take(2)
        .count()
        > 1
    {
        // 多套餐并存时，单个来源的档位不能代表整个 Key；身份由各来源独立展示。
        for field in [
            "plan_type",
            "pool_tier",
            "membership_level",
            "subscription_type",
        ] {
            snapshot.extensions.remove(field);
        }
    }
    let successful = snapshot
        .sources
        .iter()
        .any(|source| source.query_status == ProviderQuotaQueryStatus::Ok);
    let failed_sources = snapshot
        .sources
        .iter()
        .filter(|source| source.refresh_state.error.is_some());
    let first_error = failed_sources.clone().find_map(|source| {
        source
            .refresh_state
            .error
            .as_ref()
            .map(|error| format!("{}: {error}", source.id))
    });
    // 部分成功也保留失败来源的退避期限，后台刷新不会立即重复打失败接口。
    snapshot.refresh_state = ProviderQuotaRefreshState {
        last_attempt_at: Some(update.now_unix_secs),
        last_success_at: if successful {
            Some(update.now_unix_secs)
        } else {
            previous
                .as_ref()
                .and_then(|snapshot| snapshot.refresh_state.last_success_at)
        },
        error: first_error,
        next_eligible_at: failed_sources
            .clone()
            .filter_map(|source| source.refresh_state.next_eligible_at)
            .max(),
        failure_count: Some(
            failed_sources
                .filter_map(|source| source.refresh_state.failure_count)
                .max()
                .unwrap_or(0),
        ),
    };
    Ok(snapshot)
}

fn same_source(previous: &ProviderQuotaSource, next: &ProviderQuotaSource) -> bool {
    previous.id == next.id
        && previous.product == next.product
        && previous.scope == next.scope
        && previous.region == next.region
        && next
            .plan_id
            .as_ref()
            .is_none_or(|plan_id| previous.plan_id.as_ref() == Some(plan_id))
}

fn append_source_values(
    target: &mut ProviderQuotaSnapshotContract,
    source: &ProviderQuotaSnapshotContract,
    source_id: &str,
) {
    target.balances.extend(
        source
            .balances
            .iter()
            .filter(|balance| balance.source_id.as_deref() == Some(source_id))
            .cloned(),
    );
    target.windows.extend(
        source
            .windows
            .iter()
            .filter(|window| window.source_id.as_deref() == Some(source_id))
            .cloned(),
    );
}

pub(super) fn latest_snapshot(key: &StoredProviderCatalogKey) -> Option<Value> {
    key.status_snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.get("quota"))
        .cloned()
        .filter(Value::is_object)
}

pub(super) fn latest_refresh_state(key: &StoredProviderCatalogKey) -> ProviderQuotaRefreshState {
    typed_snapshot(key)
        .map(|snapshot| snapshot.refresh_state)
        .unwrap_or_default()
}

pub(super) fn failure_refresh_state(
    key: &StoredProviderCatalogKey,
    class: StableErrorClass,
    now_unix_secs: u64,
) -> ProviderQuotaRefreshState {
    build_failure_refresh_state(
        &key.id,
        &latest_refresh_state(key),
        class,
        None,
        None,
        now_unix_secs,
    )
}

fn success_refresh_state(now_unix_secs: u64) -> ProviderQuotaRefreshState {
    ProviderQuotaRefreshState {
        last_attempt_at: Some(now_unix_secs),
        last_success_at: Some(now_unix_secs),
        error: None,
        next_eligible_at: None,
        failure_count: Some(0),
    }
}

fn failure_state_for_attempt(
    seed_id: &str,
    previous: &ProviderQuotaRefreshState,
    attempt: &AttemptResult,
    class: StableErrorClass,
    detail: Option<&str>,
    now_unix_secs: u64,
) -> ProviderQuotaRefreshState {
    let retry_at = match attempt {
        AttemptResult::HttpFailure { headers, .. } => {
            retry_after_eligibility(headers, now_unix_secs)
        }
        _ => None,
    };
    build_failure_refresh_state(seed_id, previous, class, detail, retry_at, now_unix_secs)
}

fn build_failure_refresh_state(
    seed_id: &str,
    previous: &ProviderQuotaRefreshState,
    class: StableErrorClass,
    detail: Option<&str>,
    retry_at: Option<u64>,
    now_unix_secs: u64,
) -> ProviderQuotaRefreshState {
    let failure_count = previous.failure_count.unwrap_or(0).saturating_add(1);
    let seed = seed_id
        .bytes()
        .fold(u64::from(failure_count), |value, byte| {
            value
                .wrapping_mul(1_099_511_628_211)
                .wrapping_add(u64::from(byte))
        });
    ProviderQuotaRefreshState {
        last_attempt_at: Some(now_unix_secs),
        last_success_at: previous.last_success_at,
        error: Some(match detail {
            Some(detail) => format!("{}: {detail}", class.code()),
            None => class.persisted_error(),
        }),
        next_eligible_at: Some(retry_at.unwrap_or_else(|| {
            now_unix_secs.saturating_add(official_balance_backoff_with_jitter_secs(
                failure_count,
                seed,
            ))
        })),
        failure_count: Some(failure_count),
    }
}

fn typed_snapshot(key: &StoredProviderCatalogKey) -> Option<ProviderQuotaSnapshotContract> {
    latest_snapshot(key)
        .and_then(|value| serde_json::from_value::<ProviderQuotaSnapshotContract>(value).ok())
}
