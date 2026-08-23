use super::AdminAppState;
use crate::handlers::admin::provider::shared::payloads::{
    AdminImportProviderModelsRequest, AdminProviderModelCreateRequest,
    AdminProviderModelUpdatePatch,
};
use crate::handlers::admin::shared::{normalize_json_array, normalize_json_object};
use crate::GatewayError;
use aether_admin::provider::{
    models as admin_provider_models_pure, models_write as admin_provider_models_write_pure,
};
use aether_data_contracts::repository::global_models::{
    AdminProviderModelListQuery, CreateAdminProviderModelWithBindingsRecord,
    StoredAdminProviderModel, StoredModelEndpointBinding, UpsertAdminProviderModelRecord,
    UpsertModelEndpointBindingRecord,
};
use axum::http;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

struct AdminModelEndpointInference {
    endpoint_ids: Vec<String>,
    source: &'static str,
}

pub(crate) enum AdminProviderModelCreateMutation {
    Bound(CreateAdminProviderModelWithBindingsRecord),
    Unbound(UpsertAdminProviderModelRecord),
}

pub(crate) fn stored_admin_provider_model_from_upsert(
    model: &UpsertAdminProviderModelRecord,
) -> StoredAdminProviderModel {
    StoredAdminProviderModel {
        id: model.id.clone(),
        provider_id: model.provider_id.clone(),
        global_model_id: model.global_model_id.clone(),
        provider_model_name: model.provider_model_name.clone(),
        provider_model_mappings: model.provider_model_mappings.clone(),
        price_per_request: model.price_per_request,
        tiered_pricing: model.tiered_pricing.clone(),
        supports_vision: model.supports_vision,
        supports_function_calling: model.supports_function_calling,
        supports_streaming: model.supports_streaming,
        supports_extended_thinking: model.supports_extended_thinking,
        supports_image_generation: model.supports_image_generation,
        is_active: model.is_active,
        is_available: model.is_available,
        config: model.config.clone(),
        created_at_unix_ms: None,
        updated_at_unix_secs: None,
        global_model_name: None,
        global_model_display_name: None,
        global_model_default_price_per_request: None,
        global_model_default_tiered_pricing: None,
        global_model_supported_capabilities: None,
        global_model_config: None,
    }
}

fn normalize_provider_model_mapping_scopes(
    value: Option<serde_json::Value>,
) -> Option<serde_json::Value> {
    let Some(mut value) = value else {
        return None;
    };
    let Some(items) = value.as_array_mut() else {
        return Some(value);
    };
    for item in items {
        let Some(object) = item.as_object_mut() else {
            continue;
        };
        normalize_provider_model_mapping_string_array_field(
            object,
            "api_formats",
            crate::ai_serving::normalize_api_format_alias,
        );
        normalize_provider_model_mapping_string_array_field(object, "endpoint_ids", |value| {
            value.trim().to_string()
        });
        normalize_provider_model_mapping_string_array_field(object, "operations", |value| {
            value.trim().to_ascii_lowercase()
        });
    }
    Some(value)
}

fn normalize_provider_model_mapping_string_array_field(
    object: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    normalize: impl Fn(&str) -> String,
) {
    let Some(array) = object.get(field).and_then(serde_json::Value::as_array) else {
        return;
    };
    let mut seen = BTreeSet::new();
    let normalized = array
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(normalize)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.clone()))
        .map(serde_json::Value::String)
        .collect::<Vec<_>>();
    if normalized.is_empty() {
        object.remove(field);
    } else {
        object.insert(field.to_string(), serde_json::Value::Array(normalized));
    }
}

impl<'a> AdminAppState<'a> {
    pub(crate) async fn build_admin_provider_model_create_mutation(
        &self,
        model: UpsertAdminProviderModelRecord,
        endpoint_ids: Option<Vec<String>>,
        source: Option<&str>,
    ) -> Result<AdminProviderModelCreateMutation, GatewayError> {
        let provider_endpoints = self
            .list_provider_catalog_endpoints_by_provider_ids(std::slice::from_ref(
                &model.provider_id,
            ))
            .await?;
        if provider_endpoints.is_empty() {
            // 未配置 Endpoint 的 Provider 保留旧版未绑定模型；一旦存在 Endpoint，创建必须精确绑定。
            if let Some(endpoint_id) = endpoint_ids
                .as_ref()
                .into_iter()
                .flatten()
                .map(String::as_str)
                .map(str::trim)
                .find(|endpoint_id| !endpoint_id.is_empty())
            {
                return Err(GatewayError::Client {
                    status: http::StatusCode::BAD_REQUEST,
                    message: format!("Endpoint {endpoint_id} 不属于当前 Provider"),
                });
            }
            return Ok(AdminProviderModelCreateMutation::Unbound(model));
        }

        let prospective_model = stored_admin_provider_model_from_upsert(&model);
        let (endpoint_ids, source) = match endpoint_ids {
            Some(endpoint_ids) => {
                let endpoint_ids = self
                    .validate_admin_model_endpoint_ids(&model.provider_id, endpoint_ids)
                    .await?;
                (endpoint_ids, source.unwrap_or("manual").to_string())
            }
            None => {
                let (endpoint_ids, source) = self
                    .infer_unambiguous_admin_model_endpoint_ids(
                        &model.provider_id,
                        &prospective_model,
                    )
                    .await?;
                (endpoint_ids, source.to_string())
            }
        };
        CreateAdminProviderModelWithBindingsRecord::new(model, endpoint_ids, source)
            .map(AdminProviderModelCreateMutation::Bound)
            .map_err(|err| GatewayError::Internal(err.to_string()))
    }

    pub(crate) async fn create_admin_provider_model_from_mutation(
        &self,
        mutation: &AdminProviderModelCreateMutation,
    ) -> Result<Option<StoredAdminProviderModel>, GatewayError> {
        match mutation {
            AdminProviderModelCreateMutation::Bound(record) => {
                self.create_admin_provider_model_with_bindings(record).await
            }
            AdminProviderModelCreateMutation::Unbound(record) => {
                self.create_admin_provider_model(record).await
            }
        }
    }

    async fn validate_admin_model_endpoint_ids(
        &self,
        provider_id: &str,
        endpoint_ids: Vec<String>,
    ) -> Result<Vec<String>, GatewayError> {
        let provider_endpoint_ids = self
            .list_provider_catalog_endpoints_by_provider_ids(&[provider_id.to_string()])
            .await?
            .into_iter()
            .map(|endpoint| endpoint.id)
            .collect::<BTreeSet<_>>();
        let mut normalized_endpoint_ids = BTreeSet::new();
        for endpoint_id in endpoint_ids {
            let normalized = endpoint_id.trim();
            if normalized.is_empty() || !provider_endpoint_ids.contains(normalized) {
                return Err(GatewayError::Client {
                    status: http::StatusCode::BAD_REQUEST,
                    message: format!("Endpoint {endpoint_id} 不属于当前 Provider"),
                });
            }
            if !normalized_endpoint_ids.insert(normalized.to_string()) {
                return Err(GatewayError::Client {
                    status: http::StatusCode::BAD_REQUEST,
                    message: format!("Endpoint {normalized} 在绑定列表中重复"),
                });
            }
        }
        if normalized_endpoint_ids.is_empty() {
            return Err(GatewayError::Client {
                status: http::StatusCode::BAD_REQUEST,
                message: "至少选择一个 Endpoint 绑定".to_string(),
            });
        }
        Ok(normalized_endpoint_ids.into_iter().collect())
    }

    pub(crate) async fn infer_admin_model_endpoint_ids(
        &self,
        provider_id: &str,
        model: &StoredAdminProviderModel,
    ) -> Result<(Vec<String>, &'static str), GatewayError> {
        let inference = self
            .infer_admin_model_endpoint_binding(provider_id, model)
            .await?;
        Ok((inference.endpoint_ids, inference.source))
    }

    pub(crate) async fn infer_unambiguous_admin_model_endpoint_ids(
        &self,
        provider_id: &str,
        model: &StoredAdminProviderModel,
    ) -> Result<(Vec<String>, &'static str), GatewayError> {
        let inference = self
            .infer_admin_model_endpoint_binding(provider_id, model)
            .await?;
        if inference.endpoint_ids.is_empty() {
            return Err(GatewayError::Client {
                status: http::StatusCode::BAD_REQUEST,
                message: format!(
                    "模型 {} 无法推断 Endpoint，请明确选择至少一个 Endpoint",
                    model.provider_model_name
                ),
            });
        }
        Ok((inference.endpoint_ids, inference.source))
    }

    async fn infer_admin_model_endpoint_binding(
        &self,
        provider_id: &str,
        model: &StoredAdminProviderModel,
    ) -> Result<AdminModelEndpointInference, GatewayError> {
        let global_model = self
            .get_admin_global_model_by_id(&model.global_model_id)
            .await?;
        let mut model = model.clone();
        if let Some(global_model) = global_model {
            model.global_model_name = Some(global_model.name);
            model.global_model_supported_capabilities = global_model.supported_capabilities;
            model.global_model_config = global_model.config;
        }
        let endpoints = self
            .list_provider_catalog_endpoints_by_provider_ids(&[provider_id.to_string()])
            .await?;
        let valid_endpoint_ids = endpoints
            .iter()
            .map(|endpoint| endpoint.id.as_str())
            .collect::<BTreeSet<_>>();
        let declared_endpoint_ids = model
            .provider_model_mappings
            .as_ref()
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|mapping| {
                mapping
                    .get("endpoint_ids")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .filter_map(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|endpoint_id| !endpoint_id.is_empty())
            .map(ToOwned::to_owned)
            .collect::<BTreeSet<_>>();
        if let Some(invalid_endpoint_id) = declared_endpoint_ids
            .iter()
            .find(|endpoint_id| !valid_endpoint_ids.contains(endpoint_id.as_str()))
        {
            return Err(GatewayError::Client {
                status: http::StatusCode::BAD_REQUEST,
                message: format!("Endpoint {invalid_endpoint_id} 不属于当前 Provider"),
            });
        }
        let mapped_endpoint_ids = declared_endpoint_ids;
        if !mapped_endpoint_ids.is_empty() {
            return Ok(AdminModelEndpointInference {
                endpoint_ids: mapped_endpoint_ids.into_iter().collect(),
                source: "mapping",
            });
        }

        let keys = self
            .list_provider_catalog_keys_by_provider_ids(&[provider_id.to_string()])
            .await?;
        let mut cached_models = Vec::new();
        for key in keys {
            let cache_key = format!("upstream_models:{provider_id}:{}", key.id);
            let Some(raw) = self.runtime_state().kv_get(&cache_key).await.ok().flatten() else {
                continue;
            };
            let Ok(models) = serde_json::from_str::<Vec<serde_json::Value>>(&raw) else {
                continue;
            };
            cached_models.extend(models);
        }
        let discovered_endpoint_ids =
            aether_model_fetch::aggregate_models_for_cache(&cached_models)
                .into_iter()
                .filter(|item| {
                    item.get("id")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|id| {
                            aether_model_fetch::provider_model_matches_discovered_model(&model, id)
                        })
                })
                .flat_map(|item| {
                    item.get("endpoint_ids")
                        .and_then(serde_json::Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                })
                .filter_map(|value| value.as_str().map(str::trim).map(ToOwned::to_owned))
                .filter(|endpoint_id| valid_endpoint_ids.contains(endpoint_id.as_str()))
                .collect::<BTreeSet<_>>();
        if !discovered_endpoint_ids.is_empty() {
            return Ok(AdminModelEndpointInference {
                endpoint_ids: discovered_endpoint_ids.into_iter().collect(),
                source: "discovered",
            });
        }

        let mapped_api_formats = model
            .provider_model_mappings
            .as_ref()
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|mapping| {
                mapping
                    .get("api_formats")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .filter_map(serde_json::Value::as_str)
            .map(crate::ai_serving::normalize_api_format_alias)
            .collect::<BTreeSet<_>>();
        let mapped_format_endpoint_ids = endpoints
            .iter()
            .filter(|endpoint| {
                mapped_api_formats.contains(&crate::ai_serving::normalize_api_format_alias(
                    &endpoint.api_format,
                ))
            })
            .map(|endpoint| endpoint.id.clone())
            .collect::<Vec<_>>();
        if !mapped_format_endpoint_ids.is_empty() {
            return Ok(AdminModelEndpointInference {
                endpoint_ids: mapped_format_endpoint_ids,
                source: "mapping",
            });
        }

        // Global Model 元数据只作为第四优先级证据，不覆盖显式绑定、发现结果或 Provider Model 映射。
        let declared_global_families = crate::model_metadata::global_model_declared_families(
            model.global_model_config.as_ref(),
            model.global_model_supported_capabilities.as_ref(),
        );
        let metadata_endpoint_ids = endpoints
            .iter()
            .filter(|endpoint| declared_global_families.supports_api_format(&endpoint.api_format))
            .map(|endpoint| endpoint.id.clone())
            .collect::<Vec<_>>();
        if !metadata_endpoint_ids.is_empty() {
            return Ok(AdminModelEndpointInference {
                endpoint_ids: metadata_endpoint_ids,
                source: "mapping",
            });
        }

        let endpoint_ids = endpoints
            .into_iter()
            .map(|endpoint| endpoint.id)
            .collect::<Vec<_>>();
        if endpoint_ids.len() == 1 {
            return Ok(AdminModelEndpointInference {
                endpoint_ids,
                source: "migration",
            });
        }
        Ok(AdminModelEndpointInference {
            endpoint_ids: Vec::new(),
            source: "migration",
        })
    }

    pub(crate) async fn sync_admin_model_endpoint_bindings(
        &self,
        provider_id: &str,
        model: &StoredAdminProviderModel,
        replace_automatic: bool,
    ) -> Result<(), GatewayError> {
        let (endpoint_ids, source) = self
            .infer_admin_model_endpoint_ids(provider_id, model)
            .await?;
        if !endpoint_ids.is_empty() || replace_automatic {
            let replacement_scope_endpoint_ids = if replace_automatic {
                self.list_provider_catalog_endpoints_by_provider_ids(&[provider_id.to_string()])
                    .await?
                    .into_iter()
                    .map(|endpoint| endpoint.id)
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            self.sync_model_endpoint_bindings(
                &model.id,
                &endpoint_ids,
                source,
                replace_automatic,
                &replacement_scope_endpoint_ids,
            )
            .await?;
        }
        Ok(())
    }

    pub(crate) async fn admin_provider_model_name_exists(
        &self,
        provider_id: &str,
        provider_model_name: &str,
        exclude_model_id: Option<&str>,
    ) -> Result<bool, GatewayError> {
        let target = provider_model_name.trim();
        if target.is_empty() {
            return Ok(false);
        }
        let models = self
            .list_admin_provider_models(
                &aether_data_contracts::repository::global_models::AdminProviderModelListQuery {
                    provider_id: provider_id.to_string(),
                    is_active: None,
                    offset: 0,
                    limit: 10_000,
                },
            )
            .await?;
        Ok(models.into_iter().any(|model| {
            model.provider_model_name == target
                && exclude_model_id.is_none_or(|exclude| model.id != exclude)
        }))
    }

    pub(crate) async fn resolve_admin_global_model_by_id_or_err(
        &self,
        global_model_id: &str,
    ) -> Result<aether_data_contracts::repository::global_models::StoredAdminGlobalModel, String>
    {
        self.get_admin_global_model_by_id(global_model_id)
            .await
            .map_err(|err| format!("{err:?}"))?
            .ok_or_else(|| format!("GlobalModel {global_model_id} 不存在"))
    }

    pub(crate) async fn build_admin_provider_available_source_models_payload(
        &self,
        provider_id: &str,
    ) -> Option<serde_json::Value> {
        if !self.has_global_model_data_reader() || !self.has_provider_catalog_data_reader() {
            return None;
        }
        let provider = self
            .read_provider_catalog_providers_by_ids(&[provider_id.to_string()])
            .await
            .ok()?
            .into_iter()
            .next()?;
        let models = self
            .list_admin_provider_available_source_models(&provider.id)
            .await
            .ok()?;
        Some(
            admin_provider_models_pure::build_admin_provider_available_source_models_payload(
                models,
            ),
        )
    }

    pub(crate) async fn build_admin_provider_model_create_record(
        &self,
        provider_id: &str,
        payload: AdminProviderModelCreateRequest,
    ) -> Result<UpsertAdminProviderModelRecord, String> {
        let provider_model_name =
            admin_provider_models_write_pure::normalize_required_trimmed_string(
                &payload.provider_model_name,
                "provider_model_name",
            )?;
        if self
            .admin_provider_model_name_exists(provider_id, &provider_model_name, None)
            .await
            .map_err(|err| format!("{err:?}"))?
        {
            return Err(format!("模型 '{provider_model_name}' 已存在"));
        }
        let global_model_id = admin_provider_models_write_pure::normalize_required_trimmed_string(
            &payload.global_model_id,
            "global_model_id",
        )?;
        self.resolve_admin_global_model_by_id_or_err(&global_model_id)
            .await?;
        let price_per_request = admin_provider_models_write_pure::normalize_optional_price(
            payload.price_per_request,
            "price_per_request",
        )?;
        let tiered_pricing = normalize_json_object(payload.tiered_pricing, "tiered_pricing")?;
        let provider_model_mappings = normalize_provider_model_mapping_scopes(
            normalize_json_array(payload.provider_model_mappings, "provider_model_mappings")?,
        );
        let config = normalize_json_object(payload.config, "config")?;
        admin_provider_models_write_pure::build_admin_provider_model_create_record(
            Uuid::new_v4().to_string(),
            provider_id.to_string(),
            global_model_id,
            provider_model_name,
            provider_model_mappings,
            price_per_request,
            tiered_pricing,
            payload.supports_vision,
            payload.supports_function_calling,
            payload.supports_streaming,
            payload.supports_extended_thinking,
            payload.supports_image_generation,
            payload.is_active,
            config,
        )
    }

    pub(crate) async fn build_admin_provider_model_update_record(
        &self,
        existing: &StoredAdminProviderModel,
        patch: AdminProviderModelUpdatePatch,
    ) -> Result<UpsertAdminProviderModelRecord, String> {
        let (fields, payload) = patch.into_parts();
        let provider_model_name = if fields.contains("provider_model_name") {
            let Some(name) = payload.provider_model_name.as_deref() else {
                return Err(if fields.is_null("provider_model_name") {
                    "provider_model_name 不能为空".to_string()
                } else {
                    "provider_model_name 必须是字符串".to_string()
                });
            };
            let name = admin_provider_models_write_pure::normalize_required_trimmed_string(
                name,
                "provider_model_name",
            )?;
            if self
                .admin_provider_model_name_exists(&existing.provider_id, &name, Some(&existing.id))
                .await
                .map_err(|err| format!("{err:?}"))?
            {
                return Err(format!("模型 '{name}' 已存在"));
            }
            name
        } else {
            existing.provider_model_name.clone()
        };

        let global_model_id = if fields.contains("global_model_id") {
            let Some(global_model_id) = payload.global_model_id.as_deref() else {
                return Err(if fields.is_null("global_model_id") {
                    "global_model_id 不能为空".to_string()
                } else {
                    "global_model_id 必须是字符串".to_string()
                });
            };
            let global_model_id =
                admin_provider_models_write_pure::normalize_required_trimmed_string(
                    global_model_id,
                    "global_model_id",
                )?;
            self.resolve_admin_global_model_by_id_or_err(&global_model_id)
                .await?;
            global_model_id
        } else {
            existing.global_model_id.clone()
        };

        let price_per_request = if fields.contains("price_per_request") {
            admin_provider_models_write_pure::normalize_optional_price(
                payload.price_per_request,
                "price_per_request",
            )?
        } else {
            existing.price_per_request
        };
        let tiered_pricing = if fields.contains("tiered_pricing") {
            normalize_json_object(payload.tiered_pricing, "tiered_pricing")?
        } else {
            existing.tiered_pricing.clone()
        };
        let provider_model_mappings = if fields.contains("provider_model_mappings") {
            normalize_provider_model_mapping_scopes(normalize_json_array(
                payload.provider_model_mappings,
                "provider_model_mappings",
            )?)
        } else {
            existing.provider_model_mappings.clone()
        };
        let config = if fields.contains("config") {
            normalize_json_object(payload.config, "config")?
        } else {
            existing.config.clone()
        };

        admin_provider_models_write_pure::build_admin_provider_model_update_record(
            existing,
            global_model_id,
            provider_model_name,
            provider_model_mappings,
            price_per_request,
            tiered_pricing,
            if fields.contains("supports_vision") {
                payload.supports_vision
            } else {
                existing.supports_vision
            },
            if fields.contains("supports_function_calling") {
                payload.supports_function_calling
            } else {
                existing.supports_function_calling
            },
            if fields.contains("supports_streaming") {
                payload.supports_streaming
            } else {
                existing.supports_streaming
            },
            if fields.contains("supports_extended_thinking") {
                payload.supports_extended_thinking
            } else {
                existing.supports_extended_thinking
            },
            if fields.contains("supports_image_generation") {
                payload.supports_image_generation
            } else {
                existing.supports_image_generation
            },
            payload.is_active.unwrap_or(existing.is_active),
            payload.is_available.unwrap_or(existing.is_available),
            config,
        )
    }

    pub(crate) async fn build_admin_import_provider_models_payload(
        &self,
        provider_id: &str,
        payload: AdminImportProviderModelsRequest,
    ) -> Result<serde_json::Value, String> {
        let tiered_pricing = normalize_json_object(payload.tiered_pricing, "tiered_pricing")?;

        let provider_endpoints = self
            .list_provider_catalog_endpoints_by_provider_ids(&[provider_id.to_string()])
            .await
            .map_err(|err| format!("{err:?}"))?;
        let provider_endpoint_ids = provider_endpoints
            .iter()
            .map(|endpoint| endpoint.id.as_str())
            .collect::<BTreeSet<_>>();
        let mut import_sources = BTreeMap::new();
        for source in payload.models {
            let normalized_id = source.id.trim().to_ascii_lowercase();
            if !normalized_id.is_empty() {
                import_sources.insert(normalized_id, source);
            }
        }
        let mut requested_model_ids = payload.model_ids;
        for source in import_sources.values() {
            if !requested_model_ids
                .iter()
                .any(|model_id| model_id.trim().eq_ignore_ascii_case(source.id.trim()))
            {
                requested_model_ids.push(source.id.clone());
            }
        }

        let existing_models = self
            .list_admin_provider_models(&AdminProviderModelListQuery {
                provider_id: provider_id.to_string(),
                is_active: None,
                offset: 0,
                limit: 10_000,
            })
            .await
            .map_err(|err| format!("{err:?}"))?;
        let mut existing_by_name = existing_models
            .iter()
            .map(|model| (model.provider_model_name.clone(), model.clone()))
            .collect::<BTreeMap<_, _>>();

        let mut success = Vec::new();
        let mut errors = Vec::new();

        for model_id in requested_model_ids {
            let trimmed = match admin_provider_models_write_pure::normalize_admin_import_model_id(
                &model_id,
            ) {
                Ok(value) => value,
                Err(detail) => {
                    let raw = model_id.trim();
                    errors.push(json!({
                        "model_id": if raw.is_empty() { "<empty>" } else { raw },
                        "error": detail,
                    }));
                    continue;
                }
            };

            let source = import_sources.get(&trimmed.to_ascii_lowercase());
            let endpoint_ids = match source {
                Some(source) => {
                    let explicit_endpoint_ids = source
                        .endpoint_ids
                        .iter()
                        .map(|endpoint_id| endpoint_id.trim())
                        .filter(|endpoint_id| !endpoint_id.is_empty())
                        .collect::<BTreeSet<_>>();
                    if let Some(invalid_endpoint_id) = explicit_endpoint_ids
                        .iter()
                        .find(|endpoint_id| !provider_endpoint_ids.contains(**endpoint_id))
                    {
                        errors.push(json!({
                            "model_id": trimmed,
                            "error": format!("Endpoint {invalid_endpoint_id} 不属于当前 Provider"),
                        }));
                        continue;
                    }
                    if explicit_endpoint_ids.is_empty() {
                        let formats = source
                            .api_formats
                            .iter()
                            .map(|format| crate::ai_serving::normalize_api_format_alias(format))
                            .collect::<BTreeSet<_>>();
                        provider_endpoints
                            .iter()
                            .filter(|endpoint| {
                                formats.contains(&crate::ai_serving::normalize_api_format_alias(
                                    &endpoint.api_format,
                                ))
                            })
                            .map(|endpoint| endpoint.id.clone())
                            .collect::<Vec<_>>()
                    } else {
                        explicit_endpoint_ids
                            .into_iter()
                            .map(ToOwned::to_owned)
                            .collect::<Vec<_>>()
                    }
                }
                None => Vec::new(),
            };

            if let Some(existing) = existing_by_name.get(trimmed.as_str()) {
                let sync_result = if !endpoint_ids.is_empty() {
                    self.sync_model_endpoint_bindings(
                        &existing.id,
                        &endpoint_ids,
                        "discovered",
                        false,
                        &[],
                    )
                    .await
                    .map(|_| ())
                } else {
                    self.sync_admin_model_endpoint_bindings(provider_id, existing, false)
                        .await
                };
                if let Err(err) = sync_result {
                    errors.push(json!({
                        "model_id": trimmed,
                        "error": err.into_message(),
                    }));
                    continue;
                }
                let bindings = self
                    .list_model_endpoint_bindings(std::slice::from_ref(&existing.id))
                    .await
                    .map_err(|err| format!("{err:?}"))?;
                if !bindings.iter().any(|binding| binding.is_active) {
                    errors.push(json!({
                        "model_id": trimmed,
                        "error": "模型没有可用的 Endpoint 绑定",
                    }));
                    continue;
                }
                success.push(json!({
                    "model_id": trimmed,
                    "global_model_id": existing.global_model_id,
                    "global_model_name": existing.global_model_name,
                    "provider_model_id": existing.id,
                    "created_global_model": false,
                }));
                continue;
            }

            let provisional_record =
                admin_provider_models_write_pure::build_admin_import_provider_model_record(
                    Uuid::new_v4().to_string(),
                    provider_id.to_string(),
                    "pending-global-model".to_string(),
                    trimmed.to_string(),
                    payload.price_per_request,
                    tiered_pricing.clone(),
                )?;
            let resolved_endpoint_ids = if endpoint_ids.is_empty() {
                let prospective_model =
                    stored_admin_provider_model_from_upsert(&provisional_record);
                match self
                    .infer_unambiguous_admin_model_endpoint_ids(provider_id, &prospective_model)
                    .await
                {
                    Ok((endpoint_ids, _)) => endpoint_ids,
                    Err(GatewayError::Client { message, .. }) => {
                        errors.push(json!({"model_id": trimmed, "error": message}));
                        continue;
                    }
                    Err(err) => return Err(format!("{err:?}")),
                }
            } else {
                endpoint_ids
            };

            let mut created_global_model = false;
            let global_model = if let Some(existing) = self
                .get_admin_global_model_by_name(&trimmed)
                .await
                .map_err(|err| format!("{err:?}"))?
            {
                existing
            } else {
                let created = self
                    .create_admin_global_model(
                        &admin_provider_models_write_pure::build_admin_import_global_model_record(
                            Uuid::new_v4().to_string(),
                            trimmed.to_string(),
                            payload.price_per_request,
                            tiered_pricing.clone(),
                        )
                        .map_err(|err| err.to_string())?,
                    )
                    .await
                    .map_err(|err| format!("{err:?}"))?;
                let Some(created) = created else {
                    errors.push(json!({"model_id": trimmed, "error": "Create GlobalModel failed"}));
                    continue;
                };
                created_global_model = true;
                created
            };

            let record =
                admin_provider_models_write_pure::build_admin_import_provider_model_record(
                    Uuid::new_v4().to_string(),
                    provider_id.to_string(),
                    global_model.id.clone(),
                    trimmed.to_string(),
                    payload.price_per_request,
                    tiered_pricing.clone(),
                )?;

            let mutation = self
                .build_admin_provider_model_create_mutation(
                    record,
                    Some(resolved_endpoint_ids),
                    Some("discovered"),
                )
                .await;
            let mutation = match mutation {
                Ok(mutation) => mutation,
                Err(GatewayError::Client { message, .. }) => {
                    if created_global_model {
                        self.delete_unreferenced_admin_global_model(&global_model.id)
                            .await
                            .map_err(|err| format!("清理未完成的 GlobalModel 失败: {err:?}"))?;
                    }
                    errors.push(json!({"model_id": trimmed, "error": message}));
                    continue;
                }
                Err(err) => {
                    if created_global_model {
                        self.delete_unreferenced_admin_global_model(&global_model.id)
                            .await
                            .map_err(|cleanup_err| {
                                format!(
                                    "构建 Provider Model 失败: {err:?}; 清理未完成的 GlobalModel 失败: {cleanup_err:?}"
                                )
                            })?;
                    }
                    return Err(format!("{err:?}"));
                }
            };
            match self
                .create_admin_provider_model_from_mutation(&mutation)
                .await
            {
                Ok(Some(created)) => {
                    existing_by_name.insert(trimmed.to_string(), created.clone());
                    success.push(json!({
                        "model_id": trimmed,
                        "global_model_id": global_model.id,
                        "global_model_name": global_model.name,
                        "provider_model_id": created.id,
                        "created_global_model": created_global_model,
                    }));
                }
                Ok(None) => {
                    if created_global_model {
                        self.delete_unreferenced_admin_global_model(&global_model.id)
                            .await
                            .map_err(|err| format!("清理未完成的 GlobalModel 失败: {err:?}"))?;
                    }
                    errors.push(json!({
                        "model_id": trimmed,
                        "error": "Create provider model failed",
                    }));
                }
                Err(err) => {
                    if created_global_model {
                        self.delete_unreferenced_admin_global_model(&global_model.id)
                            .await
                            .map_err(|cleanup_err| {
                                format!(
                                    "创建 Provider Model 失败: {err:?}; 清理未完成的 GlobalModel 失败: {cleanup_err:?}"
                                )
                            })?;
                    }
                    errors.push(json!({
                        "model_id": trimmed,
                        "error": format!("{err:?}"),
                    }));
                }
            }
        }

        Ok(json!({
            "success": success,
            "errors": errors,
        }))
    }

    pub(crate) async fn build_admin_batch_assign_global_models_payload(
        &self,
        provider_id: &str,
        global_model_ids: Vec<String>,
    ) -> Result<serde_json::Value, String> {
        let existing_models = self
            .list_admin_provider_models(&AdminProviderModelListQuery {
                provider_id: provider_id.to_string(),
                is_active: None,
                offset: 0,
                limit: 10_000,
            })
            .await
            .map_err(|err| format!("{err:?}"))?;
        let existing_global_model_ids = existing_models
            .into_iter()
            .map(|model| model.global_model_id)
            .collect::<std::collections::BTreeSet<_>>();

        let mut success = Vec::new();
        let mut errors = Vec::new();
        for global_model_id in global_model_ids {
            let global_model_id = global_model_id.trim().to_string();
            if global_model_id.is_empty() {
                continue;
            }
            let global_model = match self
                .resolve_admin_global_model_by_id_or_err(&global_model_id)
                .await
            {
                Ok(model) => model,
                Err(detail) => {
                    errors.push(json!({
                        "global_model_id": global_model_id,
                        "error": detail,
                    }));
                    continue;
                }
            };
            if existing_global_model_ids.contains(&global_model.id) {
                errors.push(json!({
                    "global_model_id": global_model.id,
                    "error": "Model already exists",
                }));
                continue;
            }
            let record =
                admin_provider_models_write_pure::build_admin_batch_assign_provider_model_record(
                    Uuid::new_v4().to_string(),
                    provider_id.to_string(),
                    global_model.id.clone(),
                    global_model.name.clone(),
                )?;
            let mutation = self
                .build_admin_provider_model_create_mutation(record, None, None)
                .await;
            let mutation = match mutation {
                Ok(mutation) => mutation,
                Err(GatewayError::Client { message, .. }) => {
                    errors.push(json!({
                        "global_model_id": global_model.id,
                        "error": message,
                    }));
                    continue;
                }
                Err(err) => return Err(format!("{err:?}")),
            };
            match self
                .create_admin_provider_model_from_mutation(&mutation)
                .await
            {
                Ok(Some(created)) => {
                    success.push(json!({
                        "global_model_id": global_model.id,
                        "global_model_name": global_model.name,
                        "provider_model_id": created.id,
                    }));
                }
                Ok(None) => errors.push(json!({
                    "global_model_id": global_model.id,
                    "error": "Create provider model failed",
                })),
                Err(err) => errors.push(json!({
                    "global_model_id": global_model.id,
                    "error": format!("{err:?}"),
                })),
            }
        }
        Ok(json!({
            "success": success,
            "errors": errors,
        }))
    }

    pub(crate) async fn read_admin_external_models_cache(
        &self,
        request_id: &str,
    ) -> Result<Option<serde_json::Value>, GatewayError> {
        crate::handlers::admin::model::read_admin_external_models_cache(self, request_id).await
    }

    pub(crate) async fn build_admin_external_models_config_payload(
        &self,
    ) -> Result<serde_json::Value, GatewayError> {
        crate::handlers::admin::model::build_admin_external_models_config_payload(self).await
    }

    pub(crate) async fn apply_admin_external_models_config_update(
        &self,
        request_body: &axum::body::Bytes,
    ) -> Result<Result<serde_json::Value, (http::StatusCode, serde_json::Value)>, GatewayError>
    {
        crate::handlers::admin::model::apply_admin_external_models_config_update(self, request_body)
            .await
    }

    pub(crate) async fn clear_admin_external_models_cache(
        &self,
    ) -> Result<serde_json::Value, GatewayError> {
        crate::handlers::admin::model::clear_admin_external_models_cache(self).await
    }

    pub(crate) async fn list_admin_provider_models(
        &self,
        query: &aether_data_contracts::repository::global_models::AdminProviderModelListQuery,
    ) -> Result<
        Vec<aether_data_contracts::repository::global_models::StoredAdminProviderModel>,
        GatewayError,
    > {
        self.app.list_admin_provider_models(query).await
    }

    pub(crate) async fn list_admin_provider_available_source_models(
        &self,
        provider_id: &str,
    ) -> Result<
        Vec<aether_data_contracts::repository::global_models::StoredAdminProviderModel>,
        GatewayError,
    > {
        self.app
            .list_admin_provider_available_source_models(provider_id)
            .await
    }

    pub(crate) async fn get_admin_provider_model(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Result<
        Option<aether_data_contracts::repository::global_models::StoredAdminProviderModel>,
        GatewayError,
    > {
        self.app
            .get_admin_provider_model(provider_id, model_id)
            .await
    }

    pub(crate) async fn list_model_endpoint_bindings(
        &self,
        model_ids: &[String],
    ) -> Result<Vec<StoredModelEndpointBinding>, GatewayError> {
        self.app.list_model_endpoint_bindings(model_ids).await
    }

    pub(crate) async fn sync_model_endpoint_bindings(
        &self,
        model_id: &str,
        endpoint_ids: &[String],
        source: &str,
        replace_automatic: bool,
        replacement_scope_endpoint_ids: &[String],
    ) -> Result<Vec<StoredModelEndpointBinding>, GatewayError> {
        self.app
            .sync_model_endpoint_bindings(
                model_id,
                endpoint_ids,
                source,
                replace_automatic,
                replacement_scope_endpoint_ids,
            )
            .await
    }

    pub(crate) async fn upsert_model_endpoint_binding(
        &self,
        record: &UpsertModelEndpointBindingRecord,
    ) -> Result<Option<StoredModelEndpointBinding>, GatewayError> {
        self.app.upsert_model_endpoint_binding(record).await
    }

    pub(crate) async fn get_admin_global_model_by_id(
        &self,
        global_model_id: &str,
    ) -> Result<
        Option<aether_data_contracts::repository::global_models::StoredAdminGlobalModel>,
        GatewayError,
    > {
        self.app.get_admin_global_model_by_id(global_model_id).await
    }

    pub(crate) async fn get_admin_global_model_by_name(
        &self,
        model_name: &str,
    ) -> Result<
        Option<aether_data_contracts::repository::global_models::StoredAdminGlobalModel>,
        GatewayError,
    > {
        self.app.get_admin_global_model_by_name(model_name).await
    }

    pub(crate) async fn create_admin_provider_model(
        &self,
        record: &aether_data_contracts::repository::global_models::UpsertAdminProviderModelRecord,
    ) -> Result<
        Option<aether_data_contracts::repository::global_models::StoredAdminProviderModel>,
        GatewayError,
    > {
        self.app.create_admin_provider_model(record).await
    }

    pub(crate) async fn create_admin_provider_model_with_bindings(
        &self,
        record: &CreateAdminProviderModelWithBindingsRecord,
    ) -> Result<Option<StoredAdminProviderModel>, GatewayError> {
        self.app
            .create_admin_provider_model_with_bindings(record)
            .await
    }

    pub(crate) async fn update_admin_provider_model(
        &self,
        record: &aether_data_contracts::repository::global_models::UpsertAdminProviderModelRecord,
    ) -> Result<
        Option<aether_data_contracts::repository::global_models::StoredAdminProviderModel>,
        GatewayError,
    > {
        self.app.update_admin_provider_model(record).await
    }

    pub(crate) async fn update_admin_provider_model_with_bindings(
        &self,
        record: &aether_data_contracts::repository::global_models::UpdateAdminProviderModelWithBindingsRecord,
    ) -> Result<
        Option<aether_data_contracts::repository::global_models::StoredAdminProviderModel>,
        GatewayError,
    > {
        self.app
            .update_admin_provider_model_with_bindings(record)
            .await
    }

    pub(crate) async fn delete_admin_provider_model(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Result<bool, GatewayError> {
        self.app
            .delete_admin_provider_model(provider_id, model_id)
            .await
    }

    pub(crate) async fn create_admin_global_model(
        &self,
        record: &aether_data_contracts::repository::global_models::CreateAdminGlobalModelRecord,
    ) -> Result<
        Option<aether_data_contracts::repository::global_models::StoredAdminGlobalModel>,
        GatewayError,
    > {
        self.app.create_admin_global_model(record).await
    }

    pub(crate) async fn list_admin_global_models(
        &self,
        query: &aether_data_contracts::repository::global_models::AdminGlobalModelListQuery,
    ) -> Result<
        aether_data_contracts::repository::global_models::StoredAdminGlobalModelPage,
        GatewayError,
    > {
        self.app.list_admin_global_models(query).await
    }

    pub(crate) async fn list_admin_provider_models_by_global_model_id(
        &self,
        global_model_id: &str,
    ) -> Result<
        Vec<aether_data_contracts::repository::global_models::StoredAdminProviderModel>,
        GatewayError,
    > {
        self.app
            .list_admin_provider_models_by_global_model_id(global_model_id)
            .await
    }

    pub(crate) async fn update_admin_global_model(
        &self,
        record: &aether_data_contracts::repository::global_models::UpdateAdminGlobalModelRecord,
    ) -> Result<
        Option<aether_data_contracts::repository::global_models::StoredAdminGlobalModel>,
        GatewayError,
    > {
        self.app.update_admin_global_model(record).await
    }

    pub(crate) async fn delete_admin_global_model(
        &self,
        global_model_id: &str,
    ) -> Result<bool, GatewayError> {
        self.app.delete_admin_global_model(global_model_id).await
    }

    pub(crate) async fn delete_unreferenced_admin_global_model(
        &self,
        global_model_id: &str,
    ) -> Result<bool, GatewayError> {
        self.app
            .delete_unreferenced_admin_global_model(global_model_id)
            .await
    }
}
