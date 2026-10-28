use serde_json::{json, Map, Value};

use crate::{
    formats::{
        context::FormatContext,
        openai::{
            namespace::{LoweredToolChoiceName, NamespaceToolAliases},
            shared::{
                map_openai_reasoning_effort_to_claude_output,
                map_openai_reasoning_effort_to_thinking_budget,
            },
            tool_lowering::{
                custom_tool_function_arguments, tool_search_provider_arguments, ToolLoweringError,
                ToolLoweringTarget,
            },
        },
        shared::model_directives::claude_model_uses_adaptive_effort,
    },
    protocol::canonical::{
        canonical_extension_object_mut, canonical_instructions_to_claude_system,
        canonical_messages_to_claude, canonical_openai_reasoning_effort,
        canonical_tool_choice_to_claude, canonical_tool_to_claude, claude_extensions,
        claude_generation_config, claude_messages_to_canonical, claude_parallel_tool_calls,
        claude_response_format_to_canonical, claude_system_to_canonical_instructions,
        claude_thinking_to_canonical, claude_tool_choice_to_canonical, claude_tools_to_canonical,
        clear_openai_custom_tool_call_marker, compact_canonical_claude_messages, insert_f64,
        mark_claude_messages_request_source, namespace_extension_object,
        openai_responses_extension, CanonicalContentBlock, CanonicalMessage, CanonicalRequest,
        CanonicalToolChoice,
    },
};

pub fn from(body: &Value, _ctx: &FormatContext) -> Option<CanonicalRequest> {
    from_raw(body)
}

pub fn to(request: &CanonicalRequest, ctx: &FormatContext) -> Option<Value> {
    to_raw(
        request,
        ctx.mapped_model_or(request.model.as_str()),
        ctx.upstream_is_stream,
    )
}

pub fn from_raw(body_json: &Value) -> Option<CanonicalRequest> {
    let request = body_json.as_object()?;
    let mut canonical = CanonicalRequest {
        model: request
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        ..CanonicalRequest::default()
    };

    canonical.instructions = claude_system_to_canonical_instructions(request.get("system"))?;
    let system_text = canonical
        .instructions
        .iter()
        .map(|instruction| instruction.text.as_str())
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if !system_text.is_empty() {
        canonical.system = Some(system_text);
    }
    canonical.messages = claude_messages_to_canonical(request.get("messages"))?;
    canonical.generation = claude_generation_config(request);
    let (tools, builtin_tools, web_search_options) =
        claude_tools_to_canonical(request.get("tools"))?;
    canonical.tools = tools;
    canonical.tool_choice = claude_tool_choice_to_canonical(request.get("tool_choice"));
    canonical.parallel_tool_calls = claude_parallel_tool_calls(request.get("tool_choice"));
    canonical.metadata = request.get("metadata").cloned();
    canonical.thinking = claude_thinking_to_canonical(request);
    canonical.response_format = claude_response_format_to_canonical(request);

    canonical.extensions = claude_extensions(
        request,
        &[
            "model",
            "system",
            "messages",
            "max_tokens",
            "temperature",
            "top_p",
            "top_k",
            "stop",
            "stop_sequences",
            "stream",
            "tools",
            "tool_choice",
            "metadata",
            "thinking",
            "output_config",
        ],
    );
    mark_claude_messages_request_source(&mut canonical.extensions);
    if !builtin_tools.is_empty() {
        canonical_extension_object_mut(&mut canonical.extensions, "claude")
            .insert("builtin_tools".to_string(), Value::Array(builtin_tools));
    }
    if let Some(web_search_options) = web_search_options {
        canonical_extension_object_mut(&mut canonical.extensions, "openai")
            .insert("web_search_options".to_string(), web_search_options);
    }
    if let Some(output_config) = request.get("output_config").cloned() {
        canonical_extension_object_mut(&mut canonical.extensions, "claude")
            .insert("output_config".to_string(), output_config);
    }
    Some(canonical)
}

pub fn to_raw(
    canonical: &CanonicalRequest,
    mapped_model: &str,
    upstream_is_stream: bool,
) -> Option<Value> {
    let lowering =
        if crate::formats::openai::chat::request::canonical_request_has_responses_tool_surface(
            canonical,
        ) {
            NamespaceToolAliases::from_canonical_request(
                canonical,
                ToolLoweringTarget::ClaudeMessages,
            )
            .ok()?
        } else {
            NamespaceToolAliases::from_canonical_tools(&canonical.tools)
        };
    if let Some(CanonicalToolChoice::Tool { name }) = canonical.tool_choice.as_ref() {
        // deferred 但尚未被搜索发现的工具不能出现在 tool_choice 中。
        if lowering.name_is_declared_but_unoffered(name) {
            return None;
        }
    }
    let mut rewritten = canonical.clone();
    for message in &mut rewritten.messages {
        rewrite_tool_uses_for_claude(message, &lowering).ok()?;
    }
    let mut output = Map::new();
    output.insert("model".to_string(), Value::String(mapped_model.to_string()));
    output.insert(
        "messages".to_string(),
        Value::Array(compact_canonical_claude_messages(
            canonical_messages_to_claude(&rewritten)?,
        )),
    );
    output.insert(
        "max_tokens".to_string(),
        Value::from(canonical.generation.max_tokens.unwrap_or(8192)),
    );
    if let Some(system) = canonical_instructions_to_claude_system(&canonical.instructions) {
        output.insert("system".to_string(), system);
    } else if let Some(system) = canonical
        .system
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        output.insert("system".to_string(), Value::String(system.clone()));
    }
    if upstream_is_stream {
        output.insert("stream".to_string(), Value::Bool(true));
    }
    insert_f64(&mut output, "temperature", canonical.generation.temperature);
    insert_f64(&mut output, "top_p", canonical.generation.top_p);
    if let Some(top_k) = canonical.generation.top_k {
        output.insert("top_k".to_string(), Value::from(top_k));
    }
    if let Some(stop_sequences) = &canonical.generation.stop_sequences {
        output.insert(
            "stop_sequences".to_string(),
            Value::Array(stop_sequences.iter().cloned().map(Value::String).collect()),
        );
    }
    let mut tools = Vec::new();
    for (tool_index, tool) in canonical.tools.iter().enumerate() {
        if let Some(lowered) =
            lowering.lowered_tools_for_source(ToolLoweringTarget::ClaudeMessages, tool_index)
        {
            tools.extend(lowered);
        } else {
            tools.push(canonical_tool_to_claude(tool));
        }
    }
    tools.extend(lowering.discovered_tools(ToolLoweringTarget::ClaudeMessages));
    if let Some(builtin_tools) = canonical
        .extensions
        .get("claude")
        .and_then(Value::as_object)
        .and_then(|value| value.get("builtin_tools"))
        .and_then(Value::as_array)
    {
        tools.extend(builtin_tools.iter().cloned());
    }
    if !tools.is_empty() {
        output.insert("tools".to_string(), Value::Array(tools));
    }
    let tool_choice = claude_tool_choice_with_lowering(canonical, &lowering).ok()?;
    if let Some(tool_choice) = tool_choice {
        output.insert("tool_choice".to_string(), tool_choice);
    }
    if let Some(metadata) = canonical.metadata.clone() {
        output.insert("metadata".to_string(), metadata);
    }
    if let Some(thinking) = canonical.thinking.as_ref() {
        let openai_effort = canonical_openai_reasoning_effort(thinking);
        let budget_tokens = thinking
            .budget_tokens
            .or_else(|| openai_effort.and_then(map_openai_reasoning_effort_to_thinking_budget));
        let uses_adaptive = claude_model_uses_adaptive_effort(mapped_model)
            || claude_model_uses_adaptive_effort(canonical.model.as_str());
        if thinking.enabled || budget_tokens.is_some() {
            let original_claude_thinking =
                thinking.extensions.get("claude").and_then(Value::as_object);
            let source_is_adaptive = original_claude_thinking
                .and_then(|value| value.get("type"))
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("adaptive"));
            let thinking_config = if uses_adaptive || source_is_adaptive {
                let mut config = Map::new();
                config.insert("type".to_string(), Value::String("adaptive".to_string()));
                if let Some(display) = original_claude_thinking
                    .and_then(|value| value.get("display"))
                    .cloned()
                {
                    config.insert("display".to_string(), display);
                }
                Value::Object(config)
            } else {
                json!({
                    "type": "enabled",
                    "budget_tokens": budget_tokens.unwrap_or(1024),
                })
            };
            output.insert("thinking".to_string(), thinking_config);
        }
        if let Some(output_effort) =
            openai_effort.and_then(map_openai_reasoning_effort_to_claude_output)
        {
            output.insert(
                "output_config".to_string(),
                json!({
                    "effort": output_effort,
                }),
            );
        }
    }
    output.extend(namespace_extension_object(
        &canonical.extensions,
        "claude",
        &output,
    ));
    if let Some(response_format) = canonical.response_format.as_ref() {
        let claude_format = response_format
            .extensions
            .get("claude")
            .and_then(|value| value.get("output_config_format"))
            .cloned()
            .or_else(|| {
                let json_schema = response_format.json_schema.as_ref()?.as_object()?;
                let schema = json_schema.get("schema")?.clone();
                let mut format = Map::new();
                format.insert("type".to_string(), Value::String("json_schema".to_string()));
                format.insert("schema".to_string(), schema);
                if let Some(name) = json_schema.get("name").cloned() {
                    format.insert("name".to_string(), name);
                }
                Some(Value::Object(format))
            });
        if let Some(claude_format) = claude_format {
            output
                .entry("output_config".to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Some(output_config) = output
                .get_mut("output_config")
                .and_then(Value::as_object_mut)
            {
                output_config.insert("format".to_string(), claude_format);
            }
        }
    }
    Some(Value::Object(output))
}

/// 读取 Responses 工具调用上的 namespace；`functions` 默认空间由 lowering 归一。
fn claude_tool_namespace(
    extensions: &std::collections::BTreeMap<String, Value>,
) -> Result<Option<String>, ToolLoweringError> {
    let Some(responses) = openai_responses_extension(extensions).and_then(Value::as_object) else {
        return Ok(None);
    };
    let Some(namespace) = responses.get("namespace") else {
        return Ok(None);
    };
    namespace
        .as_str()
        .map(str::trim)
        .filter(|namespace| !namespace.is_empty())
        .map(|namespace| Ok(Some(namespace.to_string())))
        .unwrap_or_else(|| {
            Err(ToolLoweringError::new(
                "messages[].content[].openai_responses.namespace",
                "Responses tool call namespace is invalid",
            ))
        })
}

fn claude_tool_item_type(extensions: &std::collections::BTreeMap<String, Value>) -> Option<&str> {
    openai_responses_extension(extensions)
        .and_then(|value| value.get("item_type"))
        .and_then(Value::as_str)
}

/// 把 Responses 历史工具调用/结果改写为 Claude 可执行形态。
fn rewrite_tool_uses_for_claude(
    message: &mut CanonicalMessage,
    aliases: &NamespaceToolAliases,
) -> Result<(), ToolLoweringError> {
    for block in &mut message.content {
        match block {
            CanonicalContentBlock::ToolUse {
                name,
                input,
                extensions,
                ..
            } => {
                let namespace =
                    claude_tool_namespace(extensions)?.filter(|namespace| namespace != "functions");
                let provider_name = aliases.tool_use_provider_name(namespace.as_deref(), name)?;
                let custom_lowered =
                    aliases.custom_tool_is_function_lowered(namespace.as_deref(), name);
                let is_tool_search = claude_tool_item_type(extensions) == Some("tool_search_call");
                *name = provider_name;
                if custom_lowered {
                    *input = custom_tool_function_arguments(input);
                    clear_openai_custom_tool_call_marker(extensions);
                } else if is_tool_search {
                    *input = tool_search_provider_arguments(input);
                }
            }
            CanonicalContentBlock::ToolResult {
                output,
                content_text,
                extensions,
                ..
            } if claude_tool_item_type(extensions) == Some("tool_search_output") => {
                if let Some(tools) = output.as_ref() {
                    if let Some(text) = aliases.tool_search_output_text(tools) {
                        *output = Some(Value::String(text.clone()));
                        *content_text = Some(text);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Claude tool_choice 使用 provider 别名；namespace 子工具在请求中可能只给出 child 名。
fn claude_tool_choice_with_lowering(
    canonical: &CanonicalRequest,
    aliases: &NamespaceToolAliases,
) -> Result<Option<Value>, ()> {
    let Some(mut choice) = canonical_tool_choice_to_claude(
        canonical.tool_choice.as_ref(),
        canonical.parallel_tool_calls,
    ) else {
        return Ok(None);
    };
    if let Some(CanonicalToolChoice::Tool { name }) = canonical.tool_choice.as_ref() {
        match aliases.resolve_tool_choice_name(name) {
            LoweredToolChoiceName::Unchanged => {}
            LoweredToolChoiceName::Alias(alias) => {
                if let Some(object) = choice.as_object_mut() {
                    object.insert("name".to_string(), Value::String(alias));
                }
            }
            // 多个 namespace/顶层同名函数时不能任选一个,必须 fail closed。
            LoweredToolChoiceName::Ambiguous => return Err(()),
        }
    }
    Ok(Some(choice))
}
