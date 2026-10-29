use std::collections::BTreeMap;
use std::sync::Arc;

use aether_provider_transport::antigravity::{
    antigravity_model_thinking_variant, AntigravityThinkingLevel,
};
use serde_json::{Map, Value};

use crate::ai_serving::transport::antigravity::{
    antigravity_account_observation_due, build_antigravity_safe_v1internal_request,
    build_antigravity_static_identity_headers, classify_local_antigravity_request_support,
    AntigravityEnvelopeRequestType, AntigravityRequestAuth,
    AntigravityRequestAuthUnsupportedReason, AntigravityRequestEnvelopeSupport,
    AntigravityRequestSideSupport, AntigravityRequestSideUnsupportedReason,
};
use crate::ai_serving::transport::{
    build_standard_provider_request_headers, GatewayProviderTransportSnapshot,
    StandardProviderRequestHeaders, StandardProviderRequestHeadersInput,
};
use crate::AppState;

pub(crate) const ANTIGRAVITY_V1INTERNAL_ENVELOPE_NAME: &str = "antigravity:v1internal";

pub(crate) enum AntigravityV1InternalRequestError {
    TransportUnsupported,
    EnvelopeUnsupported,
    UpstreamUrlUnavailable,
    HeaderRulesApplyFailed,
}

pub(crate) struct AntigravityV1InternalRequestInput<'a> {
    pub(crate) state: &'a AppState,
    pub(crate) parts: &'a http::request::Parts,
    pub(crate) transport: &'a Arc<GatewayProviderTransportSnapshot>,
    pub(crate) trace_id: &'a str,
    pub(crate) client_api_format: &'a str,
    pub(crate) requested_model: &'a str,
    pub(crate) antigravity_thinking_level: Option<AntigravityThinkingLevel>,
    pub(crate) mapped_model: &'a str,
    pub(crate) provider_api_format: &'a str,
    pub(crate) auth_header: &'a str,
    pub(crate) auth_value: &'a str,
    pub(crate) request_headers: &'a http::HeaderMap,
    pub(crate) original_request_body: &'a Value,
    pub(crate) gemini_request_body: &'a Value,
    pub(crate) upstream_is_stream: bool,
    pub(crate) same_format: bool,
}

pub(crate) struct AntigravityV1InternalRequest {
    pub(crate) transport: Arc<GatewayProviderTransportSnapshot>,
    pub(crate) body: Value,
    pub(crate) headers: StandardProviderRequestHeaders,
    pub(crate) upstream_url: String,
}

pub(crate) async fn build_antigravity_v1internal_provider_request(
    input: AntigravityV1InternalRequestInput<'_>,
) -> Result<AntigravityV1InternalRequest, AntigravityV1InternalRequestError> {
    ensure_converted_openai_antigravity_thinking_resolved(&input)?;
    let payload = build_antigravity_v1internal_payload(
        input.state,
        input.transport,
        input.trace_id,
        input.mapped_model,
        input.gemini_request_body,
    )
    .await?;

    let upstream_url = crate::ai_serving::build_provider_transport_request_url_for_request_body(
        &payload.transport,
        input.provider_api_format,
        Some(input.mapped_model),
        input.upstream_is_stream,
        input.parts.uri.query(),
        None,
        None,
        Some(&payload.body),
    )
    .ok_or(AntigravityV1InternalRequestError::UpstreamUrlUnavailable)?;

    let extra_headers: BTreeMap<String, String> =
        build_antigravity_static_identity_headers(&payload.auth);
    let mut headers =
        build_standard_provider_request_headers(StandardProviderRequestHeadersInput {
            transport: &payload.transport,
            provider_api_format: input.provider_api_format,
            same_format: input.same_format,
            headers: input.request_headers,
            auth_header: input.auth_header,
            auth_value: input.auth_value,
            extra_headers: &extra_headers,
            header_rules: payload.transport.endpoint.header_rules.as_ref(),
            provider_request_body: &payload.body,
            original_request_body: input.original_request_body,
            upstream_is_stream: input.upstream_is_stream,
        })
        .ok_or(AntigravityV1InternalRequestError::HeaderRulesApplyFailed)?;
    headers
        .headers
        .insert("accept".to_string(), "text/event-stream".to_string());

    Ok(AntigravityV1InternalRequest {
        transport: payload.transport,
        body: payload.body,
        headers,
        upstream_url,
    })
}

struct AntigravityV1InternalPayload {
    transport: Arc<GatewayProviderTransportSnapshot>,
    auth: AntigravityRequestAuth,
    body: Value,
}

async fn build_antigravity_v1internal_payload(
    state: &AppState,
    transport: &Arc<GatewayProviderTransportSnapshot>,
    trace_id: &str,
    mapped_model: &str,
    gemini_request_body: &Value,
) -> Result<AntigravityV1InternalPayload, AntigravityV1InternalRequestError> {
    let mut resolved_transport = Arc::clone(transport);
    let mut antigravity_support = classify_local_antigravity_request_support(
        &resolved_transport,
        gemini_request_body,
        AntigravityEnvelopeRequestType::Agent,
    );

    let is_missing_project = matches!(
        antigravity_support,
        AntigravityRequestSideSupport::Unsupported(
            AntigravityRequestSideUnsupportedReason::UnsupportedAuth(
                AntigravityRequestAuthUnsupportedReason::MissingProjectId
            )
        )
    );
    let is_due = is_missing_project
        || (matches!(
            antigravity_support,
            AntigravityRequestSideSupport::Supported(_)
        ) && antigravity_account_observation_due(
            resolved_transport.key.upstream_metadata.as_ref(),
            crate::clock::current_unix_secs(),
        ));

    if is_due {
        if let Some(hydrated) = state
            .hydrate_antigravity_project_metadata_for_transport(&resolved_transport)
            .await
        {
            resolved_transport = Arc::new(hydrated);
            antigravity_support = classify_local_antigravity_request_support(
                &resolved_transport,
                gemini_request_body,
                AntigravityEnvelopeRequestType::Agent,
            );
        }
    }

    let auth = match antigravity_support {
        AntigravityRequestSideSupport::Supported(spec) => spec.auth,
        AntigravityRequestSideSupport::Unsupported(_) => {
            return Err(AntigravityV1InternalRequestError::TransportUnsupported);
        }
    };

    let body = match build_antigravity_safe_v1internal_request(
        &auth,
        trace_id,
        mapped_model,
        gemini_request_body,
        AntigravityEnvelopeRequestType::Agent,
    ) {
        AntigravityRequestEnvelopeSupport::Supported(envelope) => envelope,
        AntigravityRequestEnvelopeSupport::Unsupported(_) => {
            return Err(AntigravityV1InternalRequestError::EnvelopeUnsupported);
        }
    };

    Ok(AntigravityV1InternalPayload {
        transport: resolved_transport,
        auth,
        body,
    })
}

const GEMINI_MODEL_PREFIX: &[u8] = b"gemini-";

pub(crate) fn has_antigravity_base_gemini_client_suffix(
    provider_type: &str,
    provider_api_format: &str,
    mapped_model: &str,
    requested_model: &str,
) -> bool {
    is_antigravity_base_gemini_thinking_target(provider_type, provider_api_format, mapped_model)
        && antigravity_model_thinking_variant(requested_model).is_some()
}

pub(crate) fn apply_antigravity_gemini_thinking_selection(
    provider_request_body: &mut Value,
    provider_type: &str,
    provider_api_format: &str,
    mapped_model: &str,
    requested_model: Option<&str>,
    antigravity_thinking_level: Option<AntigravityThinkingLevel>,
) {
    if !is_antigravity_base_gemini_thinking_target(provider_type, provider_api_format, mapped_model)
    {
        return;
    }
    let Some(requested_model) = requested_model else {
        return;
    };
    if !is_gemini_family_model(requested_model) {
        return;
    }
    if let Some((_, level)) = antigravity_model_thinking_variant(requested_model) {
        // 基础上游模型没有后缀可供终态归一化；先应用客户端后缀，再由正文规则覆盖。
        inject_gemini_thinking_level(provider_request_body, level);
        return;
    }
    let Some(level) = antigravity_thinking_level else {
        return;
    };
    // Bare no-effort requests already default to Medium at the envelope. Only
    // recover explicit tiers lost by conversion, such as `extra-low` (Minimal).
    if level == AntigravityThinkingLevel::Medium {
        return;
    }
    if !gemini_thinking_tier_is_absent(provider_request_body) {
        return;
    }
    inject_gemini_thinking_level(provider_request_body, level);
}

fn is_antigravity_base_gemini_thinking_target(
    provider_type: &str,
    provider_api_format: &str,
    mapped_model: &str,
) -> bool {
    provider_type.trim().eq_ignore_ascii_case("antigravity")
        && crate::ai_serving::normalize_api_format_alias(provider_api_format)
            == "gemini:generate_content"
        && is_gemini_family_model(mapped_model)
        && aether_ai_formats::gemini_model_uses_thinking_level(mapped_model)
        && antigravity_model_thinking_variant(mapped_model).is_none()
}

fn ensure_converted_openai_antigravity_thinking_resolved(
    input: &AntigravityV1InternalRequestInput<'_>,
) -> Result<(), AntigravityV1InternalRequestError> {
    if input.same_format
        || !is_openai_converted_source(input.client_api_format)
        || crate::ai_serving::normalize_api_format_alias(input.provider_api_format)
            != "gemini:generate_content"
        || !is_gemini_family_model(input.requested_model)
        || !aether_ai_formats::gemini_model_uses_thinking_level(input.mapped_model)
    {
        return Ok(());
    }
    if input.antigravity_thinking_level.is_none()
        && gemini_thinking_tier_is_absent(input.gemini_request_body)
    {
        return Err(AntigravityV1InternalRequestError::EnvelopeUnsupported);
    }
    Ok(())
}

fn is_gemini_family_model(model: &str) -> bool {
    let model = model.trim();
    model
        .as_bytes()
        .get(..GEMINI_MODEL_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(GEMINI_MODEL_PREFIX))
        && model.len() > GEMINI_MODEL_PREFIX.len()
        && !model.ends_with('-')
}

fn is_openai_converted_source(client_api_format: &str) -> bool {
    matches!(
        crate::ai_serving::normalize_api_format_alias(client_api_format).as_str(),
        "openai:chat" | "openai:responses" | "openai:responses:compact"
    )
}

fn gemini_thinking_tier_is_absent(body: &Value) -> bool {
    let payload = if let Some(inner) = body.get("request") {
        if inner
            .as_object()
            .is_some_and(|request| request.contains_key("contents"))
        {
            inner.as_object()
        } else {
            body.as_object()
        }
    } else {
        body.as_object()
    };
    let Some(payload) = payload else {
        return false;
    };
    for generation_key in ["generationConfig", "generation_config"] {
        let Some(generation_value) = payload.get(generation_key) else {
            continue;
        };
        let Some(generation_config) = generation_value.as_object() else {
            return false;
        };
        for thinking_key in ["thinkingConfig", "thinking_config"] {
            let Some(thinking_value) = generation_config.get(thinking_key) else {
                continue;
            };
            let Some(thinking_config) = thinking_value.as_object() else {
                return false;
            };
            if thinking_config.contains_key("thinkingLevel")
                || thinking_config.contains_key("thinking_level")
                || thinking_config.contains_key("thinkingBudget")
                || thinking_config.contains_key("thinking_budget")
            {
                return false;
            }
        }
    }
    true
}

fn inject_gemini_thinking_level(body: &mut Value, level: AntigravityThinkingLevel) {
    let use_inner_request = body
        .get("request")
        .and_then(Value::as_object)
        .is_some_and(|request| request.contains_key("contents"));
    if !use_inner_request
        && body.as_object().is_some_and(|object| {
            object.contains_key("request") && !object.contains_key("contents")
        })
    {
        return;
    }
    let payload = if use_inner_request {
        body.get_mut("request").and_then(Value::as_object_mut)
    } else {
        body.as_object_mut()
    };
    let Some(payload) = payload else {
        return;
    };
    let generation_key =
        if !payload.contains_key("generationConfig") && payload.contains_key("generation_config") {
            "generation_config"
        } else {
            "generationConfig"
        };
    let Some(generation_config) = payload
        .entry(generation_key.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
    else {
        return;
    };
    let thinking_key = if generation_config.contains_key("thinkingConfig") {
        "thinkingConfig"
    } else if generation_config.contains_key("thinking_config")
        || generation_key == "generation_config"
    {
        "thinking_config"
    } else {
        "thinkingConfig"
    };
    let Some(thinking_config) = generation_config
        .entry(thinking_key.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
    else {
        return;
    };
    let level_key = if thinking_config.contains_key("thinkingLevel") {
        "thinkingLevel"
    } else if thinking_config.contains_key("thinking_level") || thinking_key == "thinking_config" {
        "thinking_level"
    } else {
        "thinkingLevel"
    };
    thinking_config.insert(
        level_key.to_string(),
        Value::String(level.as_str().to_string()),
    );
}
