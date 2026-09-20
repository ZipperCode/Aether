use serde_json::{Map, Value};

use super::auth::{AntigravityRequestAuth, ANTIGRAVITY_REQUEST_USER_AGENT};
use super::schema::{normalize_claude_unions, normalize_tool_parameters, SchemaBudget};

/// 只识别真实 HTTP 400 的 Gemini 签名拒绝；调用方还必须确认 Antigravity 适配身份。
pub fn is_antigravity_corrupted_thought_signature(status_code: u16, body: &Value) -> bool {
    status_code == 400
        && body.pointer("/error/status").and_then(Value::as_str) == Some("INVALID_ARGUMENT")
        && body
            .pointer("/error/message")
            .and_then(Value::as_str)
            .is_some_and(|message| {
                message.trim().trim_end_matches('.').trim_end() == "Corrupted thought signature"
            })
}

/// 上游明确拒绝的历史签名类型；Base64 路径只允许修复被指明的 model part。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntigravityThoughtSignatureRejection {
    Corrupted,
    Base64DecodeFailure {
        content_index: usize,
        part_index: usize,
    },
}

/// 识别 Google 指明字段的 Base64 拒绝；结构化 fieldViolations 出现时不退回宽松文案匹配。
pub fn classify_antigravity_thought_signature_rejection(
    status_code: u16,
    body: &Value,
) -> Option<AntigravityThoughtSignatureRejection> {
    if is_antigravity_corrupted_thought_signature(status_code, body) {
        return Some(AntigravityThoughtSignatureRejection::Corrupted);
    }
    if status_code != 400
        || body.pointer("/error/status").and_then(Value::as_str) != Some("INVALID_ARGUMENT")
    {
        return None;
    }

    let error = body.get("error")?;
    if let Some(details) = error.get("details").and_then(Value::as_array) {
        let mut has_field_violations = false;
        for detail in details {
            let Some(violations) = detail.get("fieldViolations") else {
                continue;
            };
            has_field_violations = true;
            let violations = violations.as_array()?;
            for violation in violations {
                let Some(field) = violation.get("field").and_then(Value::as_str) else {
                    continue;
                };
                let Some(description) = violation.get("description").and_then(Value::as_str) else {
                    continue;
                };
                if let Some((content_index, part_index)) =
                    base64_thought_signature_location(field, description)
                {
                    return Some(AntigravityThoughtSignatureRejection::Base64DecodeFailure {
                        content_index,
                        part_index,
                    });
                }
            }
        }
        if has_field_violations {
            return None;
        }
    }

    let message = error.get("message").and_then(Value::as_str)?;
    let (field, _) = message.split_once("' (TYPE_BYTES)")?;
    let field = field.strip_prefix("Invalid value at '")?;
    base64_thought_signature_location(field, message).map(|(content_index, part_index)| {
        AntigravityThoughtSignatureRejection::Base64DecodeFailure {
            content_index,
            part_index,
        }
    })
}

/// 只接受 Gemini proto 回显的精确 part-root 字段，避免修改其他 bytes 或嵌套工具数据。
fn base64_thought_signature_location(field: &str, description: &str) -> Option<(usize, usize)> {
    if !description.contains("TYPE_BYTES") || !description.contains("Base64 decoding failed") {
        return None;
    }
    let field = field.strip_prefix("request.contents[")?;
    let (content_index, field) = field.split_once("].parts[")?;
    let (part_index, suffix) = field.split_once("].thought_signature")?;
    // proto 数组下标只允许无符号十进制数字，不能把宽松整数解析当成字段路径校验。
    if !suffix.is_empty()
        || [content_index, part_index]
            .iter()
            .any(|index| index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    Some((content_index.parse().ok()?, part_index.parse().ok()?))
}

/// 上游明确拒绝后克隆历史请求，仅替换 model 消息 part 自身已有的签名；无变化时不重试。
pub fn repair_antigravity_thought_signatures(body: &Value) -> Option<Value> {
    let mut repaired = body.clone();
    let contents = antigravity_request_contents_mut(body, &mut repaired)?;
    let mut changed = false;
    for content in contents {
        if content.get("role").and_then(Value::as_str) != Some("model") {
            continue;
        }
        let Some(parts) = content.get_mut("parts").and_then(Value::as_array_mut) else {
            continue;
        };
        for part in parts {
            changed |= replace_thought_signature(part);
        }
    }
    changed.then_some(repaired)
}

/// 按拒绝类型修复；旧错误保留全量历史降级，新 Base64 错误只替换命中的一个 part。
pub fn repair_antigravity_thought_signature_rejection(
    body: &Value,
    rejection: AntigravityThoughtSignatureRejection,
) -> Option<Value> {
    let AntigravityThoughtSignatureRejection::Base64DecodeFailure {
        content_index,
        part_index,
    } = rejection
    else {
        return repair_antigravity_thought_signatures(body);
    };
    let mut repaired = body.clone();
    let contents = antigravity_request_contents_mut(body, &mut repaired)?;
    let content = contents.get_mut(content_index)?;
    if content.get("role").and_then(Value::as_str) != Some("model") {
        return None;
    }
    let part = content
        .get_mut("parts")
        .and_then(Value::as_array_mut)?
        .get_mut(part_index)?;
    replace_thought_signature(part).then_some(repaired)
}

fn antigravity_request_contents_mut<'a>(
    body: &Value,
    repaired: &'a mut Value,
) -> Option<&'a mut Vec<Value>> {
    let request = if body.pointer("/request/contents").is_some() {
        repaired.get_mut("request")?
    } else {
        repaired
    };
    request.get_mut("contents")?.as_array_mut()
}

fn replace_thought_signature(part: &mut Value) -> bool {
    let mut changed = false;
    for alias in ["thoughtSignature", "thought_signature"] {
        if let Some(signature) = part.get_mut(alias) {
            if signature.as_str() != Some("skip_thought_signature_validator") {
                *signature = Value::String("skip_thought_signature_validator".to_string());
                changed = true;
            }
        }
    }
    changed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntigravityEnvelopeRequestType {
    Agent,
    Checkpoint,
    EndpointTest,
}

impl AntigravityEnvelopeRequestType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Checkpoint => "checkpoint",
            Self::EndpointTest => "endpoint_test",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AntigravityRequestEnvelopeSupport {
    Supported(Value),
    Unsupported(AntigravityRequestEnvelopeUnsupportedReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AntigravityRequestEnvelopeUnsupportedReason {
    NonObjectBody,
    MissingContents,
    MissingRequestId,
    MissingModel,
    ToolSchemaBudgetExceeded,
}

pub fn classify_antigravity_safe_request_body(
    request_body: &Value,
) -> Result<(), AntigravityRequestEnvelopeUnsupportedReason> {
    let Value::Object(map) = request_body else {
        return Err(AntigravityRequestEnvelopeUnsupportedReason::NonObjectBody);
    };
    if !map.contains_key("contents") && existing_v1internal_request_object(map).is_none() {
        return Err(AntigravityRequestEnvelopeUnsupportedReason::MissingContents);
    }

    Ok(())
}

/// 构建 Antigravity 私有 v1internal envelope，并只在该边界规范化工具字段与搜索名称。
pub fn build_antigravity_safe_v1internal_request(
    auth: &AntigravityRequestAuth,
    request_id: &str,
    model: &str,
    request_body: &Value,
    request_type: AntigravityEnvelopeRequestType,
) -> AntigravityRequestEnvelopeSupport {
    if request_id.trim().is_empty() {
        return AntigravityRequestEnvelopeSupport::Unsupported(
            AntigravityRequestEnvelopeUnsupportedReason::MissingRequestId,
        );
    }
    if model.trim().is_empty() {
        return AntigravityRequestEnvelopeSupport::Unsupported(
            AntigravityRequestEnvelopeUnsupportedReason::MissingModel,
        );
    }
    if let Err(reason) = classify_antigravity_safe_request_body(request_body) {
        return AntigravityRequestEnvelopeSupport::Unsupported(reason);
    }

    let Value::Object(source) = request_body else {
        return AntigravityRequestEnvelopeSupport::Unsupported(
            AntigravityRequestEnvelopeUnsupportedReason::NonObjectBody,
        );
    };

    if let Some(existing_request) = existing_v1internal_request_object(source) {
        let mut inner_request: Map<String, Value> = existing_request.clone();
        inner_request.remove("model");
        inner_request.remove("safetySettings");
        inner_request.remove("safety_settings");
        normalize_antigravity_claude_thought_history(&mut inner_request, model);
        normalize_antigravity_builtin_tool_names(&mut inner_request);
        if normalize_antigravity_function_declaration_parameters(&mut inner_request, model).is_err()
        {
            return AntigravityRequestEnvelopeSupport::Unsupported(
                AntigravityRequestEnvelopeUnsupportedReason::ToolSchemaBudgetExceeded,
            );
        }
        let request_id = non_empty_string_field(source, "requestId").unwrap_or(request_id);
        let user_agent =
            non_empty_string_field(source, "userAgent").unwrap_or(ANTIGRAVITY_REQUEST_USER_AGENT);
        let existing_request_type = existing_v1internal_request_type(source);

        let mut envelope = serde_json::json!({
            "project": auth.project_id,
            "requestId": request_id,
            "request": Value::Object(inner_request),
            "model": model,
            "userAgent": user_agent,
        });
        if let Some(existing_request_type) = existing_request_type {
            envelope["requestType"] = Value::String(existing_request_type.to_string());
        } else if request_type != AntigravityEnvelopeRequestType::Agent {
            envelope["requestType"] = Value::String(request_type.as_str().to_string());
        }
        return AntigravityRequestEnvelopeSupport::Supported(envelope);
    }

    let mut inner_request: Map<String, Value> = source.clone();
    inner_request.remove("model");
    inner_request.remove("safetySettings");
    inner_request.remove("safety_settings");
    normalize_antigravity_claude_thought_history(&mut inner_request, model);
    normalize_antigravity_builtin_tool_names(&mut inner_request);
    if normalize_antigravity_function_declaration_parameters(&mut inner_request, model).is_err() {
        return AntigravityRequestEnvelopeSupport::Unsupported(
            AntigravityRequestEnvelopeUnsupportedReason::ToolSchemaBudgetExceeded,
        );
    }

    let mut envelope = serde_json::json!({
        "project": auth.project_id,
        "requestId": request_id,
        "request": Value::Object(inner_request),
        "model": model,
        "userAgent": ANTIGRAVITY_REQUEST_USER_AGENT,
    });
    if request_type != AntigravityEnvelopeRequestType::Agent {
        envelope["requestType"] = Value::String(request_type.as_str().to_string());
    }
    AntigravityRequestEnvelopeSupport::Supported(envelope)
}

/// 私有 v1internal 使用现代 `googleSearch`；只归一化别名，避免旧 retrieval 名称引发畸形工具调用。
fn normalize_antigravity_builtin_tool_names(request: &mut Map<String, Value>) {
    let Some(tools) = request.get_mut("tools").and_then(Value::as_array_mut) else {
        return;
    };

    for tool in tools {
        let Some(tool_object) = tool.as_object_mut() else {
            continue;
        };

        if let Some(payload) = tool_object.remove("google_search") {
            tool_object
                .entry("googleSearch".to_string())
                .or_insert(payload);
        }
    }
}

/// Claude requires a replayable signature on historical thinking blocks. In
/// particular, Responses reasoning summaries are not signed thinking. Omit that
/// non-replayable metadata instead of inventing a signature or promoting private
/// reasoning into ordinary assistant text. Keep Gemini's native policy unchanged.
fn normalize_antigravity_claude_thought_history(request: &mut Map<String, Value>, model: &str) {
    if !model.trim().to_ascii_lowercase().starts_with("claude-") {
        return;
    }
    let Some(contents) = request.get_mut("contents").and_then(Value::as_array_mut) else {
        return;
    };
    contents.retain_mut(|message| {
        if message.get("role").and_then(Value::as_str) != Some("model") {
            return true;
        }
        let Some(parts) = message.get_mut("parts").and_then(Value::as_array_mut) else {
            return true;
        };
        let previous_len = parts.len();
        parts.retain(|part| {
            part.get("thought").and_then(Value::as_bool) != Some(true)
                || ["thoughtSignature", "thought_signature"].iter().any(|key| {
                    part.get(*key)
                        .and_then(Value::as_str)
                        .is_some_and(|signature| !signature.trim().is_empty())
                })
        });
        // Do not introduce empty messages when a turn contained only a summary.
        !parts.is_empty() || parts.len() == previous_len
    });
}

/// 私有工具参数沿用上游有界 Schema 降级，Claude 另处理不可接受的联合；不影响公共 Gemini。
fn normalize_antigravity_function_declaration_parameters(
    request: &mut Map<String, Value>,
    model: &str,
) -> Result<(), ()> {
    let Some(tools) = request.get_mut("tools").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    let mut budget = SchemaBudget::default();
    let claude = model.trim().to_ascii_lowercase().starts_with("claude-");

    for tool in tools {
        let Some(tool_object) = tool.as_object_mut() else {
            continue;
        };
        for key in ["functionDeclarations", "function_declarations"] {
            let Some(declarations) = tool_object.get_mut(key).and_then(Value::as_array_mut) else {
                continue;
            };
            for declaration in declarations {
                let Some(declaration_object) = declaration.as_object_mut() else {
                    continue;
                };
                if let Some(parameters) = declaration_object.remove("parametersJsonSchema") {
                    declaration_object
                        .entry("parameters".to_string())
                        .or_insert(parameters);
                }
                if let Some(parameters) = declaration_object.remove("parameters_json_schema") {
                    declaration_object
                        .entry("parameters".to_string())
                        .or_insert(parameters);
                }
                if let Some(parameters) = declaration_object.get_mut("parameters") {
                    normalize_tool_parameters(parameters, &mut budget)?;
                    if claude {
                        normalize_claude_unions(parameters, &mut budget)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn existing_v1internal_request_object(source: &Map<String, Value>) -> Option<&Map<String, Value>> {
    source
        .get("request")
        .and_then(Value::as_object)
        .filter(|request| request.contains_key("contents"))
}

fn non_empty_string_field<'a>(source: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    source
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn existing_v1internal_request_type(source: &Map<String, Value>) -> Option<&str> {
    match non_empty_string_field(source, "requestType")? {
        "agent" => Some("agent"),
        "checkpoint" => Some("checkpoint"),
        "endpoint_test" => Some("endpoint_test"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        build_antigravity_safe_v1internal_request, classify_antigravity_safe_request_body,
        classify_antigravity_thought_signature_rejection,
        is_antigravity_corrupted_thought_signature, repair_antigravity_thought_signature_rejection,
        repair_antigravity_thought_signatures, AntigravityEnvelopeRequestType,
        AntigravityRequestAuth, AntigravityRequestEnvelopeSupport,
        AntigravityThoughtSignatureRejection,
    };
    use crate::antigravity::ANTIGRAVITY_REQUEST_USER_AGENT;
    use crate::same_format_provider::{
        build_same_format_provider_request_body, SameFormatProviderFamily,
        SameFormatProviderRequestBodyInput,
    };

    #[test]
    fn antigravity_claude_fabric_union_regression_across_client_formats() {
        use aether_ai_formats::formats::shared::standard_matrix::build_standard_request_body;
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("fabric_exec_schema.json")).unwrap();
        let clients = [
            (
                "gemini:generate_content",
                json!({"contents": [{"role":"user","parts":[{"text":"hi"}]}],
                "tools":[{"functionDeclarations":[{"name":"fabric_exec","parametersJsonSchema":schema}]}]}),
            ),
            (
                "claude:messages",
                json!({"messages":[{"role":"user","content":"hi"}],"max_tokens":128,
                "tools":[{"name":"fabric_exec","input_schema":schema}]}),
            ),
            (
                "openai:chat",
                json!({"messages":[{"role":"user","content":"hi"}],
                "tools":[{"type":"function","function":{"name":"fabric_exec","parameters":schema}}]}),
            ),
            (
                "openai:responses",
                json!({"input":"hi",
                "tools":[{"type":"function","name":"fabric_exec","parameters":schema}]}),
            ),
        ];
        for (format, body) in clients {
            for model in ["claude-opus-4-6-thinking", "gemini-test"] {
                let converted = build_standard_request_body(
                    &body,
                    format,
                    model,
                    "antigravity",
                    "gemini:generate_content",
                    "",
                    true,
                    None,
                    None,
                )
                .unwrap();
                for request in [converted.clone(), json!({"request":converted})] {
                    let AntigravityRequestEnvelopeSupport::Supported(output) =
                        build_antigravity_safe_v1internal_request(
                            &sample_auth(),
                            "union-regression",
                            model,
                            &request,
                            AntigravityEnvelopeRequestType::Agent,
                        )
                    else {
                        panic!("failed {format} {model}");
                    };
                    let s = &output["request"]["tools"][0]["functionDeclarations"][0]["parameters"];
                    assert_eq!(s["required"], json!(["code"]));
                    assert_eq!(
                        s["properties"]["payloads"]["additionalProperties"],
                        json!({"type":"string"})
                    );
                    assert_eq!(s["properties"]["tokenBudget"]["minimum"], 1);
                    if model.starts_with("claude-") {
                        assert_eq!(
                            s["properties"]["resultFormat"],
                            json!({"type":"string","enum":["auto","json","text","yaml"]})
                        );
                        let display = &s["properties"]["display"];
                        assert!(
                            display.get("type").is_none(),
                            "do not select one union branch"
                        );
                        assert!(display.get("anyOf").is_none());
                        assert!(display["description"].as_str().unwrap().contains("object"));
                        assert!(display["description"].as_str().unwrap().contains("string"));
                    } else {
                        assert!(s["properties"]["resultFormat"]["anyOf"].is_array());
                        assert!(s["properties"]["display"]["anyOf"].is_array());
                    }
                    let AntigravityRequestEnvelopeSupport::Supported(twice) =
                        build_antigravity_safe_v1internal_request(
                            &sample_auth(),
                            "union-regression",
                            model,
                            &output,
                            AntigravityEnvelopeRequestType::Agent,
                        )
                    else {
                        panic!("idempotence");
                    };
                    assert_eq!(twice, output);
                }
            }
        }
    }

    #[test]
    fn antigravity_claude_omits_only_unsigned_thought_parts() {
        let signed = json!({"text":"signed plan","thought":true,"thoughtSignature":"signed-value"});
        let signed_alias =
            json!({"text":"signed alias","thought":true,"thought_signature":"alias-value"});
        let call = json!({"functionCall":{"id":"call_1","name":"lookup","args":{}},"thoughtSignature":"skip_thought_signature_validator"});
        let result = json!({"role":"user","parts":[{"functionResponse":{"id":"call_1","name":"lookup","response":{"result":"ok"}}}]});
        let body = json!({
            "contents":[
                {"role":"user","parts":[{"text":"hello"}]},
                {"role":"model","parts":[{"text":"unsigned-only summary","thought":true}]},
                {"role":"model","parts":[
                    {"text":"unsigned summary","thought":true},
                    {"text":"empty signature","thought":true,"thoughtSignature":""},
                    {"text":"blank signature","thought":true,"thoughtSignature":"  "},
                    {"text":"non-string signature","thought":true,"thoughtSignature":12},
                    signed,signed_alias,{"text":"visible answer"},call
                ]},
                result
            ],
            "generationConfig":{"maxOutputTokens":64000,"thinkingConfig":{"includeThoughts":true,"thinkingBudget":4096}}
        });
        for model in [
            "claude-sonnet-4-6",
            "claude-opus-4-6-thinking",
            "gemini-3.8-flash-high",
        ] {
            for wrapped in [false, true] {
                let input = if wrapped {
                    json!({"request":body})
                } else {
                    body.clone()
                };
                let original = input.clone();
                let AntigravityRequestEnvelopeSupport::Supported(output) =
                    build_antigravity_safe_v1internal_request(
                        &sample_auth(),
                        "thought-test",
                        model,
                        &input,
                        AntigravityEnvelopeRequestType::Agent,
                    )
                else {
                    panic!("expected supported envelope");
                };
                let expected = if model.starts_with("claude-") {
                    json!([
                        {"role":"user","parts":[{"text":"hello"}]},
                        {"role":"model","parts":[signed,signed_alias,{"text":"visible answer"},call]},
                        result
                    ])
                } else {
                    body["contents"].clone()
                };
                assert_eq!(
                    output["request"]["contents"], expected,
                    "{model} wrapped={wrapped}"
                );
                assert_eq!(
                    output["request"]["generationConfig"],
                    body["generationConfig"]
                );
                assert_eq!(input, original, "do not mutate caller-owned input");
                let rebuilt = build_antigravity_safe_v1internal_request(
                    &sample_auth(),
                    "thought-test",
                    model,
                    &output,
                    AntigravityEnvelopeRequestType::Agent,
                );
                assert_eq!(
                    rebuilt,
                    AntigravityRequestEnvelopeSupport::Supported(output)
                );
            }
        }
    }

    #[test]
    fn antigravity_claude_cross_format_unsigned_reasoning_history() {
        use aether_ai_formats::formats::shared::standard_matrix::build_standard_request_body;
        let clients = [
            (
                "openai:responses",
                json!({
                    "max_output_tokens":64000,
                    "input":[
                        {"role":"user","content":"hello"},
                        {"type":"reasoning","id":"rs_history","status":"completed","summary":[{"type":"summary_text","text":"historical summary"}],"content":[]},
                        {"role":"assistant","content":[{"type":"output_text","text":"visible answer"}]},
                        {"type":"function_call","call_id":"call_1","name":"lookup","arguments":"{}"},
                        {"type":"function_call_output","call_id":"call_1","output":"ok"},
                        {"role":"user","content":"continue"}
                    ]
                }),
            ),
            (
                "claude:messages",
                json!({
                    "max_tokens":64000,
                    "messages":[
                        {"role":"user","content":"hello"},
                        {"role":"assistant","content":[
                            {"type":"thinking","thinking":"historical summary"},
                            {"type":"text","text":"visible answer"},
                            {"type":"tool_use","id":"call_1","name":"lookup","input":{}}
                        ]},
                        {"role":"user","content":[{"type":"tool_result","tool_use_id":"call_1","content":"ok"},{"type":"text","text":"continue"}]}
                    ]
                }),
            ),
        ];
        for (format, body) in clients {
            for model in [
                "claude-sonnet-4-6",
                "claude-opus-4-6-thinking",
                "gemini-3.8-flash-high",
            ] {
                let converted = build_standard_request_body(
                    &body,
                    format,
                    model,
                    "antigravity",
                    "gemini:generate_content",
                    "",
                    true,
                    None,
                    None,
                )
                .unwrap();
                assert!(
                    converted["contents"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .flat_map(|m| m["parts"].as_array().unwrap())
                        .any(|p| p["thought"] == true),
                    "fixture must exercise unsigned thoughts: {format}"
                );
                let AntigravityRequestEnvelopeSupport::Supported(output) =
                    build_antigravity_safe_v1internal_request(
                        &sample_auth(),
                        "reasoning-history",
                        model,
                        &converted,
                        AntigravityEnvelopeRequestType::Agent,
                    )
                else {
                    panic!("expected supported envelope");
                };
                let messages = output["request"]["contents"].as_array().unwrap();
                assert!(messages
                    .iter()
                    .all(|m| !m["parts"].as_array().unwrap().is_empty()));
                let parts: Vec<_> = messages
                    .iter()
                    .flat_map(|m| m["parts"].as_array().unwrap())
                    .collect();
                assert_eq!(
                    parts.iter().any(|p| p["thought"] == true),
                    !model.starts_with("claude-")
                );
                assert!(parts.iter().any(|p| p["text"] == "visible answer"));
                assert!(parts.iter().any(|p| p["text"] == "continue"));
                assert!(parts.iter().any(|p| p["functionCall"]["id"] == "call_1"
                    && p["functionCall"]["name"] == "lookup"));
                assert!(parts.iter().any(|p| p["functionResponse"]["id"] == "call_1"
                    && p["functionResponse"]["name"] == "lookup"));
                if model.starts_with("claude-") {
                    assert!(
                        !parts.iter().any(|p| p["text"] == "historical summary"),
                        "do not promote private reasoning to visible text"
                    );
                }
                assert_eq!(
                    output["request"]["generationConfig"]["maxOutputTokens"],
                    64000
                );
            }
        }
    }

    fn sample_auth() -> AntigravityRequestAuth {
        AntigravityRequestAuth {
            project_id: "project-ant-123".to_string(),
            client_version: None,
            session_id: None,
        }
    }

    /// 仅接受已确认的 HTTP 400 Gemini 错误结构，避免普通签名文案触发语义降级。
    #[test]
    fn corrupted_thought_signature_requires_exact_http_error_contract() {
        let matching = json!({
            "error": {
                "status": "INVALID_ARGUMENT",
                "message": "  Corrupted thought signature...  "
            }
        });

        assert!(is_antigravity_corrupted_thought_signature(400, &matching));
        assert!(!is_antigravity_corrupted_thought_signature(200, &matching));
        assert!(!is_antigravity_corrupted_thought_signature(
            400,
            &json!({"error": {"status": "FAILED_PRECONDITION", "message": "Corrupted thought signature."}}),
        ));
        assert!(!is_antigravity_corrupted_thought_signature(
            400,
            &json!({"error": {"status": "INVALID_ARGUMENT", "message": "Invalid thought signature."}}),
        ));
    }

    /// Base64 拒绝必须同时指定 Gemini 历史字段和 TYPE_BYTES 解码事实，不能从普通 bytes 错误推断。
    #[test]
    fn base64_thought_signature_rejection_requires_exact_field_contract() {
        let message = "Invalid value at 'request.contents[2].parts[1].thought_signature' (TYPE_BYTES), Base64 decoding failed";
        let matching = json!({"error": {"status": "INVALID_ARGUMENT", "message": message}});
        assert_eq!(
            classify_antigravity_thought_signature_rejection(400, &matching),
            Some(AntigravityThoughtSignatureRejection::Base64DecodeFailure {
                content_index: 2,
                part_index: 1,
            })
        );
        assert_eq!(
            classify_antigravity_thought_signature_rejection(
                400,
                &json!({"error": {
                    "status": "INVALID_ARGUMENT",
                    "message": message,
                    "details": [{"fieldViolations": [{
                        "field": "request.contents[2].parts[1].inline_data",
                        "description": "TYPE_BYTES Base64 decoding failed",
                    }]}],
                }}),
            ),
            None,
            "出现结构化 fieldViolations 后不得回退到宽松 message 匹配"
        );
        for (status, field, description) in [
            (
                200,
                "request.contents[2].parts[1].thought_signature",
                "TYPE_BYTES Base64 decoding failed",
            ),
            (
                400,
                "request.contents[2].parts[1].inline_data",
                "TYPE_BYTES Base64 decoding failed",
            ),
            (
                400,
                "request.contents[2].parts[1].thoughtSignature",
                "TYPE_BYTES Base64 decoding failed",
            ),
            (
                400,
                "request.contents[2].parts[1].thought_signature",
                "TYPE_BYTES invalid bytes",
            ),
            (
                400,
                "request.contents[+2].parts[1].thought_signature",
                "TYPE_BYTES Base64 decoding failed",
            ),
            (
                400,
                "request.contents[ 2].parts[1].thought_signature",
                "TYPE_BYTES Base64 decoding failed",
            ),
            (
                400,
                "request.contents[2].parts[-1].thought_signature",
                "TYPE_BYTES Base64 decoding failed",
            ),
            (
                400,
                "request.contents[2].parts[1 ].thought_signature",
                "TYPE_BYTES Base64 decoding failed",
            ),
            (
                400,
                "request.contents[].parts[1].thought_signature",
                "TYPE_BYTES Base64 decoding failed",
            ),
            (
                400,
                "request.contents[999999999999999999999999999999].parts[1].thought_signature",
                "TYPE_BYTES Base64 decoding failed",
            ),
            (
                400,
                "request.contents[2].parts[1].functionCall.args.thought_signature",
                "TYPE_BYTES Base64 decoding failed",
            ),
            (
                400,
                "request.contents[2].parts[1].thought_signature.extra",
                "TYPE_BYTES Base64 decoding failed",
            ),
        ] {
            assert_eq!(
                classify_antigravity_thought_signature_rejection(
                    status,
                    &json!({"error": {
                        "status": "INVALID_ARGUMENT",
                        "details": [{"fieldViolations": [{"field": field, "description": description}]}],
                    }}),
                ),
                None
            );
        }
        assert_eq!(
            classify_antigravity_thought_signature_rejection(
                400,
                &json!({"error": {"status": "FAILED_PRECONDITION", "message": message}}),
            ),
            None
        );
    }

    /// Base64 位置只替换目标 model part 的根签名，用户、工具和嵌套业务字段必须保持原样。
    #[test]
    fn base64_thought_signature_repair_is_targeted_and_immutable() {
        let source = json!({"request": {"contents": [
            {"role": "model", "parts": [{
                "thoughtSignature": "unrelated-model-signature",
                "functionCall": {"name": "lookup", "args": {"thoughtSignature": "tool-data"}},
            }]},
            {"role": "model", "parts": [{
                "thoughtSignature": "rejected-signature",
                "thought_signature": "rejected-snake-signature",
                "text": "keep historical text",
            }]},
            {"role": "user", "parts": [{"thoughtSignature": "user-signature"}]},
            {"role": "tool", "parts": [{"thoughtSignature": "tool-signature"}]},
        ]}});
        let original = source.clone();
        let rejection = AntigravityThoughtSignatureRejection::Base64DecodeFailure {
            content_index: 1,
            part_index: 0,
        };
        let repaired = repair_antigravity_thought_signature_rejection(&source, rejection)
            .expect("target model signature should be replaceable");
        let contents = repaired.pointer("/request/contents").unwrap();
        assert_eq!(
            contents[0]["parts"][0]["thoughtSignature"],
            "unrelated-model-signature"
        );
        assert_eq!(
            contents[0]["parts"][0]["functionCall"]["args"]["thoughtSignature"],
            "tool-data"
        );
        assert_eq!(
            contents[1]["parts"][0]["thoughtSignature"],
            "skip_thought_signature_validator"
        );
        assert_eq!(
            contents[1]["parts"][0]["thought_signature"],
            "skip_thought_signature_validator"
        );
        assert_eq!(contents[1]["parts"][0]["text"], "keep historical text");
        assert_eq!(
            contents[2]["parts"][0]["thoughtSignature"],
            "user-signature"
        );
        assert_eq!(
            contents[3]["parts"][0]["thoughtSignature"],
            "tool-signature"
        );
        assert_eq!(source, original);
        for rejection in [
            AntigravityThoughtSignatureRejection::Base64DecodeFailure {
                content_index: 2,
                part_index: 0,
            },
            AntigravityThoughtSignatureRejection::Base64DecodeFailure {
                content_index: 99,
                part_index: 0,
            },
            AntigravityThoughtSignatureRejection::Base64DecodeFailure {
                content_index: 3,
                part_index: 0,
            },
            AntigravityThoughtSignatureRejection::Base64DecodeFailure {
                content_index: 1,
                part_index: 99,
            },
        ] {
            assert!(repair_antigravity_thought_signature_rejection(&source, rejection).is_none());
            assert_eq!(source, original);
        }
        assert!(repair_antigravity_thought_signature_rejection(
            &json!({"contents": [{"role": "model", "parts": [{"text": "unsigned"}]}]}),
            AntigravityThoughtSignatureRejection::Base64DecodeFailure {
                content_index: 0,
                part_index: 0,
            },
        )
        .is_none());
    }

    /// 定点替换公开与私有正文中的历史 model 签名，同时保留用户/工具数据及原始输入。
    #[test]
    fn thought_signature_repair_is_scoped_and_immutable() {
        let contents = json!([
            {
                "role": "model",
                "parts": [
                    {
                        "thoughtSignature": "foreign-signature",
                        "thought_signature": "foreign-snake-signature",
                        "thought": "keep reasoning",
                        "functionCall": {"name": "lookup", "args": {"thoughtSignature": "tool-data"}}
                    },
                    {"text": "no signature is inserted"}
                ]
            },
            {
                "role": "user",
                "parts": [{"thoughtSignature": "user-data", "text": "keep"}]
            }
        ]);

        for source in [
            json!({"contents": contents.clone(), "metadata": {"thoughtSignature": "root-data"}}),
            json!({"request": {"contents": contents.clone()}, "thoughtSignature": "envelope-data"}),
        ] {
            let original = source.clone();
            let repaired = repair_antigravity_thought_signatures(&source)
                .expect("historical model signatures should be replaceable");
            let repaired_contents = repaired
                .pointer(if source.get("request").is_some() {
                    "/request/contents"
                } else {
                    "/contents"
                })
                .expect("contents should remain present");

            assert_eq!(
                repaired_contents[0]["parts"][0]["thoughtSignature"],
                "skip_thought_signature_validator"
            );
            assert_eq!(
                repaired_contents[0]["parts"][0]["thought_signature"],
                "skip_thought_signature_validator"
            );
            assert_eq!(
                repaired_contents[0]["parts"][0]["thought"],
                "keep reasoning"
            );
            assert_eq!(
                repaired_contents[0]["parts"][0]["functionCall"]["args"]["thoughtSignature"],
                "tool-data"
            );
            assert!(repaired_contents[0]["parts"][1]
                .get("thoughtSignature")
                .is_none());
            assert_eq!(
                repaired_contents[1]["parts"][0]["thoughtSignature"],
                "user-data"
            );
            assert_eq!(source, original);
        }

        assert!(repair_antigravity_thought_signatures(&json!({
            "contents": [{
                "role": "model",
                "parts": [{"thoughtSignature": "skip_thought_signature_validator"}]
            }]
        }))
        .is_none());
        assert!(repair_antigravity_thought_signatures(&json!({
            "contents": [{"role": "model", "parts": [{"text": "plain"}]}]
        }))
        .is_none());
    }

    /// 验证真实 Agent envelope 保留业务字段并应用最终工具 schema/搜索工具私有拼写。
    #[test]
    fn antigravity_combined_client_conversion_preserves_schemas_until_transport() {
        use aether_ai_formats::formats::shared::standard_matrix::build_standard_request_body;
        let schema = json!({"type": "object", "properties": {
            "mode": {"const": "fast"},
            "payloads": {"type": "object", "patternProperties": {"^.*$": {"type": "string"}}},
            "name": {"type": "string", "minLength": 1}
        }, "required": ["mode"]});
        let clients = [
            (
                "claude:messages",
                json!({"model": "client-model", "max_tokens": 128,
                "messages": [{"role": "user", "content": "hi"}],
                "tools": [{"name": "probe", "input_schema": schema}]}),
            ),
            (
                "openai:chat",
                json!({"model": "client-model",
                "messages": [{"role": "user", "content": "hi"}],
                "tools": [{"type": "function", "function": {"name": "probe", "parameters": schema}}]}),
            ),
            (
                "openai:responses",
                json!({"model": "client-model", "input": "hi",
                "tools": [{"type": "function", "name": "probe", "parameters": schema}]}),
            ),
            (
                "gemini:generate_content",
                json!({"model": "client-model",
                "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
                "tools": [{"functionDeclarations": [{"name": "probe", "parameters": schema}]}]}),
            ),
        ];
        for (source, original) in clients {
            for model in ["claude-sonnet-test", "gemini-test"] {
                let converted = build_standard_request_body(
                    &original,
                    source,
                    model,
                    " AnTiGrAvItY ",
                    "gemini:generate_content",
                    "",
                    true,
                    None,
                    None,
                )
                .expect("Antigravity conversion");
                assert_eq!(
                    converted["tools"][0]["functionDeclarations"][0]["parameters"], schema,
                    "{source}"
                );
                let AntigravityRequestEnvelopeSupport::Supported(envelope) =
                    build_antigravity_safe_v1internal_request(
                        &sample_auth(),
                        "review-test",
                        model,
                        &converted,
                        AntigravityEnvelopeRequestType::Agent,
                    )
                else {
                    panic!("schema should fit budget");
                };
                let parameters =
                    &envelope["request"]["tools"][0]["functionDeclarations"][0]["parameters"];
                assert_eq!(
                    parameters["properties"]["mode"],
                    json!({"type": "string", "enum": ["fast"]})
                );
                assert_eq!(
                    parameters["properties"]["payloads"]["additionalProperties"],
                    json!({"type": "string"})
                );
                assert_eq!(parameters["properties"]["name"]["minLength"], 1);
                assert_eq!(envelope["model"], model);
            }
            // The default public Gemini policy must remain unchanged.
            let public = build_standard_request_body(
                &original,
                source,
                "gemini-test",
                "gemini",
                "gemini:generate_content",
                "",
                true,
                None,
                None,
            )
            .unwrap();
            let parameters = &public["tools"][0]["functionDeclarations"][0]["parameters"];
            if source == "gemini:generate_content" {
                assert_eq!(parameters, &schema);
            } else {
                assert!(parameters["properties"]["mode"].get("const").is_none());
                assert_eq!(parameters["properties"]["name"]["minLength"], "1");
            }
        }
    }

    #[test]
    fn antigravity_responses_conversion_keeps_scoped_previous_response_history() {
        use aether_ai_formats::{
            api::record_converted_response_history,
            formats::shared::standard_matrix::build_standard_request_body,
        };
        let schema = json!({"type": "object", "properties": {"mode": {"const": "fast"}}});
        let response_id = "resp_antigravity_schema_history_test";
        let scope = "antigravity-schema-history";
        record_converted_response_history(&json!({
            "needs_conversion": true, "client_api_format": "openai:responses",
            "provider_api_format": "openai:chat", "api_key_id": scope,
            "original_request_body": {"model": "client", "input": "first"}
        }), &json!({"id": response_id, "status": "completed", "output": [{
            "type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "remembered"}]
        }]})).expect("seed scoped history");
        let input = json!({"model": "client", "previous_response_id": response_id,
            "input": "second", "tools": [{"type": "function", "name": "probe", "parameters": schema}]});
        let output = build_standard_request_body(
            &input,
            "openai:responses",
            "claude-test",
            "antigravity",
            "gemini:generate_content",
            "",
            true,
            None,
            Some(scope),
        )
        .expect("expand history");
        assert_eq!(output["contents"][0]["parts"][0]["text"], "first");
        assert_eq!(output["contents"][1]["parts"][0]["text"], "remembered");
        assert_eq!(output["contents"][2]["parts"][0]["text"], "second");
        assert_eq!(
            output["tools"][0]["functionDeclarations"][0]["parameters"],
            schema
        );
        assert!(build_standard_request_body(
            &input,
            "openai:responses",
            "claude-test",
            "antigravity",
            "gemini:generate_content",
            "",
            true,
            None,
            Some("different-key")
        )
        .is_none());
    }

    #[test]
    fn antigravity_rejects_shared_schema_budget_exhaustion_on_both_envelope_paths() {
        use super::AntigravityRequestEnvelopeUnsupportedReason;
        let declaration = json!({"name": "probe", "parameters": {
            "type": "object", "description": "x".repeat(600_000)
        }});
        let mut body = json!({"contents": [], "tools": [{"functionDeclarations": [declaration]}]});
        assert!(matches!(
            build_antigravity_safe_v1internal_request(
                &sample_auth(),
                "test",
                "claude-test",
                &body,
                AntigravityEnvelopeRequestType::Agent,
            ),
            AntigravityRequestEnvelopeSupport::Supported(_)
        ));
        body["tools"][0]["functionDeclarations"]
            .as_array_mut()
            .unwrap()
            .push(declaration);
        for wrapped in [false, true] {
            let input = if wrapped {
                json!({"request": body})
            } else {
                body.clone()
            };
            let snapshot = input.clone();
            assert_eq!(
                build_antigravity_safe_v1internal_request(
                    &sample_auth(),
                    "test",
                    "claude-test",
                    &input,
                    AntigravityEnvelopeRequestType::Agent,
                ),
                AntigravityRequestEnvelopeSupport::Unsupported(
                    AntigravityRequestEnvelopeUnsupportedReason::ToolSchemaBudgetExceeded
                )
            );
            assert_eq!(input, snapshot);
        }
    }

    #[test]
    fn search_only_request_keeps_the_modern_google_search_spelling() {
        // Reproduces the live failure: a grounding-only request (no function
        // declarations) that went out as `googleSearchRetrieval` came back as
        // `Malformed function call: call:google_search{query:current UTC date}`
        // from daily-cloudcode-pa.googleapis.com.
        let request_body = json!({
            "contents": [
                { "role": "user", "parts": [{ "text": "today's UTC date?" }] }
            ],
            "tools": [{ "googleSearch": {} }]
        });

        let envelope = match build_antigravity_safe_v1internal_request(
            &sample_auth(),
            "request-ant-search-1",
            "gemini-3.8-flash-high",
            &request_body,
            AntigravityEnvelopeRequestType::Agent,
        ) {
            AntigravityRequestEnvelopeSupport::Supported(envelope) => envelope,
            AntigravityRequestEnvelopeSupport::Unsupported(reason) => {
                panic!("search-only envelope should be supported: {reason:?}")
            }
        };

        let tools = envelope["request"]["tools"]
            .as_array()
            .expect("tools should survive");
        assert_eq!(tools.len(), 1, "{tools:?}");
        assert_eq!(tools[0]["googleSearch"], json!({}));
        assert!(
            tools[0].get("googleSearchRetrieval").is_none(),
            "the Gemini 1.5 spelling must not be reintroduced: {tools:?}"
        );
    }

    #[test]
    fn real_agent_request_preserves_antigravity_agent_fields() {
        let request_body = json!({
            "model": "client-side-model-should-not-be-nested",
            "contents": [
                {
                    "role": "user",
                    "parts": [
                        { "text": "Reply with OK only." }
                    ]
                }
            ],
            "systemInstruction": {
                "role": "user",
                "parts": [
                    { "text": "Antigravity agent system prompt" }
                ]
            },
            "generationConfig": {
                "maxOutputTokens": 8192,
                "thinkingConfig": {
                    "includeThoughts": true,
                    "thinkingBudget": 4000
                }
            },
            "toolConfig": {
                "includeServerSideToolInvocations": true,
                "functionCallingConfig": {
                    "mode": "VALIDATED"
                }
            },
            "tools": [
                {
                    "googleSearch": {}
                },
                {
                    "functionDeclarations": [
                        {
                            "name": "run_command",
                            "description": "Run a command",
                            "parameters": {
                                "type": "object",
                                "properties": {
                                    "cmd": { "type": "string" }
                                },
                                "required": ["cmd"]
                            }
                        }
                    ]
                }
            ],
            "labels": {
                "trajectory_id": "trajectory-123",
                "used_claude": "false"
            },
            "sessionId": "session-ant-123",
            "safetySettings": [
                { "category": "HARM_CATEGORY_UNSPECIFIED" }
            ]
        });

        assert_eq!(
            classify_antigravity_safe_request_body(&request_body),
            Ok(())
        );

        let envelope = match build_antigravity_safe_v1internal_request(
            &sample_auth(),
            "request-ant-agent-123",
            "gemini-3.5-flash-low",
            &request_body,
            AntigravityEnvelopeRequestType::Agent,
        ) {
            AntigravityRequestEnvelopeSupport::Supported(envelope) => envelope,
            AntigravityRequestEnvelopeSupport::Unsupported(reason) => {
                panic!("real agent envelope should be supported: {reason:?}")
            }
        };

        assert_eq!(envelope["project"], "project-ant-123");
        assert_eq!(envelope["requestId"], "request-ant-agent-123");
        assert_eq!(envelope["model"], "gemini-3.5-flash-low");
        assert_eq!(envelope["userAgent"], ANTIGRAVITY_REQUEST_USER_AGENT);
        assert!(envelope.get("requestType").is_none());
        assert!(envelope["request"].get("model").is_none());
        assert!(envelope["request"].get("safetySettings").is_none());
        assert_eq!(
            envelope["request"]["systemInstruction"]["parts"][0]["text"],
            "Antigravity agent system prompt"
        );
        assert_eq!(
            envelope["request"]["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            4000
        );
        assert_eq!(
            envelope["request"]["toolConfig"]["functionCallingConfig"]["mode"],
            "VALIDATED"
        );
        assert_eq!(
            envelope["request"]["toolConfig"]["includeServerSideToolInvocations"],
            true
        );
        assert!(envelope["request"]["toolConfig"]
            .get("include_server_side_tool_invocations")
            .is_none());
        assert!(envelope["request"]["tools"][0]
            .get("googleSearchRetrieval")
            .is_none());
        assert_eq!(envelope["request"]["tools"][0]["googleSearch"], json!({}));
        assert_eq!(
            envelope["request"]["tools"][1]["functionDeclarations"][0]["name"],
            "run_command"
        );
        assert_eq!(
            envelope["request"]["tools"][1]["functionDeclarations"][0]["parameters"]["properties"]
                ["cmd"]["type"],
            "string"
        );
        assert!(envelope["request"]["tools"][1]["functionDeclarations"][0]
            .get("parametersJsonSchema")
            .is_none());
        assert_eq!(
            envelope["request"]["labels"]["trajectory_id"],
            "trajectory-123"
        );
        assert_eq!(envelope["request"]["sessionId"], "session-ant-123");
    }

    #[test]
    fn checkpoint_request_type_builds_checkpoint_envelope() {
        let request_body = json!({
            "contents": [
                {
                    "role": "user",
                    "parts": [
                        { "text": "checkpoint context" }
                    ]
                }
            ],
            "generationConfig": {
                "maxOutputTokens": 8192,
                "thinkingConfig": {
                    "includeThoughts": true,
                    "thinkingBudget": 4000
                }
            },
            "toolConfig": {
                "functionCallingConfig": {
                    "mode": "NONE"
                }
            }
        });

        let envelope = match build_antigravity_safe_v1internal_request(
            &sample_auth(),
            "request-ant-checkpoint-123",
            "gemini-3.5-flash-low",
            &request_body,
            AntigravityEnvelopeRequestType::Checkpoint,
        ) {
            AntigravityRequestEnvelopeSupport::Supported(envelope) => envelope,
            AntigravityRequestEnvelopeSupport::Unsupported(reason) => {
                panic!("checkpoint envelope should be supported: {reason:?}")
            }
        };

        assert_eq!(envelope["requestType"], "checkpoint");
        assert_eq!(
            envelope["request"]["toolConfig"]["functionCallingConfig"]["mode"],
            "NONE"
        );
    }

    /// 验证已封装请求不会二次包裹，且内部蛇形搜索工具仍在私有边界归一。
    #[test]
    fn existing_v1internal_envelope_is_not_double_wrapped() {
        let request_body = json!({
            "project": "client-side-project",
            "requestId": "client-request-id-123",
            "model": "gemini-3.5-flash-low",
            "userAgent": "antigravity",
            "requestType": "checkpoint",
            "request": {
                "contents": [
                    {
                        "role": "user",
                        "parts": [
                            { "text": "checkpoint context" }
                        ]
                    }
                ],
                "generationConfig": {
                    "thinkingConfig": {
                        "includeThoughts": true
                    }
                },
                "toolConfig": {
                    "functionCallingConfig": {
                        "mode": "NONE"
                    }
                },
                "tools": [{
                    "google_search": {
                        "dynamicRetrievalConfig": {
                            "mode": "MODE_UNSPECIFIED"
                        }
                    }
                }]
            }
        });

        assert_eq!(
            classify_antigravity_safe_request_body(&request_body),
            Ok(())
        );

        let envelope = match build_antigravity_safe_v1internal_request(
            &sample_auth(),
            "trace-request-id-should-not-overwrite-client-id",
            "mapped-antigravity-model",
            &request_body,
            AntigravityEnvelopeRequestType::Agent,
        ) {
            AntigravityRequestEnvelopeSupport::Supported(envelope) => envelope,
            AntigravityRequestEnvelopeSupport::Unsupported(reason) => {
                panic!("existing v1internal envelope should be supported: {reason:?}")
            }
        };

        assert_eq!(envelope["project"], "project-ant-123");
        assert_eq!(envelope["requestId"], "client-request-id-123");
        assert_eq!(envelope["model"], "mapped-antigravity-model");
        assert_eq!(envelope["userAgent"], "antigravity");
        assert_eq!(envelope["requestType"], "checkpoint");
        assert!(envelope["request"].get("request").is_none());
        assert_eq!(
            envelope["request"]["contents"][0]["parts"][0]["text"],
            "checkpoint context"
        );
        assert_eq!(
            envelope["request"]["toolConfig"]["functionCallingConfig"]["mode"],
            "NONE"
        );
        assert!(envelope["request"]["tools"][0]
            .get("google_search")
            .is_none());
        assert_eq!(
            envelope["request"]["tools"][0]["googleSearch"],
            json!({
                "dynamicRetrievalConfig": {
                    "mode": "MODE_UNSPECIFIED"
                }
            })
        );
    }

    /// 用十二个合成工具复现线上元声明错误，覆盖公开 Gemini、两种声明拼写及已有 envelope。
    #[test]
    fn antigravity_envelope_removes_schema_metadata_from_twelve_tools() {
        for declarations_key in ["functionDeclarations", "function_declarations"] {
            let mut declarations = Vec::new();
            let mut expected_declarations = Vec::new();
            for index in 0..12 {
                let parameters = json!({
                    "type": "object",
                    "properties": {"query": {"type": "string", "minLength": 1}},
                    "required": ["query"]
                });
                let mut declaration = json!({"name": format!("lookup_{index}")});
                declaration["parameters"] = parameters.clone();
                expected_declarations.push(declaration.clone());
                declaration.as_object_mut().unwrap().remove("parameters");
                let field = [
                    "parameters",
                    "parametersJsonSchema",
                    "parameters_json_schema",
                ][index % 3];
                declaration[field] = parameters;
                declaration[field]["$schema"] =
                    json!("https://json-schema.org/draft/2020-12/schema");
                declarations.push(declaration);
            }
            let request_body = json!({
                "contents": [{"role": "user", "parts": [{"text": "synthetic schema regression"}]}],
                "tools": [{(declarations_key): declarations}]
            });
            let original = request_body.clone();
            let gemini_body =
                build_same_format_provider_request_body(SameFormatProviderRequestBodyInput {
                    body_json: &request_body,
                    mapped_model: "gemini-upstream",
                    client_api_format: "gemini:generate_content",
                    provider_api_format: "gemini:generate_content",
                    source_model: None,
                    family: SameFormatProviderFamily::Gemini,
                    body_rules: None,
                    request_headers: None,
                    upstream_is_stream: true,
                    force_body_stream_field: false,
                    kiro_auth_config: None,
                    is_claude_code: false,
                    enable_model_directives: false,
                })
                .expect("ordinary Gemini body should build");
            assert_eq!(gemini_body, original);

            for source in [gemini_body.clone(), json!({"request": gemini_body})] {
                let original_source = source.clone();
                let AntigravityRequestEnvelopeSupport::Supported(envelope) =
                    build_antigravity_safe_v1internal_request(
                        &sample_auth(),
                        "synthetic-schema-regression",
                        "gemini-upstream",
                        &source,
                        AntigravityEnvelopeRequestType::Agent,
                    )
                else {
                    panic!("schema envelope should be supported");
                };
                assert_eq!(
                    envelope["request"]["tools"][0][declarations_key],
                    json!(expected_declarations)
                );
                assert_eq!(envelope["request"]["contents"], original["contents"]);
                assert!(envelope["request"].get("request").is_none());
                assert_eq!(source, original_source);
            }
            assert_eq!(request_body, original);
        }
    }

    #[test]
    fn antigravity_envelope_downgrades_fabric_schema_on_all_input_paths() {
        let schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "code": { "type": "string", "description": "TypeScript function body" },
                "payloads": {
                    "type": "object",
                    "patternProperties": { "^.*$": { "type": "string" } }
                },
                "resultFormat": {
                    "anyOf": [
                        { "type": "string", "const": "auto" },
                        { "type": "string", "const": "yaml" },
                        { "type": "string", "const": "json" },
                        { "type": "string", "const": "text" }
                    ]
                },
                "display": {
                    "anyOf": [
                        { "type": "object", "properties": { "name": { "type": "string" } } },
                        { "type": "string" }
                    ]
                },
                "tokenBudget": { "type": "number", "minimum": 1 }
            },
            "required": ["code"],
            "minProperties": "1"
        });
        let mut expected = schema.clone();
        expected.as_object_mut().unwrap().remove("$schema");
        expected["minProperties"] = json!(1);
        expected["properties"]["payloads"] = json!({
            "type": "object", "additionalProperties": { "type": "string" }
        });
        for (index, format) in ["auto", "yaml", "json", "text"].iter().enumerate() {
            expected["properties"]["resultFormat"]["anyOf"][index] =
                json!({ "type": "string", "enum": [format] });
        }

        for declarations_key in ["functionDeclarations", "function_declarations"] {
            for parameters_key in [
                "parametersJsonSchema",
                "parameters_json_schema",
                "parameters",
            ] {
                for wrapped in [false, true] {
                    let mut body = json!({
                        "contents": [{ "role": "user", "parts": [{ "text": "hello" }] }],
                        "tools": [{ "googleSearch": {} }, {}],
                        "generationConfig": { "maxOutputTokens": 4096 },
                        "labels": { "const": "not a schema" }
                    });
                    body["tools"][1][declarations_key] = json!([
                        { "name": "fabric_exec", "description": "Execute TypeScript" },
                        { "name": "other", "parameters": { "type": "object" } }
                    ]);
                    body["tools"][1][declarations_key][0][parameters_key] = schema.clone();
                    if wrapped {
                        body = json!({ "request": body, "requestId": "client-id" });
                    }
                    let original = body.clone();
                    let AntigravityRequestEnvelopeSupport::Supported(envelope) =
                        build_antigravity_safe_v1internal_request(
                            &sample_auth(),
                            "trace-id",
                            "gemini-test",
                            &body,
                            AntigravityEnvelopeRequestType::Agent,
                        )
                    else {
                        panic!("Fabric request should be supported");
                    };
                    let declaration = &envelope["request"]["tools"][1][declarations_key][0];
                    assert_eq!(
                        declaration["parameters"], expected,
                        "{declarations_key}/{parameters_key}/wrapped={wrapped}"
                    );
                    assert_eq!(declaration["name"], "fabric_exec");
                    assert!(declaration.get("parametersJsonSchema").is_none());
                    assert!(declaration.get("parameters_json_schema").is_none());
                    assert_eq!(
                        envelope["request"]["generationConfig"]["maxOutputTokens"],
                        4096
                    );
                    assert_eq!(envelope["request"]["labels"]["const"], "not a schema");
                    assert_eq!(
                        envelope["request"]["tools"][0],
                        json!({ "googleSearch": {} })
                    );
                    assert_eq!(
                        envelope["requestId"],
                        if wrapped { "client-id" } else { "trace-id" }
                    );
                    assert_eq!(envelope["project"], "project-ant-123");
                    assert_eq!(body, original, "caller-owned input must remain unchanged");

                    let AntigravityRequestEnvelopeSupport::Supported(rebuilt) =
                        build_antigravity_safe_v1internal_request(
                            &sample_auth(),
                            "trace-id",
                            "gemini-test",
                            &envelope,
                            AntigravityEnvelopeRequestType::Agent,
                        )
                    else {
                        panic!("converted envelope should be supported");
                    };
                    assert_eq!(rebuilt, envelope, "normalization must be idempotent");
                }
            }
        }
    }

    #[test]
    fn antigravity_schema_aliases_do_not_override_existing_parameters() {
        let body = json!({
            "contents": [],
            "tools": [{ "functionDeclarations": [{
                "name": "select",
                "parameters": { "type": "string", "const": "existing" },
                "parametersJsonSchema": { "type": "string", "const": "camel" },
                "parameters_json_schema": { "type": "string", "const": "snake" }
            }, {
                "name": "select_alias",
                "parametersJsonSchema": { "type": "string", "const": "camel" },
                "parameters_json_schema": { "type": "string", "const": "snake" }
            }] }]
        });
        let AntigravityRequestEnvelopeSupport::Supported(envelope) =
            build_antigravity_safe_v1internal_request(
                &sample_auth(),
                "trace-id",
                "gemini-test",
                &body,
                AntigravityEnvelopeRequestType::Agent,
            )
        else {
            panic!("request should be supported");
        };
        let declarations = &envelope["request"]["tools"][0]["functionDeclarations"];
        assert_eq!(
            declarations[0]["parameters"],
            json!({ "type": "string", "enum": ["existing"] })
        );
        assert_eq!(
            declarations[1]["parameters"],
            json!({ "type": "string", "enum": ["camel"] })
        );
        for declaration in declarations.as_array().unwrap() {
            assert!(declaration.get("parametersJsonSchema").is_none());
            assert!(declaration.get("parameters_json_schema").is_none());
        }
    }

    #[test]
    fn antigravity_envelope_normalizes_json_schema_parameter_spellings() {
        let request_body = json!({
            "contents": [{
                "role": "user",
                "parts": [{ "text": "hello" }]
            }],
            "tools": [{
                "function_declarations": [{
                    "name": "lookup",
                    "parametersJsonSchema": { "type": "object" }
                }, {
                    "name": "weather",
                    "parameters_json_schema": { "type": "object" }
                }, {
                    "name": "existing",
                    "parameters": { "$schema": "dialect", "type": "object", "required": ["keep"] },
                    "parametersJsonSchema": { "type": "string" },
                    "parameters_json_schema": { "type": "number" }
                }, {
                    "name": "camel_precedence",
                    "parametersJsonSchema": { "$schema": "dialect", "type": "boolean" },
                    "parameters_json_schema": { "type": "number" }
                }, {
                    "name": "no_parameters"
                }]
            }]
        });

        let envelope = match build_antigravity_safe_v1internal_request(
            &sample_auth(),
            "request-ant-schema-123",
            "gemini-3.5-flash-low",
            &request_body,
            AntigravityEnvelopeRequestType::Agent,
        ) {
            AntigravityRequestEnvelopeSupport::Supported(envelope) => envelope,
            AntigravityRequestEnvelopeSupport::Unsupported(reason) => {
                panic!("schema envelope should be supported: {reason:?}")
            }
        };

        let declarations = &envelope["request"]["tools"][0]["function_declarations"];
        assert_eq!(declarations[0]["parameters"]["type"], "object");
        assert_eq!(declarations[1]["parameters"]["type"], "object");
        assert!(declarations[0].get("parametersJsonSchema").is_none());
        assert!(declarations[1].get("parameters_json_schema").is_none());
        assert_eq!(
            declarations[2]["parameters"],
            json!({"type": "object", "required": ["keep"]})
        );
        assert_eq!(declarations[3]["parameters"], json!({"type": "boolean"}));
        assert!(declarations[4].get("parameters").is_none());
        for declaration in declarations.as_array().unwrap() {
            assert!(declaration.get("parametersJsonSchema").is_none());
            assert!(declaration.get("parameters_json_schema").is_none());
        }
    }
}
