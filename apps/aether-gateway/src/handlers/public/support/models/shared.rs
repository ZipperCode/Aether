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

fn auth_allows_model(
    auth: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    row: &StoredModelCatalogEntry,
) -> bool {
    auth.and_then(crate::data::auth::GatewayAuthApiKeySnapshot::effective_allowed_models)
        .is_none_or(|allowed| allowed.iter().any(|value| value == &row.global_model_name))
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

pub(super) fn filter_eligible_model_rows(
    rows: Vec<StoredMinimalCandidateSelectionRow>,
    auth_snapshot: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    api_format: &str,
) -> Vec<StoredMinimalCandidateSelectionRow> {
    rows.into_iter()
        .filter(|row| {
            row.global_model_is_active
                && row.provider_model_is_active
                && row.provider_model_is_available
                && row.provider_is_active
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
