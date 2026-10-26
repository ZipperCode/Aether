use std::collections::{BTreeSet, HashMap};

use aether_data_contracts::repository::candidate_selection::StoredMinimalCandidateSelectionRow;
use aether_data_contracts::DataLayerError;

use super::types::{
    EnumerateMinimalCandidateSelectionInput, SchedulerMinimalCandidateSelectionCandidate,
};

pub fn enumerate_minimal_candidate_selection(
    input: EnumerateMinimalCandidateSelectionInput<'_>,
) -> Result<Vec<SchedulerMinimalCandidateSelectionCandidate>, DataLayerError> {
    enumerate_minimal_candidate_selection_inner(input, false)
}

pub fn enumerate_minimal_candidate_selection_with_model_directives(
    input: EnumerateMinimalCandidateSelectionInput<'_>,
    enable_model_directives: bool,
) -> Result<Vec<SchedulerMinimalCandidateSelectionCandidate>, DataLayerError> {
    enumerate_minimal_candidate_selection_inner(input, enable_model_directives)
}

fn enumerate_minimal_candidate_selection_inner(
    input: EnumerateMinimalCandidateSelectionInput<'_>,
    enable_model_directives: bool,
) -> Result<Vec<SchedulerMinimalCandidateSelectionCandidate>, DataLayerError> {
    let EnumerateMinimalCandidateSelectionInput {
        rows,
        normalized_api_format,
        request_operation,
        requested_model_name,
        resolved_global_model_name,
        require_streaming,
        auth_constraints,
        ..
    } = input;

    if normalized_api_format.is_empty() {
        return Ok(Vec::new());
    }
    if !crate::auth_constraints_allow_api_format(auth_constraints, normalized_api_format) {
        return Ok(Vec::new());
    }
    if !crate::auth_constraints_allow_model_with_model_directives(
        auth_constraints,
        requested_model_name,
        resolved_global_model_name,
        enable_model_directives,
    ) {
        return Ok(Vec::new());
    }

    let mut candidates = Vec::with_capacity(rows.len());
    let mut allowed_models_by_key = HashMap::<String, Vec<String>>::new();
    for row in rows {
        if !crate::auth_constraints_allow_provider(
            auth_constraints,
            &row.provider_id,
            &row.provider_name,
            &row.provider_type,
        ) {
            continue;
        }
        if require_streaming && !row.supports_streaming() {
            continue;
        }
        let prepared_allowed_models = if let Some(allowed_models) = row.key_allowed_models.as_ref()
        {
            Some(
                allowed_models_by_key
                    .entry(row.key_id.clone())
                    .or_insert_with(|| {
                        let mut models = allowed_models
                            .iter()
                            .map(|value| value.trim().to_string())
                            .filter(|value| !value.is_empty())
                            .collect::<Vec<_>>();
                        models.sort_unstable();
                        models.dedup();
                        models
                    })
                    .as_slice(),
            )
        } else {
            None
        };
        let resolved_provider_model =
            crate::model::resolve_provider_model_name_with_prepared_allowed_models(
                &row,
                requested_model_name,
                normalized_api_format,
                enable_model_directives,
                request_operation,
                prepared_allowed_models,
            )
            .or_else(|| {
                // 逻辑号池组：枚举列表里每个 Pool 组只有一个代表 Key 行，代表账号
                // 的显式模型清单（与请求不兼容或显式为空）不得在成员展开存在之前
                // 否决整个组。这里只忽略代表的模型清单限制，Endpoint/操作作用域仍
                // 由 scoped 配置/默认目标解析严格保持，也不会产生新的上游模型名；
                // 真实成员水合仍逐账号严格校验（含空清单拒绝）。通用解析器不做此
                // 回退，因为展开后的成员行同样带 pool 标志。
                if !row.routing_facts.provider_pool_enabled {
                    return None;
                }
                crate::model::resolve_selected_provider_model_name(
                    &row,
                    normalized_api_format,
                    request_operation,
                )
                .map(|selected_provider_model_name| (selected_provider_model_name, None))
            });
        let Some((selected_provider_model_name, mapping_matched_model)) = resolved_provider_model
        else {
            continue;
        };

        let supports_streaming = row.supports_streaming();
        candidates.push(SchedulerMinimalCandidateSelectionCandidate {
            provider_id: row.provider_id,
            provider_name: row.provider_name,
            provider_type: row.provider_type,
            provider_priority: row.provider_priority,
            endpoint_id: row.endpoint_id,
            endpoint_api_format: row.endpoint_api_format,
            key_id: row.key_id,
            key_name: row.key_name,
            key_auth_type: row.key_auth_type,
            key_internal_priority: row.key_internal_priority,
            key_global_priority_for_format: crate::extract_global_priority_for_format(
                row.key_global_priority_by_format.as_ref(),
                normalized_api_format,
            )?,
            key_capabilities: row.key_capabilities,
            routing_facts: row.routing_facts,
            model_id: row.model_id,
            global_model_id: row.global_model_id,
            global_model_name: row.global_model_name,
            selected_provider_model_name,
            supports_streaming,
            mapping_matched_model,
        });
    }

    Ok(candidates)
}

pub fn collect_global_model_names_for_required_capability(
    rows: Vec<StoredMinimalCandidateSelectionRow>,
    normalized_api_format: &str,
    required_capability: &str,
    require_streaming: bool,
    auth_constraints: Option<&crate::SchedulerAuthConstraints>,
) -> Vec<String> {
    if normalized_api_format.is_empty() || required_capability.trim().is_empty() {
        return Vec::new();
    }
    if !crate::auth_constraints_allow_api_format(auth_constraints, normalized_api_format) {
        return Vec::new();
    }

    let mut model_names = BTreeSet::new();
    for row in rows {
        if !crate::auth_constraints_allow_provider(
            auth_constraints,
            &row.provider_id,
            &row.provider_name,
            &row.provider_type,
        ) {
            continue;
        }
        if !crate::row_supports_required_capability(&row, required_capability) {
            continue;
        }
        if require_streaming && !row.supports_streaming() {
            continue;
        }
        if !crate::auth_constraints_allow_model(
            auth_constraints,
            &row.global_model_name,
            &row.global_model_name,
        ) {
            continue;
        }
        model_names.insert(row.global_model_name);
    }

    model_names.into_iter().collect()
}
