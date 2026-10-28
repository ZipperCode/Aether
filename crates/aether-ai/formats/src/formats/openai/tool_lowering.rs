//! Responses client-executed tool search lowering: shared vocabulary for the
//! request-side tool inventory and the response-side identity restoration.
//!
//! The request-local inventory itself lives in [`super::namespace`]; this
//! module owns the target protocol, the fail-closed error and the wire-level
//! argument shapes shared by Chat/Claude request lowering and the Responses
//! emitters.

use serde_json::{json, Value};

use crate::formats::id::FormatId;

/// Provider protocol that a Responses tool inventory is lowered into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolLoweringTarget {
    OpenAiChat,
    ClaudeMessages,
}

impl ToolLoweringTarget {
    pub(crate) fn for_format(format: FormatId) -> Option<Self> {
        match format {
            FormatId::OpenAiChat => Some(Self::OpenAiChat),
            FormatId::ClaudeMessages => Some(Self::ClaudeMessages),
            _ => None,
        }
    }

    /// Report contexts name the provider that produced the response; only that
    /// target decides how a custom text tool was represented on the wire.
    pub(crate) fn from_report_context(report_context: &Value) -> Option<Self> {
        report_context
            .get("provider_api_format")
            .and_then(Value::as_str)
            .and_then(FormatId::parse)
            .and_then(Self::for_format)
    }
}

/// Provider function name a client-executed Responses `tool_search` tool is
/// offered under. The name is reserved for the search tool, never aliased.
pub(crate) const TOOL_SEARCH_FUNCTION_NAME: &str = "tool_search";

/// Codex names the namespace of plain top-level functions `functions`; it is
/// equivalent to no namespace and is normalized away so history calls pair.
pub(crate) const RESPONSES_DEFAULT_TOOL_NAMESPACE: &str = "functions";

/// Description suffix telling the model where a custom tool's free-form input
/// belongs once the tool is lowered to a JSON function.
pub(crate) const CUSTOM_TOOL_INPUT_INSTRUCTION: &str = "Pass the tool's complete \
free-form input as the `input` string, exactly as it should be sent to the tool.";

/// Fail-closed reason for a Responses tool inventory that cannot be lowered
/// into the target protocol without losing semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolLoweringError {
    location: String,
    reason: String,
}

impl ToolLoweringError {
    pub(crate) fn new(location: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            location: location.into(),
            reason: reason.into(),
        }
    }

    /// JSON path of the offending field, e.g. `tools[2].tools[0].format`.
    pub(crate) fn location(&self) -> &str {
        &self.location
    }

    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }
}

impl std::fmt::Display for ToolLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.location, self.reason)
    }
}

/// What a provider-emitted tool name means for the original Responses client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResponsesToolRestore {
    /// The provider name is the Responses name; keep the existing behaviour.
    Unchanged,
    /// Responses namespace child: emit `function_call` with `namespace`.
    NamespaceChild { namespace: String, name: String },
    /// Custom text tool lowered to a provider function: emit `custom_tool_call`.
    CustomText {
        namespace: Option<String>,
        name: String,
    },
    /// Client-executed `tool_search`: emit `tool_search_call`.
    ClientToolSearch,
}

impl ResponsesToolRestore {
    /// Responses item name for a provider-emitted name.
    pub(crate) fn responses_name<'a>(&'a self, provider_name: &'a str) -> &'a str {
        match self {
            Self::Unchanged | Self::ClientToolSearch => provider_name,
            Self::NamespaceChild { name, .. } | Self::CustomText { name, .. } => name,
        }
    }

    /// Responses `namespace` field for the restored item, when it has one.
    pub(crate) fn namespace(&self) -> Option<&str> {
        match self {
            Self::NamespaceChild { namespace, .. } => Some(namespace),
            Self::CustomText { namespace, .. } => namespace.as_deref(),
            Self::Unchanged | Self::ClientToolSearch => None,
        }
    }

    /// Original custom text tool identity for a function-lowered call.
    pub(crate) fn custom_text_identity(&self) -> Option<(Option<&str>, &str)> {
        match self {
            Self::CustomText { namespace, name } => Some((namespace.as_deref(), name)),
            _ => None,
        }
    }

    pub(crate) fn is_client_tool_search(&self) -> bool {
        matches!(self, Self::ClientToolSearch)
    }
}

/// Schema a custom text tool is offered under once lowered to a function.
pub(crate) fn custom_tool_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "input": {
                "type": "string",
                "description": "The tool's complete free-form input.",
            },
        },
        "required": ["input"],
        "additionalProperties": false,
    })
}

/// Provider function arguments for a canonical custom text tool input.
///
/// Custom tool inputs are text. An object that is exactly `{"input": "<string>"}`
/// is treated as an already lowered function argument (callers should store the
/// extracted text as a JSON string to avoid that ambiguity); every other value is
/// serialized into the single `input` string.
pub(crate) fn custom_tool_function_arguments(input: &Value) -> Value {
    if let Value::Object(object) = input {
        if object.len() == 1 && object.get("input").is_some_and(Value::is_string) {
            return input.clone();
        }
    }
    let text = match input {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    json!({ "input": text })
}

/// Custom text input restored from provider function arguments.
pub(crate) fn custom_tool_input_from_arguments(arguments: &Value) -> Option<String> {
    match arguments {
        Value::String(text) => Some(text.clone()),
        Value::Object(object) => object
            .get("input")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        _ => None,
    }
}

const TOOL_SEARCH_CALL_FIELDS: [&str; 4] = ["arguments", "execution", "status", "created_by"];

/// Provider-facing arguments for a Responses `tool_search_call` history item.
///
/// The canonical block keeps the whole Responses item payload, so a model that
/// sees the search call again gets only the declared search arguments.
pub(crate) fn tool_search_provider_arguments(input: &Value) -> Value {
    match input.as_object() {
        Some(object)
            if object.contains_key("arguments")
                && !object.is_empty()
                && object
                    .keys()
                    .all(|key| TOOL_SEARCH_CALL_FIELDS.contains(&key.as_str())) =>
        {
            object
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}))
        }
        _ => input.clone(),
    }
}

/// Responses `arguments` value for a tool search call restored from provider
/// arguments.
///
/// Provider-emitted arguments are already the declared search parameters and
/// are returned verbatim: a legitimate search schema may itself contain a
/// field named `arguments`, so response restoration must never try to unwrap a
/// Responses envelope here. Only request-side lowering uses
/// [`tool_search_provider_arguments`] to strip the canonical item payload.
pub(crate) fn tool_search_call_arguments(arguments: &Value) -> Value {
    arguments.clone()
}

/// Tool specs carried by a canonical `tool_search_output` tool result.
///
/// `output` is the canonical block output (the Responses `tools` array, or the
/// same array as a JSON string); `content_text` is the transport text fallback.
pub(crate) fn tool_search_output_tools(
    output: Option<&Value>,
    content_text: Option<&str>,
) -> Option<Vec<Value>> {
    let specs = match output {
        Some(Value::Array(specs)) => Some(specs.clone()),
        Some(Value::String(text)) => serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|value| value.as_array().cloned()),
        Some(Value::Null) | None => None,
        Some(_) => None,
    };
    specs.or_else(|| {
        content_text
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .and_then(|value| value.as_array().cloned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lowering_target_reads_the_provider_format_only() {
        let context = json!({"provider_api_format": "openai:chat"});
        assert_eq!(
            ToolLoweringTarget::from_report_context(&context),
            Some(ToolLoweringTarget::OpenAiChat)
        );
        let context = json!({"provider_api_format": "claude:messages"});
        assert_eq!(
            ToolLoweringTarget::from_report_context(&context),
            Some(ToolLoweringTarget::ClaudeMessages)
        );
        assert_eq!(
            ToolLoweringTarget::from_report_context(&json!({
                "provider_api_format": "gemini:generate_content"
            })),
            None
        );
        assert_eq!(ToolLoweringTarget::from_report_context(&json!({})), None);
    }

    #[test]
    fn custom_text_input_round_trips_between_text_and_function_arguments() {
        let text = custom_tool_function_arguments(&Value::String("*** Begin Patch".to_string()));
        assert_eq!(text, json!({"input": "*** Begin Patch"}));
        assert_eq!(
            custom_tool_input_from_arguments(&text),
            Some("*** Begin Patch".to_string())
        );

        let already_lowered = json!({"input": "raw text"});
        assert_eq!(
            custom_tool_function_arguments(&already_lowered),
            already_lowered
        );

        let structured = json!({"path": "a.txt", "patch": "x"});
        let arguments = custom_tool_function_arguments(&structured);
        assert_eq!(
            custom_tool_input_from_arguments(&arguments),
            Some(structured.to_string())
        );

        assert_eq!(
            custom_tool_function_arguments(&Value::Null),
            json!({"input": ""})
        );
    }

    #[test]
    fn custom_text_input_rejects_arguments_without_a_string_input() {
        assert_eq!(custom_tool_input_from_arguments(&json!({})), None);
        assert_eq!(
            custom_tool_input_from_arguments(&json!({"input": {"nested": true}})),
            None
        );
        assert_eq!(custom_tool_input_from_arguments(&json!(3)), None);
    }

    #[test]
    fn tool_search_wrapper_is_unwrapped_only_for_the_responses_item_payload() {
        let wrapper = json!({
            "arguments": {"goal": "find shipping tools"},
            "execution": "client",
            "status": "completed",
        });
        assert_eq!(
            tool_search_provider_arguments(&wrapper),
            json!({"goal": "find shipping tools"})
        );
        let arguments = json!({"goal": "find shipping tools"});
        assert_eq!(tool_search_provider_arguments(&arguments), arguments);
        assert_eq!(tool_search_call_arguments(&arguments), arguments);
        // Provider-emitted arguments are never treated as a Responses envelope:
        // a legitimate search schema may itself use these field names.
        assert_eq!(tool_search_call_arguments(&wrapper), wrapper);
        let ambiguous = json!({"arguments": "q", "limit": 5});
        assert_eq!(tool_search_provider_arguments(&ambiguous), ambiguous);
        assert_eq!(tool_search_call_arguments(&ambiguous), ambiguous);
    }

    #[test]
    fn tool_search_output_specs_accept_array_and_json_text() {
        let specs = json!([{"type": "function", "name": "get_shipping_eta"}]);
        assert_eq!(
            tool_search_output_tools(Some(&specs), None),
            Some(specs.as_array().cloned().unwrap())
        );
        let text = specs.to_string();
        assert_eq!(
            tool_search_output_tools(Some(&Value::String(text.clone())), None),
            Some(specs.as_array().cloned().unwrap())
        );
        assert_eq!(
            tool_search_output_tools(None, Some(text.as_str())),
            Some(specs.as_array().cloned().unwrap())
        );
        assert_eq!(
            tool_search_output_tools(Some(&json!({"not": "specs"})), None),
            None
        );
        assert_eq!(tool_search_output_tools(None, Some("not json")), None);
    }

    #[test]
    fn restore_accessors_return_the_original_identity() {
        let restore = ResponsesToolRestore::NamespaceChild {
            namespace: "crm".to_string(),
            name: "lookup".to_string(),
        };
        assert_eq!(restore.responses_name("aether_ns_lookup_00"), "lookup");
        assert_eq!(restore.namespace(), Some("crm"));
        assert_eq!(restore.custom_text_identity(), None);
        assert!(!restore.is_client_tool_search());

        let restore = ResponsesToolRestore::CustomText {
            namespace: None,
            name: "apply_patch".to_string(),
        };
        assert_eq!(restore.custom_text_identity(), Some((None, "apply_patch")));
        assert_eq!(restore.responses_name("apply_patch"), "apply_patch");

        let restore = ResponsesToolRestore::ClientToolSearch;
        assert!(restore.is_client_tool_search());
        assert_eq!(restore.responses_name("tool_search"), "tool_search");
        assert_eq!(restore.namespace(), None);

        let restore = ResponsesToolRestore::Unchanged;
        assert_eq!(restore.responses_name("plain"), "plain");
    }
}
