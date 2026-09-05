use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogAuthMaintenanceCandidate, StoredProviderCatalogEndpoint,
    StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use futures_util::{stream, StreamExt};
use serde_json::Value;
use tracing::{info, warn};

use crate::admin_api::provider_oauth_maintenance_endpoint_for_provider;
use crate::provider_key_auth::provider_key_is_oauth_managed;
use crate::{AppState, GatewayError};

use super::{shared_auth_maintenance_gate, system_config_bool, AuthMaintenanceGate};

const OAUTH_TOKEN_REFRESH_LOOKAHEAD_SECS: u64 = 120;
const OAUTH_REFRESH_FAILED_PREFIX: &str = "[REFRESH_FAILED] ";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub(crate) struct OAuthTokenRefreshRunSummary {
    /// 本轮检查过的轻量候选数，不包含完整凭据载荷。
    pub(crate) scanned: usize,
    /// 轻量筛选时符合 OAuth 刷新条件的候选数。
    pub(crate) eligible: usize,
    /// 刷新后实际发生凭据或过期时间变化的 Key 数。
    pub(crate) refreshed: usize,
    /// 成功解析出可用 OAuth 授权的 Key 数。
    pub(crate) resolved: usize,
    /// 因资格变化、缺少 Endpoint 或无刷新结果而跳过的 Key 数。
    pub(crate) skipped: usize,
    /// 单 Key 强读取或刷新失败且已隔离的 Key 数。
    pub(crate) failed: usize,
}

impl OAuthTokenRefreshRunSummary {
    /// 合并一个候选或 Provider 的独立结果，计数均使用饱和加法避免异常规模溢出。
    fn merge(&mut self, other: Self) {
        self.scanned = self.scanned.saturating_add(other.scanned);
        self.eligible = self.eligible.saturating_add(other.eligible);
        self.refreshed = self.refreshed.saturating_add(other.refreshed);
        self.resolved = self.resolved.saturating_add(other.resolved);
        self.skipped = self.skipped.saturating_add(other.skipped);
        self.failed = self.failed.saturating_add(other.failed);
    }
}

/// 使用进程级共享认证维护闸门执行一轮 OAuth 自动刷新。
pub(crate) async fn perform_oauth_token_refresh_once(
    state: &AppState,
) -> Result<OAuthTokenRefreshRunSummary, GatewayError> {
    perform_oauth_token_refresh_once_with_gate(state, shared_auth_maintenance_gate()).await
}

/// 以指定共享闸门执行一轮 OAuth 刷新；测试可注入隔离闸门验证并发边界。
async fn perform_oauth_token_refresh_once_with_gate(
    state: &AppState,
    gate: AuthMaintenanceGate,
) -> Result<OAuthTokenRefreshRunSummary, GatewayError> {
    if !state.has_provider_catalog_data_reader() || !state.has_provider_catalog_data_writer() {
        return Ok(OAuthTokenRefreshRunSummary::default());
    }
    if !system_config_bool(&state.data, "enable_oauth_token_refresh", true)
        .await
        .map_err(|err| GatewayError::Internal(err.to_string()))?
    {
        return Ok(OAuthTokenRefreshRunSummary::default());
    }

    let providers = state.list_provider_catalog_providers(true).await?;
    let provider_ids = providers
        .iter()
        .map(|provider| provider.id.clone())
        .collect::<Vec<_>>();
    if provider_ids.is_empty() {
        return Ok(OAuthTokenRefreshRunSummary::default());
    }

    let endpoints = state
        .list_provider_catalog_endpoints_by_provider_ids(&provider_ids)
        .await?;
    let candidates = state
        .list_provider_catalog_auth_maintenance_candidates_by_provider_ids(&provider_ids)
        .await?;
    let endpoints_by_provider = group_endpoints_by_provider(endpoints);
    let mut candidates_by_provider = group_candidates_by_provider(candidates);
    let mut summary = OAuthTokenRefreshRunSummary::default();
    let refresh_cutoff_unix_secs =
        now_unix_secs().saturating_add(OAUTH_TOKEN_REFRESH_LOOKAHEAD_SECS);

    for provider in providers {
        let provider_candidates = candidates_by_provider
            .remove(provider.id.as_str())
            .unwrap_or_default();
        let provider_endpoints = endpoints_by_provider
            .get(provider.id.as_str())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut eligible_candidates = Vec::new();
        for candidate in provider_candidates {
            summary.scanned = summary.scanned.saturating_add(1);
            if !oauth_refresh_maintenance_candidate(&provider, &candidate, refresh_cutoff_unix_secs)
            {
                summary.skipped = summary.skipped.saturating_add(1);
                continue;
            }
            summary.eligible = summary.eligible.saturating_add(1);
            eligible_candidates.push(candidate);
        }
        if eligible_candidates.is_empty() {
            continue;
        }

        let Some(endpoint) = provider_oauth_maintenance_endpoint_for_provider(
            &provider.provider_type,
            provider_endpoints,
        ) else {
            summary.skipped = summary.skipped.saturating_add(eligible_candidates.len());
            continue;
        };

        let provider_ref = &provider;
        let endpoint_ref = &endpoint;
        let provider_summary = stream::iter(eligible_candidates.into_iter().map(|candidate| {
            let gate = gate.clone();
            async move {
                let key_id = candidate.id.clone();
                match refresh_oauth_candidate_under_gate(
                    state,
                    provider_ref,
                    endpoint_ref,
                    candidate,
                    refresh_cutoff_unix_secs,
                    gate,
                )
                .await
                {
                    Ok(result) => result,
                    Err(err) => {
                        warn!(
                            event_name = "oauth_token_refresh_failed",
                            log_type = "ops",
                            worker = "oauth_token_refresh",
                            provider_id = %provider_ref.id,
                            key_id,
                            error = ?err,
                            "gateway oauth token auto refresh failed"
                        );
                        OAuthTokenRefreshRunSummary {
                            failed: 1,
                            ..OAuthTokenRefreshRunSummary::default()
                        }
                    }
                }
            }
        }))
        .buffer_unordered(gate.concurrency())
        .fold(
            OAuthTokenRefreshRunSummary::default(),
            |mut accumulated, result| async move {
                accumulated.merge(result);
                accumulated
            },
        )
        .await;
        summary.merge(provider_summary);
    }

    if summary.eligible > 0 || summary.refreshed > 0 || summary.failed > 0 {
        info!(
            event_name = "oauth_token_refresh_completed",
            log_type = "ops",
            worker = "oauth_token_refresh",
            scanned = summary.scanned,
            eligible = summary.eligible,
            refreshed = summary.refreshed,
            resolved = summary.resolved,
            skipped = summary.skipped,
            failed = summary.failed,
            "gateway completed oauth token auto refresh scan"
        );
    }

    Ok(summary)
}

/// 在共享许可内强读取并复核单个 Key，然后完成一次 OAuth 刷新及凭据变化检测。
async fn refresh_oauth_candidate_under_gate(
    state: &AppState,
    provider: &StoredProviderCatalogProvider,
    endpoint: &StoredProviderCatalogEndpoint,
    candidate: StoredProviderCatalogAuthMaintenanceCandidate,
    refresh_cutoff_unix_secs: u64,
    gate: AuthMaintenanceGate,
) -> Result<OAuthTokenRefreshRunSummary, GatewayError> {
    let _permit = gate.acquire().await?;
    let Some(key) = state
        .list_provider_catalog_keys_by_ids_strong(std::slice::from_ref(&candidate.id))
        .await?
        .into_iter()
        .find(|key| key.id == candidate.id && key.provider_id == provider.id)
    else {
        return Ok(OAuthTokenRefreshRunSummary {
            skipped: 1,
            ..OAuthTokenRefreshRunSummary::default()
        });
    };
    if !oauth_refresh_candidate(provider, &key, refresh_cutoff_unix_secs) {
        return Ok(OAuthTokenRefreshRunSummary {
            skipped: 1,
            ..OAuthTokenRefreshRunSummary::default()
        });
    }

    let before = ProviderKeyCredentialVersion::from(&key);
    drop(key);
    let Some(transport) = state
        .read_provider_transport_snapshot_uncached(&provider.id, &endpoint.id, &candidate.id)
        .await?
    else {
        return Ok(OAuthTokenRefreshRunSummary {
            skipped: 1,
            ..OAuthTokenRefreshRunSummary::default()
        });
    };
    let is_agent_identity =
        crate::provider_transport::is_codex_agent_identity_transport(&transport);
    let needs_agent_task_recovery = is_agent_identity
        && agent_identity_needs_task_recovery(
            transport.key.decrypted_auth_config.as_deref(),
            before.oauth_invalid_reason.as_deref(),
        );
    if !needs_agent_task_recovery
        && !auth_config_has_refresh_token(transport.key.decrypted_auth_config.as_deref())
    {
        return Ok(OAuthTokenRefreshRunSummary {
            skipped: 1,
            ..OAuthTokenRefreshRunSummary::default()
        });
    }

    let refresh_result = if needs_agent_task_recovery {
        state
            .force_local_oauth_refresh_entry(&transport)
            .await
            .map(|entry| entry.map(|_| ()))
            .map_err(|err| GatewayError::Internal(err.to_string()))
    } else {
        state
            .resolve_local_oauth_request_auth(&transport)
            .await
            .map(|auth| auth.map(|_| ()))
    }?;
    let Some(()) = refresh_result else {
        return Ok(OAuthTokenRefreshRunSummary {
            skipped: 1,
            ..OAuthTokenRefreshRunSummary::default()
        });
    };
    // 刷新后的强读取开始前释放解密 transport，保证一个 permit 不同时保留两个完整对象。
    drop(transport);

    Ok(OAuthTokenRefreshRunSummary {
        refreshed: usize::from(provider_key_credentials_changed(state, &before).await?),
        resolved: 1,
        ..OAuthTokenRefreshRunSummary::default()
    })
}

/// 按 Provider 分组轻量 Endpoint，避免为每个 Key 重复查询目录。
fn group_endpoints_by_provider(
    endpoints: Vec<StoredProviderCatalogEndpoint>,
) -> BTreeMap<String, Vec<StoredProviderCatalogEndpoint>> {
    let mut grouped = BTreeMap::new();
    for endpoint in endpoints {
        grouped
            .entry(endpoint.provider_id.clone())
            .or_insert_with(Vec::new)
            .push(endpoint);
    }
    grouped
}

/// 按 Provider 分组轻量认证维护候选；候选不携带密文或大型状态 JSON。
fn group_candidates_by_provider(
    candidates: Vec<StoredProviderCatalogAuthMaintenanceCandidate>,
) -> BTreeMap<String, Vec<StoredProviderCatalogAuthMaintenanceCandidate>> {
    let mut grouped = BTreeMap::new();
    for candidate in candidates {
        grouped
            .entry(candidate.provider_id.clone())
            .or_insert_with(Vec::new)
            .push(candidate);
    }
    grouped
}

/// 仅用轻量投影判断候选是否值得等待执行许可，不读取任何认证密文。
fn oauth_refresh_maintenance_candidate(
    provider: &StoredProviderCatalogProvider,
    candidate: &StoredProviderCatalogAuthMaintenanceCandidate,
    refresh_cutoff_unix_secs: u64,
) -> bool {
    let regular_oauth_candidate = candidate.oauth_invalid_at_unix_secs.is_none()
        && candidate
            .expires_at_unix_secs
            .is_some_and(|expires_at| expires_at <= refresh_cutoff_unix_secs);
    let possible_agent_candidate = provider.provider_type.trim().eq_ignore_ascii_case("codex")
        && candidate.auth_type.trim().eq_ignore_ascii_case("oauth")
        && (candidate.expires_at_unix_secs.is_none()
            || candidate
                .oauth_invalid_reason
                .as_deref()
                .is_some_and(|reason| reason.contains(OAUTH_REFRESH_FAILED_PREFIX)));
    candidate.is_active
        && candidate.has_auth_config
        && (regular_oauth_candidate || possible_agent_candidate)
        && maintenance_candidate_is_oauth_managed(candidate, &provider.provider_type)
}

/// 复现完整 Key 的 OAuth 管理分类中仅依赖轻量字段的部分，供许可前筛选使用。
fn maintenance_candidate_is_oauth_managed(
    candidate: &StoredProviderCatalogAuthMaintenanceCandidate,
    provider_type: &str,
) -> bool {
    let auth_type = candidate.auth_type.trim();
    auth_type.eq_ignore_ascii_case("oauth")
        || (provider_type.trim().eq_ignore_ascii_case("kiro")
            && auth_type.eq_ignore_ascii_case("bearer")
            && candidate.has_auth_config)
        || (provider_type.trim().eq_ignore_ascii_case("grok") && candidate.has_auth_config)
}

/// 在取得许可并强读取完整 Key 后重复资格判断，防止轻量扫描后的并发修改被执行。
fn oauth_refresh_candidate(
    provider: &StoredProviderCatalogProvider,
    key: &StoredProviderCatalogKey,
    refresh_cutoff_unix_secs: u64,
) -> bool {
    let has_auth_config = key
        .encrypted_auth_config
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    let regular_oauth_candidate = key.oauth_invalid_at_unix_secs.is_none()
        && key
            .expires_at_unix_secs
            .is_some_and(|expires_at| expires_at <= refresh_cutoff_unix_secs);
    // The catalog row is encrypted here, so exact Agent Identity validation is
    // deferred until the transport snapshot has decrypted auth_config.
    let possible_agent_candidate = provider.provider_type.trim().eq_ignore_ascii_case("codex")
        && key.auth_type.trim().eq_ignore_ascii_case("oauth")
        && (key.expires_at_unix_secs.is_none()
            || key
                .oauth_invalid_reason
                .as_deref()
                .is_some_and(|reason| reason.contains(OAUTH_REFRESH_FAILED_PREFIX)));
    key.is_active
        && has_auth_config
        && (regular_oauth_candidate || possible_agent_candidate)
        && provider_key_is_oauth_managed(key, provider.provider_type.as_str())
}

/// 刷新前需要保留的最小凭据版本，避免执行期间同时持有完整目录 Key 与 transport。
struct ProviderKeyCredentialVersion {
    /// Key 标识，用于刷新完成后执行单 Key 强读取。
    id: String,
    /// 刷新前的加密 API Key；仅在进程内用于变化比较，不写日志。
    encrypted_api_key: Option<String>,
    /// 刷新前的加密 OAuth 配置；仅在进程内用于变化比较，不写日志。
    encrypted_auth_config: Option<String>,
    /// 刷新前的过期时间，Unix 秒。
    expires_at_unix_secs: Option<u64>,
    /// 强读取时的 OAuth 失效原因，用于 Agent Identity 恢复判断。
    oauth_invalid_reason: Option<String>,
}

impl From<&StoredProviderCatalogKey> for ProviderKeyCredentialVersion {
    /// 从完整 Key 提取刷新比较所需的最小字段，并让完整对象在上游请求前释放。
    fn from(key: &StoredProviderCatalogKey) -> Self {
        Self {
            id: key.id.clone(),
            encrypted_api_key: key.encrypted_api_key.clone(),
            encrypted_auth_config: key.encrypted_auth_config.clone(),
            expires_at_unix_secs: key.expires_at_unix_secs,
            oauth_invalid_reason: key.oauth_invalid_reason.clone(),
        }
    }
}

/// 判断 agent identity 是否缺少可恢复任务，供后台刷新重新建立授权流程。
fn agent_identity_needs_task_recovery(
    auth_config: Option<&str>,
    oauth_invalid_reason: Option<&str>,
) -> bool {
    if oauth_invalid_reason.is_some_and(|reason| reason.contains(OAUTH_REFRESH_FAILED_PREFIX)) {
        return true;
    }
    auth_config
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
        .is_some_and(|config| {
            crate::provider_transport::is_codex_agent_identity_auth_config_value(&config)
                && !crate::provider_transport::codex_agent_identity_auth_config_has_task_id(&config)
        })
}

/// 比较刷新前后加密凭据与过期时间，判断是否需要持久化并失效相关缓存。
async fn provider_key_credentials_changed(
    state: &AppState,
    before: &ProviderKeyCredentialVersion,
) -> Result<bool, GatewayError> {
    let Some(after) = state
        .list_provider_catalog_keys_by_ids_strong(std::slice::from_ref(&before.id))
        .await?
        .into_iter()
        .next()
    else {
        return Ok(false);
    };
    Ok(after.encrypted_api_key != before.encrypted_api_key
        || after.encrypted_auth_config != before.encrypted_auth_config
        || after.expires_at_unix_secs != before.expires_at_unix_secs)
}

/// 判断鉴权配置是否包含可用刷新令牌，同时接受标准 snake_case 与旧版 camelCase 字段。
fn auth_config_has_refresh_token(auth_config: Option<&str>) -> bool {
    let Some(auth_config) = auth_config.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(auth_config) else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    ["refresh_token", "refreshToken"].iter().any(|field| {
        object
            .get(*field)
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
    })
}

/// 返回当前 Unix 秒；系统时钟早于 epoch 时以零值继续本轮维护。
fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use aether_data_contracts::repository::provider_catalog::{
        StoredProviderCatalogAuthMaintenanceCandidate, StoredProviderCatalogKey,
        StoredProviderCatalogProvider,
    };

    use super::{
        agent_identity_needs_task_recovery, auth_config_has_refresh_token, oauth_refresh_candidate,
        oauth_refresh_maintenance_candidate,
    };

    /// 验证旧版 Antigravity `refreshToken` 仍可进入后台刷新候选。
    #[test]
    fn legacy_antigravity_refresh_token_is_refreshable() {
        assert!(auth_config_has_refresh_token(Some(
            r#"{"refreshToken":"legacy-refresh-token"}"#,
        )));
    }

    /// 验证即将过期且持有加密鉴权配置的 Antigravity OAuth Key 会被选中刷新。
    #[test]
    fn expiring_antigravity_oauth_key_is_refresh_candidate() {
        let provider = StoredProviderCatalogProvider::new(
            "provider-antigravity".to_string(),
            "Antigravity".to_string(),
            None,
            "antigravity".to_string(),
        )
        .expect("provider should build");
        let mut key = StoredProviderCatalogKey::new(
            "key-antigravity".to_string(),
            provider.id.clone(),
            "Antigravity OAuth".to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("key should build");
        key.encrypted_auth_config = Some("encrypted-auth-config".to_string());
        key.expires_at_unix_secs = Some(120);

        assert!(oauth_refresh_candidate(&provider, &key, 120));
        assert!(oauth_refresh_maintenance_candidate(
            &provider,
            &StoredProviderCatalogAuthMaintenanceCandidate::from(&key),
            120,
        ));
    }

    /// 验证轻量候选对 OAuth、旧版 Kiro 与 Grok 会话的分类和完整 Key 复核一致。
    #[test]
    fn lightweight_oauth_classification_matches_full_key_revalidation() {
        for (provider_type, auth_type, expected) in [
            ("antigravity", "oauth", true),
            ("kiro", "bearer", true),
            ("grok", "api_key", true),
            ("custom", "api_key", false),
        ] {
            let provider = StoredProviderCatalogProvider::new(
                format!("provider-{provider_type}"),
                provider_type.to_string(),
                None,
                provider_type.to_string(),
            )
            .expect("provider should build");
            let mut key = StoredProviderCatalogKey::new(
                format!("key-{provider_type}"),
                provider.id.clone(),
                provider_type.to_string(),
                auth_type.to_string(),
                None,
                true,
            )
            .expect("key should build");
            key.encrypted_auth_config = Some("encrypted-auth-config".to_string());
            key.expires_at_unix_secs = Some(120);
            let candidate = StoredProviderCatalogAuthMaintenanceCandidate::from(&key);

            assert_eq!(
                oauth_refresh_maintenance_candidate(&provider, &candidate, 120),
                expected,
                "lightweight classification should match {provider_type}",
            );
            assert_eq!(
                oauth_refresh_candidate(&provider, &key, 120),
                expected,
                "full-key revalidation should match {provider_type}",
            );
        }
    }

    #[test]
    fn pending_agent_identity_without_task_is_recoverable() {
        let config = serde_json::json!({
            "auth_mode": "agentIdentity",
            "agent_runtime_id": "runtime-1",
            "agent_private_key": "private-key-present",
        });
        assert!(agent_identity_needs_task_recovery(
            Some(&config.to_string()),
            None,
        ));
    }

    #[test]
    fn refresh_failure_marker_forces_agent_task_recovery() {
        assert!(agent_identity_needs_task_recovery(
            Some("{}"),
            Some("[REFRESH_FAILED] temporary"),
        ));
    }
}
