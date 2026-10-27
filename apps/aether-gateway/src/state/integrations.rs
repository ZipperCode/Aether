use std::time::Duration;

use aether_contracts::{ExecutionPlan, ExecutionResult, ProxySnapshot};
use aether_data_contracts::repository::candidates::{
    StoredRequestCandidate, UpsertRequestCandidateRecord,
};
use aether_data_contracts::repository::global_models::{
    AdminGlobalModelListQuery, AdminProviderModelListQuery,
    CreateAdminProviderModelWithBindingsRecord, StoredAdminGlobalModelPage,
    StoredAdminProviderModel, StoredModelEndpointBinding, UpsertAdminProviderModelRecord,
};
use aether_data_contracts::repository::provider_catalog::{
    ProviderCatalogKeyRuntimeMetadataUpdate, ProviderCatalogUpstreamMetadataNamespaceUpdate,
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey,
    StoredProviderCatalogModelFetchCandidate, StoredProviderCatalogProvider,
};
use aether_data_contracts::repository::quota::StoredProviderQuotaSnapshot;
use aether_data_contracts::DataLayerError;
use aether_model_fetch::{
    aggregate_models_for_cache, build_antigravity_load_code_assist_plan,
    fetch_models_from_transports, merge_upstream_metadata, model_fetch_interval_minutes,
    ModelFetchAssociationStore, ModelFetchTransportRuntime,
};
use aether_scheduler_core::SchedulerAffinityTarget;
use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, warn};

use super::{AppState, GatewayError};
use crate::clock::current_unix_secs;
use crate::model_fetch::{safe_model_fetch_error, CodexCatalogRuntime, ModelFetchRuntimeState};
use crate::orchestration::{AntigravityModelQuotaCandidate, AntigravityModelQuotaRefreshCapture};
use crate::provider_transport::{GatewayProviderTransportSnapshot, LocalResolvedOAuthRequestAuth};
use crate::request_candidate_runtime::{
    RequestCandidateRuntimeCapabilityReader, RequestCandidateRuntimeReader,
    RequestCandidateRuntimeWriter,
};
use crate::scheduler::state::{SchedulerModelQuotaState, SchedulerRuntimeState};
use crate::{execution_runtime, provider_transport};

const MODEL_FETCH_RESPONSE_BODY_LIMIT_BYTES: usize = 8 * 1024 * 1024;

#[async_trait]
impl provider_transport::ProviderTransportSnapshotSource for AppState {
    fn encryption_key(&self) -> Option<&str> {
        AppState::encryption_key(self)
    }

    async fn list_provider_catalog_providers_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogProvider>, DataLayerError> {
        self.read_provider_catalog_providers_by_ids(ids)
            .await
            .map_err(provider_transport_snapshot_data_error)
    }

    async fn list_provider_catalog_endpoints_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogEndpoint>, DataLayerError> {
        self.read_provider_catalog_endpoints_by_ids(ids)
            .await
            .map_err(provider_transport_snapshot_data_error)
    }

    async fn list_provider_catalog_keys_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogKey>, DataLayerError> {
        self.read_provider_catalog_keys_by_ids(ids)
            .await
            .map_err(provider_transport_snapshot_data_error)
    }
}

fn provider_transport_snapshot_data_error(error: GatewayError) -> DataLayerError {
    DataLayerError::UnexpectedValue(error.into_message())
}

impl AppState {
    pub(crate) async fn hydrate_antigravity_project_metadata_for_transport(
        &self,
        transport: &GatewayProviderTransportSnapshot,
    ) -> Option<GatewayProviderTransportSnapshot> {
        if !provider_transport::antigravity::is_antigravity_provider_transport(transport) {
            return None;
        }
        let had_usable_transport = matches!(
            provider_transport::antigravity::resolve_local_antigravity_request_auth(transport),
            provider_transport::antigravity::AntigravityRequestAuthSupport::Supported(_)
        );
        let now_unix_secs = current_unix_secs();
        let observation_due = provider_transport::antigravity::antigravity_account_observation_due(
            transport.key.upstream_metadata.as_ref(),
            now_unix_secs,
        );

        // 未到期时压制查询：有可用项目直接返回快照；无可用项目（处于失败冷却期）直接返回 None。
        if !observation_due {
            return if had_usable_transport {
                Some(transport.clone())
            } else {
                None
            };
        }

        let plan = match build_antigravity_load_code_assist_plan(self, transport).await {
            Ok(plan) => plan,
            Err(err) => {
                warn!(
                    provider_id = %transport.provider.id,
                    endpoint_id = %transport.endpoint.id,
                    key_id = %transport.key.id,
                    error = %err,
                    "antigravity project metadata hydration failed"
                );
                return self
                    .persist_antigravity_hydration_failure(
                        transport,
                        had_usable_transport,
                        now_unix_secs,
                    )
                    .await;
            }
        };

        let result =
            match execution_runtime::execute_execution_runtime_sync_plan(self, None, &plan).await {
                Ok(result) => result,
                Err(err) => {
                    warn!(
                        provider_id = %transport.provider.id,
                        endpoint_id = %transport.endpoint.id,
                        key_id = %transport.key.id,
                        error = ?err,
                        "antigravity project metadata hydration request failed"
                    );
                    return self
                        .persist_antigravity_hydration_failure(
                            transport,
                            had_usable_transport,
                            now_unix_secs,
                        )
                        .await;
                }
            };

        if !(200..300).contains(&result.status_code) {
            warn!(
                provider_id = %transport.provider.id,
                endpoint_id = %transport.endpoint.id,
                key_id = %transport.key.id,
                status_code = result.status_code,
                "antigravity project metadata hydration returned non-success status"
            );
            return self
                .persist_antigravity_hydration_failure(
                    transport,
                    had_usable_transport,
                    now_unix_secs,
                )
                .await;
        }

        let parsed_info = result
            .body
            .as_ref()
            .and_then(|body| body.json_body.as_ref())
            .and_then(provider_transport::antigravity::parse_antigravity_code_assist_response);

        let Some(info) = parsed_info else {
            warn!(
                provider_id = %transport.provider.id,
                endpoint_id = %transport.endpoint.id,
                key_id = %transport.key.id,
                "antigravity project metadata hydration response missing project or invalid"
            );
            return self
                .persist_antigravity_hydration_failure(
                    transport,
                    had_usable_transport,
                    now_unix_secs,
                )
                .await;
        };

        let observation_update =
            provider_transport::antigravity::antigravity_account_observation_update(
                Some(&info),
                now_unix_secs,
            );
        let mut hydrated = transport.clone();
        if let Ok(Some(persisted_meta)) = self
            .persist_antigravity_account_observation(
                &transport.key.id,
                &observation_update,
                now_unix_secs,
            )
            .await
        {
            hydrated.key.upstream_metadata = Some(persisted_meta);
        } else {
            let merged_metadata = merge_upstream_metadata(
                transport.key.upstream_metadata.as_ref(),
                &observation_update,
            );
            hydrated.key.upstream_metadata = Some(merged_metadata);
        }

        if matches!(
            provider_transport::antigravity::resolve_local_antigravity_request_auth(&hydrated),
            provider_transport::antigravity::AntigravityRequestAuthSupport::Supported(_)
        ) {
            Some(hydrated)
        } else {
            None
        }
    }

    pub(crate) async fn persist_antigravity_hydration_failure(
        &self,
        transport: &GatewayProviderTransportSnapshot,
        had_usable_transport: bool,
        now_unix_secs: u64,
    ) -> Option<GatewayProviderTransportSnapshot> {
        let observation_update =
            provider_transport::antigravity::antigravity_account_observation_update(
                None,
                now_unix_secs,
            );
        let mut hydrated = transport.clone();
        if let Ok(Some(persisted_meta)) = self
            .persist_antigravity_account_observation(
                &transport.key.id,
                &observation_update,
                now_unix_secs,
            )
            .await
        {
            hydrated.key.upstream_metadata = Some(persisted_meta);
        } else {
            let merged_metadata = merge_upstream_metadata(
                transport.key.upstream_metadata.as_ref(),
                &observation_update,
            );
            hydrated.key.upstream_metadata = Some(merged_metadata);
        }

        if had_usable_transport
            && matches!(
                provider_transport::antigravity::resolve_local_antigravity_request_auth(&hydrated),
                provider_transport::antigravity::AntigravityRequestAuthSupport::Supported(_)
            )
        {
            Some(hydrated)
        } else {
            None
        }
    }

    pub(crate) async fn persist_antigravity_account_observation(
        &self,
        key_id: &str,
        observation_patch: &Value,
        now_unix_secs: u64,
    ) -> Result<Option<Value>, GatewayError> {
        let obs_obj = observation_patch
            .get("antigravity")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                GatewayError::Internal(
                    "antigravity observation patch missing antigravity object".to_string(),
                )
            })?;

        for _ in 0..3 {
            let latest_key = match self
                .list_provider_catalog_keys_by_ids_strong(&[key_id.to_string()])
                .await?
                .into_iter()
                .next()
            {
                Some(key) => key,
                None => return Ok(None),
            };

            let expected_upstream_metadata_value = latest_key
                .upstream_metadata
                .as_ref()
                .and_then(Value::as_object)
                .and_then(|m| m.get("antigravity"))
                .cloned();

            let mut target_map = expected_upstream_metadata_value
                .as_ref()
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();

            for (k, v) in obs_obj {
                target_map.insert(k.clone(), v.clone());
            }

            let updated_namespace = Value::Object(target_map);

            let update = ProviderCatalogKeyRuntimeMetadataUpdate {
                key_id: key_id.to_string(),
                namespace: "antigravity".to_string(),
                expected_upstream_metadata_value,
                upstream_metadata_value: updated_namespace.clone(),
                status_snapshot_patch: serde_json::json!({}),
                updated_at_unix_secs: Some(now_unix_secs),
            };

            match self
                .update_provider_catalog_key_runtime_metadata(&update)
                .await
            {
                Ok(true) => {
                    let mut full_meta = latest_key
                        .upstream_metadata
                        .as_ref()
                        .and_then(Value::as_object)
                        .cloned()
                        .unwrap_or_default();
                    full_meta.insert("antigravity".to_string(), updated_namespace);
                    return Ok(Some(Value::Object(full_meta)));
                }
                Ok(false) => {
                    continue;
                }
                Err(err) => {
                    warn!(
                        key_id,
                        error = ?err,
                        "antigravity hydration metadata CAS persistence error"
                    );
                    return Err(err);
                }
            }
        }
        warn!(
            key_id,
            "antigravity hydration metadata CAS retries exhausted"
        );
        Err(GatewayError::Internal(
            "antigravity account observation CAS conflict retries exhausted".to_string(),
        ))
    }

    pub(crate) async fn hydrate_gemini_cli_project_metadata_for_transport(
        &self,
        transport: &GatewayProviderTransportSnapshot,
    ) -> Option<GatewayProviderTransportSnapshot> {
        if !provider_transport::is_gemini_cli_provider_transport(transport) {
            return None;
        }
        if provider_transport::resolve_gemini_cli_project_id(transport).is_some() {
            return Some(transport.clone());
        }

        let outcome =
            match fetch_models_from_transports(self, std::slice::from_ref(transport)).await {
                Ok(outcome) => outcome,
                Err(err) => {
                    warn!(
                        provider_id = %transport.provider.id,
                        endpoint_id = %transport.endpoint.id,
                        key_id = %transport.key.id,
                        error = %safe_model_fetch_error(&err),
                        "gemini_cli project metadata hydration failed"
                    );
                    return None;
                }
            };
        let upstream_metadata = outcome.upstream_metadata.as_ref()?;
        let merged_metadata =
            merge_upstream_metadata(transport.key.upstream_metadata.as_ref(), upstream_metadata);

        let mut hydrated = transport.clone();
        hydrated.key.upstream_metadata = Some(merged_metadata.clone());
        if provider_transport::resolve_gemini_cli_project_id(&hydrated).is_none() {
            return None;
        }

        if let Err(err) = self
            .update_provider_catalog_key_upstream_metadata(
                &transport.key.id,
                Some(&merged_metadata),
                Some(current_unix_secs()),
            )
            .await
        {
            warn!(
                provider_id = %transport.provider.id,
                endpoint_id = %transport.endpoint.id,
                key_id = %transport.key.id,
                error = ?err,
                "gemini_cli project metadata hydration could not persist metadata"
            );
        }

        Some(hydrated)
    }
}

#[async_trait]
impl provider_transport::TransportTunnelAffinityLookup for AppState {
    async fn lookup_tunnel_attachment_owner(
        &self,
        node_id: &str,
    ) -> Result<Option<provider_transport::TransportTunnelAttachmentOwner>, String> {
        self.tunnel
            .lookup_attachment_owner(self.data.as_ref(), node_id)
            .await
            .map(|owner| {
                owner.map(|owner| provider_transport::TransportTunnelAttachmentOwner {
                    gateway_instance_id: owner.gateway_instance_id,
                    relay_base_url: owner.relay_base_url,
                    observed_at_unix_secs: owner.observed_at_unix_secs,
                })
            })
    }
}

#[async_trait]
impl provider_transport::VideoTaskTransportSnapshotLookup for AppState {
    async fn read_video_task_provider_transport_snapshot(
        &self,
        provider_id: &str,
        endpoint_id: &str,
        key_id: &str,
    ) -> Result<Option<GatewayProviderTransportSnapshot>, String> {
        self.read_provider_transport_snapshot(provider_id, endpoint_id, key_id)
            .await
            .map_err(GatewayError::into_message)
    }

    async fn resolve_video_task_proxy(
        &self,
        transport: &GatewayProviderTransportSnapshot,
    ) -> Option<ProxySnapshot> {
        self.resolve_transport_proxy_snapshot_with_tunnel_affinity(transport)
            .await
    }
}

#[async_trait]
impl ModelFetchTransportRuntime for AppState {
    async fn resolve_local_oauth_request_auth(
        &self,
        transport: &GatewayProviderTransportSnapshot,
    ) -> Result<Option<LocalResolvedOAuthRequestAuth>, String> {
        AppState::resolve_local_oauth_request_auth(self, transport)
            .await
            .map_err(GatewayError::into_message)
    }

    async fn resolve_model_fetch_proxy(
        &self,
        transport: &GatewayProviderTransportSnapshot,
    ) -> Option<ProxySnapshot> {
        self.resolve_transport_proxy_snapshot_with_tunnel_affinity(transport)
            .await
    }

    async fn execute_model_fetch_execution_plan(
        &self,
        plan: &ExecutionPlan,
    ) -> Result<ExecutionResult, String> {
        let bounded_plan = execution_runtime::transport::with_upstream_response_body_limit(
            plan,
            MODEL_FETCH_RESPONSE_BODY_LIMIT_BYTES,
        );
        execution_runtime::execute_execution_runtime_sync_plan(self, None, &bounded_plan)
            .await
            .map_err(GatewayError::into_message)
    }
}

#[async_trait]
impl CodexCatalogRuntime for AppState {
    fn codex_catalog_runtime_state(&self) -> &aether_runtime_state::RuntimeState {
        self.runtime_state.as_ref()
    }

    async fn read_codex_catalog_transport_snapshot(
        &self,
        provider_id: &str,
        endpoint_id: &str,
        key_id: &str,
    ) -> Result<Option<GatewayProviderTransportSnapshot>, String> {
        self.read_provider_transport_snapshot(provider_id, endpoint_id, key_id)
            .await
            .map_err(GatewayError::into_message)
    }

    async fn read_codex_catalog_credential_scope_strong(
        &self,
        provider_id: &str,
        key_id: &str,
    ) -> Result<Option<String>, String> {
        let Some(key) = self
            .list_provider_catalog_keys_by_ids_strong(&[key_id.to_string()])
            .await
            .map_err(GatewayError::into_message)?
            .into_iter()
            .find(|key| key.id == key_id && key.provider_id == provider_id && key.is_active)
        else {
            return Ok(None);
        };

        if let Some(scope) =
            crate::model_fetch::codex_catalog_credential_scope_from_stored_key(&key, None, None)
        {
            return Ok(Some(scope));
        }

        let decrypted_auth_config = self
            .decrypt_provider_catalog_key_auth_config(&key)
            .map_err(GatewayError::into_message)?;
        let decrypted_api_key = self
            .decrypt_provider_catalog_key_api_key(&key)
            .map_err(GatewayError::into_message)?;

        Ok(
            crate::model_fetch::codex_catalog_credential_scope_from_stored_key(
                &key,
                decrypted_auth_config.as_deref(),
                decrypted_api_key.as_deref(),
            ),
        )
    }
}

#[async_trait]
impl ModelFetchRuntimeState for AppState {
    fn has_provider_catalog_data_reader(&self) -> bool {
        AppState::has_provider_catalog_data_reader(self)
    }

    fn has_provider_catalog_data_writer(&self) -> bool {
        AppState::has_provider_catalog_data_writer(self)
    }

    async fn list_provider_catalog_providers(
        &self,
        active_only: bool,
    ) -> Result<Vec<StoredProviderCatalogProvider>, GatewayError> {
        AppState::list_provider_catalog_providers(self, active_only).await
    }

    async fn list_provider_catalog_providers_for_model_fetch(
        &self,
        active_only: bool,
    ) -> Result<Vec<StoredProviderCatalogProvider>, GatewayError> {
        self.data
            .list_provider_catalog_providers(active_only)
            .await
            .map_err(|err| GatewayError::Internal(err.to_string()))
    }

    async fn list_provider_catalog_endpoints_by_provider_ids(
        &self,
        provider_ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogEndpoint>, GatewayError> {
        AppState::list_provider_catalog_endpoints_by_provider_ids(self, provider_ids).await
    }

    async fn list_provider_catalog_endpoints_for_model_fetch(
        &self,
        provider_ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogEndpoint>, GatewayError> {
        self.data
            .list_provider_catalog_endpoints_by_provider_ids(provider_ids)
            .await
            .map_err(|err| GatewayError::Internal(err.to_string()))
    }

    async fn read_provider_transport_snapshot(
        &self,
        provider_id: &str,
        endpoint_id: &str,
        key_id: &str,
    ) -> Result<Option<GatewayProviderTransportSnapshot>, GatewayError> {
        AppState::read_provider_transport_snapshot(self, provider_id, endpoint_id, key_id).await
    }

    async fn execute_execution_runtime_sync_plan(
        &self,
        plan: &ExecutionPlan,
    ) -> Result<ExecutionResult, GatewayError> {
        execution_runtime::execute_execution_runtime_sync_plan(self, None, plan).await
    }

    async fn read_recent_codex_catalog_client_version(
        &self,
        provider_id: &str,
        key_id: &str,
    ) -> Option<String> {
        crate::model_fetch::read_codex_management_catalog(self, provider_id, key_id)
            .await
            .map(|catalog| catalog.client_version)
    }

    async fn update_provider_catalog_key_model_fetch_state(
        &self,
        key_id: &str,
        allowed_models: Option<&Value>,
        last_models_fetch_at_unix_secs: Option<u64>,
        last_models_fetch_error: Option<&str>,
        updated_at_unix_secs: Option<u64>,
    ) -> Result<(), GatewayError> {
        AppState::update_provider_catalog_key_model_fetch_state(
            self,
            key_id,
            allowed_models,
            last_models_fetch_at_unix_secs,
            last_models_fetch_error,
            updated_at_unix_secs,
        )
        .await?;
        Ok(())
    }

    async fn update_provider_catalog_key_model_fetch_success(
        &self,
        key_id: &str,
        allowed_models: Option<&Value>,
        last_models_fetch_at_unix_secs: u64,
        upstream_metadata_updates: &[ProviderCatalogUpstreamMetadataNamespaceUpdate],
        updated_at_unix_secs: Option<u64>,
    ) -> Result<(), GatewayError> {
        AppState::update_provider_catalog_key_model_fetch_success(
            self,
            key_id,
            allowed_models,
            last_models_fetch_at_unix_secs,
            upstream_metadata_updates,
            updated_at_unix_secs,
        )
        .await?;
        Ok(())
    }

    async fn persist_antigravity_account_observation(
        &self,
        key_id: &str,
        observation_patch: &Value,
        now_unix_secs: u64,
    ) -> Result<(), GatewayError> {
        AppState::persist_antigravity_account_observation(
            self,
            key_id,
            observation_patch,
            now_unix_secs,
        )
        .await?
        .ok_or_else(|| {
            GatewayError::Internal(
                "provider key disappeared during account observation".to_string(),
            )
        })?;
        Ok(())
    }

    async fn capture_antigravity_model_quota_refresh(
        &self,
        transport: &GatewayProviderTransportSnapshot,
    ) -> Result<Option<AntigravityModelQuotaRefreshCapture>, GatewayError> {
        crate::orchestration::capture_antigravity_model_quota_refresh(self, transport).await
    }

    async fn reconcile_antigravity_model_quota_refresh(
        &self,
        capture: &AntigravityModelQuotaRefreshCapture,
        positive_metadata: Option<&Value>,
    ) -> Result<(), GatewayError> {
        crate::orchestration::reconcile_antigravity_model_quota_refresh(
            self,
            capture,
            positive_metadata,
        )
        .await
    }

    async fn write_upstream_models_cache(
        &self,
        provider_id: &str,
        key_id: &str,
        cached_models: &[Value],
    ) {
        let models = aggregate_models_for_cache(cached_models);
        if models.is_empty() {
            return;
        }
        let Ok(serialized) = serde_json::to_string(&models) else {
            return;
        };
        let cache_key = format!("upstream_models:{provider_id}:{key_id}");
        if let Err(err) = self
            .runtime_state
            .kv_set(
                &cache_key,
                serialized,
                Some(std::time::Duration::from_secs(
                    model_fetch_interval_minutes().saturating_mul(60),
                )),
            )
            .await
        {
            debug!(
                provider_id = %provider_id,
                key_id = %key_id,
                error = %err,
                "gateway model fetch cache write failed"
            );
        }
    }
}

#[async_trait]
impl ModelFetchAssociationStore for AppState {
    type Error = String;

    fn has_global_model_reader(&self) -> bool {
        self.data.has_global_model_reader()
    }

    fn has_global_model_writer(&self) -> bool {
        self.data.has_global_model_writer()
    }

    fn model_fetch_internal_error(&self, message: String) -> Self::Error {
        message
    }

    async fn list_admin_provider_models(
        &self,
        query: &AdminProviderModelListQuery,
    ) -> Result<Vec<StoredAdminProviderModel>, Self::Error> {
        AppState::list_admin_provider_models(self, query)
            .await
            .map_err(|err| format!("{err:?}"))
    }

    /// 图片发现复用现有绑定读取，不扩大 Key 维护投影。
    async fn list_model_endpoint_bindings(
        &self,
        model_ids: &[String],
    ) -> Result<Vec<StoredModelEndpointBinding>, Self::Error> {
        AppState::list_model_endpoint_bindings(self, model_ids)
            .await
            .map_err(|err| format!("{err:?}"))
    }

    async fn list_admin_global_models(
        &self,
        query: &AdminGlobalModelListQuery,
    ) -> Result<StoredAdminGlobalModelPage, Self::Error> {
        AppState::list_admin_global_models(self, query)
            .await
            .map_err(|err| format!("{err:?}"))
    }

    async fn create_admin_provider_model_with_bindings(
        &self,
        record: &CreateAdminProviderModelWithBindingsRecord,
    ) -> Result<Option<StoredAdminProviderModel>, Self::Error> {
        AppState::create_admin_provider_model_with_bindings(self, record)
            .await
            .map_err(|err| format!("{err:?}"))
    }

    async fn create_admin_provider_model(
        &self,
        record: &UpsertAdminProviderModelRecord,
    ) -> Result<Option<StoredAdminProviderModel>, Self::Error> {
        AppState::create_admin_provider_model(self, record)
            .await
            .map_err(|err| format!("{err:?}"))
    }

    async fn update_admin_provider_model(
        &self,
        record: &UpsertAdminProviderModelRecord,
    ) -> Result<Option<StoredAdminProviderModel>, Self::Error> {
        AppState::update_admin_provider_model(self, record)
            .await
            .map_err(|err| format!("{err:?}"))
    }

    async fn sync_model_endpoint_bindings(
        &self,
        model_id: &str,
        endpoint_ids: &[String],
        source: &str,
        replace_automatic: bool,
        replacement_scope_endpoint_ids: &[String],
    ) -> Result<Vec<StoredModelEndpointBinding>, Self::Error> {
        AppState::sync_model_endpoint_bindings(
            self,
            model_id,
            endpoint_ids,
            source,
            replace_automatic,
            replacement_scope_endpoint_ids,
        )
        .await
        .map_err(|err| format!("{err:?}"))
    }

    /// 为模型抓取和批末 reconcile 提供实时轻量 Key 投影。
    async fn list_model_fetch_candidates_by_provider_ids(
        &self,
        provider_ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogModelFetchCandidate>, Self::Error> {
        AppState::list_provider_catalog_model_fetch_candidates_by_provider_ids(self, provider_ids)
            .await
            .map_err(|err| format!("{err:?}"))
    }
}

#[async_trait]
impl RequestCandidateRuntimeReader for AppState {
    async fn read_request_candidates_by_request_id(
        &self,
        request_id: &str,
    ) -> Result<Vec<StoredRequestCandidate>, GatewayError> {
        AppState::read_request_candidates_by_request_id(self, request_id).await
    }
}

#[async_trait]
impl RequestCandidateRuntimeCapabilityReader for AppState {
    async fn read_request_candidate_user_model_capability_settings(
        &self,
        user_id: &str,
    ) -> Result<Option<Value>, GatewayError> {
        AppState::read_user_model_capability_settings(self, user_id).await
    }

    async fn read_request_candidate_api_key_force_capabilities(
        &self,
        user_id: &str,
        api_key_id: &str,
    ) -> Result<Option<Value>, GatewayError> {
        AppState::read_auth_api_key_force_capabilities(self, user_id, api_key_id).await
    }
}

#[async_trait]
impl RequestCandidateRuntimeWriter for AppState {
    fn has_request_candidate_data_writer(&self) -> bool {
        AppState::has_request_candidate_data_writer(self)
    }

    async fn upsert_request_candidate(
        &self,
        candidate: UpsertRequestCandidateRecord,
    ) -> Result<Option<StoredRequestCandidate>, GatewayError> {
        AppState::upsert_request_candidate(self, candidate).await
    }

    async fn enqueue_request_candidate_status(
        &self,
        candidate: UpsertRequestCandidateRecord,
    ) -> Result<Option<()>, GatewayError> {
        AppState::enqueue_request_candidate_status(self, candidate).await
    }

    fn try_enqueue_request_candidate_status(
        &self,
        candidate: UpsertRequestCandidateRecord,
    ) -> Result<(), UpsertRequestCandidateRecord> {
        AppState::try_enqueue_request_candidate_status(self, candidate)
    }
}

#[async_trait]
impl SchedulerRuntimeState for AppState {
    async fn read_provider_quota_snapshot(
        &self,
        provider_id: &str,
    ) -> Result<Option<StoredProviderQuotaSnapshot>, GatewayError> {
        AppState::read_provider_quota_snapshot(self, provider_id).await
    }

    async fn read_provider_catalog_providers_by_ids(
        &self,
        provider_ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogProvider>, GatewayError> {
        AppState::read_provider_catalog_providers_by_ids(self, provider_ids).await
    }

    async fn read_provider_catalog_keys_by_ids(
        &self,
        key_ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogKey>, GatewayError> {
        AppState::read_provider_catalog_keys_by_ids(self, key_ids).await
    }

    async fn read_provider_catalog_keys_by_ids_strong(
        &self,
        key_ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogKey>, GatewayError> {
        AppState::list_provider_catalog_keys_by_ids_strong(self, key_ids).await
    }

    async fn read_antigravity_model_quota_blocked(
        &self,
        candidates: &[AntigravityModelQuotaCandidate<'_>],
        now_unix_secs: u64,
    ) -> Result<Vec<SchedulerModelQuotaState>, GatewayError> {
        // The scoped fact is keyed by the account-stable refresh-token
        // generation, so the reader decrypts the same credential the writer
        // observed. A malformed/legacy ciphertext on one account must not hide
        // valid siblings: only that candidate reports state unavailable and it
        // is excluded from the shared lookup instead of falling back to a
        // weaker identity or failing the whole batch.
        let mut readable = Vec::with_capacity(candidates.len());
        let mut auth_configs = Vec::with_capacity(candidates.len());
        let mut result = vec![SchedulerModelQuotaState::StateUnavailable; candidates.len()];
        for (index, candidate) in candidates.iter().enumerate() {
            match self.decrypt_provider_catalog_key_auth_config(candidate.key) {
                Ok(auth_config) => {
                    readable.push((index, *candidate));
                    auth_configs.push(auth_config);
                }
                Err(err) => {
                    warn!(
                        error = ?err,
                        provider_id = %candidate.key.provider_id,
                        key_id = %candidate.key.id,
                        "failed to decrypt antigravity key auth config for runtime model quota read"
                    );
                }
            }
        }
        if readable.is_empty() {
            return Ok(result);
        }
        let descriptors = readable
            .iter()
            .map(|(_, candidate)| *candidate)
            .collect::<Vec<_>>();
        let auth_config_refs = auth_configs
            .iter()
            .map(Option::as_deref)
            .collect::<Vec<_>>();
        // Always the configured shared backend: another gateway instance may
        // have learned the scoped fact after this process cached its catalog.
        let blocked = crate::orchestration::antigravity_model_quota_blocked_many_with_auth_configs(
            self.runtime_state.as_ref(),
            &descriptors,
            &auth_config_refs,
            now_unix_secs,
        )
        .await?;
        for ((index, _), blocked) in readable.into_iter().zip(blocked) {
            result[index] = if blocked {
                SchedulerModelQuotaState::Blocked
            } else {
                SchedulerModelQuotaState::Available
            };
        }
        Ok(result)
    }

    async fn read_recent_request_candidates(
        &self,
        limit: usize,
    ) -> Result<Vec<StoredRequestCandidate>, GatewayError> {
        AppState::read_recent_runtime_request_candidates(self, limit).await
    }

    fn provider_key_rpm_reset_at(&self, key_id: &str, now_unix_secs: u64) -> Option<u64> {
        AppState::provider_key_rpm_reset_at(self, key_id, now_unix_secs)
    }

    fn read_cached_scheduler_affinity_target(
        &self,
        cache_key: &str,
        ttl: Duration,
    ) -> Option<SchedulerAffinityTarget> {
        AppState::read_scheduler_affinity_target(self, cache_key, ttl)
    }

    fn scheduler_affinity_epoch(&self) -> u64 {
        AppState::scheduler_affinity_epoch(self)
    }

    fn remember_scheduler_affinity_target(
        &self,
        cache_key: &str,
        target: SchedulerAffinityTarget,
        ttl: Duration,
        max_entries: usize,
    ) {
        AppState::remember_scheduler_affinity_target(self, cache_key, target, ttl, max_entries);
    }

    fn remember_scheduler_affinity_target_for_epoch(
        &self,
        cache_key: &str,
        target: SchedulerAffinityTarget,
        ttl: Duration,
        max_entries: usize,
        expected_epoch: Option<u64>,
    ) -> bool {
        AppState::remember_scheduler_affinity_target_for_epoch(
            self,
            cache_key,
            target,
            ttl,
            max_entries,
            expected_epoch,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use aether_provider_transport::snapshot::{
        GatewayProviderTransportEndpoint, GatewayProviderTransportKey,
        GatewayProviderTransportProvider, GatewayProviderTransportSnapshot,
    };
    use serde_json::json;

    fn sample_antigravity_snapshot(
        project_id: Option<&str>,
        metadata: Option<Value>,
    ) -> GatewayProviderTransportSnapshot {
        GatewayProviderTransportSnapshot {
            provider: GatewayProviderTransportProvider {
                id: "provider-antigravity".to_string(),
                name: "antigravity".to_string(),
                provider_type: "antigravity".to_string(),
                website: None,
                is_active: true,
                keep_priority_on_conversion: false,
                enable_format_conversion: false,
                concurrent_limit: None,
                max_retries: None,
                proxy: None,
                request_timeout_secs: None,
                stream_first_byte_timeout_secs: None,
                config: None,
            },
            endpoint: GatewayProviderTransportEndpoint {
                id: "endpoint-antigravity".to_string(),
                provider_id: "provider-antigravity".to_string(),
                api_format: "gemini:generate_content".to_string(),
                api_family: Some("gemini".to_string()),
                endpoint_kind: Some("generate_content".to_string()),
                is_active: true,
                base_url: "https://cloudcode-pa.googleapis.com".to_string(),
                header_rules: None,
                body_rules: None,
                max_retries: None,
                custom_path: None,
                config: None,
                format_acceptance_config: None,
                proxy: None,
            },
            key: GatewayProviderTransportKey {
                id: "key-antigravity-1".to_string(),
                provider_id: "provider-antigravity".to_string(),
                name: "Antigravity Key".to_string(),
                auth_type: "oauth".to_string(),
                is_active: true,
                api_formats: Some(vec!["gemini:generate_content".to_string()]),
                auth_type_by_format: None,
                allow_auth_channel_mismatch_formats: None,
                allowed_models: None,
                capabilities: None,
                rate_multipliers: None,
                global_priority_by_format: None,
                expires_at_unix_secs: None,
                proxy: None,
                fingerprint: None,
                upstream_metadata: metadata,
                decrypted_api_key: "antigravity-token".to_string(),
                decrypted_auth_config: Some(
                    json!({
                        "provider_type": "antigravity",
                        "project_id": project_id,
                    })
                    .to_string(),
                ),
            },
        }
    }

    #[tokio::test]
    async fn antigravity_hydration_skips_when_usable_and_observation_not_due() {
        let app = AppState::new().expect("app state should build");
        let now = current_unix_secs();
        let meta = json!({
            "antigravity": {
                "project_id": "cached-proj",
                "subscription_tier": "pro",
                "account_info_status": "success",
                "account_info_checked_at": now - 300,
            }
        });
        let transport = sample_antigravity_snapshot(Some("cached-proj"), Some(meta));
        let hydrated = app
            .hydrate_antigravity_project_metadata_for_transport(&transport)
            .await;
        assert!(hydrated.is_some());
        let hydrated = hydrated.unwrap();
        assert_eq!(
            hydrated
                .key
                .upstream_metadata
                .as_ref()
                .unwrap()
                .pointer("/antigravity/subscription_tier")
                .and_then(Value::as_str),
            Some("pro")
        );
    }

    #[tokio::test]
    async fn antigravity_hydration_suppresses_discovery_when_missing_project_and_not_due() {
        let app = AppState::new().expect("app state should build");
        let now = current_unix_secs();
        // 观察处于 60 秒失败冷却期内，且缺少 project_id，应直接返回 None 压制多余请求
        let meta = json!({
            "antigravity": {
                "account_info_status": "error",
                "account_info_checked_at": now - 10,
            }
        });
        let transport = sample_antigravity_snapshot(None, Some(meta));
        let hydrated = app
            .hydrate_antigravity_project_metadata_for_transport(&transport)
            .await;
        assert!(
            hydrated.is_none(),
            "cooling down failed account must not re-query upstream"
        );
    }

    #[tokio::test]
    async fn antigravity_hydration_failure_preserves_usable_project_and_cached_tier() {
        let app = AppState::new().expect("app state should build");
        let now = current_unix_secs();
        let meta = json!({
            "antigravity": {
                "project_id": "cached-proj",
                "subscription_tier": "ultra",
                "paid_tier_id": "g1-ultra-tier",
                "account_info_status": "success",
                "account_info_checked_at": now - 1000,
            },
            "other_namespace": {
                "some_key": "some_value"
            }
        });
        let transport = sample_antigravity_snapshot(Some("cached-proj"), Some(meta));

        // 模拟 hydration 失败写入：即使失败，可用传输仍然存活，且保留先前的套餐与命名空间
        let hydrated = app
            .persist_antigravity_hydration_failure(&transport, true, now)
            .await;
        assert!(
            hydrated.is_some(),
            "usable cached transport must survive best-effort failure"
        );
        let hydrated = hydrated.unwrap();
        let meta = hydrated.key.upstream_metadata.as_ref().unwrap();

        assert_eq!(
            meta.pointer("/antigravity/account_info_status")
                .and_then(Value::as_str),
            Some("error")
        );
        assert_eq!(
            meta.pointer("/antigravity/account_info_checked_at")
                .and_then(Value::as_u64),
            Some(now)
        );
        assert_eq!(
            meta.pointer("/antigravity/subscription_tier")
                .and_then(Value::as_str),
            Some("ultra"),
            "previous subscription tier must survive failure"
        );
        assert_eq!(
            meta.pointer("/antigravity/paid_tier_id")
                .and_then(Value::as_str),
            Some("g1-ultra-tier")
        );
        assert_eq!(
            meta.pointer("/other_namespace/some_key")
                .and_then(Value::as_str),
            Some("some_value"),
            "unrelated namespaces must survive hydration failure update"
        );
    }

    #[tokio::test]
    async fn antigravity_hydration_failure_returns_none_when_project_missing() {
        let app = AppState::new().expect("app state should build");
        let now = current_unix_secs();
        let transport = sample_antigravity_snapshot(None, None);

        let hydrated = app
            .persist_antigravity_hydration_failure(&transport, false, now)
            .await;
        assert!(
            hydrated.is_none(),
            "missing companion project cannot return usable transport on failure"
        );
    }

    #[tokio::test]
    async fn runtime_model_quota_credential_failure_does_not_poison_healthy_siblings() {
        use crate::data::GatewayDataState;
        use crate::scheduler::state::SchedulerModelQuotaState;
        use aether_data::repository::provider_catalog::InMemoryProviderCatalogReadRepository;
        use std::sync::Arc;

        const AUTH_CONFIG: &str = r#"{"refresh_token":"fixture-integrations-good"}"#;
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(Arc::new(
                    InMemoryProviderCatalogReadRepository::seed(vec![], vec![], vec![]),
                ))
                .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY),
            );

        let sealed = app
            .seal_provider_catalog_key_auth_config("provider-a", "key-good", AUTH_CONFIG)
            .expect("auth config should seal");
        let mut good = StoredProviderCatalogKey::new(
            "key-good".to_string(),
            "provider-a".to_string(),
            "good".to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("good key should build");
        good.encrypted_auth_config = Some(sealed);
        let mut bad = StoredProviderCatalogKey::new(
            "key-bad".to_string(),
            "provider-a".to_string(),
            "bad".to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("bad key should build");
        bad.encrypted_auth_config = Some("malformed-provider-catalog-credential".to_string());
        let free = StoredProviderCatalogKey::new(
            "key-free".to_string(),
            "provider-a".to_string(),
            "free".to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("free key should build");

        let now_unix_secs = current_unix_secs();
        crate::orchestration::record_antigravity_model_quota_exhaustion_with_auth_config(
            app.runtime_state.as_ref(),
            &good,
            Some(AUTH_CONFIG),
            "antigravity",
            "gemini-3-pro",
            now_unix_secs + 3_600,
            now_unix_secs,
            "test",
        )
        .await
        .expect("runtime model quota fact should record");

        let candidates = [
            AntigravityModelQuotaCandidate {
                key: &good,
                provider_type: "antigravity",
                model: "gemini-3-pro",
            },
            AntigravityModelQuotaCandidate {
                key: &bad,
                provider_type: "antigravity",
                model: "gemini-3-pro",
            },
            AntigravityModelQuotaCandidate {
                key: &free,
                provider_type: "antigravity",
                model: "gemini-3-pro",
            },
        ];
        let states = app
            .read_antigravity_model_quota_blocked(&candidates, now_unix_secs)
            .await
            .expect("one malformed credential must not fail the whole batch");
        assert_eq!(
            states,
            vec![
                SchedulerModelQuotaState::Blocked,
                SchedulerModelQuotaState::StateUnavailable,
                SchedulerModelQuotaState::Available,
            ]
        );
    }

    async fn model_fetch_model_state(
        app: &AppState,
        key: &StoredProviderCatalogKey,
        model: &str,
    ) -> SchedulerModelQuotaState {
        let candidates = [AntigravityModelQuotaCandidate {
            key,
            provider_type: "antigravity",
            model,
        }];
        <AppState as SchedulerRuntimeState>::read_antigravity_model_quota_blocked(
            app,
            &candidates,
            current_unix_secs(),
        )
        .await
        .expect("model quota read should succeed")
        .into_iter()
        .next()
        .expect("one candidate")
    }

    #[tokio::test]
    async fn model_fetch_positive_refresh_clears_captured_fact_but_stale_capture_keeps() {
        use crate::data::GatewayDataState;
        use crate::orchestration::AntigravityModelQuotaCandidate;
        use aether_data::repository::provider_catalog::InMemoryProviderCatalogReadRepository;
        use aether_data_contracts::repository::provider_catalog::ProviderCatalogReadRepository;
        use std::sync::Arc;

        const AUTH_CONFIG: &str = r#"{"refresh_token":"fixture-model-fetch-refresh"}"#;
        const MODEL: &str = "gemini-3-pro";

        let sealer = AppState::new()
            .expect("sealer state should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(Arc::new(
                    InMemoryProviderCatalogReadRepository::seed(vec![], vec![], vec![]),
                ))
                .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY),
            );
        let sealed = sealer
            .seal_provider_catalog_key_auth_config("provider-fetch", "key-fetch", AUTH_CONFIG)
            .expect("auth config should seal");
        let mut key = StoredProviderCatalogKey::new(
            "key-fetch".to_string(),
            "provider-fetch".to_string(),
            "fetch".to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("key should build");
        key.encrypted_auth_config = Some(sealed);
        key.upstream_metadata = Some(json!({
            "antigravity": {
                "project_id": "project-fetch",
                "quota_by_model": {
                    "gemini-3-pro": {"remaining_fraction": 0.0}
                }
            }
        }));
        let repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
            vec![],
            vec![],
            vec![key.clone()],
        ));
        let catalog_reader: Arc<dyn ProviderCatalogReadRepository> = repository.clone();
        let app = AppState::new()
            .expect("state should build")
            .with_data_state_for_tests(
                GatewayDataState::with_provider_catalog_reader_for_tests(catalog_reader)
                    .attach_provider_catalog_repository_for_tests(repository)
                    .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY),
            );

        let serving_transport = GatewayProviderTransportSnapshot {
            provider: GatewayProviderTransportProvider {
                id: "provider-fetch".to_string(),
                name: "fetch".to_string(),
                provider_type: "antigravity".to_string(),
                website: None,
                is_active: true,
                keep_priority_on_conversion: false,
                enable_format_conversion: false,
                concurrent_limit: None,
                max_retries: None,
                proxy: None,
                request_timeout_secs: None,
                stream_first_byte_timeout_secs: None,
                config: None,
            },
            endpoint: GatewayProviderTransportEndpoint {
                id: "endpoint-fetch".to_string(),
                provider_id: "provider-fetch".to_string(),
                api_format: "gemini:generate_content".to_string(),
                api_family: Some("gemini".to_string()),
                endpoint_kind: Some("generate_content".to_string()),
                is_active: true,
                base_url: "https://integrations.invalid".to_string(),
                header_rules: None,
                body_rules: None,
                max_retries: None,
                custom_path: None,
                config: None,
                format_acceptance_config: None,
                proxy: None,
            },
            key: GatewayProviderTransportKey {
                id: "key-fetch".to_string(),
                provider_id: "provider-fetch".to_string(),
                name: "fetch".to_string(),
                auth_type: "oauth".to_string(),
                is_active: true,
                api_formats: Some(vec!["gemini:generate_content".to_string()]),
                auth_type_by_format: None,
                allow_auth_channel_mismatch_formats: None,
                allowed_models: None,
                capabilities: None,
                rate_multipliers: None,
                global_priority_by_format: None,
                expires_at_unix_secs: None,
                proxy: None,
                fingerprint: None,
                upstream_metadata: key.upstream_metadata.clone(),
                decrypted_api_key: "fetch-token".to_string(),
                decrypted_auth_config: Some(AUTH_CONFIG.to_string()),
            },
        };

        let now_unix_secs = current_unix_secs();
        crate::orchestration::record_antigravity_model_quota_exhaustion_with_auth_config(
            app.runtime_state.as_ref(),
            &key,
            Some(AUTH_CONFIG),
            "antigravity",
            MODEL,
            now_unix_secs + 3_600,
            now_unix_secs,
            "test",
        )
        .await
        .expect("fact should record");
        let capture =
            <AppState as ModelFetchRuntimeState>::capture_antigravity_model_quota_refresh(
                &app,
                &serving_transport,
            )
            .await
            .expect("capture should not fail")
            .expect("antigravity key should capture");
        assert_eq!(
            model_fetch_model_state(&app, &key, MODEL).await,
            SchedulerModelQuotaState::Blocked
        );

        let positive_bucket = json!({
            "project_id": "project-fetch",
            "quota_by_model": {
                "gemini-3-pro": {"remaining_fraction": 0.8}
            }
        });
        // The reconcile reads positivity from the strongly persisted Key state.
        let updated = app
            .update_provider_catalog_key_runtime_metadata(
                &ProviderCatalogKeyRuntimeMetadataUpdate {
                    key_id: key.id.clone(),
                    namespace: "antigravity".to_string(),
                    expected_upstream_metadata_value: key
                        .upstream_metadata
                        .as_ref()
                        .and_then(|value| value.get("antigravity"))
                        .cloned(),
                    upstream_metadata_value: positive_bucket.clone(),
                    status_snapshot_patch: json!({}),
                    updated_at_unix_secs: Some(now_unix_secs),
                },
            )
            .await
            .expect("positive metadata should persist");
        assert!(updated, "positive metadata CAS should match the seeded key");
        // Production callers pass the metadata document that was just persisted;
        // reconcile intersects it with the strongly persisted positive buckets.
        let incoming_positive = json!({ "antigravity": positive_bucket.clone() });
        <AppState as ModelFetchRuntimeState>::reconcile_antigravity_model_quota_refresh(
            &app,
            &capture,
            Some(&incoming_positive),
        )
        .await
        .expect("positive refresh should reconcile");
        assert_eq!(
            model_fetch_model_state(&app, &key, MODEL).await,
            SchedulerModelQuotaState::Available
        );

        // A fact learned after the capture is not byte-identical, so a positive
        // refresh with a stale capture must keep it.
        crate::orchestration::record_antigravity_model_quota_exhaustion_with_auth_config(
            app.runtime_state.as_ref(),
            &key,
            Some(AUTH_CONFIG),
            "antigravity",
            MODEL,
            now_unix_secs + 7_200,
            now_unix_secs,
            "test",
        )
        .await
        .expect("fact should record");
        let stale_capture =
            <AppState as ModelFetchRuntimeState>::capture_antigravity_model_quota_refresh(
                &app,
                &serving_transport,
            )
            .await
            .expect("capture should not fail")
            .expect("antigravity key should capture");
        crate::orchestration::record_antigravity_model_quota_exhaustion_with_auth_config(
            app.runtime_state.as_ref(),
            &key,
            Some(AUTH_CONFIG),
            "antigravity",
            MODEL,
            now_unix_secs + 10_800,
            now_unix_secs,
            "test",
        )
        .await
        .expect("newer fact should record");
        <AppState as ModelFetchRuntimeState>::reconcile_antigravity_model_quota_refresh(
            &app,
            &stale_capture,
            Some(&incoming_positive),
        )
        .await
        .expect("stale capture reconcile should be a no-op");
        assert_eq!(
            model_fetch_model_state(&app, &key, MODEL).await,
            SchedulerModelQuotaState::Blocked,
            "a fact written after the capture must survive a positive refresh"
        );
    }
}
