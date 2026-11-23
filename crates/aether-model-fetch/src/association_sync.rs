use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use aether_data_contracts::repository::global_models::{
    AdminGlobalModelListQuery, AdminProviderModelListQuery, CreateAdminGlobalModelRecord,
    CreateAdminProviderModelWithBindingsRecord, StoredAdminGlobalModel, StoredAdminGlobalModelPage,
    StoredAdminProviderModel, StoredModelEndpointBinding, UpsertAdminProviderModelRecord,
};
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogModelFetchCandidate;
use aether_scheduler_core::{compiled_model_mappings, CompiledModelMappings};
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use crate::json_string_list;
#[async_trait]
pub trait ModelFetchAssociationStore {
    type Error: Send;

    fn has_global_model_reader(&self) -> bool;
    fn has_global_model_writer(&self) -> bool;
    fn model_fetch_internal_error(&self, message: String) -> Self::Error;

    async fn list_admin_provider_models(
        &self,
        query: &AdminProviderModelListQuery,
    ) -> Result<Vec<StoredAdminProviderModel>, Self::Error>;

    /// 读取准确绑定，为配置驱动的图片目录提供启用状态与端点证据。
    async fn list_model_endpoint_bindings(
        &self,
        model_ids: &[String],
    ) -> Result<Vec<StoredModelEndpointBinding>, Self::Error>;

    async fn list_admin_global_models(
        &self,
        query: &AdminGlobalModelListQuery,
    ) -> Result<StoredAdminGlobalModelPage, Self::Error>;

    async fn get_admin_global_model_by_name(
        &self,
        name: &str,
    ) -> Result<Option<StoredAdminGlobalModel>, Self::Error>;

    async fn create_admin_global_model(
        &self,
        record: &CreateAdminGlobalModelRecord,
    ) -> Result<Option<StoredAdminGlobalModel>, Self::Error>;

    async fn create_admin_provider_model_with_bindings(
        &self,
        record: &CreateAdminProviderModelWithBindingsRecord,
    ) -> Result<Option<StoredAdminProviderModel>, Self::Error>;

    async fn create_admin_provider_model(
        &self,
        record: &UpsertAdminProviderModelRecord,
    ) -> Result<Option<StoredAdminProviderModel>, Self::Error>;

    async fn update_admin_provider_model(
        &self,
        record: &UpsertAdminProviderModelRecord,
    ) -> Result<Option<StoredAdminProviderModel>, Self::Error>;

    async fn sync_model_endpoint_bindings(
        &self,
        model_id: &str,
        endpoint_ids: &[String],
        source: &str,
        replace_automatic: bool,
        replacement_scope_endpoint_ids: &[String],
    ) -> Result<Vec<StoredModelEndpointBinding>, Self::Error>;

    /// 按 Provider 读取模型抓取轻量 Key 投影，不得返回认证材料或完整运行态。
    async fn list_model_fetch_candidates_by_provider_ids(
        &self,
        provider_ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogModelFetchCandidate>, Self::Error>;
}

/// 同步单次模型发现直接产生的模型关联与 Endpoint 绑定，并把 Provider 全量可用性核对留给批次边界。
pub async fn sync_provider_model_discovery_associations<S>(
    state: &S,
    provider_id: &str,
    current_allowed_models: &[String],
    discovered_models: &[Value],
    allow_unbound_models: bool,
    replace_automatic_bindings: bool,
    authoritative_endpoint_ids: &[String],
) -> Result<(), S::Error>
where
    S: ModelFetchAssociationStore + Sync + ?Sized,
{
    if !state.has_global_model_reader() || !state.has_global_model_writer() {
        return Ok(());
    }

    auto_associate_provider_by_key_whitelist(
        state,
        provider_id,
        current_allowed_models,
        discovered_models,
        allow_unbound_models,
    )
    .await?;
    sync_provider_model_endpoint_bindings(
        state,
        provider_id,
        discovered_models,
        replace_automatic_bindings,
        authoritative_endpoint_ids,
    )
    .await?;
    Ok(())
}

/// 完整同步一次 Provider 模型关联；独立调用场景仍在末尾立即核对所有 Key 的白名单可用性。
pub async fn sync_provider_model_whitelist_associations<S>(
    state: &S,
    provider_id: &str,
    current_allowed_models: &[String],
    discovered_models: &[Value],
    allow_unbound_models: bool,
    replace_automatic_bindings: bool,
    authoritative_endpoint_ids: &[String],
) -> Result<(), S::Error>
where
    S: ModelFetchAssociationStore + Sync + ?Sized,
{
    sync_provider_model_discovery_associations(
        state,
        provider_id,
        current_allowed_models,
        discovered_models,
        allow_unbound_models,
        replace_automatic_bindings,
        authoritative_endpoint_ids,
    )
    .await?;
    reconcile_provider_model_whitelist_availability(state, provider_id).await
}

async fn sync_provider_model_endpoint_bindings<S>(
    state: &S,
    provider_id: &str,
    discovered_models: &[Value],
    replace_automatic: bool,
    authoritative_endpoint_ids: &[String],
) -> Result<(), S::Error>
where
    S: ModelFetchAssociationStore + Sync + ?Sized,
{
    if discovered_models.is_empty() && !replace_automatic {
        return Ok(());
    }
    let provider_models = state
        .list_admin_provider_models(&AdminProviderModelListQuery {
            provider_id: provider_id.to_string(),
            is_active: None,
            offset: 0,
            limit: 10_000,
        })
        .await?;
    if provider_models.is_empty() {
        return Ok(());
    }
    let model_ids = provider_models
        .iter()
        .map(|model| model.id.clone())
        .collect::<Vec<_>>();
    let existing_bindings = state.list_model_endpoint_bindings(&model_ids).await?;
    let mut disabled_endpoints_by_model = BTreeMap::<String, BTreeSet<String>>::new();
    for binding in existing_bindings {
        if !binding.is_active {
            disabled_endpoints_by_model
                .entry(binding.model_id)
                .or_default()
                .insert(binding.endpoint_id);
        }
    }
    for model in provider_models {
        let disabled_endpoints = disabled_endpoints_by_model.get(&model.id);
        let endpoint_ids = discovered_endpoint_ids_for_provider_model(&model, discovered_models)
            .into_iter()
            .filter(|endpoint_id| {
                disabled_endpoints.is_none_or(|disabled| !disabled.contains(endpoint_id))
            })
            .collect::<Vec<_>>();
        let scoped_authoritative_endpoint_ids = authoritative_endpoint_ids
            .iter()
            .filter(|endpoint_id| {
                disabled_endpoints.is_none_or(|disabled| !disabled.contains(*endpoint_id))
            })
            .cloned()
            .collect::<Vec<_>>();
        if endpoint_ids.is_empty()
            && (!replace_automatic || scoped_authoritative_endpoint_ids.is_empty())
        {
            continue;
        }
        state
            .sync_model_endpoint_bindings(
                &model.id,
                &endpoint_ids,
                "discovered",
                replace_automatic,
                &scoped_authoritative_endpoint_ids,
            )
            .await?;
    }
    Ok(())
}

fn discovered_model_has_endpoint_evidence(model_id: &str, discovered_models: &[Value]) -> bool {
    discovered_models.iter().any(|item| {
        item.get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|id| id.eq_ignore_ascii_case(model_id))
            && !json_string_list(item.get("endpoint_ids")).is_empty()
    })
}

/// 为开启自动发布的 Provider 补齐缺失的 Global Model，仅处理具备真实 Endpoint 证据且未命中已有模型/别名的发现 ID。
pub async fn ensure_discovered_global_models<S>(
    state: &S,
    provider_id: &str,
    discovered_filtered_ids: &[String],
    discovered_models: &[Value],
    default_tiered_pricing: Option<&Value>,
) -> Result<(), S::Error>
where
    S: ModelFetchAssociationStore + Sync + ?Sized,
{
    if discovered_filtered_ids.is_empty()
        || discovered_models.is_empty()
        || !state.has_global_model_reader()
        || !state.has_global_model_writer()
    {
        return Ok(());
    }

    let mut normalized_ids = Vec::new();
    let mut seen_ids = BTreeSet::new();
    for raw_id in discovered_filtered_ids {
        let trimmed = raw_id.trim();
        if trimmed.is_empty() || trimmed.len() > 100 {
            continue;
        }
        if seen_ids.insert(trimmed.to_ascii_lowercase()) {
            normalized_ids.push(trimmed.to_string());
        }
    }
    if normalized_ids.is_empty() {
        return Ok(());
    }

    let existing_provider_models = state
        .list_admin_provider_models(&AdminProviderModelListQuery {
            provider_id: provider_id.to_string(),
            is_active: None,
            offset: 0,
            limit: 10_000,
        })
        .await?;
    let mut all_global_models = state
        .list_admin_global_models(&AdminGlobalModelListQuery {
            offset: 0,
            limit: 10_000,
            is_active: None,
            search: None,
        })
        .await?
        .items;

    for trimmed in normalized_ids {
        if !discovered_model_has_endpoint_evidence(&trimmed, discovered_models) {
            continue;
        }
        if existing_provider_models
            .iter()
            .any(|model| provider_model_matches_discovered_model(model, &trimmed))
        {
            continue;
        }
        let target = std::slice::from_ref(&trimmed);
        if all_global_models.iter().any(|global_model| {
            global_model.name.trim().eq_ignore_ascii_case(&trimmed)
                || global_model_matches_allowed_models(
                    &global_model.name,
                    global_model.config.as_ref(),
                    target,
                )
        }) {
            continue;
        }
        if let Some(existing) = state.get_admin_global_model_by_name(&trimmed).await? {
            all_global_models.push(existing);
            continue;
        }

        let record = CreateAdminGlobalModelRecord::new(
            Uuid::new_v4().to_string(),
            trimmed.clone(),
            trimmed.clone(),
            true,
            None,
            default_tiered_pricing.cloned(),
            None,
            None,
        )
        .map_err(|err| state.model_fetch_internal_error(err.to_string()))?;
        match state.create_admin_global_model(&record).await {
            Ok(Some(created)) => {
                all_global_models.push(created);
            }
            Ok(None) => {}
            Err(err) => {
                if let Some(existing) = state.get_admin_global_model_by_name(&trimmed).await? {
                    all_global_models.push(existing);
                } else {
                    return Err(err);
                }
            }
        }
    }

    Ok(())
}

async fn auto_associate_provider_by_key_whitelist<S>(
    state: &S,
    provider_id: &str,
    allowed_models: &[String],
    discovered_models: &[Value],
    allow_unbound_models: bool,
) -> Result<(), S::Error>
where
    S: ModelFetchAssociationStore + Sync + ?Sized,
{
    if allowed_models.is_empty() {
        return Ok(());
    }
    if discovered_models.is_empty() && !allow_unbound_models {
        return Ok(());
    }

    let mut provider_models = state
        .list_admin_provider_models(&AdminProviderModelListQuery {
            provider_id: provider_id.to_string(),
            is_active: None,
            offset: 0,
            limit: 10_000,
        })
        .await?;
    let mut linked_global_model_ids = provider_models
        .iter()
        .map(|model| model.global_model_id.clone())
        .collect::<BTreeSet<_>>();
    let mut existing_provider_model_names = provider_models
        .iter()
        .map(|model| model.provider_model_name.clone())
        .collect::<BTreeSet<_>>();
    let global_models = state
        .list_admin_global_models(&AdminGlobalModelListQuery {
            offset: 0,
            limit: 10_000,
            is_active: Some(true),
            search: None,
        })
        .await?
        .items;

    for global_model in global_models {
        if linked_global_model_ids.contains(&global_model.id)
            || existing_provider_model_names.contains(&global_model.name)
        {
            continue;
        }

        let matched_allowed_models = allowed_models
            .iter()
            .filter(|allowed_model| {
                global_model_matches_allowed_models(
                    &global_model.name,
                    global_model.config.as_ref(),
                    std::slice::from_ref(*allowed_model),
                )
            })
            .collect::<Vec<_>>();
        if matched_allowed_models.is_empty() {
            continue;
        }
        if provider_models.iter().any(|model| {
            provider_model_matches_discovered_model(model, &global_model.name)
                || matched_allowed_models
                    .iter()
                    .any(|allowed| provider_model_matches_discovered_model(model, allowed))
        }) {
            continue;
        }
        let record = UpsertAdminProviderModelRecord::new(
            Uuid::new_v4().to_string(),
            provider_id.to_string(),
            global_model.id.clone(),
            global_model.name.clone(),
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
        )
        .map_err(|err| state.model_fetch_internal_error(err.to_string()))?;
        let prospective_model = StoredAdminProviderModel {
            id: record.id.clone(),
            provider_id: record.provider_id.clone(),
            global_model_id: record.global_model_id.clone(),
            provider_model_name: record.provider_model_name.clone(),
            provider_model_mappings: record.provider_model_mappings.clone(),
            price_per_request: record.price_per_request,
            tiered_pricing: record.tiered_pricing.clone(),
            supports_vision: record.supports_vision,
            supports_function_calling: record.supports_function_calling,
            supports_streaming: record.supports_streaming,
            supports_extended_thinking: record.supports_extended_thinking,
            supports_image_generation: record.supports_image_generation,
            is_active: record.is_active,
            is_available: record.is_available,
            config: record.config.clone(),
            created_at_unix_ms: None,
            updated_at_unix_secs: None,
            global_model_name: Some(global_model.name.clone()),
            global_model_display_name: Some(global_model.display_name.clone()),
            global_model_default_price_per_request: global_model.default_price_per_request,
            global_model_default_tiered_pricing: global_model.default_tiered_pricing.clone(),
            global_model_supported_capabilities: global_model.supported_capabilities.clone(),
            global_model_config: global_model.config.clone(),
        };
        let endpoint_ids =
            discovered_endpoint_ids_for_provider_model(&prospective_model, discovered_models);
        if endpoint_ids.is_empty() {
            if allow_unbound_models {
                state.create_admin_provider_model(&record).await?;
            }
            continue;
        }
        let mutation = CreateAdminProviderModelWithBindingsRecord::new(
            record,
            endpoint_ids,
            "discovered".to_string(),
        )
        .map_err(|err| state.model_fetch_internal_error(err.to_string()))?;
        if let Err(err) = state
            .create_admin_provider_model_with_bindings(&mutation)
            .await
        {
            let refreshed = state
                .list_admin_provider_models(&AdminProviderModelListQuery {
                    provider_id: provider_id.to_string(),
                    is_active: None,
                    offset: 0,
                    limit: 10_000,
                })
                .await?;
            let concurrent_exists = refreshed.iter().any(|model| {
                model.global_model_id == global_model.id
                    || model.provider_model_name == global_model.name
                    || provider_model_matches_discovered_model(model, &global_model.name)
            });
            if !concurrent_exists {
                return Err(err);
            }
            provider_models = refreshed;
            linked_global_model_ids = provider_models
                .iter()
                .map(|model| model.global_model_id.clone())
                .collect();
            existing_provider_model_names = provider_models
                .iter()
                .map(|model| model.provider_model_name.clone())
                .collect();
        } else {
            linked_global_model_ids.insert(global_model.id.clone());
            existing_provider_model_names.insert(global_model.name.clone());
        }
    }

    Ok(())
}

/// 基于一个 Provider 的全部 active Key 白名单统一核对模型可用性；批量抓取每个 Provider 只调用一次。
pub async fn reconcile_provider_model_whitelist_availability<S>(
    state: &S,
    provider_id: &str,
) -> Result<(), S::Error>
where
    S: ModelFetchAssociationStore + Sync + ?Sized,
{
    if !state.has_global_model_reader() || !state.has_global_model_writer() {
        return Ok(());
    }
    let active_keys = state
        .list_model_fetch_candidates_by_provider_ids(&[provider_id.to_string()])
        .await?
        .into_iter()
        .filter(|key| key.is_active)
        .collect::<Vec<_>>();
    if active_keys.is_empty() || active_keys.iter().any(|key| key.allowed_models.is_none()) {
        return Ok(());
    }

    let all_allowed_models = active_keys
        .iter()
        .flat_map(|key| json_string_list(key.allowed_models.as_ref()))
        .collect::<BTreeSet<_>>();
    let provider_models = state
        .list_admin_provider_models(&AdminProviderModelListQuery {
            provider_id: provider_id.to_string(),
            is_active: None,
            offset: 0,
            limit: 10_000,
        })
        .await?;

    for model in provider_models {
        let matcher = ProviderModelNameMatcher::new(&model);
        let is_available = all_allowed_models
            .iter()
            .any(|allowed_model| matcher.matches(allowed_model));
        if model.is_available == is_available {
            continue;
        }

        // 关联配置可能包含人工定价和格式映射，只同步动态可用状态。
        let record = provider_model_record_with_availability(&model, is_available)
            .map_err(|err| state.model_fetch_internal_error(err.to_string()))?;
        state.update_admin_provider_model(&record).await?;
    }

    Ok(())
}

fn provider_model_record_with_availability(
    model: &StoredAdminProviderModel,
    is_available: bool,
) -> Result<UpsertAdminProviderModelRecord, aether_data_contracts::DataLayerError> {
    UpsertAdminProviderModelRecord::new(
        model.id.clone(),
        model.provider_id.clone(),
        model.global_model_id.clone(),
        model.provider_model_name.clone(),
        model.provider_model_mappings.clone(),
        model.price_per_request,
        model.tiered_pricing.clone(),
        model.supports_vision,
        model.supports_function_calling,
        model.supports_streaming,
        model.supports_extended_thinking,
        model.supports_image_generation,
        model.is_active,
        is_available,
        model.config.clone(),
    )
}

fn global_model_mapping_patterns(config: Option<&Value>) -> Vec<String> {
    config
        .and_then(Value::as_object)
        .and_then(|object| object.get("model_mappings"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub fn global_model_matches_allowed_models(
    global_model_name: &str,
    config: Option<&Value>,
    allowed_models: &[String],
) -> bool {
    let matcher = ProviderModelNameMatcher::from_parts(
        vec![global_model_name.to_string()],
        global_model_mapping_patterns(config),
    );
    allowed_models
        .iter()
        .any(|allowed_model| matcher.matches(allowed_model))
}

pub fn provider_model_matches_discovered_model(
    model: &StoredAdminProviderModel,
    discovered_model_name: &str,
) -> bool {
    ProviderModelNameMatcher::new(model).matches(discovered_model_name)
}

struct ProviderModelNameMatcher {
    candidate_names: Vec<String>,
    global_model_mappings: Arc<CompiledModelMappings>,
}

impl ProviderModelNameMatcher {
    fn new(model: &StoredAdminProviderModel) -> Self {
        Self::from_parts(
            provider_model_candidate_names(model),
            global_model_mapping_patterns(model.global_model_config.as_ref()),
        )
    }

    fn from_parts(candidate_names: Vec<String>, global_model_mappings: Vec<String>) -> Self {
        Self {
            candidate_names,
            global_model_mappings: compiled_model_mappings(&global_model_mappings),
        }
    }

    fn matches(&self, model_name: &str) -> bool {
        let model_name = model_name.trim();
        !model_name.is_empty()
            && (self
                .candidate_names
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(model_name))
                || self.global_model_mappings.matches_any(model_name))
    }
}

fn discovered_endpoint_ids_for_provider_model(
    model: &StoredAdminProviderModel,
    discovered_models: &[Value],
) -> Vec<String> {
    let matcher = ProviderModelNameMatcher::new(model);
    discovered_models
        .iter()
        .filter(|discovered_model| {
            discovered_model
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|model_id| matcher.matches(model_id))
        })
        .flat_map(|model| json_string_list(model.get("endpoint_ids")))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn provider_model_candidate_names(model: &StoredAdminProviderModel) -> Vec<String> {
    let mut names = Vec::new();
    push_unique_model_name(&mut names, &model.provider_model_name);
    if let Some(global_model_name) = model.global_model_name.as_deref() {
        push_unique_model_name(&mut names, global_model_name);
    }
    if let Some(mappings) = model.provider_model_mappings.as_ref() {
        match mappings {
            Value::Array(items) => {
                for item in items {
                    if let Some(name) = item
                        .as_str()
                        .or_else(|| item.get("name").and_then(Value::as_str))
                    {
                        push_unique_model_name(&mut names, name);
                    }
                }
            }
            Value::String(name) => push_unique_model_name(&mut names, name),
            Value::Object(object) => {
                if let Some(name) = object.get("name").and_then(Value::as_str) {
                    push_unique_model_name(&mut names, name);
                }
            }
            _ => {}
        }
    }
    names
}

fn push_unique_model_name(names: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if value.is_empty()
        || names
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(value))
    {
        return;
    }
    names.push(value.to_string());
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use serde_json::json;

    use super::{
        discovered_endpoint_ids_for_provider_model, sync_provider_model_whitelist_associations,
        ModelFetchAssociationStore,
    };
    use aether_data_contracts::repository::global_models::{
        AdminGlobalModelListQuery, AdminProviderModelListQuery, CreateAdminGlobalModelRecord,
        CreateAdminProviderModelWithBindingsRecord, StoredAdminGlobalModel,
        StoredAdminGlobalModelPage, StoredAdminProviderModel, StoredModelEndpointBinding,
        UpsertAdminProviderModelRecord,
    };
    use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogModelFetchCandidate;

    type BindingSyncCall = (String, Vec<String>, String, bool, Vec<String>);

    #[derive(Default)]
    struct AssociationTestStore {
        global_models: Mutex<Vec<StoredAdminGlobalModel>>,
        provider_models: Mutex<Vec<StoredAdminProviderModel>>,
        bindings: Mutex<Vec<StoredModelEndpointBinding>>,
        created: Mutex<Vec<CreateAdminProviderModelWithBindingsRecord>>,
        unbound_created: Mutex<Vec<UpsertAdminProviderModelRecord>>,
        binding_syncs: Mutex<Vec<BindingSyncCall>>,
        fail_create_global_error: Option<String>,
    }

    #[async_trait]
    impl ModelFetchAssociationStore for AssociationTestStore {
        type Error = String;

        fn has_global_model_reader(&self) -> bool {
            true
        }

        fn has_global_model_writer(&self) -> bool {
            true
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
            query: &AdminGlobalModelListQuery,
        ) -> Result<StoredAdminGlobalModelPage, Self::Error> {
            let items = self
                .global_models
                .lock()
                .expect("global models mutex")
                .iter()
                .filter(|item| {
                    query
                        .is_active
                        .is_none_or(|active| item.is_active == active)
                })
                .cloned()
                .collect::<Vec<_>>();
            let total = items.len();
            Ok(StoredAdminGlobalModelPage { items, total })
        }

        async fn get_admin_global_model_by_name(
            &self,
            name: &str,
        ) -> Result<Option<StoredAdminGlobalModel>, Self::Error> {
            Ok(self
                .global_models
                .lock()
                .expect("global models mutex")
                .iter()
                .find(|item| item.name == name)
                .cloned())
        }

        async fn create_admin_global_model(
            &self,
            record: &CreateAdminGlobalModelRecord,
        ) -> Result<Option<StoredAdminGlobalModel>, Self::Error> {
            if let Some(err) = &self.fail_create_global_error {
                return Err(err.clone());
            }
            let mut globals = self.global_models.lock().expect("global models mutex");
            if globals.iter().any(|item| item.name == record.name) {
                return Err("duplicate global model name".to_string());
            }
            let stored = StoredAdminGlobalModel::new(
                record.id.clone(),
                record.name.clone(),
                record.display_name.clone(),
                record.is_active,
                record.default_price_per_request,
                record.default_tiered_pricing.clone(),
                record.supported_capabilities.clone(),
                record.config.clone(),
                0,
                0,
                0,
                None,
                None,
            )
            .map_err(|err| err.to_string())?;
            globals.push(stored.clone());
            Ok(Some(stored))
        }

        async fn create_admin_provider_model_with_bindings(
            &self,
            record: &CreateAdminProviderModelWithBindingsRecord,
        ) -> Result<Option<StoredAdminProviderModel>, Self::Error> {
            {
                let provider_models = self.provider_models.lock().expect("provider models mutex");
                if provider_models.iter().any(|item| {
                    item.provider_id == record.model.provider_id
                        && item.provider_model_name == record.model.provider_model_name
                }) {
                    return Err("duplicate provider model".to_string());
                }
            }
            self.created
                .lock()
                .expect("created mutex")
                .push(record.clone());
            let stored = StoredAdminProviderModel::new(
                record.model.id.clone(),
                record.model.provider_id.clone(),
                record.model.global_model_id.clone(),
                record.model.provider_model_name.clone(),
                record.model.provider_model_mappings.clone(),
                record.model.price_per_request,
                record.model.tiered_pricing.clone(),
                record.model.supports_vision,
                record.model.supports_function_calling,
                record.model.supports_streaming,
                record.model.supports_extended_thinking,
                record.model.supports_image_generation,
                record.model.is_active,
                record.model.is_available,
                record.model.config.clone(),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .map_err(|err| err.to_string())?;
            self.provider_models
                .lock()
                .expect("provider models mutex")
                .push(stored.clone());
            let mut bindings = self.bindings.lock().expect("bindings mutex");
            for endpoint_id in &record.endpoint_ids {
                bindings.push(
                    StoredModelEndpointBinding::new(
                        record.model.id.clone(),
                        endpoint_id.clone(),
                        record.source.clone(),
                        true,
                        None,
                        None,
                    )
                    .map_err(|err| err.to_string())?,
                );
            }
            Ok(Some(stored))
        }

        async fn create_admin_provider_model(
            &self,
            record: &UpsertAdminProviderModelRecord,
        ) -> Result<Option<StoredAdminProviderModel>, Self::Error> {
            self.unbound_created
                .lock()
                .expect("unbound created mutex")
                .push(record.clone());
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
            source: &str,
            replace_automatic: bool,
            replacement_scope_endpoint_ids: &[String],
        ) -> Result<Vec<StoredModelEndpointBinding>, Self::Error> {
            self.binding_syncs
                .lock()
                .expect("binding syncs mutex")
                .push((
                    model_id.to_string(),
                    endpoint_ids.to_vec(),
                    source.to_string(),
                    replace_automatic,
                    replacement_scope_endpoint_ids.to_vec(),
                ));
            Ok(Vec::new())
        }

        /// 测试仓储返回空的模型抓取候选集合。
        async fn list_model_fetch_candidates_by_provider_ids(
            &self,
            _provider_ids: &[String],
        ) -> Result<Vec<StoredProviderCatalogModelFetchCandidate>, Self::Error> {
            Ok(Vec::new())
        }

        async fn list_model_endpoint_bindings(
            &self,
            model_ids: &[String],
        ) -> Result<Vec<StoredModelEndpointBinding>, Self::Error> {
            Ok(self
                .bindings
                .lock()
                .expect("bindings mutex")
                .iter()
                .filter(|binding| model_ids.contains(&binding.model_id))
                .cloned()
                .collect())
        }
    }

    fn global_model() -> StoredAdminGlobalModel {
        StoredAdminGlobalModel::new(
            "global-1".to_string(),
            "gpt-5".to_string(),
            "GPT 5".to_string(),
            true,
            None,
            None,
            None,
            None,
            0,
            0,
            0,
            None,
            None,
        )
        .expect("global model should build")
    }

    #[tokio::test]
    async fn whitelist_association_waits_for_endpoint_evidence_before_creating_model() {
        let state = AssociationTestStore {
            global_models: Mutex::new(vec![global_model()]),
            ..AssociationTestStore::default()
        };

        sync_provider_model_whitelist_associations(
            &state,
            "provider-1",
            &["gpt-5".to_string()],
            &[],
            false,
            false,
            &[],
        )
        .await
        .expect("association should succeed");
        assert!(state.created.lock().expect("created mutex").is_empty());

        sync_provider_model_whitelist_associations(
            &state,
            "provider-1",
            &["gpt-5".to_string()],
            &[json!({"id": "gpt-5", "endpoint_ids": ["endpoint-1"]})],
            false,
            false,
            &[],
        )
        .await
        .expect("association should succeed");
        let created = state.created.lock().expect("created mutex");
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].endpoint_ids, vec!["endpoint-1".to_string()]);
    }

    #[tokio::test]
    async fn association_allows_unbound_model_without_provider_endpoints() {
        let state = AssociationTestStore {
            global_models: Mutex::new(vec![global_model()]),
            ..AssociationTestStore::default()
        };

        sync_provider_model_whitelist_associations(
            &state,
            "provider-1",
            &["gpt-5".to_string()],
            &[],
            true,
            false,
            &[],
        )
        .await
        .expect("unbound association should succeed");

        assert!(state.created.lock().expect("created mutex").is_empty());
        let unbound_created = state.unbound_created.lock().expect("unbound created mutex");
        assert_eq!(unbound_created.len(), 1);
        assert_eq!(unbound_created[0].provider_model_name, "gpt-5");
    }

    fn provider_model(
        provider_model_name: &str,
        provider_model_mappings: Option<serde_json::Value>,
        global_model_config: Option<serde_json::Value>,
    ) -> StoredAdminProviderModel {
        StoredAdminProviderModel {
            id: "model-1".to_string(),
            provider_id: "provider-1".to_string(),
            global_model_id: "global-1".to_string(),
            provider_model_name: provider_model_name.to_string(),
            provider_model_mappings,
            price_per_request: None,
            tiered_pricing: None,
            supports_vision: None,
            supports_function_calling: None,
            supports_streaming: None,
            supports_extended_thinking: None,
            supports_image_generation: None,
            is_active: true,
            is_available: true,
            config: None,
            created_at_unix_ms: None,
            updated_at_unix_secs: None,
            global_model_name: Some("claude-opus".to_string()),
            global_model_display_name: None,
            global_model_default_price_per_request: None,
            global_model_default_tiered_pricing: None,
            global_model_supported_capabilities: None,
            global_model_config,
        }
    }

    #[test]
    fn endpoint_discovery_uses_global_model_regex_mappings() {
        let model = provider_model(
            "claude-opus",
            None,
            Some(json!({ "model_mappings": ["claude-opus-.*"] })),
        );
        let discovered_models = vec![json!({
            "id": "claude-opus-4-6",
            "endpoint_ids": ["endpoint-1"]
        })];

        assert_eq!(
            discovered_endpoint_ids_for_provider_model(&model, &discovered_models),
            vec!["endpoint-1".to_string()]
        );
    }

    #[test]
    fn endpoint_discovery_treats_provider_mapping_objects_as_exact_aliases() {
        let model = provider_model(
            "claude-opus",
            Some(json!([{
                "name": "upstream-opus",
                "priority": 1,
                "endpoint_ids": ["endpoint-1"]
            }])),
            None,
        );
        let discovered_models = vec![
            json!({ "id": "upstream-opus", "endpoint_ids": ["endpoint-1"] }),
            json!({ "id": "upstream-opus-latest", "endpoint_ids": ["endpoint-2"] }),
        ];

        assert_eq!(
            discovered_endpoint_ids_for_provider_model(&model, &discovered_models),
            vec!["endpoint-1".to_string()]
        );
    }

    #[test]
    fn discovered_model_match_supports_exact_names_aliases_and_global_regex() {
        let model = provider_model(
            "claude-opus-upstream",
            Some(json!([{ "name": "upstream-opus", "priority": 1 }])),
            Some(json!({ "model_mappings": ["claude-opus-.*"] })),
        );

        assert!(super::provider_model_matches_discovered_model(
            &model,
            "CLAUDE-OPUS-UPSTREAM"
        ));
        assert!(super::provider_model_matches_discovered_model(
            &model,
            "upstream-opus"
        ));
        assert!(super::provider_model_matches_discovered_model(
            &model,
            "claude-opus-4-6"
        ));
        assert!(!super::provider_model_matches_discovered_model(
            &model,
            "claude-sonnet-4-6"
        ));
    }

    #[tokio::test]
    async fn ensure_discovered_global_models_requires_endpoint_evidence_and_preserves_disabled_aliases(
    ) {
        let disabled_global = StoredAdminGlobalModel::new(
            "global-disabled".to_string(),
            "disabled-global-model".to_string(),
            "Disabled Global".to_string(),
            false,
            None,
            None,
            None,
            Some(json!({ "model_mappings": ["disabled-alias-.*"] })),
            0,
            0,
            0,
            None,
            None,
        )
        .expect("disabled global should build");
        let mut disabled_pm = provider_model(
            "disabled-pm",
            Some(json!([{ "name": "disabled-pm-alias", "priority": 1 }])),
            None,
        );
        disabled_pm.id = "pm-disabled".to_string();
        disabled_pm.is_active = false;

        let state = AssociationTestStore {
            global_models: Mutex::new(vec![disabled_global]),
            provider_models: Mutex::new(vec![disabled_pm]),
            ..AssociationTestStore::default()
        };
        let pricing = json!({ "tiers": [{ "up_to": null, "input_price_per_1m": 0.0, "output_price_per_1m": 0.0 }] });

        super::ensure_discovered_global_models(
            &state,
            "provider-1",
            &[
                "disabled-global-model".to_string(),
                "disabled-alias-v2".to_string(),
                "disabled-pm-alias".to_string(),
                "no-endpoint-model".to_string(),
                "brand-new-model".to_string(),
            ],
            &[
                json!({ "id": "disabled-global-model", "endpoint_ids": ["ep-1"] }),
                json!({ "id": "disabled-alias-v2", "endpoint_ids": ["ep-1"] }),
                json!({ "id": "disabled-pm-alias", "endpoint_ids": ["ep-1"] }),
                json!({ "id": "no-endpoint-model" }),
                json!({ "id": "brand-new-model", "endpoint_ids": ["ep-1"] }),
            ],
            Some(&pricing),
        )
        .await
        .expect("ensure should succeed");

        let globals = state.global_models.lock().expect("globals mutex");
        assert_eq!(globals.len(), 2);
        let created = globals
            .iter()
            .find(|g| g.name == "brand-new-model")
            .expect("brand-new-model should be created");
        assert!(created.is_active);
        assert_eq!(created.default_tiered_pricing.as_ref(), Some(&pricing));
    }

    #[tokio::test]
    async fn ensure_and_associate_handle_concurrent_uniqueness_and_propagate_storage_errors() {
        let state = AssociationTestStore::default();
        let discovered = vec![json!({ "id": "gpt-concurrent", "endpoint_ids": ["ep-1"] })];
        let ids = vec!["gpt-concurrent".to_string()];

        let (r1, r2) = tokio::join!(
            super::ensure_discovered_global_models(&state, "provider-1", &ids, &discovered, None),
            super::ensure_discovered_global_models(&state, "provider-2", &ids, &discovered, None),
        );
        r1.expect("first ensure should succeed");
        r2.expect("second ensure should succeed");
        assert_eq!(state.global_models.lock().expect("globals").len(), 1);

        let (a1, a2) = tokio::join!(
            super::sync_provider_model_discovery_associations(
                &state,
                "provider-1",
                &ids,
                &discovered,
                false,
                false,
                &[],
            ),
            super::sync_provider_model_discovery_associations(
                &state,
                "provider-1",
                &ids,
                &discovered,
                false,
                false,
                &[],
            ),
        );
        a1.expect("first associate should succeed");
        a2.expect("second associate should succeed");
        assert_eq!(state.provider_models.lock().expect("pms").len(), 1);

        let failing_state = AssociationTestStore {
            fail_create_global_error: Some("db connection lost".to_string()),
            ..AssociationTestStore::default()
        };
        let err = super::ensure_discovered_global_models(
            &failing_state,
            "provider-1",
            &ids,
            &discovered,
            None,
        )
        .await
        .expect_err("real storage error must propagate");
        assert_eq!(err, "db connection lost");
    }

    #[tokio::test]
    async fn sync_provider_model_endpoint_bindings_never_reenables_or_deletes_disabled_binding() {
        let mut active_pm = provider_model("gpt-5", None, None);
        active_pm.id = "pm-1".to_string();
        let disabled_binding = StoredModelEndpointBinding::new(
            "pm-1".to_string(),
            "ep-1".to_string(),
            "discovered".to_string(),
            false,
            None,
            None,
        )
        .expect("binding should build");
        let state = AssociationTestStore {
            provider_models: Mutex::new(vec![active_pm]),
            bindings: Mutex::new(vec![disabled_binding]),
            ..AssociationTestStore::default()
        };

        super::sync_provider_model_endpoint_bindings(
            &state,
            "provider-1",
            &[json!({ "id": "gpt-5", "endpoint_ids": ["ep-1", "ep-2"] })],
            true,
            &["ep-1".to_string(), "ep-2".to_string()],
        )
        .await
        .expect("binding sync should succeed");

        let syncs = state.binding_syncs.lock().expect("binding syncs mutex");
        assert_eq!(syncs.len(), 1);
        assert_eq!(syncs[0].1, vec!["ep-2".to_string()]);
        assert_eq!(syncs[0].4, vec!["ep-2".to_string()]);
    }

    #[tokio::test]
    async fn auto_associate_preserves_disabled_provider_model_aliases() {
        let mut disabled_pm = provider_model(
            "custom-disabled",
            Some(json!([{ "name": "gpt-5", "priority": 1 }])),
            None,
        );
        disabled_pm.id = "pm-disabled".to_string();
        disabled_pm.global_model_id = "other-global".to_string();
        disabled_pm.is_active = false;

        let state = AssociationTestStore {
            global_models: Mutex::new(vec![global_model()]),
            provider_models: Mutex::new(vec![disabled_pm]),
            ..AssociationTestStore::default()
        };

        super::sync_provider_model_discovery_associations(
            &state,
            "provider-1",
            &["gpt-5".to_string()],
            &[json!({ "id": "gpt-5", "endpoint_ids": ["ep-1"] })],
            false,
            false,
            &[],
        )
        .await
        .expect("association should succeed");

        assert!(state.created.lock().expect("created").is_empty());
    }
}
