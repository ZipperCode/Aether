use std::collections::BTreeSet;

use aether_ai_formats::{api_format_permission_covers, normalize_api_format_alias};
use aether_data_contracts::repository::global_models::{
    AdminProviderModelListQuery, StoredAdminProviderModel, StoredModelEndpointBinding,
};
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogProvider,
};
use serde_json::{json, Value};

use crate::{aggregate_models_for_cache, json_string_list, ModelFetchAssociationStore};

pub const CODEX_IMAGE_API_FORMAT: &str = "openai:image";

/// 每个 Provider 读取一次已启用的图片配置；运行态不可用不能阻止旧白名单恢复。
pub async fn load_codex_image_models<S: ModelFetchAssociationStore + Sync + ?Sized>(
    state: &S,
    provider: &StoredProviderCatalogProvider,
    endpoints: &[StoredProviderCatalogEndpoint],
) -> Result<Vec<Value>, S::Error> {
    if !provider.provider_type.trim().eq_ignore_ascii_case("codex")
        || !state.has_global_model_reader()
        || !endpoints.iter().any(|endpoint| {
            endpoint.is_active
                && normalize_api_format_alias(&endpoint.api_format) == CODEX_IMAGE_API_FORMAT
        })
    {
        return Ok(Vec::new());
    }
    let models = state
        .list_admin_provider_models(&AdminProviderModelListQuery {
            provider_id: provider.id.clone(),
            is_active: Some(true),
            offset: 0,
            limit: 10_000,
        })
        .await?;
    let model_ids = models
        .iter()
        .map(|model| model.id.clone())
        .collect::<Vec<_>>();
    let bindings = state.list_model_endpoint_bindings(&model_ids).await?;
    Ok(configured_codex_image_models(&models, &bindings, endpoints))
}

fn configured_codex_image_models(
    models: &[StoredAdminProviderModel],
    bindings: &[StoredModelEndpointBinding],
    endpoints: &[StoredProviderCatalogEndpoint],
) -> Vec<Value> {
    let image_endpoint_ids = endpoints
        .iter()
        .filter(|endpoint| {
            endpoint.is_active
                && normalize_api_format_alias(&endpoint.api_format) == CODEX_IMAGE_API_FORMAT
        })
        .map(|endpoint| endpoint.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut cards = Vec::new();
    for model in models.iter().filter(|model| model.is_active) {
        let capability = model
            .supports_image_generation
            .or_else(|| {
                model
                    .global_model_config
                    .as_ref()?
                    .get("image_generation")?
                    .as_bool()
            })
            .or_else(|| {
                json_string_list(model.global_model_supported_capabilities.as_ref())
                    .iter()
                    .any(|capability| capability == "image_generation")
                    .then_some(true)
            });
        if capability == Some(false) {
            continue;
        }
        let mappings = match model.provider_model_mappings.as_ref() {
            Some(Value::Array(items)) => items.iter().collect::<Vec<_>>(),
            Some(item) => vec![item],
            None => Vec::new(),
        };
        for binding in bindings.iter().filter(|binding| {
            binding.model_id == model.id
                && binding.is_active
                && image_endpoint_ids.contains(binding.endpoint_id.as_str())
        }) {
            let mapping_name = |mapping: &Value| {
                mapping
                    .as_str()
                    .or_else(|| mapping.get("name").and_then(Value::as_str))
                    .map(str::trim)
                    .map(ToOwned::to_owned)
            };
            let mut names = mappings
                .iter()
                .filter(|mapping| {
                    mapping
                        .get("api_formats")
                        .filter(|value| !value.is_null())
                        .is_none_or(|formats| {
                            let formats = json_string_list(Some(formats));
                            // 显式空映射范围与调度器一致，表示没有可用格式。
                            formats.iter().any(|format| {
                                api_format_permission_covers(format, CODEX_IMAGE_API_FORMAT)
                            })
                        })
                        && mapping
                            .get("endpoint_ids")
                            .filter(|value| !value.is_null())
                            .is_none_or(|ids| {
                                let ids = json_string_list(Some(ids));
                                ids.contains(&binding.endpoint_id)
                            })
                })
                .filter_map(|mapping| mapping_name(mapping))
                .collect::<BTreeSet<_>>();
            // 显式限定默认名称的映射也约束默认分支，不能额外制造一个未映射别名。
            if !mappings.iter().any(|mapping| {
                mapping_name(mapping).as_deref() == Some(model.provider_model_name.trim())
            }) {
                names.insert(model.provider_model_name.trim().to_string());
            }
            for name in names {
                let family_name = name
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                // 旧迁移曾给文本模型添加图片绑定；绑定必须同时具有图片能力或已知家族语义。
                if name.is_empty()
                    || !(capability == Some(true)
                        || family_name.starts_with("gpt-image-")
                        || family_name == "chatgpt-image-latest")
                {
                    continue;
                }
                cards.push(json!({
                    "id": name,
                    "object": "model",
                    "owned_by": "openai",
                    "api_formats": [CODEX_IMAGE_API_FORMAT],
                    "supports_image_generation": true,
                    "endpoint_ids": [&binding.endpoint_id],
                }));
            }
        }
    }
    aggregate_models_for_cache(&cards)
}

/// 仅扩展管理/关联投影；原生目录与 codex_models 元数据不得传入此函数后再持久化。
pub fn supplement_codex_management_models(
    models: &mut Vec<Value>,
    endpoints: &[StoredProviderCatalogEndpoint],
    key_api_formats: Option<&Value>,
    image_models: &[Value],
    native_catalog: bool,
) {
    let key_formats = json_string_list(key_api_formats);
    let permitted_endpoints = endpoints
        .iter()
        .filter(|endpoint| {
            endpoint.is_active
                && (key_formats.is_empty()
                    || key_formats
                        .iter()
                        .any(|format| api_format_permission_covers(format, &endpoint.api_format)))
        })
        .collect::<Vec<_>>();

    if native_catalog {
        for model in models.iter_mut() {
            let formats = json_string_list(model.get("api_formats"));
            if !formats.iter().any(|format| {
                matches!(
                    normalize_api_format_alias(format).as_str(),
                    "openai:chat" | "openai:responses" | "openai:responses:compact"
                )
            }) {
                continue;
            }
            let mut formats = formats.into_iter().collect::<BTreeSet<_>>();
            let mut endpoint_ids = json_string_list(model.get("endpoint_ids"))
                .into_iter()
                .collect::<BTreeSet<_>>();
            for endpoint in &permitted_endpoints {
                let format = normalize_api_format_alias(&endpoint.api_format);
                // Search 是独立接口；supports_search_tool 表示工具发现能力，不能作其权限开关。
                if matches!(
                    format.as_str(),
                    "openai:responses" | "openai:responses:compact" | "openai:search"
                ) {
                    formats.insert(format);
                    endpoint_ids.insert(endpoint.id.clone());
                }
            }
            model["api_formats"] = json!(formats);
            model["endpoint_ids"] = json!(endpoint_ids);
        }
    }
    let permitted_image_ids = permitted_endpoints
        .iter()
        .filter(|endpoint| {
            normalize_api_format_alias(&endpoint.api_format) == CODEX_IMAGE_API_FORMAT
        })
        .map(|endpoint| endpoint.id.as_str())
        .collect::<BTreeSet<_>>();
    for image_model in image_models {
        let endpoint_ids = json_string_list(image_model.get("endpoint_ids"))
            .into_iter()
            .filter(|id| permitted_image_ids.contains(id.as_str()))
            .collect::<Vec<_>>();
        if endpoint_ids.is_empty() {
            continue;
        }
        let mut image_model = image_model.clone();
        image_model["endpoint_ids"] = json!(endpoint_ids);
        models.push(image_model);
    }
    *models = aggregate_models_for_cache(models);
    for model in models {
        if json_string_list(model.get("api_formats"))
            .iter()
            .any(|format| format == CODEX_IMAGE_API_FORMAT)
        {
            model["supports_image_generation"] = Value::Bool(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(id: &str, format: &str, active: bool) -> StoredProviderCatalogEndpoint {
        StoredProviderCatalogEndpoint::new(
            id.to_string(),
            "codex".to_string(),
            format.to_string(),
            None,
            None,
            active,
        )
        .expect("endpoint should build")
    }

    fn model(id: &str, name: &str) -> StoredAdminProviderModel {
        serde_json::from_value(json!({
            "id": id, "provider_id": "codex", "global_model_id": id,
            "provider_model_name": name, "is_active": true, "is_available": false,
        }))
        .expect("provider model should build")
    }

    fn binding(model_id: &str, endpoint_id: &str, active: bool) -> StoredModelEndpointBinding {
        StoredModelEndpointBinding::new(
            model_id.to_string(),
            endpoint_id.to_string(),
            "manual".to_string(),
            active,
            None,
            None,
        )
        .expect("binding should build")
    }

    #[test]
    fn codex_configured_images_preserve_names_bindings_and_capability_boundaries() {
        let mut mapped = model("mapped", "text-alias");
        mapped.supports_image_generation = Some(true);
        mapped.provider_model_mappings = Some(json!([
            {"name": "text-alias", "api_formats": ["openai:responses"]},
            {"name": "canvas-future-upstream", "api_formats": ["openai:image"], "endpoint_ids": ["image"]},
            {"name": "wrong-endpoint", "api_formats": ["openai:image"], "endpoint_ids": ["another-image"]},
            {"name": "empty-format-image", "api_formats": []},
            {"name": "empty-endpoint-image", "endpoint_ids": []}
        ]));
        let mut denied = model("denied", "gpt-image-disabled");
        denied.supports_image_generation = Some(false);
        let mut disabled = model("disabled", "gpt-image-off");
        disabled.is_active = false;
        let cards = configured_codex_image_models(
            &[
                model("future", "gpt-image-42-next"),
                mapped,
                model("migration", "gpt-6-astra"),
                denied,
                disabled,
            ],
            &[
                binding("future", "image", true),
                binding("future", "image-off", true),
                binding("future", "another-image", false),
                binding("mapped", "image", true),
                binding("migration", "image", true),
                binding("denied", "image", true),
                binding("disabled", "image", true),
            ],
            &[
                endpoint("image", "openai:image", true),
                endpoint("image-off", "openai:image", false),
                endpoint("another-image", "openai:image", true),
            ],
        );
        assert_eq!(
            cards
                .iter()
                .map(|card| card["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["canvas-future-upstream", "gpt-image-42-next"]
        );
        for card in cards {
            assert_eq!(card["api_formats"], json!(["openai:image"]));
            assert_eq!(card["endpoint_ids"], json!(["image"]));
            assert_eq!(card["supports_image_generation"], true);
        }
    }

    #[test]
    fn codex_native_projection_adds_search_and_compact_without_deferred_tool_entitlement() {
        let native = vec![
            json!({"slug": "future-true", "supports_search_tool": true}),
            json!({"slug": "future-false", "supports_search_tool": false}),
            json!({"slug": "future-missing"}),
        ];
        let original = native.clone();
        let endpoints = vec![
            endpoint("responses", "openai:responses", true),
            endpoint("compact", "openai:responses:compact", true),
            endpoint("search", "openai:search", true),
            endpoint("search-off", "openai:search", false),
            endpoint("live", "codex:live", true),
            endpoint("image", "openai:image", true),
        ];
        let mut models =
            crate::project_codex_models_for_legacy_cache([("openai:responses", native.as_slice())]);
        supplement_codex_management_models(
            &mut models,
            &endpoints,
            Some(&json!(["openai:responses"])),
            &[],
            true,
        );
        for card in &models {
            assert_eq!(
                card["api_formats"],
                json!([
                    "openai:responses",
                    "openai:responses:compact",
                    "openai:search"
                ])
            );
            assert_eq!(
                card["endpoint_ids"],
                json!(["compact", "responses", "search"])
            );
        }
        assert_eq!(native, original);
        assert_eq!(
            models
                .iter()
                .find(|card| card["id"] == "future-false")
                .unwrap()["supports_search_tool"],
            false
        );
    }

    #[test]
    fn codex_projection_respects_key_formats_presets_and_native_image_cards() {
        let endpoints = vec![
            endpoint("responses", "openai:responses", true),
            endpoint("compact", "openai:responses:compact", true),
            endpoint("image", "openai:image", true),
        ];
        let images = vec![
            json!({"id": "gpt-image-future", "api_formats": ["openai:image"], "endpoint_ids": ["image"], "supports_image_generation": true}),
        ];
        let initial = vec![json!({"id": "text", "api_formats": ["openai:responses"]})];
        let mut models = initial.clone();
        supplement_codex_management_models(
            &mut models,
            &endpoints,
            Some(&json!(["openai:responses"])),
            &images,
            false,
        );
        assert_eq!(models, initial);
        supplement_codex_management_models(&mut models, &endpoints, None, &images, false);
        assert!(models.iter().any(|model| model["id"] == "gpt-image-future"));
        // 再投影不删除上游已有卡片；版本化缓存仍只保存未补全的原生卡。
        let existing = models.clone();
        supplement_codex_management_models(&mut models, &endpoints, None, &[], false);
        assert_eq!(models, existing);
    }
}
