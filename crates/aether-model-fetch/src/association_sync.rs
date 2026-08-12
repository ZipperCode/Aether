use std::collections::BTreeSet;
use std::sync::Arc;

use aether_data_contracts::repository::global_models::{
    AdminGlobalModelListQuery, AdminProviderModelListQuery,
    CreateAdminProviderModelWithBindingsRecord, StoredAdminGlobalModelPage,
    StoredAdminProviderModel, StoredModelEndpointBinding, UpsertAdminProviderModelRecord,
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

    async fn list_provider_catalog_keys_by_provider_ids(
        &self,
        provider_ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogKey>, Self::Error>;
}

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
    reconcile_provider_model_availability_by_key_whitelist(state, provider_id).await?;
    Ok(())
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
    for model in provider_models {
        let endpoint_ids = discovered_endpoint_ids_for_provider_model(&model, discovered_models);
        if endpoint_ids.is_empty() && (!replace_automatic || authoritative_endpoint_ids.is_empty())
        {
            continue;
        }
        state
            .sync_model_endpoint_bindings(
                &model.id,
                &endpoint_ids,
                "discovered",
                replace_automatic,
                authoritative_endpoint_ids,
            )
            .await?;
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
        state
            .create_admin_provider_model_with_bindings(&mutation)
            .await?;
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
        AdminGlobalModelListQuery, AdminProviderModelListQuery,
        CreateAdminProviderModelWithBindingsRecord, StoredAdminGlobalModel,
        StoredAdminGlobalModelPage, StoredAdminProviderModel, StoredModelEndpointBinding,
        UpsertAdminProviderModelRecord,
    };
    use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;

    #[derive(Default)]
    struct AssociationTestStore {
        global_models: Vec<StoredAdminGlobalModel>,
        created: Mutex<Vec<CreateAdminProviderModelWithBindingsRecord>>,
        unbound_created: Mutex<Vec<UpsertAdminProviderModelRecord>>,
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
            _query: &AdminProviderModelListQuery,
        ) -> Result<Vec<StoredAdminProviderModel>, Self::Error> {
            Ok(Vec::new())
        }

        async fn list_admin_global_models(
            &self,
            _query: &AdminGlobalModelListQuery,
        ) -> Result<StoredAdminGlobalModelPage, Self::Error> {
            Ok(StoredAdminGlobalModelPage {
                items: self.global_models.clone(),
                total: self.global_models.len(),
            })
        }

        async fn create_admin_provider_model_with_bindings(
            &self,
            record: &CreateAdminProviderModelWithBindingsRecord,
        ) -> Result<Option<StoredAdminProviderModel>, Self::Error> {
            self.created
                .lock()
                .expect("created mutex")
                .push(record.clone());
            Ok(None)
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
            _model_id: &str,
            _endpoint_ids: &[String],
            _source: &str,
            _replace_automatic: bool,
            _replacement_scope_endpoint_ids: &[String],
        ) -> Result<Vec<StoredModelEndpointBinding>, Self::Error> {
            Ok(Vec::new())
        }

        async fn list_provider_catalog_keys_by_provider_ids(
            &self,
            _provider_ids: &[String],
        ) -> Result<Vec<StoredProviderCatalogKey>, Self::Error> {
            Ok(Vec::new())
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
            global_models: vec![global_model()],
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
            global_models: vec![global_model()],
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
}
