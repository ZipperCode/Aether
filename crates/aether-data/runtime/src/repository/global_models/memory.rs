use std::sync::RwLock;

use async_trait::async_trait;

use super::{
    AdminGlobalModelListQuery, AdminProviderModelListQuery, CreateAdminGlobalModelRecord,
    CreateAdminProviderModelWithBindingsRecord, GlobalModelReadRepository, GlobalModelSnapshot,
    GlobalModelWriteRepository, PublicCatalogModelListQuery, PublicCatalogModelSearchQuery,
    PublicGlobalModelQuery, StoredAdminGlobalModel, StoredAdminGlobalModelPage,
    StoredAdminProviderModel, StoredModelEndpointBinding, StoredProviderActiveGlobalModel,
    StoredProviderModelStats, StoredPublicCatalogModel, StoredPublicGlobalModel,
    StoredPublicGlobalModelPage, UpdateAdminGlobalModelRecord,
    UpdateAdminProviderModelWithBindingsRecord, UpsertAdminProviderModelRecord,
    UpsertModelEndpointBindingRecord,
};
use crate::DataLayerError;

#[derive(Debug, Default)]
pub struct InMemoryGlobalModelReadRepository {
    items: RwLock<Vec<StoredPublicGlobalModel>>,
    admin_global_model_items: RwLock<Vec<StoredAdminGlobalModel>>,
    public_catalog_items: RwLock<Vec<StoredPublicCatalogModel>>,
    admin_provider_model_items: RwLock<Vec<StoredAdminProviderModel>>,
    provider_model_stats: RwLock<Vec<StoredProviderModelStats>>,
    active_global_model_refs: RwLock<Vec<StoredProviderActiveGlobalModel>>,
    model_endpoint_bindings: RwLock<Vec<StoredModelEndpointBinding>>,
    endpoint_provider_ids: RwLock<std::collections::BTreeMap<String, String>>,
    enforce_endpoint_ownership: bool,
}

impl InMemoryGlobalModelReadRepository {
    pub fn seed<I>(items: I) -> Self
    where
        I: IntoIterator<Item = StoredPublicGlobalModel>,
    {
        Self {
            items: RwLock::new(items.into_iter().collect()),
            admin_global_model_items: RwLock::new(Vec::new()),
            public_catalog_items: RwLock::new(Vec::new()),
            admin_provider_model_items: RwLock::new(Vec::new()),
            provider_model_stats: RwLock::new(Vec::new()),
            active_global_model_refs: RwLock::new(Vec::new()),
            model_endpoint_bindings: RwLock::new(Vec::new()),
            endpoint_provider_ids: RwLock::new(std::collections::BTreeMap::new()),
            enforce_endpoint_ownership: false,
        }
    }

    pub fn with_public_catalog_models<I>(self, items: I) -> Self
    where
        I: IntoIterator<Item = StoredPublicCatalogModel>,
    {
        *self
            .public_catalog_items
            .write()
            .expect("public catalog model repository lock") = items.into_iter().collect();
        self
    }

    pub fn with_provider_model_stats<I>(self, items: I) -> Self
    where
        I: IntoIterator<Item = StoredProviderModelStats>,
    {
        *self
            .provider_model_stats
            .write()
            .expect("provider model stats repository lock") = items.into_iter().collect();
        self
    }

    pub fn with_admin_provider_models<I>(self, items: I) -> Self
    where
        I: IntoIterator<Item = StoredAdminProviderModel>,
    {
        *self
            .admin_provider_model_items
            .write()
            .expect("admin provider model repository lock") = items.into_iter().collect();
        self
    }

    pub fn with_active_global_model_refs<I>(self, items: I) -> Self
    where
        I: IntoIterator<Item = StoredProviderActiveGlobalModel>,
    {
        *self
            .active_global_model_refs
            .write()
            .expect("active global model repository lock") = items.into_iter().collect();
        self
    }

    pub fn with_admin_global_models<I>(self, items: I) -> Self
    where
        I: IntoIterator<Item = StoredAdminGlobalModel>,
    {
        *self
            .admin_global_model_items
            .write()
            .expect("admin global model repository lock") = items.into_iter().collect();
        self
    }

    pub fn with_model_endpoint_bindings<I>(self, items: I) -> Self
    where
        I: IntoIterator<Item = StoredModelEndpointBinding>,
    {
        *self
            .model_endpoint_bindings
            .write()
            .expect("model endpoint binding repository lock") = items.into_iter().collect();
        self
    }

    pub fn with_endpoint_provider_ids<I, K, V>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        *self
            .endpoint_provider_ids
            .write()
            .expect("endpoint provider repository lock") = items
            .into_iter()
            .map(|(endpoint_id, provider_id)| (endpoint_id.into(), provider_id.into()))
            .collect();
        self.enforce_endpoint_ownership = true;
        self
    }

    fn validate_endpoint_ownership(
        &self,
        provider_id: &str,
        endpoint_ids: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<(), DataLayerError> {
        // Provider Catalog 是独立的内存仓库，动态新建的 Endpoint 不会自动同步到此测试替身。
        // 只有测试显式注入归属表时才启用严格校验；SQL 适配器仍在同一事务内强制校验。
        if !self.enforce_endpoint_ownership {
            return Ok(());
        }
        let owners = self
            .endpoint_provider_ids
            .read()
            .expect("endpoint provider repository lock");
        for endpoint_id in endpoint_ids {
            let endpoint_id = endpoint_id.as_ref();
            if owners.get(endpoint_id).map(String::as_str) != Some(provider_id) {
                return Err(DataLayerError::UnexpectedValue(
                    "model endpoint binding belongs to another provider".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn snapshot(&self) -> GlobalModelSnapshot {
        GlobalModelSnapshot::seed(
            self.items
                .read()
                .expect("global model repository lock")
                .clone(),
        )
        .with_admin_global_models(
            self.admin_global_model_items
                .read()
                .expect("admin global model repository lock")
                .clone(),
        )
        .with_public_catalog_models(
            self.public_catalog_items
                .read()
                .expect("public catalog model repository lock")
                .clone(),
        )
        .with_admin_provider_models(
            self.admin_provider_model_items
                .read()
                .expect("admin provider model repository lock")
                .clone(),
        )
        .with_provider_model_stats(
            self.provider_model_stats
                .read()
                .expect("provider model stats repository lock")
                .clone(),
        )
        .with_active_global_model_refs(
            self.active_global_model_refs
                .read()
                .expect("active global model repository lock")
                .clone(),
        )
    }
}

#[async_trait]
impl GlobalModelReadRepository for InMemoryGlobalModelReadRepository {
    async fn list_public_models(
        &self,
        query: &PublicGlobalModelQuery,
    ) -> Result<StoredPublicGlobalModelPage, DataLayerError> {
        Ok(self.snapshot().list_public_models(query))
    }

    async fn get_public_model_by_name(
        &self,
        model_name: &str,
    ) -> Result<Option<StoredPublicGlobalModel>, DataLayerError> {
        Ok(self.snapshot().get_public_model_by_name(model_name))
    }

    async fn list_public_catalog_models(
        &self,
        query: &PublicCatalogModelListQuery,
    ) -> Result<Vec<StoredPublicCatalogModel>, DataLayerError> {
        Ok(self.snapshot().list_public_catalog_models(query))
    }

    async fn search_public_catalog_models(
        &self,
        query: &PublicCatalogModelSearchQuery,
    ) -> Result<Vec<StoredPublicCatalogModel>, DataLayerError> {
        Ok(self.snapshot().search_public_catalog_models(query))
    }

    async fn list_admin_global_models(
        &self,
        query: &AdminGlobalModelListQuery,
    ) -> Result<StoredAdminGlobalModelPage, DataLayerError> {
        Ok(self.snapshot().list_admin_global_models(query))
    }

    async fn list_admin_provider_models(
        &self,
        query: &AdminProviderModelListQuery,
    ) -> Result<Vec<StoredAdminProviderModel>, DataLayerError> {
        Ok(self.snapshot().list_admin_provider_models(query))
    }

    async fn get_admin_provider_model(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Result<Option<StoredAdminProviderModel>, DataLayerError> {
        Ok(self
            .snapshot()
            .get_admin_provider_model(provider_id, model_id))
    }

    async fn list_admin_provider_available_source_models(
        &self,
        provider_id: &str,
    ) -> Result<Vec<StoredAdminProviderModel>, DataLayerError> {
        Ok(self
            .snapshot()
            .list_admin_provider_available_source_models(provider_id))
    }

    async fn get_admin_global_model_by_id(
        &self,
        global_model_id: &str,
    ) -> Result<Option<StoredAdminGlobalModel>, DataLayerError> {
        Ok(self
            .snapshot()
            .get_admin_global_model_by_id(global_model_id))
    }

    async fn get_admin_global_model_by_name(
        &self,
        model_name: &str,
    ) -> Result<Option<StoredAdminGlobalModel>, DataLayerError> {
        Ok(self.snapshot().get_admin_global_model_by_name(model_name))
    }

    async fn list_admin_provider_models_by_global_model_id(
        &self,
        global_model_id: &str,
    ) -> Result<Vec<StoredAdminProviderModel>, DataLayerError> {
        Ok(self
            .snapshot()
            .list_admin_provider_models_by_global_model_id(global_model_id))
    }

    async fn list_provider_model_stats(
        &self,
        provider_ids: &[String],
    ) -> Result<Vec<StoredProviderModelStats>, DataLayerError> {
        Ok(self.snapshot().list_provider_model_stats(provider_ids))
    }

    async fn list_active_global_model_ids_by_provider_ids(
        &self,
        provider_ids: &[String],
    ) -> Result<Vec<StoredProviderActiveGlobalModel>, DataLayerError> {
        Ok(self
            .snapshot()
            .list_active_global_model_ids_by_provider_ids(provider_ids))
    }

    async fn list_model_endpoint_bindings(
        &self,
        model_ids: &[String],
    ) -> Result<Vec<StoredModelEndpointBinding>, DataLayerError> {
        if model_ids.is_empty() {
            return Ok(Vec::new());
        }
        let model_ids = model_ids
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        let mut bindings = self
            .model_endpoint_bindings
            .read()
            .expect("model endpoint binding repository lock")
            .iter()
            .filter(|binding| model_ids.contains(binding.model_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        bindings.sort_by(|left, right| {
            left.model_id
                .cmp(&right.model_id)
                .then(left.endpoint_id.cmp(&right.endpoint_id))
        });
        Ok(bindings)
    }
}

#[async_trait]
impl GlobalModelWriteRepository for InMemoryGlobalModelReadRepository {
    async fn create_admin_provider_model(
        &self,
        record: &UpsertAdminProviderModelRecord,
    ) -> Result<Option<StoredAdminProviderModel>, DataLayerError> {
        record.validate()?;
        let global_models = self
            .admin_global_model_items
            .read()
            .expect("admin global model repository lock");
        let global_model = global_models
            .iter()
            .find(|model| model.id == record.global_model_id)
            .cloned()
            .ok_or_else(|| DataLayerError::UnexpectedValue("global model not found".to_string()))?;

        let stored = StoredAdminProviderModel::new(
            record.id.clone(),
            record.provider_id.clone(),
            record.global_model_id.clone(),
            record.provider_model_name.clone(),
            record.provider_model_mappings.clone(),
            record.price_per_request,
            record.tiered_pricing.clone(),
            record.supports_vision,
            record.supports_function_calling,
            record.supports_streaming,
            record.supports_extended_thinking,
            record.supports_image_generation,
            record.is_active,
            record.is_available,
            record.config.clone(),
            Some(1_711_000_000),
            Some(1_711_000_000),
            Some(global_model.name.clone()),
            Some(global_model.display_name.clone()),
            global_model.default_price_per_request,
            global_model.default_tiered_pricing.clone(),
            global_model.supported_capabilities.clone(),
            global_model.config.clone(),
        )?;
        self.admin_provider_model_items
            .write()
            .expect("admin provider model repository lock")
            .push(stored.clone());
        Ok(Some(stored))
    }

    async fn create_admin_provider_model_with_bindings(
        &self,
        record: &CreateAdminProviderModelWithBindingsRecord,
    ) -> Result<Option<StoredAdminProviderModel>, DataLayerError> {
        record.validate()?;
        self.validate_endpoint_ownership(
            &record.model.provider_id,
            record.replacement_bindings.as_ref().map_or_else(
                || record.endpoint_ids.iter().collect::<Vec<_>>(),
                |bindings| {
                    bindings
                        .iter()
                        .map(|binding| &binding.endpoint_id)
                        .collect()
                },
            ),
        )?;
        let global_models = self
            .admin_global_model_items
            .read()
            .expect("admin global model repository lock");
        let global_model = global_models
            .iter()
            .find(|model| model.id == record.model.global_model_id)
            .cloned()
            .ok_or_else(|| DataLayerError::UnexpectedValue("global model not found".to_string()))?;
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
            Some(1_711_000_000),
            Some(1_711_000_000),
            Some(global_model.name),
            Some(global_model.display_name),
            global_model.default_price_per_request,
            global_model.default_tiered_pricing,
            global_model.supported_capabilities,
            global_model.config,
        )?;
        let new_bindings = if let Some(replacement_bindings) = &record.replacement_bindings {
            replacement_bindings
                .iter()
                .map(|binding| {
                    StoredModelEndpointBinding::new(
                        binding.model_id.clone(),
                        binding.endpoint_id.clone(),
                        binding.source.clone(),
                        binding.is_active,
                        Some(1_711_000_000),
                        Some(1_711_000_000),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            record
                .endpoint_ids
                .iter()
                .map(|endpoint_id| {
                    StoredModelEndpointBinding::new(
                        record.model.id.clone(),
                        endpoint_id.clone(),
                        record.source.clone(),
                        true,
                        Some(1_711_000_000),
                        Some(1_711_000_000),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut items = self
            .admin_provider_model_items
            .write()
            .expect("admin provider model repository lock");
        if items.iter().any(|item| item.id == record.model.id) {
            return Err(DataLayerError::UnexpectedValue(
                "provider model already exists".to_string(),
            ));
        }
        let mut bindings = self
            .model_endpoint_bindings
            .write()
            .expect("model endpoint binding repository lock");
        items.push(stored.clone());
        bindings.extend(new_bindings);
        Ok(Some(stored))
    }

    async fn update_admin_provider_model(
        &self,
        record: &UpsertAdminProviderModelRecord,
    ) -> Result<Option<StoredAdminProviderModel>, DataLayerError> {
        record.validate()?;
        let global_models = self
            .admin_global_model_items
            .read()
            .expect("admin global model repository lock");
        let global_model = global_models
            .iter()
            .find(|model| model.id == record.global_model_id)
            .cloned()
            .ok_or_else(|| DataLayerError::UnexpectedValue("global model not found".to_string()))?;
        let mut items = self
            .admin_provider_model_items
            .write()
            .expect("admin provider model repository lock");
        let Some(existing) = items
            .iter_mut()
            .find(|item| item.id == record.id && item.provider_id == record.provider_id)
        else {
            return Ok(None);
        };
        existing.global_model_id = record.global_model_id.clone();
        existing.provider_model_name = record.provider_model_name.clone();
        existing.provider_model_mappings = record.provider_model_mappings.clone();
        existing.price_per_request = record.price_per_request;
        existing.tiered_pricing = record.tiered_pricing.clone();
        existing.supports_vision = record.supports_vision;
        existing.supports_function_calling = record.supports_function_calling;
        existing.supports_streaming = record.supports_streaming;
        existing.supports_extended_thinking = record.supports_extended_thinking;
        existing.supports_image_generation = record.supports_image_generation;
        existing.is_active = record.is_active;
        existing.is_available = record.is_available;
        existing.config = record.config.clone();
        existing.updated_at_unix_secs = Some(1_711_000_100);
        existing.global_model_name = Some(global_model.name.clone());
        existing.global_model_display_name = Some(global_model.display_name.clone());
        existing.global_model_default_price_per_request = global_model.default_price_per_request;
        existing.global_model_default_tiered_pricing = global_model.default_tiered_pricing.clone();
        existing.global_model_supported_capabilities = global_model.supported_capabilities.clone();
        existing.global_model_config = global_model.config.clone();
        Ok(Some(existing.clone()))
    }

    async fn update_admin_provider_model_with_bindings(
        &self,
        record: &UpdateAdminProviderModelWithBindingsRecord,
    ) -> Result<Option<StoredAdminProviderModel>, DataLayerError> {
        record.validate()?;
        self.validate_endpoint_ownership(
            &record.model.provider_id,
            record.replacement_bindings.as_ref().map_or_else(
                || {
                    record
                        .automatic_endpoint_ids
                        .iter()
                        .flatten()
                        .chain(
                            record
                                .manual_bindings
                                .iter()
                                .map(|binding| &binding.endpoint_id),
                        )
                        .collect::<Vec<_>>()
                },
                |bindings| {
                    bindings
                        .iter()
                        .map(|binding| &binding.endpoint_id)
                        .collect()
                },
            ),
        )?;
        let global_models = self
            .admin_global_model_items
            .read()
            .expect("admin global model repository lock");
        let global_model = global_models
            .iter()
            .find(|model| model.id == record.model.global_model_id)
            .cloned()
            .ok_or_else(|| DataLayerError::UnexpectedValue("global model not found".to_string()))?;
        let mut items = self
            .admin_provider_model_items
            .write()
            .expect("admin provider model repository lock");
        let Some(existing_index) = items.iter().position(|item| {
            item.id == record.model.id && item.provider_id == record.model.provider_id
        }) else {
            return Ok(None);
        };
        let mut bindings = self
            .model_endpoint_bindings
            .write()
            .expect("model endpoint binding repository lock");
        let mut updated = items[existing_index].clone();
        let mut updated_bindings = bindings.clone();

        updated.global_model_id = record.model.global_model_id.clone();
        updated.provider_model_name = record.model.provider_model_name.clone();
        updated.provider_model_mappings = record.model.provider_model_mappings.clone();
        updated.price_per_request = record.model.price_per_request;
        updated.tiered_pricing = record.model.tiered_pricing.clone();
        updated.supports_vision = record.model.supports_vision;
        updated.supports_function_calling = record.model.supports_function_calling;
        updated.supports_streaming = record.model.supports_streaming;
        updated.supports_extended_thinking = record.model.supports_extended_thinking;
        updated.supports_image_generation = record.model.supports_image_generation;
        updated.is_active = record.model.is_active;
        updated.is_available = record.model.is_available;
        updated.config = record.model.config.clone();
        updated.updated_at_unix_secs = Some(1_711_000_100);
        updated.global_model_name = Some(global_model.name);
        updated.global_model_display_name = Some(global_model.display_name);
        updated.global_model_default_price_per_request = global_model.default_price_per_request;
        updated.global_model_default_tiered_pricing = global_model.default_tiered_pricing;
        updated.global_model_supported_capabilities = global_model.supported_capabilities;
        updated.global_model_config = global_model.config;

        if let Some(replacement_bindings) = &record.replacement_bindings {
            updated_bindings.retain(|binding| binding.model_id != record.model.id);
            for replacement in replacement_bindings {
                updated_bindings.push(StoredModelEndpointBinding::new(
                    replacement.model_id.clone(),
                    replacement.endpoint_id.clone(),
                    replacement.source.clone(),
                    replacement.is_active,
                    Some(1_711_000_000),
                    Some(1_711_000_000),
                )?);
            }
        } else if let Some(endpoint_ids) = &record.automatic_endpoint_ids {
            let automatic_source = record
                .automatic_source
                .as_deref()
                .expect("validated automatic binding source");
            let endpoint_ids = endpoint_ids
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<std::collections::BTreeSet<_>>();
            updated_bindings.retain(|binding| {
                binding.model_id != record.model.id
                    || binding.source == "manual"
                    || endpoint_ids.contains(&binding.endpoint_id)
            });
            for endpoint_id in endpoint_ids {
                if let Some(binding) = updated_bindings.iter_mut().find(|binding| {
                    binding.model_id == record.model.id && binding.endpoint_id == endpoint_id
                }) {
                    if binding.source != "manual" {
                        binding.source = automatic_source.to_string();
                        binding.is_active = true;
                        binding.updated_at_unix_secs = Some(1_711_000_100);
                    }
                } else {
                    updated_bindings.push(StoredModelEndpointBinding::new(
                        record.model.id.clone(),
                        endpoint_id,
                        automatic_source.to_string(),
                        true,
                        Some(1_711_000_000),
                        Some(1_711_000_000),
                    )?);
                }
            }
        }
        for manual in record
            .replacement_bindings
            .is_none()
            .then_some(record.manual_bindings.as_slice())
            .into_iter()
            .flatten()
        {
            if let Some(binding) = updated_bindings.iter_mut().find(|binding| {
                binding.model_id == manual.model_id && binding.endpoint_id == manual.endpoint_id
            }) {
                binding.source = manual.source.clone();
                binding.is_active = manual.is_active;
                binding.updated_at_unix_secs = Some(1_711_000_100);
            } else {
                updated_bindings.push(StoredModelEndpointBinding::new(
                    manual.model_id.clone(),
                    manual.endpoint_id.clone(),
                    manual.source.clone(),
                    manual.is_active,
                    Some(1_711_000_000),
                    Some(1_711_000_000),
                )?);
            }
        }
        items[existing_index] = updated.clone();
        *bindings = updated_bindings;
        Ok(Some(updated))
    }

    async fn delete_admin_provider_model(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Result<bool, DataLayerError> {
        let mut items = self
            .admin_provider_model_items
            .write()
            .expect("admin provider model repository lock");
        let original_len = items.len();
        items.retain(|item| !(item.provider_id == provider_id && item.id == model_id));
        if items.len() != original_len {
            self.model_endpoint_bindings
                .write()
                .expect("model endpoint binding repository lock")
                .retain(|binding| binding.model_id != model_id);
        }
        Ok(items.len() != original_len)
    }

    async fn create_admin_global_model(
        &self,
        record: &CreateAdminGlobalModelRecord,
    ) -> Result<Option<StoredAdminGlobalModel>, DataLayerError> {
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
            record.usage_count.unwrap_or(0),
            Some(1_711_000_000),
            Some(1_711_000_000),
        )?;
        self.admin_global_model_items
            .write()
            .expect("admin global model repository lock")
            .push(stored);
        self.get_admin_global_model_by_id(&record.id).await
    }

    async fn update_admin_global_model(
        &self,
        record: &UpdateAdminGlobalModelRecord,
    ) -> Result<Option<StoredAdminGlobalModel>, DataLayerError> {
        {
            let mut items = self
                .admin_global_model_items
                .write()
                .expect("admin global model repository lock");
            let Some(existing) = items.iter_mut().find(|item| item.id == record.id) else {
                return Ok(None);
            };
            existing.display_name = record.display_name.clone();
            existing.is_active = record.is_active;
            existing.default_price_per_request = record.default_price_per_request;
            existing.default_tiered_pricing = record.default_tiered_pricing.clone();
            existing.supported_capabilities = record.supported_capabilities.clone();
            existing.config = record.config.clone();
            if let Some(usage_count) = record.usage_count {
                existing.usage_count = usage_count;
            }
            existing.updated_at_unix_secs = Some(1_711_000_100);
        }
        self.get_admin_global_model_by_id(&record.id).await
    }

    async fn delete_admin_global_model(
        &self,
        global_model_id: &str,
    ) -> Result<bool, DataLayerError> {
        let mut globals = self
            .admin_global_model_items
            .write()
            .expect("admin global model repository lock");
        let original_len = globals.len();
        globals.retain(|item| item.id != global_model_id);
        drop(globals);
        self.admin_provider_model_items
            .write()
            .expect("admin provider model repository lock")
            .retain(|item| item.global_model_id != global_model_id);
        let remaining_model_ids = self
            .admin_provider_model_items
            .read()
            .expect("admin provider model repository lock")
            .iter()
            .map(|item| item.id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        self.model_endpoint_bindings
            .write()
            .expect("model endpoint binding repository lock")
            .retain(|binding| remaining_model_ids.contains(binding.model_id.as_str()));
        Ok(original_len
            != self
                .admin_global_model_items
                .read()
                .expect("admin global model repository lock")
                .len())
    }

    async fn delete_unreferenced_admin_global_model(
        &self,
        global_model_id: &str,
    ) -> Result<bool, DataLayerError> {
        let mut globals = self
            .admin_global_model_items
            .write()
            .expect("admin global model repository lock");
        let provider_models = self
            .admin_provider_model_items
            .read()
            .expect("admin provider model repository lock");
        if provider_models
            .iter()
            .any(|item| item.global_model_id == global_model_id)
        {
            return Ok(false);
        }
        // 与创建路径使用相同的锁顺序，使检查与删除处于同一临界区。
        let original_len = globals.len();
        globals.retain(|item| item.id != global_model_id);
        Ok(globals.len() != original_len)
    }

    async fn sync_model_endpoint_bindings(
        &self,
        model_id: &str,
        endpoint_ids: &[String],
        source: &str,
        replace_automatic: bool,
        replacement_scope_endpoint_ids: &[String],
    ) -> Result<Vec<StoredModelEndpointBinding>, DataLayerError> {
        UpsertModelEndpointBindingRecord::new(
            "validation-model".to_string(),
            "validation-endpoint".to_string(),
            source.to_string(),
            true,
        )?;
        let provider_id = self
            .admin_provider_model_items
            .read()
            .expect("admin provider model repository lock")
            .iter()
            .find(|model| model.id == model_id)
            .map(|model| model.provider_id.clone());
        let provider_id = provider_id.ok_or_else(|| {
            DataLayerError::UnexpectedValue("provider model not found".to_string())
        })?;
        self.validate_endpoint_ownership(&provider_id, endpoint_ids)?;
        let endpoint_ids = endpoint_ids
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<std::collections::BTreeSet<_>>();
        let mut bindings = self
            .model_endpoint_bindings
            .write()
            .expect("model endpoint binding repository lock");
        let replacement_scope_endpoint_ids = replacement_scope_endpoint_ids
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<std::collections::BTreeSet<_>>();
        if replace_automatic && !replacement_scope_endpoint_ids.is_empty() {
            bindings.retain(|binding| {
                binding.model_id != model_id
                    || binding.source == "manual"
                    || !replacement_scope_endpoint_ids.contains(binding.endpoint_id.as_str())
                    || endpoint_ids.contains(&binding.endpoint_id)
            });
        }
        for endpoint_id in endpoint_ids {
            if let Some(existing) = bindings
                .iter_mut()
                .find(|binding| binding.model_id == model_id && binding.endpoint_id == endpoint_id)
            {
                if existing.source != "manual" {
                    existing.source = source.to_string();
                    existing.is_active = true;
                    existing.updated_at_unix_secs = Some(1_711_000_100);
                }
                continue;
            }
            bindings.push(StoredModelEndpointBinding::new(
                model_id.to_string(),
                endpoint_id,
                source.to_string(),
                true,
                Some(1_711_000_000),
                Some(1_711_000_000),
            )?);
        }
        let mut result = bindings
            .iter()
            .filter(|binding| binding.model_id == model_id)
            .cloned()
            .collect::<Vec<_>>();
        result.sort_by(|left, right| left.endpoint_id.cmp(&right.endpoint_id));
        Ok(result)
    }

    async fn upsert_model_endpoint_binding(
        &self,
        record: &UpsertModelEndpointBindingRecord,
    ) -> Result<Option<StoredModelEndpointBinding>, DataLayerError> {
        record.validate()?;
        let provider_id = self
            .admin_provider_model_items
            .read()
            .expect("admin provider model repository lock")
            .iter()
            .find(|model| model.id == record.model_id)
            .map(|model| model.provider_id.clone());
        let Some(provider_id) = provider_id else {
            return Ok(None);
        };
        self.validate_endpoint_ownership(&provider_id, [&record.endpoint_id])?;
        let mut bindings = self
            .model_endpoint_bindings
            .write()
            .expect("model endpoint binding repository lock");
        if let Some(existing) = bindings.iter_mut().find(|binding| {
            binding.model_id == record.model_id && binding.endpoint_id == record.endpoint_id
        }) {
            existing.source = record.source.clone();
            existing.is_active = record.is_active;
            existing.updated_at_unix_secs = Some(1_711_000_100);
            return Ok(Some(existing.clone()));
        }
        let stored = StoredModelEndpointBinding::new(
            record.model_id.clone(),
            record.endpoint_id.clone(),
            record.source.clone(),
            record.is_active,
            Some(1_711_000_000),
            Some(1_711_000_000),
        )?;
        bindings.push(stored.clone());
        Ok(Some(stored))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::InMemoryGlobalModelReadRepository;
    use crate::repository::global_models::{
        CreateAdminGlobalModelRecord, CreateAdminProviderModelWithBindingsRecord,
        GlobalModelReadRepository, GlobalModelWriteRepository, PublicCatalogModelListQuery,
        PublicCatalogModelSearchQuery, PublicGlobalModelQuery, StoredAdminGlobalModel,
        StoredAdminProviderModel, StoredModelEndpointBinding, StoredPublicCatalogModel,
        StoredPublicGlobalModel, UpdateAdminProviderModelWithBindingsRecord,
        UpsertAdminProviderModelRecord, UpsertModelEndpointBindingRecord,
    };

    fn sample_model(
        id: &str,
        name: &str,
        display_name: &str,
        is_active: bool,
    ) -> StoredPublicGlobalModel {
        StoredPublicGlobalModel::new(
            id.to_string(),
            name.to_string(),
            Some(display_name.to_string()),
            is_active,
            Some(0.02),
            Some(json!({"tiers":[{"up_to": null, "input_price_per_1m": 3.0, "output_price_per_1m": 15.0}]})),
            Some(json!(["vision"])),
            Some(json!({"family": "test"})),
            0,
        )
        .expect("global model should build")
    }

    fn sample_public_catalog_model(
        id: &str,
        provider_id: &str,
        provider_name: &str,
        provider_model_name: &str,
        name: &str,
        display_name: &str,
    ) -> StoredPublicCatalogModel {
        StoredPublicCatalogModel::new(
            id.to_string(),
            provider_id.to_string(),
            provider_name.to_string(),
            provider_model_name.to_string(),
            name.to_string(),
            display_name.to_string(),
            Some(format!("{display_name} description")),
            Some(format!("https://cdn.example/{name}.png")),
            Some(3.0),
            Some(15.0),
            Some(1.5),
            Some(0.3),
            Some(true),
            Some(true),
            Some(true),
            Some(false),
            true,
        )
        .expect("public catalog model should build")
    }

    fn sample_admin_global_model() -> StoredAdminGlobalModel {
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
            Some(1),
            Some(1),
        )
        .expect("global model should build")
    }

    fn sample_provider_model_record() -> UpsertAdminProviderModelRecord {
        UpsertAdminProviderModelRecord::new(
            "model-1".to_string(),
            "provider-1".to_string(),
            "global-1".to_string(),
            "gpt-5".to_string(),
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
        .expect("provider model record should build")
    }

    fn sample_admin_provider_model() -> StoredAdminProviderModel {
        StoredAdminProviderModel::new(
            "model-1".to_string(),
            "provider-1".to_string(),
            "global-1".to_string(),
            "gpt-5".to_string(),
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
            Some(1),
            Some(1),
            Some("gpt-5".to_string()),
            Some("GPT 5".to_string()),
            None,
            None,
            None,
            None,
        )
        .expect("provider model should build")
    }

    #[tokio::test]
    async fn endpoint_binding_writes_reject_unknown_or_cross_provider_endpoints() {
        let repository = InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models([sample_admin_global_model()])
            .with_admin_provider_models([sample_admin_provider_model()])
            .with_endpoint_provider_ids([
                ("endpoint-1", "provider-1"),
                ("endpoint-2", "provider-2"),
            ]);
        let create = CreateAdminProviderModelWithBindingsRecord::new(
            UpsertAdminProviderModelRecord {
                id: "model-2".to_string(),
                ..sample_provider_model_record()
            },
            vec!["endpoint-2".to_string()],
            "manual".to_string(),
        )
        .expect("create mutation should build");
        assert!(repository
            .create_admin_provider_model_with_bindings(&create)
            .await
            .is_err());

        let update = UpdateAdminProviderModelWithBindingsRecord::new(
            sample_provider_model_record(),
            Some(vec!["missing-endpoint".to_string()]),
            Some("mapping".to_string()),
            Vec::new(),
        )
        .expect("update mutation should build");
        assert!(repository
            .update_admin_provider_model_with_bindings(&update)
            .await
            .is_err());
        assert!(repository
            .sync_model_endpoint_bindings(
                "model-1",
                &["endpoint-2".to_string()],
                "discovered",
                false,
                &[],
            )
            .await
            .is_err());
        assert!(repository
            .upsert_model_endpoint_binding(
                &UpsertModelEndpointBindingRecord::new(
                    "model-1".to_string(),
                    "missing-endpoint".to_string(),
                    "manual".to_string(),
                    true,
                )
                .expect("binding should build"),
            )
            .await
            .is_err());
        assert!(repository
            .list_model_endpoint_bindings(&["model-1".to_string(), "model-2".to_string()])
            .await
            .expect("bindings should load")
            .is_empty());
    }

    #[tokio::test]
    async fn atomic_provider_model_update_keeps_memory_state_on_invalid_replacement_binding() {
        let original_binding = StoredModelEndpointBinding::new(
            "model-1".to_string(),
            "endpoint-1".to_string(),
            "mapping".to_string(),
            true,
            Some(1),
            Some(1),
        )
        .expect("original binding should build");
        let repository = InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models([sample_admin_global_model()])
            .with_admin_provider_models([sample_admin_provider_model()])
            .with_model_endpoint_bindings([original_binding.clone()])
            .with_endpoint_provider_ids([("endpoint-1", "provider-1")]);
        let mut updated_model = sample_provider_model_record();
        updated_model.provider_model_name = "changed-name".to_string();
        let mutation = UpdateAdminProviderModelWithBindingsRecord {
            model: updated_model,
            automatic_endpoint_ids: None,
            automatic_source: None,
            manual_bindings: Vec::new(),
            replacement_bindings: Some(vec![UpsertModelEndpointBindingRecord {
                model_id: String::new(),
                endpoint_id: "endpoint-1".to_string(),
                source: "mapping".to_string(),
                is_active: true,
            }]),
        };

        assert!(repository
            .update_admin_provider_model_with_bindings(&mutation)
            .await
            .is_err());
        let stored_model = repository
            .get_admin_provider_model("provider-1", "model-1")
            .await
            .expect("provider model should read")
            .expect("provider model should remain");
        assert_eq!(stored_model.provider_model_name, "gpt-5");
        assert_eq!(
            repository
                .list_model_endpoint_bindings(&["model-1".to_string()])
                .await
                .expect("bindings should read"),
            vec![original_binding]
        );
    }

    #[tokio::test]
    async fn unreferenced_global_model_cleanup_preserves_adopted_models() {
        let repository = InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models([sample_admin_global_model()]);

        assert!(repository
            .delete_unreferenced_admin_global_model("global-1")
            .await
            .expect("unreferenced global model cleanup should succeed"));
        assert!(repository
            .get_admin_global_model_by_id("global-1")
            .await
            .expect("global model should read")
            .is_none());

        let repository = InMemoryGlobalModelReadRepository::seed(Vec::new())
            .with_admin_global_models([sample_admin_global_model()])
            .with_admin_provider_models([sample_admin_provider_model()]);

        assert!(!repository
            .delete_unreferenced_admin_global_model("global-1")
            .await
            .expect("referenced global model cleanup should be a no-op"));
        assert!(repository
            .get_admin_global_model_by_id("global-1")
            .await
            .expect("global model should read")
            .is_some());
    }

    #[tokio::test]
    async fn manual_model_endpoint_binding_survives_automatic_reconcile() {
        let repository =
            InMemoryGlobalModelReadRepository::seed(Vec::<StoredPublicGlobalModel>::new())
                .with_admin_provider_models([sample_admin_provider_model()])
                .with_endpoint_provider_ids([
                    ("endpoint-old", "provider-1"),
                    ("endpoint-manual", "provider-1"),
                    ("endpoint-new", "provider-1"),
                ])
                .with_model_endpoint_bindings(vec![
                    StoredModelEndpointBinding::new(
                        "model-1".to_string(),
                        "endpoint-old".to_string(),
                        "discovered".to_string(),
                        true,
                        Some(1),
                        Some(1),
                    )
                    .expect("automatic binding should build"),
                    StoredModelEndpointBinding::new(
                        "model-1".to_string(),
                        "endpoint-manual".to_string(),
                        "manual".to_string(),
                        false,
                        Some(1),
                        Some(1),
                    )
                    .expect("manual binding should build"),
                ]);

        repository
            .sync_model_endpoint_bindings(
                "model-1",
                &["endpoint-manual".to_string(), "endpoint-new".to_string()],
                "discovered",
                true,
                &[
                    "endpoint-manual".to_string(),
                    "endpoint-new".to_string(),
                    "endpoint-old".to_string(),
                ],
            )
            .await
            .expect("automatic bindings should reconcile");

        let bindings = repository
            .list_model_endpoint_bindings(&["model-1".to_string()])
            .await
            .expect("bindings should load");
        assert_eq!(bindings.len(), 2);
        let manual = bindings
            .iter()
            .find(|binding| binding.endpoint_id == "endpoint-manual")
            .expect("manual binding should remain");
        assert_eq!(manual.source, "manual");
        assert!(!manual.is_active);
        assert!(bindings
            .iter()
            .any(|binding| binding.endpoint_id == "endpoint-new" && binding.is_active));
        assert!(!bindings
            .iter()
            .any(|binding| binding.endpoint_id == "endpoint-old"));

        repository
            .upsert_model_endpoint_binding(
                &UpsertModelEndpointBindingRecord::new(
                    "model-1".to_string(),
                    "endpoint-manual".to_string(),
                    "manual".to_string(),
                    true,
                )
                .expect("manual update should build"),
            )
            .await
            .expect("manual update should persist");
        let bindings = repository
            .list_model_endpoint_bindings(&["model-1".to_string()])
            .await
            .expect("bindings should reload");
        assert!(bindings
            .iter()
            .any(|binding| binding.endpoint_id == "endpoint-manual" && binding.is_active));
    }

    #[tokio::test]
    async fn scoped_automatic_reconcile_preserves_failed_endpoint_binding() {
        let repository =
            InMemoryGlobalModelReadRepository::seed(Vec::<StoredPublicGlobalModel>::new())
                .with_admin_provider_models([sample_admin_provider_model()])
                .with_endpoint_provider_ids([
                    ("endpoint-success", "provider-1"),
                    ("endpoint-failed", "provider-1"),
                ])
                .with_model_endpoint_bindings(vec![
                    StoredModelEndpointBinding::new(
                        "model-1".to_string(),
                        "endpoint-success".to_string(),
                        "discovered".to_string(),
                        true,
                        Some(1),
                        Some(1),
                    )
                    .expect("successful endpoint binding should build"),
                    StoredModelEndpointBinding::new(
                        "model-1".to_string(),
                        "endpoint-failed".to_string(),
                        "discovered".to_string(),
                        true,
                        Some(1),
                        Some(1),
                    )
                    .expect("failed endpoint binding should build"),
                ]);

        let bindings = repository
            .sync_model_endpoint_bindings(
                "model-1",
                &[],
                "discovered",
                true,
                &["endpoint-success".to_string()],
            )
            .await
            .expect("scoped reconcile should succeed");

        assert!(!bindings
            .iter()
            .any(|binding| binding.endpoint_id == "endpoint-success"));
        assert!(bindings
            .iter()
            .any(|binding| binding.endpoint_id == "endpoint-failed"));
    }

    #[tokio::test]
    async fn embedding_model_metadata_roundtrip() {
        let repository =
            InMemoryGlobalModelReadRepository::seed(Vec::<StoredPublicGlobalModel>::new());
        let record = CreateAdminGlobalModelRecord::new(
            "gm-embedding".to_string(),
            "text-embedding-3-small".to_string(),
            "Text Embedding 3 Small".to_string(),
            true,
            None,
            Some(json!({"tiers":[{"up_to":null,"input_price_per_1m":0.02}]})),
            Some(json!(["embedding"])),
            Some(json!({
                "api_formats": ["openai:embedding"],
                "dimensions": 1536
            })),
        )
        .expect("embedding global model should validate");

        repository
            .create_admin_global_model(&record)
            .await
            .expect("embedding global model should persist")
            .expect("embedding global model should be returned");

        let stored = repository
            .get_admin_global_model_by_name("text-embedding-3-small")
            .await
            .expect("embedding global model should read")
            .expect("embedding global model should exist");

        assert_eq!(stored.supported_capabilities, Some(json!(["embedding"])));
        assert_eq!(
            stored
                .config
                .as_ref()
                .and_then(|value| value.get("dimensions")),
            Some(&json!(1536))
        );
        assert_eq!(
            stored
                .default_tiered_pricing
                .as_ref()
                .and_then(|value| value.get("tiers"))
                .and_then(serde_json::Value::as_array)
                .and_then(|tiers| tiers.first())
                .and_then(|tier| tier.get("input_price_per_1m"))
                .and_then(serde_json::Value::as_f64),
            Some(0.02)
        );
    }

    #[tokio::test]
    async fn embedding_missing_billing_config_rejected() {
        let error = CreateAdminGlobalModelRecord::new(
            "gm-embedding".to_string(),
            "text-embedding-3-small".to_string(),
            "Text Embedding 3 Small".to_string(),
            true,
            None,
            None,
            Some(json!(["embedding"])),
            None,
        )
        .expect_err("embedding metadata without billing should fail closed");

        assert!(error
            .to_string()
            .contains("embedding global model requires"));
    }

    #[tokio::test]
    async fn defaults_to_active_models_only() {
        let repository = InMemoryGlobalModelReadRepository::seed(vec![
            sample_model("gm-1", "claude-sonnet-4-5", "Claude Sonnet 4.5", true),
            sample_model("gm-2", "legacy-model", "Legacy Model", false),
        ]);

        let page = repository
            .list_public_models(&PublicGlobalModelQuery {
                offset: 0,
                limit: 50,
                is_active: None,
                search: None,
            })
            .await
            .expect("list should succeed");

        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].name, "claude-sonnet-4-5");
    }

    #[tokio::test]
    async fn search_matches_name_and_display_name() {
        let repository = InMemoryGlobalModelReadRepository::seed(vec![
            sample_model("gm-1", "gpt-5", "GPT 5", true),
            sample_model("gm-2", "claude-sonnet-4-5", "Claude Sonnet 4.5", true),
        ]);

        let page = repository
            .list_public_models(&PublicGlobalModelQuery {
                offset: 0,
                limit: 50,
                is_active: None,
                search: Some("sonnet".to_string()),
            })
            .await
            .expect("list should succeed");

        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].name, "claude-sonnet-4-5");
    }

    #[tokio::test]
    async fn get_public_model_by_name_only_returns_active_exact_match() {
        let repository = InMemoryGlobalModelReadRepository::seed(vec![
            sample_model("gm-1", "gpt-5", "GPT 5", true),
            sample_model("gm-2", "gpt-5-old", "GPT 5 Old", false),
        ]);

        let model = repository
            .get_public_model_by_name("gpt-5")
            .await
            .expect("lookup should succeed");
        assert_eq!(model.expect("model should exist").name, "gpt-5");

        let missing = repository
            .get_public_model_by_name("gpt-5-old")
            .await
            .expect("lookup should succeed");
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn lists_public_catalog_models_with_provider_filter() {
        let repository =
            InMemoryGlobalModelReadRepository::seed(Vec::<StoredPublicGlobalModel>::new())
                .with_public_catalog_models(vec![
                    sample_public_catalog_model(
                        "model-1",
                        "provider-openai",
                        "openai",
                        "gpt-5-preview",
                        "gpt-5",
                        "GPT 5",
                    ),
                    sample_public_catalog_model(
                        "model-2",
                        "provider-claude",
                        "claude",
                        "claude-3-7-sonnet",
                        "claude-3-7-sonnet",
                        "Claude 3.7 Sonnet",
                    ),
                ]);

        let items = repository
            .list_public_catalog_models(&PublicCatalogModelListQuery {
                provider_id: Some("provider-openai".to_string()),
                offset: 0,
                limit: 50,
            })
            .await
            .expect("list should succeed");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].provider_id, "provider-openai");
        assert_eq!(items[0].name, "gpt-5");
    }

    #[tokio::test]
    async fn public_catalog_preserves_embedding_capability_without_contaminating_chat_models() {
        let mut embedding_model = sample_public_catalog_model(
            "model-embedding",
            "provider-openai",
            "openai",
            "text-embedding-3-small",
            "text-embedding-3-small",
            "Text Embedding 3 Small",
        );
        embedding_model.supports_embedding = Some(true);
        embedding_model.supports_streaming = Some(false);
        let chat_model = sample_public_catalog_model(
            "model-chat",
            "provider-openai",
            "openai",
            "gpt-5-upstream",
            "gpt-5",
            "GPT 5",
        );
        let repository =
            InMemoryGlobalModelReadRepository::seed(Vec::<StoredPublicGlobalModel>::new())
                .with_public_catalog_models(vec![embedding_model, chat_model]);

        let items = repository
            .list_public_catalog_models(&PublicCatalogModelListQuery {
                provider_id: Some("provider-openai".to_string()),
                offset: 0,
                limit: 50,
            })
            .await
            .expect("catalog should list");

        let embedding = items
            .iter()
            .find(|item| item.name == "text-embedding-3-small")
            .expect("embedding model should be listed");
        let chat = items
            .iter()
            .find(|item| item.name == "gpt-5")
            .expect("chat model should be listed");
        assert_eq!(embedding.supports_embedding, Some(true));
        assert_eq!(embedding.supports_streaming, Some(false));
        assert_eq!(chat.supports_embedding, Some(false));
    }

    #[tokio::test]
    async fn searches_public_catalog_models_by_provider_and_display_name() {
        let repository =
            InMemoryGlobalModelReadRepository::seed(Vec::<StoredPublicGlobalModel>::new())
                .with_public_catalog_models(vec![
                    sample_public_catalog_model(
                        "model-1",
                        "provider-openai",
                        "openai",
                        "gpt-5-preview",
                        "gpt-5",
                        "GPT 5",
                    ),
                    sample_public_catalog_model(
                        "model-2",
                        "provider-claude",
                        "claude",
                        "claude-3-7-sonnet",
                        "claude-3-7-sonnet",
                        "Claude 3.7 Sonnet",
                    ),
                ]);

        let items = repository
            .search_public_catalog_models(&PublicCatalogModelSearchQuery {
                search: "sonnet".to_string(),
                provider_id: Some("provider-claude".to_string()),
                limit: 20,
            })
            .await
            .expect("search should succeed");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].provider_name, "claude");
        assert_eq!(items[0].display_name, "Claude 3.7 Sonnet");
    }
}
