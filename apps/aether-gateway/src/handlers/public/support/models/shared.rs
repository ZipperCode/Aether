use std::collections::BTreeSet;

use aether_data_contracts::repository::candidate_selection::{
    StoredMinimalCandidateSelectionRow, StoredProviderModelMapping,
};
use aether_data_contracts::repository::global_models::StoredPublicGlobalModel;
use aether_data_contracts::repository::model_catalog::StoredModelCatalogEntry;

use crate::model_metadata::{declared_model_families, global_model_declared_families};

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

const MODELS_CROSS_FORMAT_QUERY_API_FORMATS: &[&str] = &[
    "openai:chat",
    "openai:responses",
    "openai:responses:compact",
    "openai:image",
    "claude:messages",
    "gemini:generate_content",
];
const MODELS_EMBEDDING_QUERY_API_FORMATS: &[&str] = &[
    "openai:embedding",
    "jina:embedding",
    "gemini:embedding",
    "doubao:embedding",
    "aliyun:multimodal_embedding",
];
const MODELS_RERANK_QUERY_API_FORMATS: &[&str] = &["openai:rerank", "jina:rerank"];

/// 返回模型目录查询需要覆盖的兼容协议集合。
///
/// 生成类协议允许跨格式候选参与目录发布；图像、向量与重排保持各自的产品边界。
pub(super) fn models_query_api_formats(api_format: &str) -> &'static [&'static str] {
    match crate::ai_serving::normalize_api_format_alias(api_format).as_str() {
        "openai:chat"
        | "openai:responses"
        | "openai:responses:compact"
        | "claude:messages"
        | "gemini:generate_content" => MODELS_CROSS_FORMAT_QUERY_API_FORMATS,
        "openai:image" => &["openai:image"],
        "openai:embedding"
        | "jina:embedding"
        | "gemini:embedding"
        | "doubao:embedding"
        | "aliyun:multimodal_embedding" => MODELS_EMBEDDING_QUERY_API_FORMATS,
        "openai:rerank" | "jina:rerank" => MODELS_RERANK_QUERY_API_FORMATS,
        _ => &[],
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

fn auth_snapshot_allows_provider_for_models(
    auth_snapshot: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    provider_id: &str,
    provider_name: &str,
    provider_type: &str,
) -> bool {
    let Some(allowed) = auth_snapshot
        .and_then(crate::data::auth::GatewayAuthApiKeySnapshot::effective_allowed_providers)
    else {
        return true;
    };

    allowed.iter().any(|value| {
        aether_scheduler_core::provider_matches_allowed_value(
            value,
            provider_id,
            provider_name,
            provider_type,
        )
    })
}

fn auth_snapshot_allows_model_for_models(
    auth_snapshot: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    global_model_name: &str,
) -> bool {
    let Some(allowed) = auth_snapshot
        .and_then(crate::data::auth::GatewayAuthApiKeySnapshot::effective_allowed_models)
    else {
        return true;
    };
    allowed.iter().any(|value| value == global_model_name)
}

fn mapping_scope_matches_for_models(
    mapping: &StoredProviderModelMapping,
    row: &StoredMinimalCandidateSelectionRow,
    api_format: &str,
) -> bool {
    let api_format_matches = mapping.api_formats.as_ref().is_none_or(|api_formats| {
        api_formats
            .iter()
            .any(|value| value.trim().eq_ignore_ascii_case(api_format))
    });
    if !api_format_matches {
        return false;
    }

    mapping.endpoint_ids.as_ref().is_none_or(|endpoint_ids| {
        endpoint_ids
            .iter()
            .any(|endpoint_id| endpoint_id == &row.endpoint_id)
    })
}

fn candidate_model_names_for_models(
    row: &StoredMinimalCandidateSelectionRow,
    api_format: &str,
) -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::from([row.model_provider_model_name.clone()]);
    if let Some(mappings) = row.model_provider_model_mappings.as_ref() {
        for mapping in mappings {
            if mapping_scope_matches_for_models(mapping, row, api_format) {
                names.insert(mapping.name.clone());
            }
        }
    }
    names
}

fn row_exposes_global_model_for_models(
    row: &StoredMinimalCandidateSelectionRow,
    api_format: &str,
) -> bool {
    let Some(key_allowed_models) = row.key_allowed_models.as_ref() else {
        return true;
    };
    if key_allowed_models.is_empty() {
        return false;
    }
    if key_allowed_models
        .iter()
        .any(|value| value == &row.global_model_name)
    {
        return true;
    }

    let candidate_models = candidate_model_names_for_models(row, api_format);
    for allowed_model in key_allowed_models {
        if candidate_models.contains(allowed_model) {
            return true;
        }
    }

    let Some(global_model_mappings) = row.global_model_mappings.as_ref() else {
        return false;
    };
    for allowed_model in key_allowed_models {
        for pattern in global_model_mappings {
            if matches_model_mapping_for_models(pattern, allowed_model) {
                return true;
            }
        }
    }

    false
}

pub(crate) fn filter_eligible_model_rows(
    rows: Vec<StoredMinimalCandidateSelectionRow>,
    auth_snapshot: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    api_format: &str,
) -> Vec<StoredMinimalCandidateSelectionRow> {
    rows.into_iter()
        .filter(|row| {
            auth_snapshot_allows_provider_for_models(
                auth_snapshot,
                &row.provider_id,
                &row.provider_name,
                &row.provider_type,
            )
        })
        .filter(|row| auth_snapshot_allows_model_for_models(auth_snapshot, &row.global_model_name))
        .filter(|row| row_exposes_global_model_for_models(row, api_format))
        .collect()
}

pub(super) fn filter_rows_for_models(
    rows: Vec<StoredMinimalCandidateSelectionRow>,
    auth_snapshot: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    api_format: &str,
) -> Vec<StoredMinimalCandidateSelectionRow> {
    let mut filtered = filter_eligible_model_rows(rows, auth_snapshot, api_format);
    filtered.sort_by(|left, right| left.global_model_name.cmp(&right.global_model_name));
    let mut deduped = Vec::new();
    let mut last_model_name: Option<String> = None;
    for row in filtered {
        if last_model_name.as_deref() == Some(row.global_model_name.as_str()) {
            continue;
        }
        last_model_name = Some(row.global_model_name.clone());
        deduped.push(row);
    }
    deduped
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

fn row_supports_format(row: &StoredModelCatalogEntry, api_format: &str) -> bool {
    let mut declared = global_model_declared_families(
        row.global_model_config.as_ref(),
        row.global_model_supported_capabilities.as_ref(),
    );
    if let Some(config) = row.provider_model_config.as_ref() {
        declared.merge(declared_model_families(config, false));
    }
    if let Some(mappings) = row
        .provider_model_mappings
        .as_ref()
        .and_then(serde_json::Value::as_array)
    {
        for mapping in mappings {
            declared.merge(declared_model_families(mapping, true));
        }
    }
    declared.supports_api_format_or_legacy_generation(api_format)
}

fn global_model_supports_format(model: &StoredPublicGlobalModel, api_format: &str) -> bool {
    global_model_declared_families(model.config.as_ref(), model.supported_capabilities.as_ref())
        .supports_api_format_or_legacy_generation(api_format)
}

/// `api_format` 为 None 时仅跳过模型族过滤，状态、Provider 与 API Key 权限过滤保持不变。
pub(super) fn filter_catalog_for_models(
    rows: Vec<StoredModelCatalogEntry>,
    auth: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    api_format: Option<&str>,
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
        .filter(|row| api_format.is_none_or(|format| row_supports_format(row, format)))
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

/// `api_format` 为 None 时发布所有模型族，但仍要求 Global Model 启用且满足 Key/Provider 限制。
pub(super) fn filter_global_models_for_models(
    models: Vec<StoredPublicGlobalModel>,
    auth: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    api_format: Option<&str>,
    allowed_global_model_ids: Option<&BTreeSet<String>>,
) -> Vec<StoredPublicGlobalModel> {
    let mut models = models
        .into_iter()
        .filter(|model| model.is_active)
        .filter(|model| auth_allows_model_name(auth, &model.name))
        .filter(|model| api_format.is_none_or(|format| global_model_supports_format(model, format)))
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
