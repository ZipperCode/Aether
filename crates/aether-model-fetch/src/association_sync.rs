use std::collections::BTreeSet;

use aether_data_contracts::repository::global_models::{
    AdminGlobalModelListQuery, AdminProviderModelListQuery, StoredAdminGlobalModelPage,
    StoredAdminProviderModel, UpsertAdminProviderModelRecord,
};
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
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

    async fn list_admin_global_models(
        &self,
        query: &AdminGlobalModelListQuery,
    ) -> Result<StoredAdminGlobalModelPage, Self::Error>;

    async fn create_admin_provider_model(
        &self,
        record: &UpsertAdminProviderModelRecord,
    ) -> Result<Option<StoredAdminProviderModel>, Self::Error>;

    async fn update_admin_provider_model(
        &self,
        record: &UpsertAdminProviderModelRecord,
    ) -> Result<Option<StoredAdminProviderModel>, Self::Error>;

    async fn list_provider_catalog_keys_by_provider_ids(
        &self,
        provider_ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogKey>, Self::Error>;
}

pub async fn sync_provider_model_whitelist_associations<S>(
    state: &S,
    provider_id: &str,
    current_allowed_models: &[String],
) -> Result<(), S::Error>
where
    S: ModelFetchAssociationStore + Sync + ?Sized,
{
    if !state.has_global_model_reader() || !state.has_global_model_writer() {
        return Ok(());
    }

    auto_associate_provider_by_key_whitelist(state, provider_id, current_allowed_models).await?;
    reconcile_provider_model_availability_by_key_whitelist(state, provider_id).await?;
    Ok(())
}

async fn auto_associate_provider_by_key_whitelist<S>(
    state: &S,
    provider_id: &str,
    allowed_models: &[String],
) -> Result<(), S::Error>
where
    S: ModelFetchAssociationStore + Sync + ?Sized,
{
    if allowed_models.is_empty() {
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
    let linked_global_model_ids = provider_models
        .iter()
        .map(|model| model.global_model_id.clone())
        .collect::<BTreeSet<_>>();
    let existing_provider_model_names = provider_models
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

        if !global_model_matches_allowed_models(
            &global_model.name,
            global_model.config.as_ref(),
            allowed_models,
        ) {
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
        state.create_admin_provider_model(&record).await?;
    }

    Ok(())
}

async fn reconcile_provider_model_availability_by_key_whitelist<S>(
    state: &S,
    provider_id: &str,
) -> Result<(), S::Error>
where
    S: ModelFetchAssociationStore + Sync + ?Sized,
{
    let active_keys = state
        .list_provider_catalog_keys_by_provider_ids(&[provider_id.to_string()])
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
        let compiled_mappings = compiled_model_mappings(&global_model_mapping_patterns(
            model.global_model_config.as_ref(),
        ));
        let candidate_names = provider_model_candidate_names(&model);
        let is_available = all_allowed_models.iter().any(|allowed_model| {
            allowed_model_matches_model(allowed_model, &candidate_names, &compiled_mappings)
        });
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
    let compiled_mappings = compiled_model_mappings(&global_model_mapping_patterns(config));
    allowed_models.iter().any(|allowed_model| {
        allowed_model_matches_model(
            allowed_model,
            &[global_model_name.to_string()],
            &compiled_mappings,
        )
    })
}

fn allowed_model_matches_model(
    allowed_model: &str,
    candidate_names: &[String],
    compiled_mappings: &CompiledModelMappings,
) -> bool {
    let allowed_model = allowed_model.trim();
    !allowed_model.is_empty()
        && (candidate_names
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(allowed_model))
            || compiled_mappings.matches_any(allowed_model))
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
