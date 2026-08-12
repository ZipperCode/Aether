use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use aether_data_contracts::repository::provider_catalog::{
    ProviderCatalogUpstreamMetadataNamespaceUpdate, StoredProviderCatalogEndpoint,
    StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use aether_model_fetch::{
    aggregate_models_for_cache, apply_model_filters, fetch_models_from_transports,
    global_model_matches_allowed_models, json_string_list, model_catalog_upstream_metadata,
    model_fetch_interval_minutes, model_fetch_startup_delay_seconds, model_fetch_startup_enabled,
    preset_models_for_provider, selected_models_fetch_endpoints,
    sync_provider_model_whitelist_associations, upstream_metadata_namespace_updates,
    ModelFetchAssociationStore, ModelFetchRunSummary,
};
use futures_util::stream::{self, StreamExt};
use serde_json::{json, Value};
use tracing::{debug, info, warn};

use crate::{AppState, GatewayError};

pub(crate) mod state;

use self::state::ModelFetchRuntimeState;

#[derive(Debug, Clone)]
struct SelectedFetchTarget {
    provider: StoredProviderCatalogProvider,
    key: StoredProviderCatalogKey,
    endpoints: Vec<StoredProviderCatalogEndpoint>,
}

pub(crate) fn spawn_model_fetch_worker(state: AppState) -> Option<tokio::task::JoinHandle<()>> {
    if !state.has_provider_catalog_data_reader() || !state.has_provider_catalog_data_writer() {
        return None;
    }

    Some(crate::task_runtime::spawn_singleton_worker(
        state,
        crate::task_runtime::TASK_KEY_MODEL_FETCH_WORKER,
        |state| async move {
            if model_fetch_startup_enabled() {
                let startup_delay = model_fetch_startup_delay_seconds();
                if startup_delay > 0 {
                    tokio::time::sleep(Duration::from_secs(startup_delay)).await;
                }
                if let Err(err) = run_model_fetch_cycle(&state, "startup").await {
                    warn!(error = ?err, "gateway model fetch startup failed");
                }
            } else {
                info!("gateway model fetch startup disabled");
            }

            let mut interval = tokio::time::interval(Duration::from_secs(
                model_fetch_interval_minutes().saturating_mul(60),
            ));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            interval.tick().await;
            loop {
                interval.tick().await;
                if let Err(err) = run_model_fetch_cycle(&state, "tick").await {
                    warn!(error = ?err, "gateway model fetch tick failed");
                }
            }
        },
    ))
}

pub(crate) async fn perform_model_fetch_once(
    state: &AppState,
) -> Result<ModelFetchRunSummary, GatewayError> {
    perform_model_fetch_once_with_state(state).await
}

pub(crate) async fn perform_model_fetch_for_key(
    state: &AppState,
    provider_id: &str,
    key_id: &str,
) -> Result<ModelFetchRunSummary, GatewayError> {
    let key_ids = BTreeSet::from([key_id.to_string()]);
    perform_model_fetch_for_keys_with_state(state, provider_id, &key_ids).await
}

pub(crate) async fn perform_model_fetch_for_keys(
    state: &AppState,
    provider_id: &str,
    key_ids: &BTreeSet<String>,
) -> Result<ModelFetchRunSummary, GatewayError> {
    perform_model_fetch_for_keys_with_state(state, provider_id, key_ids).await
}

pub(crate) async fn sync_global_model_provider_associations(
    state: &AppState,
    global_model_id: &str,
) -> Result<(), GatewayError> {
    if !state.has_provider_catalog_data_reader()
        || !state.has_global_model_data_reader()
        || !state.has_global_model_data_writer()
    {
        return Ok(());
    }

    let Some(global_model) = state.get_admin_global_model_by_id(global_model_id).await? else {
        return Ok(());
    };
    if !global_model.is_active {
        return Ok(());
    }

    let providers = state.list_provider_catalog_providers(true).await?;
    if providers.is_empty() {
        return Ok(());
    }
    let provider_ids = providers
        .iter()
        .map(|provider| provider.id.clone())
        .collect::<Vec<_>>();
    let active_provider_ids = provider_ids.iter().cloned().collect::<BTreeSet<_>>();
    let linked_provider_ids = state
        .list_admin_provider_models_by_global_model_id(global_model_id)
        .await?
        .into_iter()
        .map(|model| model.provider_id)
        .collect::<BTreeSet<_>>();

    let mut allowed_models_by_provider = BTreeMap::<String, BTreeSet<String>>::new();
    for key in state
        .list_provider_catalog_keys_by_provider_ids(&provider_ids)
        .await?
        .into_iter()
        .filter(|key| key.is_active && active_provider_ids.contains(&key.provider_id))
    {
        allowed_models_by_provider
            .entry(key.provider_id)
            .or_default()
            .extend(json_string_list(key.allowed_models.as_ref()));
    }

    let targets = providers
        .into_iter()
        .filter_map(|provider| {
            let allowed_models = allowed_models_by_provider
                .remove(&provider.id)
                .unwrap_or_default()
                .into_iter()
                .collect::<Vec<_>>();
            let should_sync = linked_provider_ids.contains(&provider.id)
                || global_model_matches_allowed_models(
                    &global_model.name,
                    global_model.config.as_ref(),
                    &allowed_models,
                );
            should_sync.then_some((provider.id, allowed_models))
        })
        .collect::<Vec<_>>();

    let results = stream::iter(targets.into_iter().map(
        |(provider_id, allowed_models)| async move {
            sync_provider_model_whitelist_associations(
                state,
                &provider_id,
                &allowed_models,
                &[],
                false,
                false,
                &[],
            )
            .await
        },
    ))
    .buffer_unordered(8)
    .collect::<Vec<_>>()
    .await;
    for result in results {
        result.map_err(GatewayError::Internal)?;
    }
    Ok(())
}

async fn perform_model_fetch_once_with_state<S>(
    state: &S,
) -> Result<ModelFetchRunSummary, GatewayError>
where
    S: ModelFetchRuntimeState + ?Sized,
{
    let targets = collect_fetch_targets(state, None, None).await?;
    execute_fetch_targets(state, targets, true).await
}

async fn perform_model_fetch_for_keys_with_state<S>(
    state: &S,
    provider_id: &str,
    key_ids: &BTreeSet<String>,
) -> Result<ModelFetchRunSummary, GatewayError>
where
    S: ModelFetchRuntimeState + ?Sized,
{
    let targets = collect_fetch_targets(state, Some(provider_id), Some(key_ids)).await?;
    execute_fetch_targets(state, targets, false).await
}

async fn collect_fetch_targets<S>(
    state: &S,
    provider_id_filter: Option<&str>,
    key_id_filter: Option<&BTreeSet<String>>,
) -> Result<Vec<SelectedFetchTarget>, GatewayError>
where
    S: ModelFetchRuntimeState + ?Sized,
{
    if !state.has_provider_catalog_data_reader() || !state.has_provider_catalog_data_writer() {
        return Ok(Vec::new());
    }

    let providers = state
        .list_provider_catalog_providers(true)
        .await?
        .into_iter()
        .filter(|provider| provider_id_filter.is_none_or(|provider_id| provider.id == provider_id))
        .collect::<Vec<_>>();
    if providers.is_empty() {
        return Ok(Vec::new());
    }

    let provider_ids = providers
        .iter()
        .map(|provider| provider.id.clone())
        .collect::<Vec<_>>();
    let mut endpoints_by_provider = HashMap::<String, Vec<StoredProviderCatalogEndpoint>>::new();
    for endpoint in state
        .list_provider_catalog_endpoints_by_provider_ids(&provider_ids)
        .await?
    {
        endpoints_by_provider
            .entry(endpoint.provider_id.clone())
            .or_default()
            .push(endpoint);
    }
    let mut keys_by_provider = HashMap::<String, Vec<StoredProviderCatalogKey>>::new();
    for key in <S as ModelFetchAssociationStore>::list_provider_catalog_keys_by_provider_ids(
        state,
        &provider_ids,
    )
    .await
    .map_err(GatewayError::Internal)?
    {
        keys_by_provider
            .entry(key.provider_id.clone())
            .or_default()
            .push(key);
    }

    let mut targets = Vec::new();
    for provider in providers {
        let endpoints = endpoints_by_provider
            .remove(&provider.id)
            .unwrap_or_default();
        let keys = keys_by_provider.remove(&provider.id).unwrap_or_default();
        for key in keys {
            if key_id_filter.is_some_and(|key_ids| !key_ids.contains(&key.id)) {
                continue;
            }
            if !key.is_active || !key.auto_fetch_models {
                continue;
            }
            let selected_endpoints = selected_models_fetch_endpoints(&endpoints, &key);
            targets.push(SelectedFetchTarget {
                provider: provider.clone(),
                key,
                endpoints: selected_endpoints,
            });
        }
    }
    Ok(targets)
}

async fn execute_fetch_targets<S>(
    state: &S,
    targets: Vec<SelectedFetchTarget>,
    reconcile_automatic_bindings: bool,
) -> Result<ModelFetchRunSummary, GatewayError>
where
    S: ModelFetchRuntimeState + ?Sized,
{
    let mut summary = ModelFetchRunSummary {
        attempted: targets.len(),
        succeeded: 0,
        failed: 0,
        skipped: 0,
    };
    let mut discovered_models_by_provider = BTreeMap::<String, Vec<Value>>::new();
    let mut provider_fetch_authority = BTreeMap::<String, (bool, BTreeSet<String>)>::new();
    let mut provider_failed_endpoint_ids = BTreeMap::<String, BTreeSet<String>>::new();
    for target in targets {
        let target_endpoint_ids = target
            .endpoints
            .iter()
            .map(|endpoint| endpoint.id.clone())
            .collect::<BTreeSet<_>>();
        let outcome = fetch_and_persist_key_models(state, &target).await?;
        match outcome.disposition {
            KeyFetchDisposition::Succeeded => summary.succeeded += 1,
            KeyFetchDisposition::Failed => summary.failed += 1,
            KeyFetchDisposition::Skipped => summary.skipped += 1,
        }
        if reconcile_automatic_bindings && outcome.disposition == KeyFetchDisposition::Succeeded {
            discovered_models_by_provider
                .entry(target.provider.id.clone())
                .or_default()
                .extend(outcome.discovered_models);
            let authority = provider_fetch_authority
                .entry(target.provider.id.clone())
                .or_default();
            if outcome.used_preset_catalog {
                authority.0 = true;
            } else {
                let successful_endpoint_ids = outcome
                    .successful_endpoint_ids
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                authority.1.extend(successful_endpoint_ids.iter().cloned());
                provider_failed_endpoint_ids
                    .entry(target.provider.id.clone())
                    .or_default()
                    .extend(
                        target_endpoint_ids
                            .difference(&successful_endpoint_ids)
                            .cloned(),
                    );
            }
        } else if reconcile_automatic_bindings {
            provider_failed_endpoint_ids
                .entry(target.provider.id.clone())
                .or_default()
                .extend(target_endpoint_ids);
        }
    }
    if reconcile_automatic_bindings {
        for (provider_id, models) in discovered_models_by_provider {
            let models = aggregate_models_for_cache(&models);
            let (has_preset_results, authoritative_endpoint_ids) = provider_fetch_authority
                .get(&provider_id)
                .cloned()
                .unwrap_or_default();
            let failed_endpoint_ids = provider_failed_endpoint_ids
                .get(&provider_id)
                .cloned()
                .unwrap_or_default();
            let authoritative_endpoint_ids = authoritative_endpoint_ids
                .difference(&failed_endpoint_ids)
                .cloned()
                .collect::<Vec<_>>();
            let replace_automatic_bindings =
                !authoritative_endpoint_ids.is_empty() && !has_preset_results;
            sync_provider_model_whitelist_associations(
                state,
                &provider_id,
                &[],
                &models,
                false,
                replace_automatic_bindings,
                &authoritative_endpoint_ids,
            )
            .await
            .map_err(GatewayError::Internal)?;
        }
    }
    Ok(summary)
}

async fn run_model_fetch_cycle<S>(state: &S, phase: &'static str) -> Result<(), GatewayError>
where
    S: ModelFetchRuntimeState + ?Sized,
{
    let summary = perform_model_fetch_once_with_state(state).await?;
    if summary.attempted == 0 {
        debug!(phase, "gateway model fetch found no eligible keys");
        return Ok(());
    }

    info!(
        phase,
        attempted = summary.attempted,
        succeeded = summary.succeeded,
        failed = summary.failed,
        skipped = summary.skipped,
        "gateway model fetch cycle completed"
    );
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyFetchDisposition {
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Debug)]
struct KeyFetchOutcome {
    disposition: KeyFetchDisposition,
    discovered_models: Vec<Value>,
    used_preset_catalog: bool,
    successful_endpoint_ids: Vec<String>,
}

impl KeyFetchOutcome {
    fn without_models(disposition: KeyFetchDisposition) -> Self {
        Self {
            disposition,
            discovered_models: Vec::new(),
            used_preset_catalog: false,
            successful_endpoint_ids: Vec::new(),
        }
    }
}

async fn fetch_and_persist_key_models(
    state: &(impl ModelFetchRuntimeState + ?Sized),
    target: &SelectedFetchTarget,
) -> Result<KeyFetchOutcome, GatewayError> {
    let now_unix_secs = now_unix_secs();
    if target.endpoints.is_empty() {
        if let Some(mut models) = preset_models_for_provider(&target.provider.provider_type) {
            let provider_endpoints = state
                .list_provider_catalog_endpoints_by_provider_ids(std::slice::from_ref(
                    &target.provider.id,
                ))
                .await?;
            attach_model_endpoint_ids(&mut models, &provider_endpoints);
            let fetched_model_ids = models
                .iter()
                .filter_map(|model| model.get("id"))
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            let filtered_models = apply_model_filters(
                &fetched_model_ids,
                json_string_list(target.key.locked_models.as_ref()),
                json_string_list(target.key.model_include_patterns.as_ref()),
                json_string_list(target.key.model_exclude_patterns.as_ref()),
            );
            let upstream_metadata =
                model_catalog_upstream_metadata(&target.provider.provider_type, &models);
            persist_key_fetch_success(
                state,
                &target.key,
                now_unix_secs,
                &filtered_models,
                upstream_metadata.as_ref(),
            )
            .await?;
            state
                .write_upstream_models_cache(&target.provider.id, &target.key.id, &models)
                .await;
            sync_provider_model_whitelist_associations(
                state,
                &target.provider.id,
                &filtered_models,
                &models,
                true,
                false,
                &[],
            )
            .await
            .map_err(GatewayError::Internal)?;
            return Ok(KeyFetchOutcome {
                disposition: KeyFetchDisposition::Succeeded,
                discovered_models: models,
                used_preset_catalog: true,
                successful_endpoint_ids: Vec::new(),
            });
        }
        persist_key_fetch_failure(
            state,
            &target.key,
            now_unix_secs,
            "No supported endpoint for Rust models fetch".to_string(),
        )
        .await?;
        return Ok(KeyFetchOutcome::without_models(
            KeyFetchDisposition::Skipped,
        ));
    }

    let mut transports = Vec::new();
    for endpoint in &target.endpoints {
        match state
            .read_provider_transport_snapshot(&target.provider.id, &endpoint.id, &target.key.id)
            .await?
        {
            Some(transport) => transports.push(transport),
            None => {
                warn!(
                    provider_id = %target.provider.id,
                    endpoint_id = %endpoint.id,
                    key_id = %target.key.id,
                    "gateway model fetch transport snapshot unavailable"
                );
            }
        }
    }

    if transports.is_empty() {
        persist_key_fetch_failure(
            state,
            &target.key,
            now_unix_secs,
            "Provider transport snapshot unavailable".to_string(),
        )
        .await?;
        return Ok(KeyFetchOutcome::without_models(
            KeyFetchDisposition::Skipped,
        ));
    }

    let result = match fetch_models_from_transports(state, &transports).await {
        Ok(result) => result,
        Err(err) => {
            persist_key_fetch_failure(state, &target.key, now_unix_secs, err.clone()).await?;
            warn!(
                provider_id = %target.provider.id,
                key_id = %target.key.id,
                message = %err,
                "gateway model fetch failed"
            );
            return Ok(KeyFetchOutcome::without_models(KeyFetchDisposition::Failed));
        }
    };

    if !result.has_success {
        let error = if result.errors.is_empty() {
            "Upstream models fetch failed".to_string()
        } else {
            result.errors.join("; ")
        };
        persist_key_fetch_failure(state, &target.key, now_unix_secs, error.clone()).await?;
        warn!(
            provider_id = %target.provider.id,
            key_id = %target.key.id,
            message = %error,
            "gateway model fetch failed"
        );
        return Ok(KeyFetchOutcome::without_models(KeyFetchDisposition::Failed));
    }

    let mut cached_models = result.cached_models;
    let used_preset_catalog = result.used_preset_catalog;
    let successful_endpoint_ids = result.successful_endpoint_ids;
    if used_preset_catalog {
        let provider_endpoints = state
            .list_provider_catalog_endpoints_by_provider_ids(std::slice::from_ref(
                &target.provider.id,
            ))
            .await?;
        attach_model_endpoint_ids(&mut cached_models, &provider_endpoints);
    }
    let filtered_models = apply_model_filters(
        &result.fetched_model_ids,
        json_string_list(target.key.locked_models.as_ref()),
        json_string_list(target.key.model_include_patterns.as_ref()),
        json_string_list(target.key.model_exclude_patterns.as_ref()),
    );
    persist_key_fetch_success(
        state,
        &target.key,
        now_unix_secs,
        &filtered_models,
        result.upstream_metadata.as_ref(),
    )
    .await?;
    state
        .write_upstream_models_cache(&target.provider.id, &target.key.id, &cached_models)
        .await;
    sync_provider_model_whitelist_associations(
        state,
        &target.provider.id,
        &filtered_models,
        &cached_models,
        false,
        false,
        &[],
    )
    .await
    .map_err(GatewayError::Internal)?;
    Ok(KeyFetchOutcome {
        disposition: KeyFetchDisposition::Succeeded,
        discovered_models: cached_models,
        used_preset_catalog,
        successful_endpoint_ids,
    })
}

fn attach_model_endpoint_ids(models: &mut [Value], endpoints: &[StoredProviderCatalogEndpoint]) {
    for model in models {
        if !json_string_list(model.get("endpoint_ids")).is_empty() {
            continue;
        }
        let formats = json_string_list(model.get("api_formats"))
            .into_iter()
            .map(|format| crate::ai_serving::normalize_api_format_alias(&format))
            .collect::<BTreeSet<_>>();
        if formats.is_empty() {
            continue;
        }
        let mut endpoint_ids = json_string_list(model.get("endpoint_ids"))
            .into_iter()
            .collect::<BTreeSet<_>>();
        endpoint_ids.extend(
            endpoints
                .iter()
                .filter(|endpoint| {
                    formats.contains(&crate::ai_serving::normalize_api_format_alias(
                        &endpoint.api_format,
                    ))
                })
                .map(|endpoint| endpoint.id.clone()),
        );
        if let Some(object) = model.as_object_mut() {
            object.insert("endpoint_ids".to_string(), json!(endpoint_ids));
        }
    }
}

async fn persist_key_fetch_failure(
    state: &(impl ModelFetchRuntimeState + ?Sized),
    key: &StoredProviderCatalogKey,
    now_unix_secs: u64,
    error: String,
) -> Result<(), GatewayError> {
    state
        .update_provider_catalog_key_model_fetch_state(
            &key.id,
            key.allowed_models.as_ref(),
            Some(now_unix_secs),
            Some(&error),
            Some(now_unix_secs),
        )
        .await?;
    Ok(())
}

async fn persist_key_fetch_success(
    state: &(impl ModelFetchRuntimeState + ?Sized),
    key: &StoredProviderCatalogKey,
    now_unix_secs: u64,
    allowed_models: &[String],
    upstream_metadata: Option<&Value>,
) -> Result<(), GatewayError> {
    // 自动获取成功后的空列表表示上游当前没有模型，不能降级成“不限模型”。
    let allowed_models = json!(allowed_models);
    let upstream_metadata_updates = upstream_metadata
        .map(|upstream_metadata| {
            upstream_metadata_namespace_updates(key.upstream_metadata.as_ref(), upstream_metadata)
                .into_iter()
                .map(
                    |(namespace, value)| ProviderCatalogUpstreamMetadataNamespaceUpdate {
                        namespace,
                        value,
                    },
                )
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    state
        .update_provider_catalog_key_model_fetch_success(
            &key.id,
            Some(&allowed_models),
            now_unix_secs,
            &upstream_metadata_updates,
            Some(now_unix_secs),
        )
        .await?;
    Ok(())
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::{perform_model_fetch_once_with_state, state::ModelFetchRuntimeState};
    use aether_contracts::{ExecutionPlan, ExecutionResult, ProxySnapshot};
    use aether_data_contracts::repository::global_models::{
        AdminGlobalModelListQuery, AdminProviderModelListQuery, StoredAdminGlobalModelPage,
        StoredAdminProviderModel, StoredModelEndpointBinding, UpsertAdminProviderModelRecord,
    };
    use aether_data_contracts::repository::provider_catalog::{
        ProviderCatalogUpstreamMetadataNamespaceUpdate, StoredProviderCatalogEndpoint,
        StoredProviderCatalogKey, StoredProviderCatalogProvider,
    };
    use aether_model_fetch::{
        build_models_fetch_execution_plan, ModelFetchAssociationStore, ModelFetchTransportRuntime,
    };
    use async_trait::async_trait;
    use serde_json::{json, Value};
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, Mutex};

    use crate::provider_transport::LocalResolvedOAuthRequestAuth;
    use crate::GatewayError;
    use aether_provider_transport::snapshot::{
        GatewayProviderTransportEndpoint, GatewayProviderTransportKey,
        GatewayProviderTransportProvider, GatewayProviderTransportSnapshot,
    };

    #[derive(Clone, Default)]
    struct TestState {
        providers: Arc<Vec<StoredProviderCatalogProvider>>,
        endpoints: Arc<Vec<StoredProviderCatalogEndpoint>>,
        keys: Arc<Mutex<Vec<StoredProviderCatalogKey>>>,
        transports: Arc<HashMap<(String, String, String), GatewayProviderTransportSnapshot>>,
        execution_results: Arc<Mutex<VecDeque<ExecutionResult>>>,
        executed_plans: Arc<Mutex<Vec<ExecutionPlan>>>,
        cached_models: Arc<Mutex<HashMap<(String, String), Vec<Value>>>>,
        provider_models: Arc<Mutex<Vec<StoredAdminProviderModel>>>,
        binding_syncs: Arc<Mutex<Vec<(String, Vec<String>, bool, Vec<String>)>>>,
        upstream_metadata_updates: Arc<Mutex<Vec<(String, String, Value, Option<u64>)>>>,
    }

    impl TestState {
        fn new(
            providers: Vec<StoredProviderCatalogProvider>,
            endpoints: Vec<StoredProviderCatalogEndpoint>,
            keys: Vec<StoredProviderCatalogKey>,
            transports: HashMap<(String, String, String), GatewayProviderTransportSnapshot>,
            execution_results: Vec<ExecutionResult>,
        ) -> Self {
            Self {
                providers: Arc::new(providers),
                endpoints: Arc::new(endpoints),
                keys: Arc::new(Mutex::new(keys)),
                transports: Arc::new(transports),
                execution_results: Arc::new(Mutex::new(VecDeque::from(execution_results))),
                executed_plans: Arc::new(Mutex::new(Vec::new())),
                cached_models: Arc::new(Mutex::new(HashMap::new())),
                provider_models: Arc::new(Mutex::new(Vec::new())),
                binding_syncs: Arc::new(Mutex::new(Vec::new())),
                upstream_metadata_updates: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn with_provider_models(self, models: Vec<StoredAdminProviderModel>) -> Self {
            *self.provider_models.lock().expect("provider models mutex") = models;
            self
        }

        fn key(&self, key_id: &str) -> StoredProviderCatalogKey {
            self.keys
                .lock()
                .expect("keys mutex")
                .iter()
                .find(|key| key.id == key_id)
                .cloned()
                .expect("key should exist")
        }
    }

    #[async_trait]
    impl ModelFetchTransportRuntime for TestState {
        async fn resolve_local_oauth_request_auth(
            &self,
            transport: &GatewayProviderTransportSnapshot,
        ) -> Result<Option<LocalResolvedOAuthRequestAuth>, String> {
            if transport.key.auth_type.trim().eq_ignore_ascii_case("oauth") {
                return Ok(Some(LocalResolvedOAuthRequestAuth::Header {
                    name: "authorization".to_string(),
                    value: "Bearer oauth-token".to_string(),
                }));
            }
            Ok(None)
        }

        async fn resolve_model_fetch_proxy(
            &self,
            _transport: &GatewayProviderTransportSnapshot,
        ) -> Option<ProxySnapshot> {
            None
        }

        async fn execute_model_fetch_execution_plan(
            &self,
            plan: &ExecutionPlan,
        ) -> Result<ExecutionResult, String> {
            self.executed_plans
                .lock()
                .expect("executed plans mutex")
                .push(plan.clone());
            self.execution_results
                .lock()
                .expect("execution result mutex")
                .pop_front()
                .ok_or_else(|| "missing execution result".to_string())
        }
    }

    #[async_trait]
    impl ModelFetchAssociationStore for TestState {
        type Error = String;

        fn has_global_model_reader(&self) -> bool {
            !self
                .provider_models
                .lock()
                .expect("provider models mutex")
                .is_empty()
        }

        fn has_global_model_writer(&self) -> bool {
            !self
                .provider_models
                .lock()
                .expect("provider models mutex")
                .is_empty()
        }

        fn model_fetch_internal_error(&self, message: String) -> Self::Error {
            message
        }

        async fn list_admin_provider_models(
            &self,
            query: &AdminProviderModelListQuery,
        ) -> Result<Vec<StoredAdminProviderModel>, Self::Error> {
            Ok(self
                .provider_models
                .lock()
                .expect("provider models mutex")
                .iter()
                .filter(|model| model.provider_id == query.provider_id)
                .cloned()
                .collect())
        }

        async fn list_admin_global_models(
            &self,
            _query: &AdminGlobalModelListQuery,
        ) -> Result<StoredAdminGlobalModelPage, Self::Error> {
            Ok(StoredAdminGlobalModelPage {
                items: Vec::new(),
                total: 0,
            })
        }

        async fn create_admin_provider_model_with_bindings(
            &self,
            _record: &aether_data_contracts::repository::global_models::CreateAdminProviderModelWithBindingsRecord,
        ) -> Result<Option<StoredAdminProviderModel>, Self::Error> {
            Ok(None)
        }

        async fn create_admin_provider_model(
            &self,
            _record: &UpsertAdminProviderModelRecord,
        ) -> Result<Option<StoredAdminProviderModel>, Self::Error> {
            Ok(None)
        }

        async fn update_admin_provider_model(
            &self,
            _record: &UpsertAdminProviderModelRecord,
        ) -> Result<Option<StoredAdminProviderModel>, Self::Error> {
            Ok(None)
        }

        async fn sync_model_endpoint_bindings(
            &self,
            model_id: &str,
            endpoint_ids: &[String],
            _source: &str,
            replace_automatic: bool,
            replacement_scope_endpoint_ids: &[String],
        ) -> Result<Vec<StoredModelEndpointBinding>, Self::Error> {
            self.binding_syncs
                .lock()
                .expect("binding syncs mutex")
                .push((
                    model_id.to_string(),
                    endpoint_ids.to_vec(),
                    replace_automatic,
                    replacement_scope_endpoint_ids.to_vec(),
                ));
            Ok(Vec::new())
        }

        async fn list_provider_catalog_keys_by_provider_ids(
            &self,
            provider_ids: &[String],
        ) -> Result<Vec<StoredProviderCatalogKey>, Self::Error> {
            Ok(self
                .keys
                .lock()
                .expect("keys mutex")
                .iter()
                .filter(|key| {
                    provider_ids
                        .iter()
                        .any(|provider_id| provider_id == &key.provider_id)
                })
                .cloned()
                .collect())
        }
    }

    #[async_trait]
    impl ModelFetchRuntimeState for TestState {
        fn has_provider_catalog_data_reader(&self) -> bool {
            true
        }

        fn has_provider_catalog_data_writer(&self) -> bool {
            true
        }

        async fn list_provider_catalog_providers(
            &self,
            _active_only: bool,
        ) -> Result<Vec<StoredProviderCatalogProvider>, GatewayError> {
            Ok(self.providers.as_ref().clone())
        }

        async fn list_provider_catalog_endpoints_by_provider_ids(
            &self,
            provider_ids: &[String],
        ) -> Result<Vec<StoredProviderCatalogEndpoint>, GatewayError> {
            Ok(self
                .endpoints
                .iter()
                .filter(|endpoint| {
                    provider_ids
                        .iter()
                        .any(|provider_id| provider_id == &endpoint.provider_id)
                })
                .cloned()
                .collect())
        }

        async fn read_provider_transport_snapshot(
            &self,
            provider_id: &str,
            endpoint_id: &str,
            key_id: &str,
        ) -> Result<Option<GatewayProviderTransportSnapshot>, GatewayError> {
            Ok(self
                .transports
                .get(&(
                    provider_id.to_string(),
                    endpoint_id.to_string(),
                    key_id.to_string(),
                ))
                .cloned())
        }

        async fn execute_execution_runtime_sync_plan(
            &self,
            _plan: &ExecutionPlan,
        ) -> Result<ExecutionResult, GatewayError> {
            Err(GatewayError::Internal(
                "execute_execution_runtime_sync_plan should not be called".to_string(),
            ))
        }

        async fn update_provider_catalog_key_model_fetch_state(
            &self,
            key_id: &str,
            allowed_models: Option<&Value>,
            last_models_fetch_at_unix_secs: Option<u64>,
            last_models_fetch_error: Option<&str>,
            updated_at_unix_secs: Option<u64>,
        ) -> Result<(), GatewayError> {
            let mut keys = self.keys.lock().expect("keys mutex");
            let Some(key) = keys.iter_mut().find(|item| item.id == key_id) else {
                return Err(GatewayError::Internal("key not found".to_string()));
            };
            key.allowed_models = allowed_models.cloned();
            key.last_models_fetch_at_unix_secs = last_models_fetch_at_unix_secs;
            key.last_models_fetch_error = last_models_fetch_error.map(str::to_string);
            key.updated_at_unix_secs = updated_at_unix_secs;
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
            let mut keys = self.keys.lock().expect("keys mutex");
            let Some(key) = keys.iter_mut().find(|key| key.id == key_id) else {
                return Err(GatewayError::Internal("key not found".to_string()));
            };
            key.allowed_models = allowed_models.cloned();
            key.last_models_fetch_at_unix_secs = Some(last_models_fetch_at_unix_secs);
            key.last_models_fetch_error = None;
            if !upstream_metadata_updates.is_empty() {
                let metadata = key
                    .upstream_metadata
                    .get_or_insert_with(|| json!({}))
                    .as_object_mut()
                    .expect("upstream metadata object");
                for update in upstream_metadata_updates {
                    metadata.insert(update.namespace.clone(), update.value.clone());
                }
            }
            key.updated_at_unix_secs = updated_at_unix_secs;
            drop(keys);
            self.upstream_metadata_updates
                .lock()
                .expect("metadata updates mutex")
                .extend(upstream_metadata_updates.iter().map(|update| {
                    (
                        key_id.to_string(),
                        update.namespace.clone(),
                        update.value.clone(),
                        updated_at_unix_secs,
                    )
                }));
            Ok(())
        }

        async fn write_upstream_models_cache(
            &self,
            provider_id: &str,
            key_id: &str,
            cached_models: &[Value],
        ) {
            self.cached_models.lock().expect("cache mutex").insert(
                (provider_id.to_string(), key_id.to_string()),
                cached_models.to_vec(),
            );
        }
    }

    fn sample_provider(provider_id: &str, provider_type: &str) -> StoredProviderCatalogProvider {
        StoredProviderCatalogProvider::new(
            provider_id.to_string(),
            provider_id.to_string(),
            None,
            provider_type.to_string(),
        )
        .expect("provider should build")
        .with_transport_fields(true, false, false, None, None, None, None, None, None)
    }

    fn sample_endpoint(
        endpoint_id: &str,
        provider_id: &str,
        api_format: &str,
    ) -> StoredProviderCatalogEndpoint {
        StoredProviderCatalogEndpoint::new(
            endpoint_id.to_string(),
            provider_id.to_string(),
            api_format.to_string(),
            None,
            None,
            true,
        )
        .expect("endpoint should build")
        .with_transport_fields(
            "https://cloudcode-pa.googleapis.com".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("endpoint transport should build")
    }

    fn sample_key(
        key_id: &str,
        provider_id: &str,
        auth_type: &str,
        api_formats: &[&str],
    ) -> StoredProviderCatalogKey {
        let mut key = StoredProviderCatalogKey::new(
            key_id.to_string(),
            provider_id.to_string(),
            "primary".to_string(),
            auth_type.to_string(),
            None,
            true,
        )
        .expect("key should build")
        .with_transport_fields(
            Some(json!(api_formats)),
            "encrypted".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("key transport should build");
        key.auto_fetch_models = true;
        key
    }

    fn sample_transport(
        provider_type: &str,
        provider_id: &str,
        endpoint_id: &str,
        key_id: &str,
        api_format: &str,
        auth_type: &str,
        decrypted_auth_config: Option<&str>,
    ) -> GatewayProviderTransportSnapshot {
        GatewayProviderTransportSnapshot {
            provider: GatewayProviderTransportProvider {
                id: provider_id.to_string(),
                name: provider_id.to_string(),
                provider_type: provider_type.to_string(),
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
                id: endpoint_id.to_string(),
                provider_id: provider_id.to_string(),
                api_format: api_format.to_string(),
                api_family: None,
                endpoint_kind: None,
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
                id: key_id.to_string(),
                provider_id: provider_id.to_string(),
                name: "primary".to_string(),
                auth_type: auth_type.to_string(),
                is_active: true,
                api_formats: Some(vec![api_format.to_string()]),
                auth_type_by_format: None,
                allow_auth_channel_mismatch_formats: None,
                allowed_models: None,
                capabilities: None,
                rate_multipliers: None,
                global_priority_by_format: None,
                expires_at_unix_secs: None,
                proxy: None,
                fingerprint: None,
                upstream_metadata: None,
                decrypted_api_key: "secret".to_string(),
                decrypted_auth_config: decrypted_auth_config.map(ToOwned::to_owned),
            },
        }
    }

    fn execution_result(body: Value) -> ExecutionResult {
        ExecutionResult {
            request_id: "req-1".to_string(),
            candidate_id: None,
            status_code: 200,
            headers: Default::default(),
            body: Some(aether_contracts::ResponseBody {
                json_body: Some(body),
                body_bytes_b64: None,
            }),
            telemetry: None,
            error: None,
        }
    }

    fn execution_error_result(status_code: u16, body: Value) -> ExecutionResult {
        let mut result = execution_result(body);
        result.status_code = status_code;
        result
    }

    fn sample_provider_model(
        id: &str,
        provider_id: &str,
        provider_model_name: &str,
    ) -> StoredAdminProviderModel {
        StoredAdminProviderModel::new(
            id.to_string(),
            provider_id.to_string(),
            format!("global-{id}"),
            provider_model_name.to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            true,
            None,
            None,
            None,
            Some(provider_model_name.to_string()),
            None,
            None,
            None,
            None,
            None,
        )
        .expect("provider model should build")
    }

    fn sample_provider_model_with_alias(
        id: &str,
        provider_id: &str,
        provider_model_name: &str,
        alias: &str,
    ) -> StoredAdminProviderModel {
        let mut model = sample_provider_model(id, provider_id, provider_model_name);
        model.provider_model_mappings = Some(json!([{ "name": alias }]));
        model
    }

    #[tokio::test]
    async fn gateway_runtime_state_supports_shared_models_fetch_plan_builder() {
        let state = TestState::default();
        let transport = sample_transport(
            "openai",
            "provider-openai",
            "endpoint-openai-chat",
            "key-openai-chat",
            "openai:chat",
            "api_key",
            None,
        );

        let plan = build_models_fetch_execution_plan(&state, &transport)
            .await
            .expect("shared models fetch plan should build");

        assert_eq!(plan.method, "GET");
        assert_eq!(plan.provider_id, "provider-openai");
        assert_eq!(plan.endpoint_id, "endpoint-openai-chat");
        assert_eq!(plan.key_id, "key-openai-chat");
        assert_eq!(plan.model_name.as_deref(), Some("models"));
    }

    #[tokio::test]
    async fn model_fetch_uses_preset_models_without_endpoint() {
        let provider = sample_provider("provider-codex", "codex");
        let mut key = sample_key(
            "key-codex",
            "provider-codex",
            "api_key",
            &["openai:responses"],
        );
        key.upstream_metadata = Some(json!({
            "codex": {
                "quota_by_model": {
                    "gpt-5.6-sol": {"remaining_fraction": 0.75}
                }
            }
        }));
        let state = TestState::new(vec![provider], vec![], vec![key], HashMap::new(), vec![]);

        let summary = perform_model_fetch_once_with_state(&state)
            .await
            .expect("fetch should succeed");

        assert_eq!(summary.attempted, 1);
        assert_eq!(summary.succeeded, 1);
        let updated = state.key("key-codex");
        let allowed_models = updated
            .allowed_models
            .as_ref()
            .and_then(|value| value.as_array().cloned())
            .expect("allowed_models should be set");
        assert!(allowed_models.iter().any(|model| model == "gpt-5.4"));
        let upstream_metadata = updated
            .upstream_metadata
            .as_ref()
            .expect("Codex model catalog should be persisted");
        assert_eq!(
            upstream_metadata["codex"]["quota_by_model"]["gpt-5.6-sol"]["remaining_fraction"],
            0.75
        );
        assert_eq!(
            upstream_metadata["codex_models"]["cards"]["gpt-5.6-sol"]["multi_agent_version"],
            "v2"
        );
        let capabilities = crate::ai_serving::resolve_codex_responses_model_capabilities(
            "gpt-5.6-sol",
            "gpt-5.6-sol",
            Some(upstream_metadata),
        );
        assert!(capabilities.use_responses_lite);
        assert_eq!(
            capabilities.default_reasoning_effort.as_deref(),
            Some("low")
        );
        assert!(capabilities
            .supported_reasoning_efforts
            .iter()
            .any(|effort| effort == "ultra"));
        let metadata_updates = state
            .upstream_metadata_updates
            .lock()
            .expect("metadata updates mutex");
        assert_eq!(metadata_updates.len(), 1);
        assert_eq!(metadata_updates[0].0, "key-codex");
        assert_eq!(metadata_updates[0].1, "codex_models");
        assert_eq!(
            metadata_updates[0].2["cards"]["gpt-5.6-sol"]["multi_agent_version"],
            "v2"
        );
        assert!(state
            .cached_models
            .lock()
            .expect("cache mutex")
            .contains_key(&("provider-codex".to_string(), "key-codex".to_string())));
    }

    #[tokio::test]
    async fn model_fetch_merges_antigravity_metadata_and_preserves_reset_time() {
        let provider = sample_provider("provider-antigravity", "antigravity");
        let endpoint = sample_endpoint(
            "endpoint-antigravity",
            "provider-antigravity",
            "gemini:generate_content",
        );
        let mut key = sample_key(
            "key-antigravity",
            "provider-antigravity",
            "oauth",
            &["gemini:generate_content"],
        );
        key.upstream_metadata = Some(json!({
            "antigravity": {
                "quota_by_model": {
                    "gemini-2.5-pro": {
                        "reset_time": "2026-04-12T00:00:00Z"
                    }
                }
            }
        }));
        let transport = sample_transport(
            "antigravity",
            "provider-antigravity",
            "endpoint-antigravity",
            "key-antigravity",
            "gemini:generate_content",
            "oauth",
            Some(r#"{"project_id":"project-1","client_version":"1.2.3","session_id":"sess-1"}"#),
        );
        let state = TestState::new(
            vec![provider],
            vec![endpoint],
            vec![key],
            HashMap::from([(
                (
                    "provider-antigravity".to_string(),
                    "endpoint-antigravity".to_string(),
                    "key-antigravity".to_string(),
                ),
                transport,
            )]),
            vec![execution_result(json!({
                "models": {
                    "gemini-2.5-pro": {
                        "displayName": "Gemini 2.5 Pro",
                        "quotaInfo": {
                            "remainingFraction": 0.25
                        }
                    }
                }
            }))],
        );

        let summary = perform_model_fetch_once_with_state(&state)
            .await
            .expect("fetch should succeed");

        assert_eq!(summary.succeeded, 1);
        let updated = state.key("key-antigravity");
        assert_eq!(updated.allowed_models, Some(json!(["gemini-2.5-pro"])));
        assert_eq!(
            updated
                .upstream_metadata
                .as_ref()
                .and_then(|value| value.get("antigravity"))
                .and_then(|value| value.get("quota_by_model"))
                .and_then(|value| value.get("gemini-2.5-pro"))
                .and_then(|value| value.get("reset_time")),
            Some(&json!("2026-04-12T00:00:00Z"))
        );
    }

    #[tokio::test]
    async fn model_fetch_fetches_windsurf_model_configs_and_persists_allowed_models() {
        let provider = sample_provider("provider-windsurf", "windsurf");
        let endpoint = StoredProviderCatalogEndpoint::new(
            "endpoint-windsurf-chat".to_string(),
            "provider-windsurf".to_string(),
            "openai:chat".to_string(),
            None,
            None,
            true,
        )
        .expect("endpoint should build")
        .with_transport_fields(
            "https://server.codeium.com".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("endpoint transport should build");
        let key = sample_key(
            "key-windsurf",
            "provider-windsurf",
            "api_key",
            &["openai:chat"],
        );
        let mut transport = sample_transport(
            "windsurf",
            "provider-windsurf",
            "endpoint-windsurf-chat",
            "key-windsurf",
            "openai:chat",
            "api_key",
            Some(r#"{"provider_type":"windsurf"}"#),
        );
        transport.endpoint.base_url = "https://server.codeium.com".to_string();
        transport.key.decrypted_api_key = "devin-session-token$abc".to_string();
        let state = TestState::new(
            vec![provider],
            vec![endpoint],
            vec![key],
            HashMap::from([(
                (
                    "provider-windsurf".to_string(),
                    "endpoint-windsurf-chat".to_string(),
                    "key-windsurf".to_string(),
                ),
                transport,
            )]),
            vec![execution_result(json!({
                "clientModelConfigs": [
                    {
                        "modelUid": "claude-sonnet-4-6",
                        "label": "Claude Sonnet 4.6",
                        "provider": "anthropic",
                        "supportsImages": true,
                        "creditMultiplier": 4
                    },
                    {
                        "modelUid": "gpt-5.4",
                        "label": "GPT-5.4",
                        "provider": "openai"
                    }
                ],
                "defaultOverrideModelConfig": {
                    "modelUid": "claude-sonnet-4-6"
                }
            }))],
        );

        let summary = perform_model_fetch_once_with_state(&state)
            .await
            .expect("fetch should succeed");

        assert_eq!(summary.succeeded, 1);
        let plans = state.executed_plans.lock().expect("executed plans mutex");
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].url,
            "https://server.codeium.com/exa.api_server_pb.ApiServerService/GetCascadeModelConfigs"
        );
        assert_eq!(plans[0].method, "POST");
        assert_eq!(plans[0].provider_api_format, "windsurf:model_configs");
        assert_eq!(
            plans[0]
                .body
                .json_body
                .as_ref()
                .and_then(|body| body.get("metadata"))
                .and_then(|metadata| metadata.get("apiKey")),
            Some(&json!("devin-session-token$abc"))
        );
        drop(plans);

        let updated = state.key("key-windsurf");
        assert_eq!(
            updated.allowed_models,
            Some(json!(["claude-sonnet-4-6", "gpt-5.4"]))
        );
        assert_eq!(
            updated
                .upstream_metadata
                .as_ref()
                .and_then(|value| value.get("windsurf"))
                .and_then(|value| value.get("allowed_models_count")),
            Some(&json!(2))
        );
        assert_eq!(
            updated
                .upstream_metadata
                .as_ref()
                .and_then(|value| value.get("windsurf"))
                .and_then(|value| value.get("default_model_uid")),
            Some(&json!("claude-sonnet-4-6"))
        );
        let cached = state.cached_models.lock().expect("cache mutex");
        let cached_models = cached
            .get(&("provider-windsurf".to_string(), "key-windsurf".to_string()))
            .expect("cached models should be written");
        assert_eq!(
            cached_models[0]["api_formats"],
            json!(["openai:chat", "openai:responses", "claude:messages"])
        );
    }

    #[tokio::test]
    async fn model_fetch_failure_keeps_existing_allowed_models() {
        let provider = sample_provider("provider-openai", "openai");
        let endpoint = sample_endpoint(
            "endpoint-openai-responses",
            "provider-openai",
            "openai:responses",
        );
        let mut key = sample_key(
            "key-openai-responses",
            "provider-openai",
            "api_key",
            &["openai:responses"],
        );
        key.allowed_models = Some(json!(["gpt-old"]));
        let state = TestState::new(
            vec![provider],
            vec![endpoint],
            vec![key],
            HashMap::new(),
            vec![],
        );

        let summary = perform_model_fetch_once_with_state(&state)
            .await
            .expect("fetch should finish");

        assert_eq!(summary.succeeded, 0);
        assert_eq!(summary.skipped, 1);
        let updated = state.key("key-openai-responses");
        assert_eq!(updated.allowed_models, Some(json!(["gpt-old"])));
        assert_eq!(
            updated.last_models_fetch_error.as_deref(),
            Some("Provider transport snapshot unavailable")
        );
    }

    #[tokio::test]
    async fn model_fetch_success_with_empty_catalog_persists_empty_whitelist() {
        let provider = sample_provider("provider-openai", "openai");
        let endpoint = sample_endpoint("endpoint-openai-chat", "provider-openai", "openai:chat");
        let key = sample_key(
            "key-openai-chat",
            "provider-openai",
            "api_key",
            &["openai:chat"],
        );
        let transport = sample_transport(
            "openai",
            "provider-openai",
            "endpoint-openai-chat",
            "key-openai-chat",
            "openai:chat",
            "api_key",
            None,
        );
        let state = TestState::new(
            vec![provider],
            vec![endpoint],
            vec![key],
            HashMap::from([(
                (
                    "provider-openai".to_string(),
                    "endpoint-openai-chat".to_string(),
                    "key-openai-chat".to_string(),
                ),
                transport,
            )]),
            vec![execution_result(json!({ "data": [] }))],
        );

        let summary = perform_model_fetch_once_with_state(&state)
            .await
            .expect("empty upstream catalog should still be a successful fetch");

        assert_eq!(summary.succeeded, 1);
        let updated = state.key("key-openai-chat");
        assert_eq!(updated.allowed_models, Some(json!([])));
        assert_eq!(updated.last_models_fetch_error, None);
    }

    #[tokio::test]
    async fn full_model_fetch_empty_catalog_reconciles_successful_endpoint_only() {
        let provider = sample_provider("provider-empty", "custom");
        let successful_endpoint =
            sample_endpoint("endpoint-empty", "provider-empty", "openai:chat");
        let failed_endpoint =
            sample_endpoint("endpoint-failed", "provider-empty", "claude:messages");
        let key = sample_key(
            "key-empty",
            "provider-empty",
            "api_key",
            &["openai:chat", "claude:messages"],
        );
        let state = TestState::new(
            vec![provider],
            vec![successful_endpoint, failed_endpoint],
            vec![key],
            HashMap::from([
                (
                    (
                        "provider-empty".to_string(),
                        "endpoint-empty".to_string(),
                        "key-empty".to_string(),
                    ),
                    sample_transport(
                        "custom",
                        "provider-empty",
                        "endpoint-empty",
                        "key-empty",
                        "openai:chat",
                        "api_key",
                        None,
                    ),
                ),
                (
                    (
                        "provider-empty".to_string(),
                        "endpoint-failed".to_string(),
                        "key-empty".to_string(),
                    ),
                    sample_transport(
                        "custom",
                        "provider-empty",
                        "endpoint-failed",
                        "key-empty",
                        "claude:messages",
                        "api_key",
                        None,
                    ),
                ),
            ]),
            vec![
                execution_result(json!({ "data": [] })),
                execution_error_result(503, json!({"error": {"message": "catalog unavailable"}})),
            ],
        )
        .with_provider_models(vec![sample_provider_model(
            "model-stale",
            "provider-empty",
            "stale-model",
        )]);

        let summary = perform_model_fetch_once_with_state(&state)
            .await
            .expect("empty authoritative catalog should reconcile bindings");

        assert_eq!(summary.succeeded, 1);
        let syncs = state.binding_syncs.lock().expect("binding syncs mutex");
        assert!(syncs
            .iter()
            .any(|(model_id, endpoint_ids, replace, scope)| {
                model_id == "model-stale"
                    && endpoint_ids.is_empty()
                    && *replace
                    && scope == &["endpoint-empty".to_string()]
            }));
        assert!(!syncs.iter().any(|(_, _, replace, scope)| {
            *replace
                && scope
                    .iter()
                    .any(|endpoint_id| endpoint_id == "endpoint-failed")
        }));
    }

    #[tokio::test]
    async fn full_model_fetch_reconciles_bindings_from_each_successful_endpoint_only() {
        let provider = sample_provider("provider-mixed", "custom");
        let openai_endpoint = sample_endpoint("endpoint-openai", "provider-mixed", "openai:chat");
        let claude_endpoint =
            sample_endpoint("endpoint-claude", "provider-mixed", "claude:messages");
        let key = sample_key(
            "key-mixed",
            "provider-mixed",
            "api_key",
            &["openai:chat", "claude:messages"],
        );
        let openai_transport = sample_transport(
            "custom",
            "provider-mixed",
            "endpoint-openai",
            "key-mixed",
            "openai:chat",
            "api_key",
            None,
        );
        let claude_transport = sample_transport(
            "custom",
            "provider-mixed",
            "endpoint-claude",
            "key-mixed",
            "claude:messages",
            "api_key",
            None,
        );
        let state = TestState::new(
            vec![provider],
            vec![openai_endpoint, claude_endpoint],
            vec![key],
            HashMap::from([
                (
                    (
                        "provider-mixed".to_string(),
                        "endpoint-openai".to_string(),
                        "key-mixed".to_string(),
                    ),
                    openai_transport,
                ),
                (
                    (
                        "provider-mixed".to_string(),
                        "endpoint-claude".to_string(),
                        "key-mixed".to_string(),
                    ),
                    claude_transport,
                ),
            ]),
            vec![
                execution_result(json!({"data": [{"id": "gpt-5"}]})),
                execution_error_result(
                    503,
                    json!({"error": {"message": "claude catalog unavailable"}}),
                ),
            ],
        )
        .with_provider_models(vec![
            sample_provider_model("model-gpt", "provider-mixed", "gpt-5"),
            sample_provider_model("model-claude", "provider-mixed", "claude-opus"),
        ]);

        let summary = perform_model_fetch_once_with_state(&state)
            .await
            .expect("full fetch should succeed");

        assert_eq!(summary.succeeded, 1);
        let syncs = state.binding_syncs.lock().expect("binding syncs mutex");
        assert!(syncs
            .iter()
            .any(|(model_id, endpoint_ids, replace, scope)| {
                model_id == "model-gpt"
                    && endpoint_ids == &["endpoint-openai".to_string()]
                    && *replace
                    && scope == &["endpoint-openai".to_string()]
            }));
        assert!(syncs
            .iter()
            .any(|(model_id, endpoint_ids, replace, scope)| {
                model_id == "model-claude"
                    && endpoint_ids.is_empty()
                    && *replace
                    && scope == &["endpoint-openai".to_string()]
            }));
    }

    #[tokio::test]
    async fn full_model_fetch_does_not_replace_an_endpoint_when_another_key_fails() {
        let provider = sample_provider("provider-shared", "custom");
        let endpoint = sample_endpoint("endpoint-shared", "provider-shared", "openai:chat");
        let successful_key = sample_key(
            "key-success",
            "provider-shared",
            "api_key",
            &["openai:chat"],
        );
        let failed_key = sample_key("key-failed", "provider-shared", "api_key", &["openai:chat"]);
        let state = TestState::new(
            vec![provider],
            vec![endpoint],
            vec![successful_key, failed_key],
            HashMap::from([
                (
                    (
                        "provider-shared".to_string(),
                        "endpoint-shared".to_string(),
                        "key-success".to_string(),
                    ),
                    sample_transport(
                        "custom",
                        "provider-shared",
                        "endpoint-shared",
                        "key-success",
                        "openai:chat",
                        "api_key",
                        None,
                    ),
                ),
                (
                    (
                        "provider-shared".to_string(),
                        "endpoint-shared".to_string(),
                        "key-failed".to_string(),
                    ),
                    sample_transport(
                        "custom",
                        "provider-shared",
                        "endpoint-shared",
                        "key-failed",
                        "openai:chat",
                        "api_key",
                        None,
                    ),
                ),
            ]),
            vec![
                execution_result(json!({"data": [{"id": "gpt-5"}]})),
                execution_error_result(503, json!({"error": {"message": "catalog unavailable"}})),
            ],
        )
        .with_provider_models(vec![sample_provider_model(
            "model-gpt",
            "provider-shared",
            "gpt-5",
        )]);

        let summary = perform_model_fetch_once_with_state(&state)
            .await
            .expect("full fetch should finish");

        assert_eq!(summary.succeeded, 1);
        assert_eq!(summary.failed, 1);
        let syncs = state.binding_syncs.lock().expect("binding syncs mutex");
        assert!(syncs
            .iter()
            .any(|(model_id, endpoint_ids, replace, scope)| {
                model_id == "model-gpt"
                    && endpoint_ids == &["endpoint-shared".to_string()]
                    && !replace
                    && scope.is_empty()
            }));
        assert!(!syncs.iter().any(|(_, _, replace, _)| *replace));
    }

    #[tokio::test]
    async fn full_model_fetch_does_not_replace_partially_failed_endpoint_from_another_key() {
        let provider = sample_provider("provider-partial", "custom");
        let openai_endpoint = sample_endpoint("endpoint-openai", "provider-partial", "openai:chat");
        let claude_endpoint =
            sample_endpoint("endpoint-claude", "provider-partial", "claude:messages");
        let mixed_key = sample_key(
            "key-mixed",
            "provider-partial",
            "api_key",
            &["openai:chat", "claude:messages"],
        );
        let claude_key = sample_key(
            "key-claude",
            "provider-partial",
            "api_key",
            &["claude:messages"],
        );
        let state = TestState::new(
            vec![provider],
            vec![openai_endpoint, claude_endpoint],
            vec![mixed_key, claude_key],
            HashMap::from([
                (
                    (
                        "provider-partial".to_string(),
                        "endpoint-openai".to_string(),
                        "key-mixed".to_string(),
                    ),
                    sample_transport(
                        "custom",
                        "provider-partial",
                        "endpoint-openai",
                        "key-mixed",
                        "openai:chat",
                        "api_key",
                        None,
                    ),
                ),
                (
                    (
                        "provider-partial".to_string(),
                        "endpoint-claude".to_string(),
                        "key-mixed".to_string(),
                    ),
                    sample_transport(
                        "custom",
                        "provider-partial",
                        "endpoint-claude",
                        "key-mixed",
                        "claude:messages",
                        "api_key",
                        None,
                    ),
                ),
                (
                    (
                        "provider-partial".to_string(),
                        "endpoint-claude".to_string(),
                        "key-claude".to_string(),
                    ),
                    sample_transport(
                        "custom",
                        "provider-partial",
                        "endpoint-claude",
                        "key-claude",
                        "claude:messages",
                        "api_key",
                        None,
                    ),
                ),
            ]),
            vec![
                execution_result(json!({"data": [{"id": "gpt-5"}]})),
                execution_error_result(
                    503,
                    json!({"error": {"message": "claude catalog unavailable"}}),
                ),
                execution_result(json!({"data": [{"id": "claude-opus"}]})),
            ],
        )
        .with_provider_models(vec![
            sample_provider_model("model-gpt", "provider-partial", "gpt-5"),
            sample_provider_model("model-claude", "provider-partial", "claude-opus"),
        ]);

        let summary = perform_model_fetch_once_with_state(&state)
            .await
            .expect("full fetch should finish");

        assert_eq!(summary.succeeded, 2);
        let syncs = state.binding_syncs.lock().expect("binding syncs mutex");
        assert!(syncs
            .iter()
            .any(|(model_id, endpoint_ids, replace, scope)| {
                model_id == "model-gpt"
                    && endpoint_ids == &["endpoint-openai".to_string()]
                    && *replace
                    && scope == &["endpoint-openai".to_string()]
            }));
        assert!(!syncs.iter().any(|(_, _, replace, scope)| {
            *replace
                && scope
                    .iter()
                    .any(|endpoint_id| endpoint_id == "endpoint-claude")
        }));
    }

    #[tokio::test]
    async fn full_model_fetch_merges_endpoint_bindings_across_model_aliases() {
        let provider = sample_provider("provider-alias", "custom");
        let first_endpoint = sample_endpoint("endpoint-primary", "provider-alias", "openai:chat");
        let second_endpoint =
            sample_endpoint("endpoint-alias", "provider-alias", "claude:messages");
        let key = sample_key(
            "key-alias",
            "provider-alias",
            "api_key",
            &["openai:chat", "claude:messages"],
        );
        let state = TestState::new(
            vec![provider],
            vec![first_endpoint, second_endpoint],
            vec![key],
            HashMap::from([
                (
                    (
                        "provider-alias".to_string(),
                        "endpoint-primary".to_string(),
                        "key-alias".to_string(),
                    ),
                    sample_transport(
                        "custom",
                        "provider-alias",
                        "endpoint-primary",
                        "key-alias",
                        "openai:chat",
                        "api_key",
                        None,
                    ),
                ),
                (
                    (
                        "provider-alias".to_string(),
                        "endpoint-alias".to_string(),
                        "key-alias".to_string(),
                    ),
                    sample_transport(
                        "custom",
                        "provider-alias",
                        "endpoint-alias",
                        "key-alias",
                        "claude:messages",
                        "api_key",
                        None,
                    ),
                ),
            ]),
            vec![
                execution_result(json!({"data": [{"id": "primary-name"}]})),
                execution_result(json!({"data": [{"id": "alias-name"}]})),
            ],
        )
        .with_provider_models(vec![sample_provider_model_with_alias(
            "model-alias",
            "provider-alias",
            "primary-name",
            "alias-name",
        )]);

        perform_model_fetch_once_with_state(&state)
            .await
            .expect("alias fetch should succeed");

        let syncs = state.binding_syncs.lock().expect("binding syncs mutex");
        assert!(syncs.iter().any(|(model_id, endpoint_ids, replace, _)| {
            model_id == "model-alias"
                && endpoint_ids == &["endpoint-alias".to_string(), "endpoint-primary".to_string()]
                && *replace
        }));
    }

    #[tokio::test]
    async fn preset_catalog_fetch_does_not_replace_existing_automatic_bindings() {
        let provider = sample_provider("provider-claude-code", "claude_code");
        let endpoint = sample_endpoint(
            "endpoint-claude-messages",
            "provider-claude-code",
            "claude:messages",
        );
        let key = sample_key(
            "key-claude-code",
            "provider-claude-code",
            "oauth",
            &["claude:messages"],
        );
        let transport = sample_transport(
            "claude_code",
            "provider-claude-code",
            "endpoint-claude-messages",
            "key-claude-code",
            "claude:messages",
            "oauth",
            None,
        );
        let state = TestState::new(
            vec![provider],
            vec![endpoint],
            vec![key],
            HashMap::from([(
                (
                    "provider-claude-code".to_string(),
                    "endpoint-claude-messages".to_string(),
                    "key-claude-code".to_string(),
                ),
                transport,
            )]),
            Vec::new(),
        )
        .with_provider_models(vec![sample_provider_model(
            "model-claude-opus",
            "provider-claude-code",
            "claude-opus-4-6",
        )]);

        let summary = perform_model_fetch_once_with_state(&state)
            .await
            .expect("preset fetch should succeed");

        assert_eq!(summary.succeeded, 1);
        let syncs = state.binding_syncs.lock().expect("binding syncs mutex");
        assert!(syncs.iter().any(|(model_id, endpoint_ids, replace, _)| {
            model_id == "model-claude-opus"
                && endpoint_ids == &["endpoint-claude-messages".to_string()]
                && !*replace
        }));
        assert!(syncs.iter().all(|(_, _, replace, _)| !*replace));
    }
}
