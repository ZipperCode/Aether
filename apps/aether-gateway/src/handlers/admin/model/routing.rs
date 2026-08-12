use super::resolve_admin_global_model_by_id_or_err;
use crate::handlers::admin::request::AdminAppState;
use crate::handlers::admin::shared::{json_string_list, provider_catalog_key_supports_format};
use aether_data_contracts::repository::global_models::{
    AdminProviderModelListQuery, UpsertAdminProviderModelRecord,
};
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey,
};
use aether_scheduler_core::{
    compiled_model_mappings, is_provider_key_circuit_open_at,
    provider_key_circuit_payload_is_active_open_at, provider_key_health_score,
    CompiledModelMappings,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub(crate) async fn build_admin_global_model_routing_payload(
    state: &AdminAppState<'_>,
    global_model_id: &str,
    include_whitelist: bool,
) -> Option<serde_json::Value> {
    if !state.has_global_model_data_reader() || !state.has_provider_catalog_data_reader() {
        return None;
    }
    let global_model = state
        .get_admin_global_model_by_id(global_model_id)
        .await
        .ok()??;
    let provider_models = state
        .list_admin_provider_models_by_global_model_id(global_model_id)
        .await
        .ok()?;
    let provider_ids = provider_models
        .iter()
        .map(|model| model.provider_id.clone())
        .collect::<Vec<_>>();
    let model_ids = provider_models
        .iter()
        .map(|model| model.id.clone())
        .collect::<Vec<_>>();
    let model_bindings = state.list_model_endpoint_bindings(&model_ids).await.ok()?;
    let mut runtime_quarantines_by_model_and_endpoint = BTreeMap::new();
    for entry in state.app().endpoint_capability_quarantine_snapshot() {
        runtime_quarantines_by_model_and_endpoint
            .entry((
                entry.model_id().to_string(),
                entry.endpoint_id().to_string(),
            ))
            .or_insert_with(Vec::new)
            .push(json!({
                "key_id": entry.key_id(),
                "client_api_format": entry.client_api_format(),
                "request_mode": entry.request_mode(),
                "request_operation": entry.request_operation(),
            }));
    }
    let mut bindings_by_model_and_endpoint = BTreeMap::new();
    for binding in model_bindings {
        bindings_by_model_and_endpoint.insert(
            (binding.model_id.clone(), binding.endpoint_id.clone()),
            binding,
        );
    }
    let providers = state
        .read_provider_catalog_providers_by_ids(&provider_ids)
        .await
        .ok()?
        .into_iter()
        .map(|provider| (provider.id.clone(), provider))
        .collect::<BTreeMap<_, _>>();
    let endpoints = state
        .list_provider_catalog_endpoints_by_provider_ids(&provider_ids)
        .await
        .ok()
        .unwrap_or_default();
    let keys = state
        .list_provider_catalog_keys_by_provider_ids(&provider_ids)
        .await
        .ok()
        .unwrap_or_default();
    let mut endpoints_by_provider = BTreeMap::<String, Vec<StoredProviderCatalogEndpoint>>::new();
    for endpoint in endpoints {
        endpoints_by_provider
            .entry(endpoint.provider_id.clone())
            .or_default()
            .push(endpoint);
    }
    let mut keys_by_provider = BTreeMap::<String, Vec<StoredProviderCatalogKey>>::new();
    for key in keys {
        keys_by_provider
            .entry(key.provider_id.clone())
            .or_default()
            .push(key);
    }

    let scheduling_mode = state
        .read_system_config_json_value("scheduling_mode")
        .await
        .ok()
        .flatten()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "cache_affinity".to_string());
    let priority_mode = state
        .read_system_config_json_value("provider_priority_mode")
        .await
        .ok()
        .flatten()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "provider".to_string());
    let keep_priority_on_conversion = state
        .read_system_config_json_value("keep_priority_on_conversion")
        .await
        .ok()
        .flatten()
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let now_unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();

    let global_model_mappings = global_model
        .config
        .as_ref()
        .and_then(|value| value.get("model_mappings"))
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let compiled_global_model_mappings = compiled_model_mappings(&global_model_mappings);

    let mut providers_payload = Vec::new();
    let mut all_keys_whitelist = Vec::new();
    for model in provider_models {
        let Some(provider) = providers.get(&model.provider_id) else {
            continue;
        };
        let provider_model_mapping_names =
            provider_model_mapping_names_for_routing(model.provider_model_mappings.as_ref());
        let key_match_model_names = key_match_model_names_for_routing(
            &global_model.name,
            &model.provider_model_name,
            &provider_model_mapping_names,
        );
        let mut endpoint_payloads = Vec::new();
        let mut active_endpoints = 0usize;
        for endpoint in endpoints_by_provider
            .get(&provider.id)
            .cloned()
            .unwrap_or_default()
        {
            let model_binding =
                bindings_by_model_and_endpoint.get(&(model.id.clone(), endpoint.id.clone()));
            let runtime_capability_quarantines = runtime_quarantines_by_model_and_endpoint
                .remove(&(model.id.clone(), endpoint.id.clone()))
                .unwrap_or_default();
            let binding_is_active = model_binding.is_some_and(|binding| binding.is_active);
            let endpoint_is_active = routing_endpoint_is_active(
                provider.is_active,
                model.is_active,
                model.is_available,
                endpoint.is_active,
                binding_is_active,
            );
            if endpoint_is_active {
                active_endpoints += 1;
            }
            let mut endpoint_keys = keys_by_provider
                .get(&provider.id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|key| {
                    provider_catalog_key_supports_format(
                        key,
                        provider.provider_type.as_str(),
                        &endpoint.api_format,
                    )
                })
                .filter(|key| {
                    key_allowed_models_match_global_model_for_routing(
                        key.allowed_models.as_ref(),
                        &key_match_model_names,
                        &compiled_global_model_mappings,
                    )
                })
                .collect::<Vec<_>>();
            endpoint_keys.sort_by(|left, right| {
                left.internal_priority
                    .cmp(&right.internal_priority)
                    .then_with(|| left.id.cmp(&right.id))
            });
            let key_payloads = endpoint_keys
                .iter()
                .map(|key| {
                    let effective_rpm = key.learned_rpm_limit.or(key.rpm_limit);
                    let is_adaptive = key.rpm_limit.is_none();
                    let allowed_models = json_string_list(key.allowed_models.as_ref());
                    let matched_models = matched_model_names_for_routing(
                        key.allowed_models.as_ref(),
                        &global_model.name,
                        &compiled_global_model_mappings,
                    );
                    let circuit_breaker_formats = key
                        .circuit_breaker_by_format
                        .as_ref()
                        .and_then(serde_json::Value::as_object)
                        .map(|entries| {
                            entries
                                .iter()
                                .filter_map(|(api_format, value)| {
                                    provider_key_circuit_payload_is_active_open_at(
                                        value,
                                        now_unix_secs,
                                    )
                                    .then_some(())
                                        .map(|_| api_format.clone())
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let next_probe_at = key
                        .circuit_breaker_by_format
                        .as_ref()
                        .and_then(serde_json::Value::as_object)
                        .and_then(|entries| entries.get(&endpoint.api_format))
                        .and_then(|value| value.get("next_probe_at"))
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned);
                    let payload = json!({
                        "id": key.id,
                        "name": key.name,
                        "masked_key": state.masked_catalog_api_key_for_provider(
                            key,
                            &provider.provider_type,
                        ),
                        "is_active": key.is_active,
                        "is_adaptive": is_adaptive,
                        "effective_rpm": effective_rpm,
                        "allowed_models": allowed_models,
                        "matched_models": matched_models,
                        "health_score": provider_key_health_score(key, &endpoint.api_format),
                        "circuit_breaker_open": is_provider_key_circuit_open_at(key, &endpoint.api_format, now_unix_secs),
                        "circuit_breaker_formats": circuit_breaker_formats,
                        "next_probe_at": next_probe_at,
                    });
                    payload
                })
                .collect::<Vec<_>>();
            endpoint_payloads.push(json!({
                "id": endpoint.id,
                "api_format": endpoint.api_format,
                "base_url": endpoint.base_url,
                "custom_path": endpoint.custom_path,
                "format_acceptance_config": endpoint.format_acceptance_config,
                "is_active": endpoint.is_active,
                "model_binding": model_binding.map(|binding| json!({
                    "source": binding.source,
                    "is_active": binding.is_active,
                })),
                "runtime_capability_quarantines": runtime_capability_quarantines,
                "keys": key_payloads,
                "total_keys": key_payloads.len(),
                "active_keys": if endpoint_is_active {
                    key_payloads.iter().filter(|value| value["is_active"] == json!(true)).count()
                } else {
                    0
                },
            }));
        }
        let model_mappings = model
            .provider_model_mappings
            .as_ref()
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        providers_payload.push(json!({
            "id": &provider.id,
            "name": &provider.name,
            "model_id": &model.id,
            "provider_priority": provider.provider_priority,
            "enable_format_conversion": provider.enable_format_conversion,
            "keep_priority_on_conversion": provider.keep_priority_on_conversion,
            "billing_type": provider.billing_type.clone(),
            "monthly_quota_usd": provider.monthly_quota_usd,
            "monthly_used_usd": provider.monthly_used_usd,
            "is_active": provider.is_active,
            "provider_model_name": &model.provider_model_name,
            "model_mappings": model_mappings,
            "model_is_active": model.is_active,
            "model_is_available": model.is_available,
            "endpoints": endpoint_payloads,
            "total_endpoints": endpoint_payloads.len(),
            "active_endpoints": active_endpoints,
        }));
    }

    // 与 Python 逻辑对齐：供前端实时匹配的白名单数据来自“全站活跃 Provider 的活跃 Key”
    // （仅保留配置了非空 allowed_models 的 Key），而不是仅当前 GlobalModel 关联 Provider。
    if include_whitelist {
        let active_providers = state
            .list_provider_catalog_providers(true)
            .await
            .ok()
            .unwrap_or_default();
        let active_provider_ids = active_providers
            .iter()
            .map(|provider| provider.id.clone())
            .collect::<Vec<_>>();
        let active_provider_metadata_by_id = active_providers
            .into_iter()
            .map(|provider| (provider.id, (provider.name, provider.provider_type)))
            .collect::<BTreeMap<_, _>>();
        let active_keys = if active_provider_ids.is_empty() {
            Vec::new()
        } else {
            state
                .list_provider_catalog_keys_by_provider_ids(&active_provider_ids)
                .await
                .ok()
                .unwrap_or_default()
        };
        for key in active_keys {
            if !key.is_active {
                continue;
            }
            let allowed_models = json_string_list(key.allowed_models.as_ref());
            if allowed_models.is_empty() {
                continue;
            }
            let (provider_name, provider_type) = active_provider_metadata_by_id
                .get(&key.provider_id)
                .cloned()
                .unwrap_or_default();
            all_keys_whitelist.push(json!({
                "key_id": key.id,
                "key_name": key.name,
                "masked_key": state.masked_catalog_api_key_for_provider(&key, &provider_type),
                "provider_id": key.provider_id,
                "provider_name": provider_name,
                "allowed_models": allowed_models,
            }));
        }
    }

    providers_payload.sort_by(|left, right| {
        left.get("provider_priority")
            .and_then(serde_json::Value::as_i64)
            .cmp(
                &right
                    .get("provider_priority")
                    .and_then(serde_json::Value::as_i64),
            )
            .then_with(|| {
                left.get("name")
                    .and_then(serde_json::Value::as_str)
                    .cmp(&right.get("name").and_then(serde_json::Value::as_str))
            })
    });

    let active_providers = providers_payload
        .iter()
        .filter(|provider| {
            provider["is_active"] == json!(true)
                && provider["model_is_active"] == json!(true)
                && provider["model_is_available"] == json!(true)
                && provider["active_endpoints"].as_u64().unwrap_or(0) > 0
                && provider["endpoints"].as_array().is_some_and(|endpoints| {
                    endpoints.iter().any(|endpoint| {
                        endpoint["is_active"] == json!(true)
                            && endpoint["model_binding"]["is_active"] == json!(true)
                            && endpoint["active_keys"].as_u64().unwrap_or(0) > 0
                    })
                })
        })
        .count();
    let total_providers = providers_payload.len();

    Some(json!({
        "global_model_id": &global_model.id,
        "global_model_name": &global_model.name,
        "display_name": &global_model.display_name,
        "is_active": global_model.is_active,
        "global_model_mappings": global_model_mappings,
        "providers": providers_payload,
        "total_providers": total_providers,
        "active_providers": active_providers,
        "scheduling_mode": scheduling_mode,
        "priority_mode": priority_mode,
        "keep_priority_on_conversion": keep_priority_on_conversion,
        "all_keys_whitelist": all_keys_whitelist,
    }))
}

fn routing_endpoint_is_active(
    provider_is_active: bool,
    model_is_active: bool,
    model_is_available: bool,
    endpoint_is_active: bool,
    binding_is_active: bool,
) -> bool {
    provider_is_active
        && model_is_active
        && model_is_available
        && endpoint_is_active
        && binding_is_active
}

#[cfg(test)]
mod routing_endpoint_activity_tests {
    use super::routing_endpoint_is_active;

    #[test]
    fn endpoint_activity_requires_every_routing_layer_to_be_active() {
        assert!(routing_endpoint_is_active(true, true, true, true, true));
        assert!(!routing_endpoint_is_active(false, true, true, true, true));
        assert!(!routing_endpoint_is_active(true, false, true, true, true));
        assert!(!routing_endpoint_is_active(true, true, false, true, true));
        assert!(!routing_endpoint_is_active(true, true, true, false, true));
        assert!(!routing_endpoint_is_active(true, true, true, true, false));
    }
}

fn key_allowed_models_match_global_model_for_routing(
    raw_allowed_models: Option<&serde_json::Value>,
    model_names: &[String],
    global_model_mappings: &CompiledModelMappings,
) -> bool {
    // 兼容 Python 预览逻辑：None/[] 视为“不限制”，在链路预览中保留该 Key。
    let allowed_models = json_string_list(raw_allowed_models);
    if raw_allowed_models.is_none() || allowed_models.is_empty() {
        return true;
    }

    for allowed_model in allowed_models.iter().map(String::as_str).map(str::trim) {
        if allowed_model.is_empty() {
            continue;
        }
        if model_names
            .iter()
            .any(|model_name| model_name.eq_ignore_ascii_case(allowed_model))
        {
            return true;
        }
        if global_model_mappings.matches_any(allowed_model) {
            return true;
        }
    }

    false
}

fn matched_model_names_for_routing(
    raw_allowed_models: Option<&serde_json::Value>,
    global_model_name: &str,
    global_model_mappings: &CompiledModelMappings,
) -> Vec<String> {
    normalized_allowed_models(raw_allowed_models)
        .into_iter()
        .filter(|allowed_model| {
            !allowed_model.eq_ignore_ascii_case(global_model_name)
                && global_model_mappings.matches_any(allowed_model)
        })
        .collect()
}

#[derive(Debug, Deserialize)]
pub(crate) struct AdminGlobalModelMappingPreviewRequest {
    #[serde(default)]
    pub(crate) mappings: Vec<String>,
    pub(crate) expanded_rule_index: Option<usize>,
    #[serde(default = "default_mapping_preview_page")]
    pub(crate) page: usize,
    #[serde(default = "default_mapping_preview_page_size")]
    pub(crate) page_size: usize,
}

fn default_mapping_preview_page() -> usize {
    1
}

fn default_mapping_preview_page_size() -> usize {
    25
}

#[derive(Default)]
struct MappingRuleMatchSummary {
    key_ids: BTreeSet<String>,
    model_names: BTreeSet<String>,
    provider_ids: BTreeSet<String>,
    mapping_count: usize,
}

struct MappingPreviewKeyDetail {
    key_id: String,
    key_name: String,
    masked_key: String,
    provider_id: String,
    provider_name: String,
    matched_models: BTreeSet<String>,
}

pub(crate) async fn build_admin_global_model_mapping_preview_payload(
    state: &AdminAppState<'_>,
    global_model_id: &str,
    request: AdminGlobalModelMappingPreviewRequest,
) -> Option<serde_json::Value> {
    if !state.has_global_model_data_reader() || !state.has_provider_catalog_data_reader() {
        return None;
    }
    state
        .get_admin_global_model_by_id(global_model_id)
        .await
        .ok()??;
    let linked_provider_ids = state
        .list_admin_provider_models_by_global_model_id(global_model_id)
        .await
        .ok()?
        .into_iter()
        .map(|model| model.provider_id)
        .collect::<BTreeSet<_>>();
    let mappings = request
        .mappings
        .into_iter()
        .map(|mapping| mapping.trim().to_string())
        .collect::<Vec<_>>();
    let compiled = compiled_model_mappings(&mappings);
    let mut summaries = (0..mappings.len())
        .map(|_| MappingRuleMatchSummary::default())
        .collect::<Vec<_>>();
    let mut expanded_details = BTreeMap::<String, MappingPreviewKeyDetail>::new();
    let expanded_rule_index = request
        .expanded_rule_index
        .filter(|index| *index < mappings.len() && compiled.rule_is_valid(*index));
    let (provider_metadata, keys) = load_active_provider_metadata_and_keys(state).await;

    for key in keys.into_iter().filter(|key| key.is_active) {
        let allowed_models = normalized_allowed_models(key.allowed_models.as_ref());
        if allowed_models.is_empty() {
            continue;
        }
        for allowed_model in allowed_models {
            for rule_index in compiled.matching_rule_indexes(&allowed_model) {
                if !compiled.rule_is_valid(rule_index) {
                    continue;
                }
                let summary = &mut summaries[rule_index];
                summary.key_ids.insert(key.id.clone());
                summary.model_names.insert(allowed_model.clone());
                summary.provider_ids.insert(key.provider_id.clone());
                summary.mapping_count += 1;
                if expanded_rule_index == Some(rule_index) {
                    expanded_details
                        .entry(key.id.clone())
                        .or_insert_with(|| MappingPreviewKeyDetail {
                            key_id: key.id.clone(),
                            key_name: key.name.clone(),
                            masked_key: state.masked_catalog_api_key_for_provider(
                                &key,
                                provider_metadata
                                    .get(&key.provider_id)
                                    .map(|(_, provider_type)| provider_type.as_str())
                                    .unwrap_or_default(),
                            ),
                            provider_id: key.provider_id.clone(),
                            provider_name: provider_metadata
                                .get(&key.provider_id)
                                .map(|(provider_name, _)| provider_name.clone())
                                .unwrap_or_default(),
                            matched_models: BTreeSet::new(),
                        })
                        .matched_models
                        .insert(allowed_model.clone());
                }
            }
        }
    }

    let rules = mappings
        .iter()
        .enumerate()
        .map(|(index, pattern)| {
            let summary = &summaries[index];
            json!({
                "index": index,
                "pattern": pattern,
                "valid": compiled.rule_is_valid(index),
                "matched_key_count": summary.key_ids.len(),
                "matched_model_count": summary.model_names.len(),
                "matched_mapping_count": summary.mapping_count,
                "matched_provider_count": summary.provider_ids.len(),
                "matched_provider_ids": summary.provider_ids.iter().collect::<Vec<_>>(),
                "unlinked_provider_ids": summary.provider_ids.difference(&linked_provider_ids).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    let page = request.page.max(1);
    let page_size = request.page_size.clamp(1, 100);
    let total_keys = expanded_details.len();
    let offset = (page - 1).saturating_mul(page_size);
    let keys = expanded_details
        .into_values()
        .skip(offset)
        .take(page_size)
        .map(|detail| {
            json!({
                "key_id": detail.key_id,
                "key_name": detail.key_name,
                "masked_key": detail.masked_key,
                "provider_id": detail.provider_id,
                "provider_name": detail.provider_name,
                "is_linked": linked_provider_ids.contains(&detail.provider_id),
                "matched_models": detail.matched_models.into_iter().collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();

    Some(json!({
        "global_model_id": global_model_id,
        "rules": rules,
        "expanded": expanded_rule_index.map(|rule_index| json!({
            "rule_index": rule_index,
            "page": page,
            "page_size": page_size,
            "total_keys": total_keys,
            "keys": keys,
        })),
    }))
}

async fn load_active_provider_metadata_and_keys(
    state: &AdminAppState<'_>,
) -> (
    BTreeMap<String, (String, String)>,
    Vec<StoredProviderCatalogKey>,
) {
    let active_providers = state
        .list_provider_catalog_providers(true)
        .await
        .ok()
        .unwrap_or_default();
    let provider_ids = active_providers
        .iter()
        .map(|provider| provider.id.clone())
        .collect::<Vec<_>>();
    let provider_metadata = active_providers
        .into_iter()
        .map(|provider| (provider.id, (provider.name, provider.provider_type)))
        .collect::<BTreeMap<_, _>>();
    let keys = if provider_ids.is_empty() {
        Vec::new()
    } else {
        state
            .list_provider_catalog_keys_by_provider_ids(&provider_ids)
            .await
            .ok()
            .unwrap_or_default()
    };
    (provider_metadata, keys)
}

fn normalized_allowed_models(raw: Option<&serde_json::Value>) -> Vec<String> {
    let mut models = json_string_list(raw)
        .into_iter()
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty())
        .collect::<Vec<_>>();
    models.sort_unstable();
    models.dedup();
    models
}

fn provider_model_mapping_names_for_routing(
    raw_mappings: Option<&serde_json::Value>,
) -> Vec<String> {
    raw_mappings
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.as_str()
                        .or_else(|| item.get("name").and_then(serde_json::Value::as_str))
                })
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn key_match_model_names_for_routing(
    global_model_name: &str,
    provider_model_name: &str,
    provider_model_mapping_names: &[String],
) -> Vec<String> {
    let mut names = Vec::new();
    push_unique_model_name(&mut names, global_model_name);
    push_unique_model_name(&mut names, provider_model_name);
    for mapping_name in provider_model_mapping_names {
        push_unique_model_name(&mut names, mapping_name);
    }
    names
}

fn push_unique_model_name(names: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    if names
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(value))
    {
        return;
    }
    names.push(value.to_string());
}

pub(crate) async fn build_admin_assign_global_model_to_providers_payload(
    state: &AdminAppState<'_>,
    global_model_id: &str,
    provider_ids: Vec<String>,
    create_models: bool,
) -> Result<serde_json::Value, String> {
    let global_model = resolve_admin_global_model_by_id_or_err(state, global_model_id).await?;
    let providers = state
        .read_provider_catalog_providers_by_ids(&provider_ids)
        .await
        .map_err(|err| format!("{err:?}"))?
        .into_iter()
        .map(|provider| (provider.id.clone(), provider))
        .collect::<BTreeMap<_, _>>();

    let mut success = Vec::new();
    let mut errors = Vec::new();
    for provider_id in provider_ids {
        let provider_id = provider_id.trim().to_string();
        if provider_id.is_empty() {
            continue;
        }
        if !providers.contains_key(&provider_id) {
            errors.push(json!({
                "provider_id": provider_id,
                "error": "Provider not found",
            }));
            continue;
        }
        let exists = state
            .list_admin_provider_models(&AdminProviderModelListQuery {
                provider_id: provider_id.clone(),
                is_active: None,
                offset: 0,
                limit: 10_000,
            })
            .await
            .map_err(|err| format!("{err:?}"))?
            .into_iter()
            .any(|model| model.global_model_id == global_model.id);
        if exists {
            errors.push(json!({
                "provider_id": provider_id,
                "error": "Model already exists",
            }));
            continue;
        }
        if !create_models {
            errors.push(json!({
                "provider_id": provider_id,
                "error": "create_models disabled",
            }));
            continue;
        }
        let record = UpsertAdminProviderModelRecord::new(
            Uuid::new_v4().to_string(),
            provider_id.clone(),
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
        .map_err(|err| err.to_string())?;
        let mutation = match state
            .build_admin_provider_model_create_mutation(record, None, None)
            .await
        {
            Ok(mutation) => mutation,
            Err(crate::GatewayError::Client { message, .. }) => {
                errors.push(json!({
                    "provider_id": provider_id,
                    "error": message,
                }));
                continue;
            }
            Err(err) => return Err(format!("{err:?}")),
        };
        let created = state
            .create_admin_provider_model_from_mutation(&mutation)
            .await
            .map_err(|err| format!("{err:?}"))?;
        if let Some(created) = created {
            success.push(json!({
                "provider_id": provider_id,
                "provider_model_id": created.id,
                "global_model_id": global_model.id,
            }));
        } else {
            errors.push(json!({
                "provider_id": provider_id,
                "error": "Create provider model failed",
            }));
        }
    }
    let total_success = success.len();
    let total_errors = errors.len();
    Ok(json!({
        "success": success,
        "errors": errors,
        "total_success": total_success,
        "total_errors": total_errors,
    }))
}
