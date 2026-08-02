use std::collections::BTreeSet;

use aether_data_contracts::repository::global_models::StoredPublicGlobalModel;
use aether_data_contracts::repository::model_catalog::StoredModelCatalogEntry;

use super::GatewayPublicRequestContext;

pub(crate) fn models_api_format(request_context: &GatewayPublicRequestContext) -> Option<&str> {
    let signature = request_context
        .control_decision
        .as_ref()
        .and_then(|decision| decision.auth_endpoint_signature.as_deref())
        .map(str::trim)
        .filter(|signature| !signature.is_empty())?;
    match crate::ai_serving::normalize_api_format_alias(signature).as_str() {
        "openai:chat" => Some("openai:chat"),
        "openai:responses" => Some("openai:responses"),
        "openai:responses:compact" => Some("openai:responses:compact"),
        "openai:image" => Some("openai:image"),
        "openai:embedding" => Some("openai:embedding"),
        "openai:rerank" => Some("openai:rerank"),
        "claude:messages" => Some("claude:messages"),
        "gemini:generate_content" => Some("gemini:generate_content"),
        "gemini:embedding" => Some("gemini:embedding"),
        "jina:embedding" => Some("jina:embedding"),
        "jina:rerank" => Some("jina:rerank"),
        "doubao:embedding" => Some("doubao:embedding"),
        "aliyun:multimodal_embedding" => Some("aliyun:multimodal_embedding"),
        _ => None,
    }
}

pub(crate) fn matches_model_mapping_for_models(pattern: &str, model_name: &str) -> bool {
    aether_scheduler_core::matches_model_mapping(pattern, model_name)
}

pub(super) fn models_detail_id(request_path: &str) -> Option<String> {
    let raw = if let Some(value) = request_path.strip_prefix("/v1/models/") {
        value
    } else if let Some(value) = request_path.strip_prefix("/v1beta/models/") {
        value
    } else {
        return None;
    };
    let normalized = raw.trim().trim_start_matches("models/").trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

fn auth_allows_provider(
    auth: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    row: &StoredModelCatalogEntry,
) -> bool {
    auth.and_then(crate::data::auth::GatewayAuthApiKeySnapshot::effective_allowed_providers)
        .is_none_or(|allowed| {
            allowed.iter().any(|value| {
                aether_scheduler_core::provider_matches_allowed_value(
                    value,
                    &row.provider_id,
                    &row.provider_name,
                    &row.provider_type,
                )
            })
        })
}

fn auth_allows_model_name(
    auth: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    model_name: &str,
) -> bool {
    auth.and_then(crate::data::auth::GatewayAuthApiKeySnapshot::effective_allowed_models)
        .is_none_or(|allowed| allowed.iter().any(|value| value == model_name))
}

fn auth_allows_model(
    auth: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    row: &StoredModelCatalogEntry,
) -> bool {
    auth_allows_model_name(auth, &row.global_model_name)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModelFamily {
    Generation,
    Image,
    Embedding,
    Rerank,
}

enum DeclaredFamilies {
    Absent,
    Valid(Vec<ModelFamily>),
    Invalid,
}

fn known_format_family(value: &str) -> Option<ModelFamily> {
    match crate::ai_serving::normalize_api_format_alias(value).as_str() {
        "openai:chat"
        | "openai:responses"
        | "openai:responses:compact"
        | "claude:messages"
        | "gemini:generate_content" => Some(ModelFamily::Generation),
        "openai:image" => Some(ModelFamily::Image),
        "openai:embedding"
        | "gemini:embedding"
        | "jina:embedding"
        | "doubao:embedding"
        | "aliyun:multimodal_embedding" => Some(ModelFamily::Embedding),
        "openai:rerank" | "jina:rerank" => Some(ModelFamily::Rerank),
        _ => None,
    }
}

fn capability_family(value: &str) -> Option<ModelFamily> {
    match value.trim().to_ascii_lowercase().as_str() {
        "generation" | "chat" | "responses" | "text_generation" => Some(ModelFamily::Generation),
        "image_generation" | "image" => Some(ModelFamily::Image),
        "embedding" | "embeddings" => Some(ModelFamily::Embedding),
        "rerank" | "reranking" => Some(ModelFamily::Rerank),
        _ => None,
    }
}

fn declared_families(value: &serde_json::Value, mapping_null_is_absent: bool) -> DeclaredFamilies {
    let Some(object) = value.as_object() else {
        return DeclaredFamilies::Absent;
    };
    let mut families = Vec::new();
    for key in ["api_format", "client_api_format", "provider_api_format"] {
        let Some(value) = object.get(key) else {
            continue;
        };
        let Some(value) = value.as_str() else {
            return DeclaredFamilies::Invalid;
        };
        let Some(family) = known_format_family(value) else {
            return DeclaredFamilies::Invalid;
        };
        families.push(family);
    }
    if let Some(values) = object.get("api_formats") {
        if mapping_null_is_absent && values.is_null() {
            return if families.is_empty() {
                DeclaredFamilies::Absent
            } else {
                DeclaredFamilies::Valid(families)
            };
        }
        let Some(values) = values.as_array() else {
            return DeclaredFamilies::Invalid;
        };
        for value in values {
            let Some(value) = value.as_str() else {
                return DeclaredFamilies::Invalid;
            };
            let Some(family) = known_format_family(value) else {
                return DeclaredFamilies::Invalid;
            };
            families.push(family);
        }
    }
    for key in ["capabilities", "supported_capabilities"] {
        let Some(values) = object.get(key) else {
            continue;
        };
        let Some(values) = values.as_array() else {
            continue;
        };
        families.extend(
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .filter_map(capability_family),
        );
    }
    if families.is_empty() {
        DeclaredFamilies::Absent
    } else {
        DeclaredFamilies::Valid(families)
    }
}

fn merge_declared(state: &mut DeclaredFamilies, next: DeclaredFamilies) {
    match next {
        DeclaredFamilies::Invalid => *state = DeclaredFamilies::Invalid,
        DeclaredFamilies::Valid(mut next) => match state {
            DeclaredFamilies::Valid(current) => current.append(&mut next),
            DeclaredFamilies::Absent => *state = DeclaredFamilies::Valid(next),
            DeclaredFamilies::Invalid => {}
        },
        DeclaredFamilies::Absent => {}
    }
}

fn row_supports_format(row: &StoredModelCatalogEntry, api_format: &str) -> bool {
    let mut declared = DeclaredFamilies::Absent;
    for value in [
        row.global_model_config.as_ref(),
        row.provider_model_config.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        merge_declared(&mut declared, declared_families(value, false));
    }
    if let Some(capabilities) = row
        .global_model_supported_capabilities
        .as_ref()
        .and_then(serde_json::Value::as_array)
    {
        let families = capabilities
            .iter()
            .filter_map(serde_json::Value::as_str)
            .filter_map(capability_family)
            .collect::<Vec<_>>();
        if !families.is_empty() {
            merge_declared(&mut declared, DeclaredFamilies::Valid(families));
        }
    }
    if let Some(mappings) = row
        .provider_model_mappings
        .as_ref()
        .and_then(serde_json::Value::as_array)
    {
        for mapping in mappings {
            merge_declared(&mut declared, declared_families(mapping, true));
        }
    }
    let Some(requested_family) = known_format_family(api_format) else {
        return false;
    };
    match declared {
        DeclaredFamilies::Absent => requested_family == ModelFamily::Generation,
        DeclaredFamilies::Valid(families) => families.contains(&requested_family),
        DeclaredFamilies::Invalid => false,
    }
}

fn global_model_supports_format(model: &StoredPublicGlobalModel, api_format: &str) -> bool {
    let mut declared = DeclaredFamilies::Absent;
    if let Some(config) = model.config.as_ref() {
        merge_declared(&mut declared, declared_families(config, false));
    }
    if let Some(capabilities) = model
        .supported_capabilities
        .as_ref()
        .and_then(serde_json::Value::as_array)
    {
        let families = capabilities
            .iter()
            .filter_map(serde_json::Value::as_str)
            .filter_map(capability_family)
            .collect::<Vec<_>>();
        if !families.is_empty() {
            merge_declared(&mut declared, DeclaredFamilies::Valid(families));
        }
    }
    let Some(requested_family) = known_format_family(api_format) else {
        return false;
    };
    match declared {
        DeclaredFamilies::Absent => requested_family == ModelFamily::Generation,
        DeclaredFamilies::Valid(families) => families.contains(&requested_family),
        DeclaredFamilies::Invalid => false,
    }
}

pub(super) fn filter_catalog_for_models(
    rows: Vec<StoredModelCatalogEntry>,
    auth: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    api_format: &str,
) -> Vec<StoredModelCatalogEntry> {
    let mut rows = rows
        .into_iter()
        .filter(|row| {
            row.global_model_is_active
                && row.provider_model_is_active
                && row.provider_model_is_available
                && row.provider_is_active
        })
        .filter(|row| auth_allows_provider(auth, row))
        .filter(|row| auth_allows_model(auth, row))
        .filter(|row| row_supports_format(row, api_format))
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.global_model_name
            .cmp(&right.global_model_name)
            .then(left.provider_id.cmp(&right.provider_id))
            .then(left.provider_model_id.cmp(&right.provider_model_id))
    });
    rows.dedup_by(|left, right| left.global_model_name == right.global_model_name);
    rows
}

pub(super) fn filter_global_models_for_models(
    models: Vec<StoredPublicGlobalModel>,
    auth: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    api_format: &str,
    allowed_global_model_ids: Option<&BTreeSet<String>>,
) -> Vec<StoredPublicGlobalModel> {
    let mut models = models
        .into_iter()
        .filter(|model| model.is_active)
        .filter(|model| auth_allows_model_name(auth, &model.name))
        .filter(|model| global_model_supports_format(model, api_format))
        .filter(|model| {
            allowed_global_model_ids.is_none_or(|allowed_ids| allowed_ids.contains(&model.id))
        })
        .collect::<Vec<_>>();
    models.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    models.dedup_by(|left, right| left.name == right.name);
    models
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row() -> StoredModelCatalogEntry {
        StoredModelCatalogEntry {
            global_model_id: "g".into(),
            global_model_name: "model".into(),
            global_model_config: None,
            global_model_supported_capabilities: None,
            global_model_is_active: true,
            provider_model_id: "m".into(),
            provider_model_name: "model".into(),
            provider_model_mappings: None,
            provider_model_config: None,
            provider_model_is_active: true,
            provider_model_is_available: true,
            provider_id: "p".into(),
            provider_name: "provider".into(),
            provider_type: "openai".into(),
            provider_is_active: true,
        }
    }

    #[test]
    fn schema_bound_metadata_maps_each_supported_family() {
        for (capability, format) in [
            ("generation", "openai:chat"),
            ("image_generation", "openai:image"),
            ("embedding", "openai:embedding"),
            ("rerank", "openai:rerank"),
        ] {
            let mut entry = row();
            entry.global_model_supported_capabilities = Some(json!([capability]));
            assert!(
                row_supports_format(&entry, format),
                "{capability} should support {format}"
            );
        }
    }

    #[test]
    fn incidental_strings_and_unknown_formats_never_grant_support() {
        let mut entry = row();
        entry.global_model_config = Some(json!({
            "homepage": "https://example.test/openai:image",
            "path": "/v1/embeddings",
            "note": "vendor:rerank",
            "api_format": "vendor:unknown"
        }));
        assert!(!row_supports_format(&entry, "openai:image"));
        assert!(!row_supports_format(&entry, "openai:embedding"));
        assert!(!row_supports_format(&entry, "openai:rerank"));
        assert!(!row_supports_format(&entry, "openai:chat"));
    }

    #[test]
    fn legacy_fallback_is_generation_only_when_metadata_is_absent() {
        let entry = row();
        assert!(row_supports_format(&entry, "claude:messages"));
        assert!(!row_supports_format(&entry, "openai:image"));
    }

    #[test]
    fn non_family_capabilities_preserve_legacy_generation_fallback() {
        let mut entry = row();
        entry.global_model_supported_capabilities = Some(json!(["streaming", "vision"]));
        assert!(row_supports_format(&entry, "openai:chat"));
        assert!(!row_supports_format(&entry, "openai:image"));
    }

    #[test]
    fn alias_only_mapping_preserves_legacy_generation_fallback() {
        let mut entry = row();
        entry.provider_model_mappings =
            Some(json!([{"name": "provider-alias", "api_formats": null}]));
        assert!(row_supports_format(&entry, "gemini:generate_content"));
    }

    #[test]
    fn empty_arrays_preserve_legacy_generation_fallback() {
        let mut entry = row();
        entry.global_model_config = Some(json!({"api_formats": [], "capabilities": []}));
        entry.global_model_supported_capabilities = Some(json!([]));
        entry.provider_model_mappings = Some(json!([]));
        assert!(row_supports_format(&entry, "openai:responses"));
    }

    #[test]
    fn invalid_explicit_format_declarations_are_unsupported() {
        for invalid in [
            json!({"api_format": null}),
            json!({"api_format": 42}),
            json!({"api_format": "vendor:unknown"}),
            json!({"api_formats": null}),
            json!({"api_formats": "openai:chat"}),
            json!({"api_formats": ["openai:chat", "vendor:unknown"]}),
        ] {
            let mut entry = row();
            entry.global_model_config = Some(invalid);
            assert!(!row_supports_format(&entry, "openai:chat"));
        }
    }

    #[test]
    fn valid_provider_mapping_formats_select_declared_family() {
        let mut entry = row();
        entry.provider_model_mappings = Some(json!([{"api_formats": ["openai:embedding"]}]));
        assert!(row_supports_format(&entry, "gemini:embedding"));
        assert!(!row_supports_format(&entry, "openai:chat"));
    }
}
