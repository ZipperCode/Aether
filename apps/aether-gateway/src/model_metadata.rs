use serde_json::Value;

/// 模型可调用 Endpoint 的协议族。协议别名先归一化，再收敛到稳定的业务能力族。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelFamily {
    Generation,
    Image,
    Embedding,
    Rerank,
}

/// 模型元数据的解析结果。缺少声明与无效声明必须区分，避免错误元数据触发宽松兜底。
pub(crate) enum DeclaredModelFamilies {
    Absent,
    Valid(Vec<ModelFamily>),
    Invalid,
}

impl DeclaredModelFamilies {
    /// 合并来自 Global Model、Provider Model 与映射配置的声明；任一无效声明都会保持 fail-closed。
    pub(crate) fn merge(&mut self, next: Self) {
        match next {
            Self::Invalid => *self = Self::Invalid,
            Self::Valid(mut next) => match self {
                Self::Valid(current) => current.append(&mut next),
                Self::Absent => *self = Self::Valid(next),
                Self::Invalid => {}
            },
            Self::Absent => {}
        }
    }

    /// 仅在元数据明确声明匹配协议族时返回 true，供 Endpoint 自动推断使用。
    pub(crate) fn supports_api_format(&self, api_format: &str) -> bool {
        let Some(requested_family) = known_format_family(api_format) else {
            return false;
        };
        matches!(self, Self::Valid(families) if families.contains(&requested_family))
    }

    /// 保留旧目录行为：完全缺少元数据的历史模型只默认归入生成族，无效声明仍拒绝发布。
    pub(crate) fn supports_api_format_or_legacy_generation(&self, api_format: &str) -> bool {
        let Some(requested_family) = known_format_family(api_format) else {
            return false;
        };
        match self {
            Self::Absent => requested_family == ModelFamily::Generation,
            Self::Valid(families) => families.contains(&requested_family),
            Self::Invalid => false,
        }
    }
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

/// 从结构化配置字段读取模型族声明。映射中的 `api_formats: null` 表示未声明，而非无效配置。
pub(crate) fn declared_model_families(
    value: &Value,
    mapping_null_is_absent: bool,
) -> DeclaredModelFamilies {
    let Some(object) = value.as_object() else {
        return DeclaredModelFamilies::Absent;
    };
    let mut families = Vec::new();
    for key in ["api_format", "client_api_format", "provider_api_format"] {
        let Some(value) = object.get(key) else {
            continue;
        };
        let Some(value) = value.as_str() else {
            return DeclaredModelFamilies::Invalid;
        };
        let Some(family) = known_format_family(value) else {
            return DeclaredModelFamilies::Invalid;
        };
        families.push(family);
    }
    if let Some(values) = object.get("api_formats") {
        if mapping_null_is_absent && values.is_null() {
            return if families.is_empty() {
                DeclaredModelFamilies::Absent
            } else {
                DeclaredModelFamilies::Valid(families)
            };
        }
        let Some(values) = values.as_array() else {
            return DeclaredModelFamilies::Invalid;
        };
        for value in values {
            let Some(value) = value.as_str() else {
                return DeclaredModelFamilies::Invalid;
            };
            let Some(family) = known_format_family(value) else {
                return DeclaredModelFamilies::Invalid;
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
                .filter_map(Value::as_str)
                .filter_map(capability_family),
        );
    }
    if families.is_empty() {
        DeclaredModelFamilies::Absent
    } else {
        DeclaredModelFamilies::Valid(families)
    }
}

/// 统一读取 Global Model 的配置与顶层能力声明，供管理端推断和公开目录共同复用。
pub(crate) fn global_model_declared_families(
    config: Option<&Value>,
    supported_capabilities: Option<&Value>,
) -> DeclaredModelFamilies {
    let mut declared = DeclaredModelFamilies::Absent;
    if let Some(config) = config {
        declared.merge(declared_model_families(config, false));
    }
    if let Some(capabilities) = supported_capabilities.and_then(Value::as_array) {
        let families = capabilities
            .iter()
            .filter_map(Value::as_str)
            .filter_map(capability_family)
            .collect::<Vec<_>>();
        if !families.is_empty() {
            declared.merge(DeclaredModelFamilies::Valid(families));
        }
    }
    declared
}
