use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::formats::openai::tool_lowering::{
    custom_tool_input_schema, tool_search_output_tools, ResponsesToolRestore, ToolLoweringError,
    ToolLoweringTarget, CUSTOM_TOOL_INPUT_INSTRUCTION, RESPONSES_DEFAULT_TOOL_NAMESPACE,
    TOOL_SEARCH_FUNCTION_NAME,
};
use crate::protocol::canonical::{
    openai_responses_extension, openai_responses_tools_to_canonical, CanonicalContentBlock,
    CanonicalRequest, CanonicalToolDefinition, OPENAI_RESPONSES_EXTENSION_NAMESPACE,
    OPENAI_RESPONSES_LEGACY_EXTENSION_NAMESPACE,
};

const OPENAI_CHAT_TOOL_NAME_MAX_LEN: usize = 64;
const HASHED_ALIAS_PREFIX: &str = "aether_ns_";
const HASH_HEX_LEN: usize = 32;

/// Provider-neutral identity of a Responses tool after lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LoweredToolKind {
    Function,
    CustomText,
    ToolSearch,
}

/// Request-local Responses tool identity. Namespace children keep the pair;
/// `functions` is the documented default namespace and is normalized away.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ToolIdentity {
    kind: LoweredToolKind,
    namespace: Option<String>,
    name: String,
}

impl ToolIdentity {
    fn flat(kind: LoweredToolKind, name: String) -> Self {
        Self {
            kind,
            namespace: None,
            name,
        }
    }
}

/// Offered namespace child: the published alias table plus its Chat/Claude
/// tool definition.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NamespaceChatTool {
    pub source_tool_index: Option<usize>,
    pub source_child_index: usize,
    pub namespace: String,
    pub name: String,
    pub chat_name: String,
    pub description: Option<String>,
    pub parameters: Option<Value>,
    pub strict: Option<Value>,
    kind: LoweredToolKind,
}

impl NamespaceChatTool {
    pub(crate) fn to_openai_chat_tool(&self) -> Value {
        chat_function_tool(
            &self.chat_name,
            self.description.as_deref(),
            self.parameters.as_ref(),
            self.strict.as_ref(),
        )
    }

    fn provider_tool(&self, target: ToolLoweringTarget) -> Value {
        match target {
            ToolLoweringTarget::OpenAiChat => self.to_openai_chat_tool(),
            ToolLoweringTarget::ClaudeMessages => claude_function_tool(
                &self.chat_name,
                self.description.as_deref(),
                self.parameters.as_ref(),
            ),
        }
    }
}

/// Offered top-level tool that is not a namespace child.
#[derive(Debug, Clone, PartialEq)]
struct FlatLoweredTool {
    identity: ToolIdentity,
    provider_name: String,
    description: Option<String>,
    parameters: Option<Value>,
    strict: Option<Value>,
    source_tool_index: Option<usize>,
    /// Raw Responses custom tool declaration, kept for the Chat native
    /// `type: custom` passthrough of a declared top-level custom tool.
    native_custom_chat: Option<Value>,
    lowered_as_function: bool,
}

impl FlatLoweredTool {
    fn provider_definition(&self, target: ToolLoweringTarget) -> Value {
        if let Some(native) = &self.native_custom_chat {
            if target == ToolLoweringTarget::OpenAiChat {
                return native.clone();
            }
        }
        self.provider_tool(target)
    }

    fn provider_tool(&self, target: ToolLoweringTarget) -> Value {
        match target {
            ToolLoweringTarget::OpenAiChat => chat_function_tool(
                &self.provider_name,
                self.description.as_deref(),
                self.parameters.as_ref(),
                self.strict.as_ref(),
            ),
            ToolLoweringTarget::ClaudeMessages => claude_function_tool(
                &self.provider_name,
                self.description.as_deref(),
                self.parameters.as_ref(),
            ),
        }
    }
}

/// Parsed Responses tool before offering and alias allocation.
#[derive(Debug, Clone)]
struct ToolCandidate {
    identity: ToolIdentity,
    description: Option<String>,
    parameters: Option<Value>,
    strict: Option<Value>,
    source_tool_index: Option<usize>,
    source_child_index: Option<usize>,
    native_custom_chat: Option<Value>,
    deferred: bool,
    discovered: bool,
    lowered_as_function: bool,
    location: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct NamespaceToolAliases {
    tools: Vec<NamespaceChatTool>,
    by_identity: BTreeMap<(String, String), String>,
    by_chat_name: BTreeMap<String, (String, String)>,
    namespace_tool_indices: BTreeSet<usize>,
    invalid_namespace_tool_indices: BTreeSet<usize>,
    client_function_tool_names: BTreeSet<String>,
    audited_tool_indices: BTreeSet<usize>,
    omitted_tool_indices: BTreeSet<usize>,
    flat: Vec<FlatLoweredTool>,
    discovery_order: Vec<ToolIdentity>,
    restore_by_name: BTreeMap<String, ResponsesToolRestore>,
    unoffered_names: BTreeSet<String>,
    ordinary_tool_name_counts: BTreeMap<String, usize>,
    namespace_parent_name_counts: BTreeMap<String, usize>,
}

/// Resolution of a Responses tool-choice name against the offered inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoweredToolChoiceName {
    /// The name is an ordinary (or absent) tool and needs no rewriting.
    Unchanged,
    /// The name identifies exactly one offered namespace child.
    Alias(String),
    /// The name identifies a namespace parent, several children, or several
    /// ordinary tools; it cannot be represented unambiguously.
    Ambiguous,
}

impl NamespaceToolAliases {
    /// Build the request-local lowering for a Responses request. Fails closed
    /// when a tool declaration or a discovered tool spec has no lossless
    /// Chat/Claude equivalent.
    pub(crate) fn from_canonical_request(
        request: &CanonicalRequest,
        target: ToolLoweringTarget,
    ) -> Result<Self, ToolLoweringError> {
        let discovered = canonical_discovered_tool_specs(request)?;
        let (aliases, error) = assemble(&request.tools, &discovered, Some(target));
        match error {
            Some(error) => Err(error),
            None => Ok(aliases),
        }
    }

    /// Tool-table-only construction kept for callers that have no request
    /// history. Deferred tools are treated as not yet discovered.
    pub(crate) fn from_canonical_tools(tools: &[CanonicalToolDefinition]) -> Self {
        assemble(tools, &[], Some(ToolLoweringTarget::OpenAiChat)).0
    }

    /// Rebuild the same request-local inventory from the captured client body
    /// for response rehydration: declared tools (including leading
    /// `additional_tools` items) plus tools discovered by client
    /// `tool_search_output` history. The provider format in the report context
    /// selects the custom text tool representation. Requires the request to
    /// carry its complete input, which the Responses client does.
    pub(crate) fn from_report_context(report_context: &Value) -> Self {
        let Some(request) = report_context.get("original_request_body") else {
            return Self::default();
        };
        let tools_value = raw_declared_tools(request);
        let client_function_tool_names = client_function_tool_names(&tools_value);
        let target = ToolLoweringTarget::from_report_context(report_context);
        let discovered = raw_discovered_tool_specs(request);
        let tools = openai_responses_tools_to_canonical(Some(&tools_value)).unwrap_or_default();
        let mut result = assemble(&tools, &discovered, target).0;
        // The captured body can carry tools the canonical parser cannot model;
        // keep them in the hosted-search collision inventory as well.
        result
            .client_function_tool_names
            .extend(client_function_tool_names);
        result
    }

    /// Whether the lowering audited every raw provider field of `tools[index]`.
    pub(crate) fn tool_is_audited(&self, tool_index: usize) -> bool {
        self.audited_tool_indices.contains(&tool_index)
    }

    /// Provider tool JSON that replaces `tools[index]` for `target`.
    ///
    /// `None` keeps the caller's existing per-tool mapping (plain function
    /// passthrough); `Some` replaces the tool and may be empty when a deferred
    /// tool is not discovered yet.
    pub(crate) fn lowered_tools_for_source(
        &self,
        target: ToolLoweringTarget,
        tool_index: usize,
    ) -> Option<Vec<Value>> {
        if !self.audited_tool_indices.contains(&tool_index) {
            return None;
        }
        if self.namespace_tool_indices.contains(&tool_index) {
            // A namespace parent is replaced by its offered children, including
            // children of the default `functions` namespace (flattened above).
            let mut lowered = self
                .tools_for_source(tool_index)
                .map(|tool| tool.provider_tool(target))
                .collect::<Vec<_>>();
            lowered.extend(
                self.flat
                    .iter()
                    .filter(|tool| tool.source_tool_index == Some(tool_index))
                    .map(|tool| tool.provider_definition(target)),
            );
            return Some(lowered);
        }
        let declared = self
            .flat
            .iter()
            .filter(|tool| tool.source_tool_index == Some(tool_index))
            .collect::<Vec<_>>();
        if declared.is_empty() {
            return self
                .omitted_tool_indices
                .contains(&tool_index)
                .then(Vec::new);
        }
        if declared
            .iter()
            .all(|tool| tool.identity.kind == LoweredToolKind::Function)
        {
            return None;
        }
        Some(
            declared
                .iter()
                .map(|tool| tool.provider_definition(target))
                .collect(),
        )
    }

    /// Tools discovered by a client `tool_search_output` that the request does
    /// not declare; appended after the declared tools in discovery order.
    pub(crate) fn discovered_tools(&self, target: ToolLoweringTarget) -> Vec<Value> {
        self.discovery_order
            .iter()
            .filter_map(|identity| self.tool_json_for_identity(target, identity))
            .collect()
    }

    /// Model-visible result text for a client `tool_search_output` payload. It
    /// names the tools exactly as the provider request offers them.
    pub(crate) fn tool_search_output_text(&self, tools: &Value) -> Option<String> {
        let specs = tool_search_output_tools(Some(tools), None)?;
        let mut names = Vec::new();
        let mut seen = BTreeSet::new();
        for spec in &specs {
            let identities = discovered_identities_in_spec(spec).ok()?;
            for identity in identities {
                let name = self.provider_name_for_identity(&identity)?.to_string();
                if seen.insert(name.clone()) {
                    names.push(name);
                }
            }
        }
        Some(if names.is_empty() {
            "No tools matched the search.".to_string()
        } else {
            format!(
                "These tools are now available to call: {}",
                names.join(", ")
            )
        })
    }

    /// Restore a provider-emitted tool name to its Responses identity.
    pub(crate) fn restore_tool_call(&self, provider_name: &str) -> ResponsesToolRestore {
        self.restore_by_name
            .get(provider_name)
            .cloned()
            .unwrap_or(ResponsesToolRestore::Unchanged)
    }

    /// Whether a custom text tool was lowered to a provider function. Native
    /// Chat custom tools keep their own `type: custom` call shape.
    pub(crate) fn custom_tool_is_function_lowered(
        &self,
        namespace: Option<&str>,
        name: &str,
    ) -> bool {
        match namespace {
            Some(namespace) => self.tools.iter().any(|tool| {
                tool.namespace == namespace
                    && tool.name == name
                    && tool.kind == LoweredToolKind::CustomText
            }),
            None => self.flat.iter().any(|tool| {
                tool.identity.kind == LoweredToolKind::CustomText
                    && tool.identity.name == name
                    && tool.lowered_as_function
            }),
        }
    }

    /// Provider-facing name for a Responses tool-use identity in request
    /// history. Namespace children must resolve to an offered alias; the
    /// default `functions` namespace is the flat identity.
    pub(crate) fn tool_use_provider_name(
        &self,
        namespace: Option<&str>,
        name: &str,
    ) -> Result<String, ToolLoweringError> {
        let Some(namespace) = namespace.and_then(normalize_namespace) else {
            return Ok(name.to_string());
        };
        self.chat_name(&namespace, name)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                ToolLoweringError::new(
                    "messages[].content[].openai_responses.namespace",
                    "Responses namespace tool call does not match an offered namespace child",
                )
            })
    }

    /// Whether a deferred tool with this Responses name is declared but not
    /// currently offered. A tool choice that names it cannot be represented
    /// until a client `tool_search_output` discovers it.
    pub(crate) fn name_is_declared_but_unoffered(&self, name: &str) -> bool {
        self.unoffered_names.contains(name)
            && !self.flat.iter().any(|tool| tool.identity.name == name)
            && !self.tools.iter().any(|tool| tool.name == name)
    }

    /// Whether a tool call named `name` should surface to a Responses client as
    /// a hosted `web_search_call`. A client that declared its own function or
    /// custom tool called `web_search` must get a `function_call` back, or it
    /// cannot answer the call and will echo an unconvertible hosted item.
    /// When the client declares both a hosted `web_search` tool and a function
    /// of the same name, the function wins: only the client can answer it.
    /// The inventory includes tools discovered by client `tool_search_output`.
    pub(crate) fn emits_hosted_web_search_call(&self, name: &str) -> bool {
        matches!(name, "web_search" | "web_search_preview")
            && self.responses_name(name).is_none()
            && !self.client_function_tool_names.contains(name)
    }

    /// Unambiguous provider name for a Responses `tool_choice` name.
    ///
    /// Namespace children are keyed only by their child name in the Responses
    /// tool-choice contract, so a name matching one offered child is rewritten
    /// to its alias; a namespace parent, several matching children, or several
    /// ordinary tools cannot be represented and are reported as ambiguous.
    pub(crate) fn resolve_tool_choice_name(&self, name: &str) -> LoweredToolChoiceName {
        let children = self
            .namespace_children_named(name)
            .map(|child| child.chat_name.as_str())
            .collect::<Vec<_>>();
        let ordinary_matches = self
            .ordinary_tool_name_counts
            .get(name)
            .copied()
            .unwrap_or_default();
        let namespace_parent_matches = self
            .namespace_parent_name_counts
            .get(name)
            .copied()
            .unwrap_or_default();
        match (
            children.as_slice(),
            ordinary_matches,
            namespace_parent_matches,
        ) {
            ([], 0, 0) | ([], 1, _) => LoweredToolChoiceName::Unchanged,
            ([], 0, _) => LoweredToolChoiceName::Ambiguous,
            ([child], 0, _) => LoweredToolChoiceName::Alias((*child).to_string()),
            _ => LoweredToolChoiceName::Ambiguous,
        }
    }

    pub(crate) fn chat_name(&self, namespace: &str, child_name: &str) -> Option<&str> {
        let namespace = normalize_namespace(namespace)?;
        self.by_identity
            .get(&(namespace, child_name.to_string()))
            .map(String::as_str)
    }

    pub(crate) fn responses_name(&self, chat_name: &str) -> Option<(&str, &str)> {
        self.by_chat_name
            .get(chat_name)
            .map(|(namespace, child_name)| (namespace.as_str(), child_name.as_str()))
    }

    pub(crate) fn tools_for_source(
        &self,
        source_tool_index: usize,
    ) -> impl Iterator<Item = &NamespaceChatTool> {
        self.tools
            .iter()
            .filter(move |tool| tool.source_tool_index == Some(source_tool_index))
    }

    pub(crate) fn is_namespace_tool(&self, source_tool_index: usize) -> bool {
        self.namespace_tool_indices.contains(&source_tool_index)
    }

    pub(crate) fn has_invalid_namespace_tools(&self) -> bool {
        !self.invalid_namespace_tool_indices.is_empty()
    }

    pub(crate) fn namespace_children_named<'a>(
        &'a self,
        child_name: &'a str,
    ) -> impl Iterator<Item = &'a NamespaceChatTool> + 'a {
        self.tools
            .iter()
            .filter(move |tool| tool.name == child_name)
    }

    fn provider_name_for_identity(&self, identity: &ToolIdentity) -> Option<&str> {
        match identity.namespace.as_deref() {
            Some(namespace) => self.chat_name(namespace, &identity.name),
            None => self
                .flat
                .iter()
                .find(|tool| tool.identity == *identity)
                .map(|tool| tool.provider_name.as_str()),
        }
    }

    fn tool_json_for_identity(
        &self,
        target: ToolLoweringTarget,
        identity: &ToolIdentity,
    ) -> Option<Value> {
        match identity.namespace.as_deref() {
            Some(namespace) => self
                .tools
                .iter()
                .find(|tool| tool.namespace == namespace && tool.name == identity.name)
                .map(|tool| tool.provider_tool(target)),
            None => self
                .flat
                .iter()
                .find(|tool| tool.identity == *identity)
                .map(|tool| tool.provider_definition(target)),
        }
    }
}

fn canonical_discovered_tool_specs(
    request: &CanonicalRequest,
) -> Result<Vec<Value>, ToolLoweringError> {
    let mut specs = Vec::new();
    for message in &request.messages {
        for block in &message.content {
            let CanonicalContentBlock::ToolResult {
                output,
                content_text,
                extensions,
                ..
            } = block
            else {
                continue;
            };
            if responses_tool_result_item_type(extensions) != Some("tool_search_output") {
                continue;
            }
            let tools = tool_search_output_tools(output.as_ref(), content_text.as_deref())
                .ok_or_else(|| {
                    ToolLoweringError::new(
                        "messages[].content[].output",
                        "client tool_search_output must carry the discovered tools array",
                    )
                })?;
            specs.extend(tools);
        }
    }
    Ok(specs)
}

/// Declared tools of a raw Responses body, including the leading
/// `additional_tools` control items the Chat conversion merges into `tools`.
fn raw_declared_tools(request: &Value) -> Value {
    let mut additional_tools = Vec::new();
    if let Some(input) = request.get("input").and_then(Value::as_array) {
        for item in input
            .iter()
            .take_while(|item| is_additional_tools_item(item))
        {
            if let Some(tools) = item.get("tools").and_then(Value::as_array) {
                additional_tools.extend(tools.iter().cloned());
            }
        }
    }
    let existing_tools = request.get("tools");
    if additional_tools.is_empty() {
        return existing_tools.cloned().unwrap_or(Value::Null);
    }
    if existing_tools.is_some_and(|tools| !tools.is_array()) {
        return existing_tools.cloned().unwrap_or(Value::Null);
    }
    if let Some(existing) = existing_tools.and_then(Value::as_array) {
        additional_tools.extend(existing.iter().cloned());
    }
    Value::Array(additional_tools)
}

fn is_additional_tools_item(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|item_type| item_type.eq_ignore_ascii_case("additional_tools"))
        && object.get("role").and_then(Value::as_str) == Some("developer")
        && object.get("tools").is_some_and(Value::is_array)
        && object
            .keys()
            .all(|key| matches!(key.as_str(), "type" | "role" | "tools"))
}

fn raw_discovered_tool_specs(request: &Value) -> Vec<Value> {
    let mut specs = Vec::new();
    let Some(items) = request.get("input").and_then(Value::as_array) else {
        return specs;
    };
    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        if object.get("type").and_then(Value::as_str) != Some("tool_search_output") {
            continue;
        }
        if let Some(tools) = tool_search_output_tools(
            object.get("tools"),
            object.get("content").and_then(Value::as_str),
        ) {
            specs.extend(tools);
        }
    }
    specs
}

fn responses_tool_result_item_type(extensions: &BTreeMap<String, Value>) -> Option<&str> {
    openai_responses_extension(extensions)?
        .get("item_type")
        .and_then(Value::as_str)
}

/// Parse the declared tool inventory plus discovered specs and allocate
/// deterministic, collision-safe provider names.
fn assemble(
    tools: &[CanonicalToolDefinition],
    discovered_specs: &[Value],
    target: Option<ToolLoweringTarget>,
) -> (NamespaceToolAliases, Option<ToolLoweringError>) {
    let mut result = NamespaceToolAliases::default();
    let mut error = None;
    let mut candidates = Vec::new();
    let mut namespace_child_names = Vec::new();
    let mut ordinary_declared_names = BTreeSet::new();

    for (index, tool) in tools.iter().enumerate() {
        let kind = if responses_tool_extensions_only(tool) {
            declared_tool_kind(tool)
        } else {
            DeclaredToolKind::Other
        };
        match kind {
            DeclaredToolKind::Namespace => {
                result.namespace_tool_indices.insert(index);
                match parse_namespace_tool(index, tool) {
                    Ok(children) => {
                        result.audited_tool_indices.insert(index);
                        namespace_child_names
                            .extend(children.iter().map(|child| child.identity.name.clone()));
                        candidates.extend(children);
                    }
                    Err(parse_error) => {
                        result.invalid_namespace_tool_indices.insert(index);
                        keep_first_error(&mut error, parse_error);
                    }
                }
            }
            DeclaredToolKind::Function => match parse_flat_function(index, tool) {
                Ok(Some(candidate)) => {
                    result.audited_tool_indices.insert(index);
                    ordinary_declared_names.insert(tool.name.clone());
                    candidates.push(candidate);
                }
                Ok(None) => {
                    ordinary_declared_names.insert(tool.name.clone());
                }
                Err(parse_error) => {
                    keep_first_error(&mut error, parse_error);
                }
            },
            DeclaredToolKind::Custom => match parse_flat_custom(index, tool, target) {
                Ok(Some(candidate)) => {
                    result.audited_tool_indices.insert(index);
                    ordinary_declared_names.insert(tool.name.clone());
                    candidates.push(candidate);
                }
                Ok(None) => {
                    ordinary_declared_names.insert(tool.name.clone());
                }
                Err(parse_error) => {
                    keep_first_error(&mut error, parse_error);
                }
            },
            DeclaredToolKind::ToolSearch => match parse_tool_search(index, tool) {
                Ok(candidate) => {
                    result.audited_tool_indices.insert(index);
                    ordinary_declared_names.insert(TOOL_SEARCH_FUNCTION_NAME.to_string());
                    candidates.push(candidate);
                }
                Err(parse_error) => {
                    keep_first_error(&mut error, parse_error);
                }
            },
            DeclaredToolKind::Other => {
                ordinary_declared_names.insert(tool.name.clone());
            }
        }
    }

    let mut name_counts = BTreeMap::<String, usize>::new();
    for (index, tool) in tools.iter().enumerate() {
        if result.namespace_tool_indices.contains(&index) {
            *result
                .namespace_parent_name_counts
                .entry(tool.name.clone())
                .or_default() += 1;
        } else {
            *name_counts.entry(tool.name.clone()).or_default() += 1;
            *result
                .ordinary_tool_name_counts
                .entry(tool.name.clone())
                .or_default() += 1;
        }
    }
    for name in &namespace_child_names {
        *name_counts.entry(name.clone()).or_default() += 1;
    }

    drop_duplicate_namespace_identities(&mut candidates, &mut result, &mut error);

    let declared_identities = candidates
        .iter()
        .map(|candidate| candidate.identity.clone())
        .collect::<BTreeSet<_>>();
    let mut discovered_identities = BTreeSet::new();
    let mut discovered_only = Vec::new();
    match parse_discovered_tool_specs(discovered_specs) {
        Ok(parsed) => {
            for mut candidate in parsed {
                candidate.source_tool_index = None;
                if declared_identities.contains(&candidate.identity) {
                    discovered_identities.insert(candidate.identity);
                } else if discovered_identities.insert(candidate.identity.clone()) {
                    discovered_only.push(candidate);
                }
            }
        }
        Err(parse_error) => keep_first_error(&mut error, parse_error),
    }
    for candidate in &mut candidates {
        if discovered_identities.contains(&candidate.identity) {
            candidate.discovered = true;
        }
    }

    let mut offered = Vec::new();
    for candidate in &candidates {
        if candidate.deferred && !candidate.discovered {
            result
                .unoffered_names
                .insert(candidate.identity.name.clone());
            if let Some(source_tool_index) = candidate.source_tool_index {
                result.omitted_tool_indices.insert(source_tool_index);
            }
            continue;
        }
        offered.push(candidate.clone());
    }
    // Tools only known from client `tool_search_output` history are offered
    // exactly like declared tools; they are never deferred (the loaded form
    // keeps `defer_loading` but it no longer hides the tool).
    offered.extend(discovered_only.iter().cloned());
    let tool_search_declared = candidates
        .iter()
        .any(|candidate| candidate.identity.kind == LoweredToolKind::ToolSearch);
    for candidate in &candidates {
        if candidate.deferred && !candidate.discovered && !tool_search_declared {
            keep_first_error(
                &mut error,
                ToolLoweringError::new(
                    format!("{}.defer_loading", candidate.location),
                    "deferred tool cannot be discovered without a client-executed tool_search tool",
                ),
            );
        }
    }

    let mut reserved_names = ordinary_declared_names;
    for candidate in &discovered_only {
        *name_counts
            .entry(candidate.identity.name.clone())
            .or_default() += 1;
        if candidate.identity.namespace.is_none() {
            reserved_names.insert(candidate.identity.name.clone());
        }
    }
    reject_flat_provider_name_collisions(&offered, &mut error);

    allocate_namespace_aliases(&mut result, &offered, &name_counts, &reserved_names);

    // Flat identities keep their Responses name; only namespace children are
    // renamed, which keeps history names stable without lossy heuristics.
    for candidate in offered
        .iter()
        .filter(|candidate| candidate.identity.namespace.is_none())
        .cloned()
    {
        let provider_name = candidate.identity.name.clone();
        match candidate.identity.kind {
            LoweredToolKind::CustomText if candidate.lowered_as_function => {
                result.restore_by_name.insert(
                    provider_name.clone(),
                    ResponsesToolRestore::CustomText {
                        namespace: None,
                        name: candidate.identity.name.clone(),
                    },
                );
            }
            LoweredToolKind::ToolSearch => {
                result.restore_by_name.insert(
                    provider_name.clone(),
                    ResponsesToolRestore::ClientToolSearch,
                );
            }
            _ => {}
        }
        result.flat.push(FlatLoweredTool {
            identity: candidate.identity,
            provider_name,
            description: candidate.description,
            parameters: candidate.parameters,
            strict: candidate.strict,
            source_tool_index: candidate.source_tool_index,
            native_custom_chat: candidate.native_custom_chat,
            lowered_as_function: candidate.lowered_as_function,
        });
    }

    // Hosted `web_search` classification must see the complete inventory:
    // declared function/custom tools (offered or deferred) and tools discovered
    // by client `tool_search_output`, never just the raw declared list.
    result.client_function_tool_names.extend(
        candidates
            .iter()
            .chain(discovered_only.iter())
            .filter(|candidate| {
                candidate.identity.namespace.is_none()
                    && matches!(
                        candidate.identity.kind,
                        LoweredToolKind::Function | LoweredToolKind::CustomText
                    )
            })
            .map(|candidate| candidate.identity.name.clone()),
    );

    result.discovery_order = discovered_only
        .into_iter()
        .map(|candidate| candidate.identity)
        .collect();

    (result, error)
}

fn keep_first_error(slot: &mut Option<ToolLoweringError>, error: ToolLoweringError) {
    if slot.is_none() {
        *slot = Some(error);
    }
}

fn drop_duplicate_namespace_identities(
    candidates: &mut Vec<ToolCandidate>,
    result: &mut NamespaceToolAliases,
    error: &mut Option<ToolLoweringError>,
) {
    let mut sources_by_identity = BTreeMap::<ToolIdentity, BTreeSet<usize>>::new();
    for candidate in candidates.iter() {
        let (Some(namespace), Some(source_tool_index)) = (
            candidate.identity.namespace.as_ref(),
            candidate.source_tool_index,
        ) else {
            continue;
        };
        if namespace.is_empty() {
            continue;
        }
        sources_by_identity
            .entry(candidate.identity.clone())
            .or_default()
            .insert(source_tool_index);
    }
    let duplicated_sources = sources_by_identity
        .into_values()
        .filter(|sources| sources.len() > 1)
        .flatten()
        .collect::<BTreeSet<_>>();
    if duplicated_sources.is_empty() {
        return;
    }
    result
        .invalid_namespace_tool_indices
        .extend(duplicated_sources.iter().copied());
    if let Some(source_tool_index) = duplicated_sources.iter().next() {
        keep_first_error(
            error,
            ToolLoweringError::new(
                format!("tools[{source_tool_index}]"),
                "Responses namespace tool identity is declared by more than one namespace",
            ),
        );
    }
    candidates.retain(|candidate| {
        candidate
            .source_tool_index
            .is_none_or(|source| !duplicated_sources.contains(&source))
    });
}

fn reject_flat_provider_name_collisions(
    offered: &[ToolCandidate],
    error: &mut Option<ToolLoweringError>,
) {
    let mut seen = BTreeSet::<String>::new();
    for candidate in offered
        .iter()
        .filter(|candidate| candidate.identity.namespace.is_none())
    {
        if !seen.insert(candidate.identity.name.clone()) {
            keep_first_error(
                error,
                ToolLoweringError::new(
                    format!("{}.name", candidate.location),
                    format!(
                        "tool name {:?} is declared more than once or shared by different Responses tool identities",
                        candidate.identity.name
                    ),
                ),
            );
        }
    }
}

fn allocate_namespace_aliases(
    result: &mut NamespaceToolAliases,
    offered: &[ToolCandidate],
    name_counts: &BTreeMap<String, usize>,
    reserved_names: &BTreeSet<String>,
) {
    let namespace_children = offered
        .iter()
        .filter_map(|candidate| {
            candidate
                .identity
                .namespace
                .as_ref()
                .map(|namespace| (candidate, namespace))
        })
        .collect::<Vec<_>>();

    let preferred_names = namespace_children
        .iter()
        .map(|(candidate, namespace)| {
            let name = &candidate.identity.name;
            let name_is_globally_unique = name_counts.get(name).copied() == Some(1);
            let preferred = if name_is_globally_unique
                && is_valid_chat_tool_name(name)
                && !reserved_names.contains(name)
            {
                Some(name.clone())
            } else {
                let readable = format!("{namespace}__{name}");
                (is_valid_chat_tool_name(&readable) && !reserved_names.contains(&readable))
                    .then_some(readable)
            };
            (((*namespace).clone(), name.clone()), preferred)
        })
        .collect::<BTreeMap<_, _>>();

    let mut preferred_counts = BTreeMap::<String, usize>::new();
    for preferred in preferred_names.values().flatten() {
        *preferred_counts.entry(preferred.clone()).or_default() += 1;
    }

    let mut aliases_by_identity = BTreeMap::<(String, String), String>::new();
    let mut used_chat_names = reserved_names.to_owned();
    for (identity, preferred) in &preferred_names {
        if let Some(preferred) = preferred
            .as_ref()
            .filter(|name| preferred_counts.get(*name).copied() == Some(1))
        {
            aliases_by_identity.insert(identity.clone(), preferred.clone());
            used_chat_names.insert(preferred.clone());
        }
    }
    for identity in preferred_names.keys() {
        if aliases_by_identity.contains_key(identity) {
            continue;
        }
        let chat_name = allocate_hashed_alias(&identity.0, &identity.1, &used_chat_names);
        used_chat_names.insert(chat_name.clone());
        aliases_by_identity.insert(identity.clone(), chat_name);
    }

    for (candidate, namespace) in namespace_children {
        let identity = (namespace.clone(), candidate.identity.name.clone());
        let Some(chat_name) = aliases_by_identity.get(&identity).cloned() else {
            continue;
        };
        result
            .by_identity
            .insert(identity.clone(), chat_name.clone());
        result.by_chat_name.insert(chat_name.clone(), identity);
        let restore = match candidate.identity.kind {
            LoweredToolKind::CustomText => ResponsesToolRestore::CustomText {
                namespace: Some(namespace.clone()),
                name: candidate.identity.name.clone(),
            },
            _ => ResponsesToolRestore::NamespaceChild {
                namespace: namespace.clone(),
                name: candidate.identity.name.clone(),
            },
        };
        result.restore_by_name.insert(chat_name.clone(), restore);
        result.tools.push(NamespaceChatTool {
            source_tool_index: candidate.source_tool_index,
            source_child_index: candidate.source_child_index.unwrap_or(0),
            namespace: namespace.clone(),
            name: candidate.identity.name.clone(),
            chat_name,
            description: candidate.description.clone(),
            parameters: candidate.parameters.clone(),
            strict: candidate.strict.clone(),
            kind: candidate.identity.kind,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeclaredToolKind {
    Namespace,
    Function,
    Custom,
    ToolSearch,
    Other,
}

fn responses_tool_extensions_only(tool: &CanonicalToolDefinition) -> bool {
    tool.extensions.keys().all(|namespace| {
        matches!(
            namespace.as_str(),
            OPENAI_RESPONSES_EXTENSION_NAMESPACE | OPENAI_RESPONSES_LEGACY_EXTENSION_NAMESPACE
        )
    })
}

fn declared_tool_kind(tool: &CanonicalToolDefinition) -> DeclaredToolKind {
    let Some(raw) = raw_responses_tool(tool) else {
        return DeclaredToolKind::Function;
    };
    match raw
        .get("type")
        .and_then(Value::as_str)
        .map(|tool_type| tool_type.trim().to_ascii_lowercase())
    {
        Some(tool_type) if tool_type == "namespace" => DeclaredToolKind::Namespace,
        Some(tool_type) if tool_type == "function" => DeclaredToolKind::Function,
        Some(tool_type) if tool_type == "custom" => DeclaredToolKind::Custom,
        Some(tool_type) if tool_type == "tool_search" => DeclaredToolKind::ToolSearch,
        Some(_) => DeclaredToolKind::Other,
        None => {
            if object_has_only_keys(raw, &["type", "function", "defer_loading"]) {
                DeclaredToolKind::Function
            } else {
                DeclaredToolKind::Other
            }
        }
    }
}

fn parse_flat_function(
    source_tool_index: usize,
    tool: &CanonicalToolDefinition,
) -> Result<Option<ToolCandidate>, ToolLoweringError> {
    let name = tool.name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    let location = format!("tools[{source_tool_index}]");
    let deferred = match raw_responses_tool(tool) {
        None => false,
        Some(raw) => {
            if !object_has_only_keys(raw, &["type", "function", "defer_loading"]) {
                // Not an audited shape: existing extension validation decides.
                return Ok(None);
            }
            optional_bool(
                raw.get("defer_loading"),
                &format!("{location}.defer_loading"),
            )?
            .unwrap_or(false)
        }
    };
    Ok(Some(ToolCandidate {
        identity: ToolIdentity::flat(LoweredToolKind::Function, name.to_string()),
        description: None,
        parameters: None,
        strict: None,
        source_tool_index: Some(source_tool_index),
        source_child_index: None,
        native_custom_chat: None,
        deferred,
        discovered: false,
        lowered_as_function: false,
        location,
    }))
}

fn parse_flat_custom(
    source_tool_index: usize,
    tool: &CanonicalToolDefinition,
    target: Option<ToolLoweringTarget>,
) -> Result<Option<ToolCandidate>, ToolLoweringError> {
    let Some(raw) = raw_responses_tool(tool) else {
        return Ok(None);
    };
    if !object_has_only_keys(
        raw,
        &[
            "type",
            "name",
            "description",
            "format",
            "defer_loading",
            "custom",
        ],
    ) {
        return Ok(None);
    }
    let location = format!("tools[{source_tool_index}]");
    let name = tool.name.trim().to_string();
    if name.is_empty() {
        return Err(ToolLoweringError::new(
            format!("{location}.name"),
            "custom tool requires a non-empty name",
        ));
    }
    let nested = raw.get("custom").and_then(Value::as_object);
    let description =
        match optional_string(raw.get("description"), &format!("{location}.description"))? {
            Some(description) => Some(description),
            None => nested
                .map(|nested| {
                    optional_string(
                        nested.get("description"),
                        &format!("{location}.custom.description"),
                    )
                })
                .transpose()?
                .flatten(),
        };
    let format = raw
        .get("format")
        .or_else(|| nested.and_then(|nested| nested.get("format")));
    validate_custom_text_format(format, &format!("{location}.format"))?;
    let deferred = optional_bool(
        raw.get("defer_loading"),
        &format!("{location}.defer_loading"),
    )?
    .unwrap_or(false);
    let native_custom_chat = (target == Some(ToolLoweringTarget::OpenAiChat))
        .then(|| native_chat_custom_tool(raw, &name, description.as_deref()));
    if target != Some(ToolLoweringTarget::OpenAiChat)
        && target.is_some()
        && !is_valid_chat_tool_name(&name)
    {
        return Err(ToolLoweringError::new(
            format!("{location}.name"),
            "custom tool name cannot be represented as a Claude function name",
        ));
    }
    Ok(Some(ToolCandidate {
        identity: ToolIdentity::flat(LoweredToolKind::CustomText, name),
        description: custom_tool_description(description.as_deref()),
        parameters: Some(custom_tool_input_schema()),
        strict: None,
        source_tool_index: Some(source_tool_index),
        source_child_index: None,
        native_custom_chat,
        deferred,
        discovered: false,
        lowered_as_function: target == Some(ToolLoweringTarget::ClaudeMessages),
        location,
    }))
}

fn native_chat_custom_tool(
    raw: &Map<String, Value>,
    name: &str,
    description: Option<&str>,
) -> Value {
    let mut custom = raw
        .get("custom")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(|| {
            raw.iter()
                .filter(|(key, _)| !matches!(key.as_str(), "type" | "defer_loading"))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        });
    custom
        .entry("name".to_string())
        .or_insert_with(|| Value::String(name.to_string()));
    if let Some(description) = description {
        custom
            .entry("description".to_string())
            .or_insert_with(|| Value::String(description.to_string()));
    }
    json!({"type": "custom", "custom": Value::Object(custom)})
}

fn parse_tool_search(
    source_tool_index: usize,
    tool: &CanonicalToolDefinition,
) -> Result<ToolCandidate, ToolLoweringError> {
    let location = format!("tools[{source_tool_index}]");
    let Some(raw) = raw_responses_tool(tool) else {
        return Err(ToolLoweringError::new(
            location,
            "tool_search tool requires its raw Responses declaration",
        ));
    };
    if !object_has_only_keys(raw, &["type", "execution", "description", "parameters"]) {
        return Err(ToolLoweringError::new(
            format!("{location}.type"),
            "tool_search declaration has fields this gateway cannot audit",
        ));
    }
    let execution = raw
        .get("execution")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if !execution.eq_ignore_ascii_case("client") {
        return Err(ToolLoweringError::new(
            format!("{location}.execution"),
            "only client-executed tool_search has a Chat/Claude equivalent; hosted tool search remains unsupported",
        ));
    }
    let description = optional_string(raw.get("description"), &format!("{location}.description"))?;
    let parameters = optional_object(raw.get("parameters"), &format!("{location}.parameters"))?;
    Ok(ToolCandidate {
        identity: ToolIdentity::flat(
            LoweredToolKind::ToolSearch,
            TOOL_SEARCH_FUNCTION_NAME.to_string(),
        ),
        description,
        parameters,
        strict: None,
        source_tool_index: Some(source_tool_index),
        source_child_index: None,
        native_custom_chat: None,
        deferred: false,
        discovered: false,
        lowered_as_function: false,
        location,
    })
}

fn parse_namespace_tool(
    source_tool_index: usize,
    tool: &CanonicalToolDefinition,
) -> Result<Vec<ToolCandidate>, ToolLoweringError> {
    let location = format!("tools[{source_tool_index}]");
    let Some(raw) = raw_responses_tool(tool) else {
        return Err(ToolLoweringError::new(
            location,
            "Responses namespace tool requires its raw declaration",
        ));
    };
    if !object_has_only_keys(raw, &["type", "name", "description", "tools"]) {
        return Err(ToolLoweringError::new(
            location,
            "Responses namespace tool has fields this gateway cannot audit",
        ));
    }
    let namespace = non_empty_string(raw.get("name"))
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            ToolLoweringError::new(
                format!("{location}.name"),
                "Responses namespace requires a non-empty name",
            )
        })?;
    if !matches!(raw.get("description"), Some(Value::String(_))) {
        return Err(ToolLoweringError::new(
            format!("{location}.description"),
            "Responses namespace requires a description",
        ));
    }
    let children = raw
        .get("tools")
        .and_then(Value::as_array)
        .filter(|children| !children.is_empty())
        .ok_or_else(|| {
            ToolLoweringError::new(
                format!("{location}.tools"),
                "Responses namespace requires a non-empty tools array",
            )
        })?;
    let normalized_namespace = normalize_namespace(&namespace);
    let mut child_names = BTreeSet::new();
    let mut parsed = Vec::with_capacity(children.len());
    for (source_child_index, child) in children.iter().enumerate() {
        let child_location = format!("{location}.tools[{source_child_index}]");
        let child = child.as_object().ok_or_else(|| {
            ToolLoweringError::new(child_location.clone(), "namespace child must be an object")
        })?;
        let child_type = child
            .get("type")
            .and_then(Value::as_str)
            .map(|tool_type| tool_type.trim().to_ascii_lowercase())
            .ok_or_else(|| {
                ToolLoweringError::new(
                    format!("{child_location}.type"),
                    "namespace child requires a function or custom type",
                )
            })?;
        let name = non_empty_string(child.get("name"))
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                ToolLoweringError::new(
                    format!("{child_location}.name"),
                    "namespace child requires a non-empty name",
                )
            })?;
        if !child_names.insert(name.clone()) {
            return Err(ToolLoweringError::new(
                format!("{child_location}.name"),
                "namespace child names must be unique",
            ));
        }
        let deferred = optional_bool(
            child.get("defer_loading"),
            &format!("{child_location}.defer_loading"),
        )?
        .unwrap_or(false);
        let identity_namespace = normalized_namespace.clone();
        let (kind, description, parameters, strict, lowered_as_function) = match child_type.as_str()
        {
            "function" => {
                if !object_has_only_keys(
                    child,
                    &[
                        "type",
                        "name",
                        "description",
                        "parameters",
                        "strict",
                        "defer_loading",
                    ],
                ) {
                    return Err(ToolLoweringError::new(
                        child_location.clone(),
                        "namespace function child has fields this gateway cannot audit",
                    ));
                }
                let description = optional_string(
                    child.get("description"),
                    &format!("{child_location}.description"),
                )?;
                let parameters = optional_object(
                    child.get("parameters"),
                    &format!("{child_location}.parameters"),
                )?;
                let strict =
                    optional_bool_value(child.get("strict"), &format!("{child_location}.strict"))?;
                (
                    LoweredToolKind::Function,
                    description,
                    parameters,
                    strict,
                    false,
                )
            }
            "custom" => {
                if !object_has_only_keys(
                    child,
                    &["type", "name", "description", "format", "defer_loading"],
                ) {
                    return Err(ToolLoweringError::new(
                        child_location.clone(),
                        "namespace custom child has fields this gateway cannot audit",
                    ));
                }
                validate_custom_text_format(
                    child.get("format"),
                    &format!("{child_location}.format"),
                )?;
                let description = optional_string(
                    child.get("description"),
                    &format!("{child_location}.description"),
                )?;
                (
                    LoweredToolKind::CustomText,
                    custom_tool_description(description.as_deref()),
                    Some(custom_tool_input_schema()),
                    None,
                    true,
                )
            }
            other => {
                return Err(ToolLoweringError::new(
                    format!("{child_location}.type"),
                    format!("namespace child type {other:?} has no Chat/Claude equivalent"),
                ));
            }
        };
        parsed.push(ToolCandidate {
            identity: ToolIdentity {
                kind,
                namespace: identity_namespace,
                name,
            },
            description,
            parameters,
            strict,
            source_tool_index: Some(source_tool_index),
            source_child_index: Some(source_child_index),
            native_custom_chat: None,
            deferred,
            discovered: false,
            lowered_as_function,
            location: child_location,
        });
    }
    Ok(parsed)
}

fn parse_discovered_tool_specs(specs: &[Value]) -> Result<Vec<ToolCandidate>, ToolLoweringError> {
    let mut parsed = Vec::new();
    for (index, spec) in specs.iter().enumerate() {
        let location = format!("tool_search_output.tools[{index}]");
        parsed.extend(parse_discovered_tool_spec(spec, &location)?);
    }
    Ok(parsed)
}

fn discovered_identities_in_spec(spec: &Value) -> Result<Vec<ToolIdentity>, ToolLoweringError> {
    Ok(
        parse_discovered_tool_spec(spec, "tool_search_output.tools")?
            .into_iter()
            .map(|candidate| candidate.identity)
            .collect(),
    )
}

fn parse_discovered_tool_spec(
    spec: &Value,
    location: &str,
) -> Result<Vec<ToolCandidate>, ToolLoweringError> {
    let Some(object) = spec.as_object() else {
        return Err(ToolLoweringError::new(
            location,
            "discovered tool spec must be an object",
        ));
    };
    let spec_type = object
        .get("type")
        .and_then(Value::as_str)
        .map(|tool_type| tool_type.trim().to_ascii_lowercase())
        .ok_or_else(|| {
            ToolLoweringError::new(
                format!("{location}.type"),
                "discovered tool spec requires a type",
            )
        })?;
    match spec_type.as_str() {
        "function" => {
            if !object_has_only_keys(
                object,
                &[
                    "type",
                    "name",
                    "description",
                    "parameters",
                    "strict",
                    "defer_loading",
                ],
            ) {
                return Err(ToolLoweringError::new(
                    location,
                    "discovered function spec has fields this gateway cannot audit",
                ));
            }
            let name = discovered_flat_name(object, location)?;
            let candidate = flat_discovered_candidate(
                object,
                location,
                LoweredToolKind::Function,
                name,
                false,
            )?;
            Ok(vec![candidate])
        }
        "custom" => {
            if !object_has_only_keys(
                object,
                &["type", "name", "description", "format", "defer_loading"],
            ) {
                return Err(ToolLoweringError::new(
                    location,
                    "discovered custom spec has fields this gateway cannot audit",
                ));
            }
            validate_custom_text_format(object.get("format"), &format!("{location}.format"))?;
            let name = discovered_flat_name(object, location)?;
            let mut candidate = flat_discovered_candidate(
                object,
                location,
                LoweredToolKind::CustomText,
                name,
                true,
            )?;
            candidate.parameters = Some(custom_tool_input_schema());
            Ok(vec![candidate])
        }
        "namespace" => {
            if !object_has_only_keys(object, &["type", "name", "description", "tools"]) {
                return Err(ToolLoweringError::new(
                    location,
                    "discovered namespace spec has fields this gateway cannot audit",
                ));
            }
            let namespace = non_empty_string(object.get("name"))
                .map(ToOwned::to_owned)
                .ok_or_else(|| {
                    ToolLoweringError::new(
                        format!("{location}.name"),
                        "discovered namespace requires a non-empty name",
                    )
                })?;
            optional_string(
                object.get("description"),
                &format!("{location}.description"),
            )?;
            let children = object
                .get("tools")
                .and_then(Value::as_array)
                .filter(|children| !children.is_empty())
                .ok_or_else(|| {
                    ToolLoweringError::new(
                        format!("{location}.tools"),
                        "discovered namespace requires a non-empty tools array",
                    )
                })?;
            let normalized_namespace = normalize_namespace(&namespace);
            let mut child_names = BTreeSet::new();
            let mut parsed = Vec::new();
            for (child_index, child) in children.iter().enumerate() {
                let child_location = format!("{location}.tools[{child_index}]");
                let child = child.as_object().ok_or_else(|| {
                    ToolLoweringError::new(
                        child_location.clone(),
                        "discovered namespace child must be an object",
                    )
                })?;
                let child_type = child
                    .get("type")
                    .and_then(Value::as_str)
                    .map(|tool_type| tool_type.trim().to_ascii_lowercase())
                    .ok_or_else(|| {
                        ToolLoweringError::new(
                            format!("{child_location}.type"),
                            "discovered namespace child requires a function or custom type",
                        )
                    })?;
                let name = non_empty_string(child.get("name"))
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| {
                        ToolLoweringError::new(
                            format!("{child_location}.name"),
                            "discovered namespace child requires a non-empty name",
                        )
                    })?;
                if !child_names.insert(name.clone()) {
                    return Err(ToolLoweringError::new(
                        format!("{child_location}.name"),
                        "discovered namespace child names must be unique",
                    ));
                }
                let (kind, description, parameters, strict, lowered_as_function) = match child_type
                    .as_str()
                {
                    "function" => {
                        if !object_has_only_keys(
                            child,
                            &[
                                "type",
                                "name",
                                "description",
                                "parameters",
                                "strict",
                                "defer_loading",
                            ],
                        ) {
                            return Err(ToolLoweringError::new(
                                    child_location.clone(),
                                    "discovered namespace function child has fields this gateway cannot audit",
                                ));
                        }
                        (
                            LoweredToolKind::Function,
                            optional_string(
                                child.get("description"),
                                &format!("{child_location}.description"),
                            )?,
                            optional_object(
                                child.get("parameters"),
                                &format!("{child_location}.parameters"),
                            )?,
                            optional_bool_value(
                                child.get("strict"),
                                &format!("{child_location}.strict"),
                            )?,
                            false,
                        )
                    }
                    "custom" => {
                        if !object_has_only_keys(
                            child,
                            &["type", "name", "description", "format", "defer_loading"],
                        ) {
                            return Err(ToolLoweringError::new(
                                    child_location.clone(),
                                    "discovered namespace custom child has fields this gateway cannot audit",
                                ));
                        }
                        validate_custom_text_format(
                            child.get("format"),
                            &format!("{child_location}.format"),
                        )?;
                        (
                            LoweredToolKind::CustomText,
                            custom_tool_description(
                                optional_string(
                                    child.get("description"),
                                    &format!("{child_location}.description"),
                                )?
                                .as_deref(),
                            ),
                            Some(custom_tool_input_schema()),
                            None,
                            true,
                        )
                    }
                    other => {
                        return Err(ToolLoweringError::new(
                                format!("{child_location}.type"),
                                format!(
                                    "discovered namespace child type {other:?} has no Chat/Claude equivalent"
                                ),
                            ));
                    }
                };
                parsed.push(ToolCandidate {
                    identity: ToolIdentity {
                        kind,
                        namespace: normalized_namespace.clone(),
                        name,
                    },
                    description,
                    parameters,
                    strict,
                    source_tool_index: None,
                    source_child_index: Some(child_index),
                    native_custom_chat: None,
                    deferred: false,
                    discovered: true,
                    lowered_as_function,
                    location: child_location,
                });
            }
            Ok(parsed)
        }
        other => Err(ToolLoweringError::new(
            format!("{location}.type"),
            format!("discovered tool type {other:?} has no Chat/Claude equivalent"),
        )),
    }
}

fn discovered_flat_name(
    object: &Map<String, Value>,
    location: &str,
) -> Result<String, ToolLoweringError> {
    let name = non_empty_string(object.get("name"))
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            ToolLoweringError::new(
                format!("{location}.name"),
                "discovered tool requires a non-empty name",
            )
        })?;
    if !is_valid_chat_tool_name(&name) {
        return Err(ToolLoweringError::new(
            format!("{location}.name"),
            "discovered tool name cannot be offered as a Chat/Claude function name",
        ));
    }
    Ok(name)
}

fn flat_discovered_candidate(
    object: &Map<String, Value>,
    location: &str,
    kind: LoweredToolKind,
    name: String,
    lowered_as_function: bool,
) -> Result<ToolCandidate, ToolLoweringError> {
    let description = optional_string(
        object.get("description"),
        &format!("{location}.description"),
    )?;
    Ok(ToolCandidate {
        identity: ToolIdentity::flat(kind, name),
        description: if kind == LoweredToolKind::CustomText {
            custom_tool_description(description.as_deref())
        } else {
            description
        },
        parameters: optional_object(object.get("parameters"), &format!("{location}.parameters"))?,
        strict: optional_bool_value(object.get("strict"), &format!("{location}.strict"))?,
        source_tool_index: None,
        source_child_index: None,
        native_custom_chat: None,
        deferred: false,
        discovered: true,
        lowered_as_function,
        location: location.to_string(),
    })
}

fn normalize_namespace(namespace: &str) -> Option<String> {
    let namespace = namespace.trim();
    if namespace.is_empty() || namespace.eq_ignore_ascii_case(RESPONSES_DEFAULT_TOOL_NAMESPACE) {
        return None;
    }
    Some(namespace.to_string())
}

fn custom_tool_description(description: Option<&str>) -> Option<String> {
    let base = description.map(str::trim).filter(|base| !base.is_empty());
    Some(match base {
        Some(base) => format!("{base}\n\n{CUSTOM_TOOL_INPUT_INSTRUCTION}"),
        None => CUSTOM_TOOL_INPUT_INSTRUCTION.to_string(),
    })
}

fn validate_custom_text_format(
    format: Option<&Value>,
    location: &str,
) -> Result<(), ToolLoweringError> {
    let Some(format) = format else {
        return Ok(());
    };
    if format.is_null() {
        return Ok(());
    }
    let Some(object) = format.as_object() else {
        return Err(ToolLoweringError::new(
            location,
            "custom tool format must be an object",
        ));
    };
    let format_type = object
        .get("type")
        .and_then(Value::as_str)
        .map(|format_type| format_type.trim().to_ascii_lowercase());
    match format_type.as_deref() {
        Some("text") if object_has_only_keys(object, &["type"]) => Ok(()),
        Some("text") => Err(ToolLoweringError::new(
            location,
            "custom text format has fields this gateway cannot audit",
        )),
        Some(other) => Err(ToolLoweringError::new(
            location,
            format!("custom tool format {other:?} has no Chat/Claude text equivalent"),
        )),
        None => Err(ToolLoweringError::new(
            location,
            "custom tool format requires a type",
        )),
    }
}

fn chat_function_tool(
    name: &str,
    description: Option<&str>,
    parameters: Option<&Value>,
    strict: Option<&Value>,
) -> Value {
    let mut function = Map::new();
    function.insert("name".to_string(), Value::String(name.to_string()));
    if let Some(description) = description {
        function.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
    if let Some(parameters) = parameters {
        function.insert("parameters".to_string(), parameters.clone());
    }
    if let Some(strict) = strict {
        function.insert("strict".to_string(), strict.clone());
    }
    json!({
        "type": "function",
        "function": Value::Object(function),
    })
}

fn claude_function_tool(
    name: &str,
    description: Option<&str>,
    parameters: Option<&Value>,
) -> Value {
    let mut tool = Map::new();
    tool.insert("name".to_string(), Value::String(name.to_string()));
    if let Some(description) = description {
        tool.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
    tool.insert("input_schema".to_string(), claude_input_schema(parameters));
    Value::Object(tool)
}

fn claude_input_schema(parameters: Option<&Value>) -> Value {
    match parameters {
        Some(Value::Object(schema)) => {
            let mut schema = schema.clone();
            schema
                .entry("type".to_string())
                .or_insert_with(|| Value::String("object".to_string()));
            schema
                .entry("properties".to_string())
                .or_insert_with(|| json!({}));
            Value::Object(schema)
        }
        _ => json!({"type": "object", "properties": {}}),
    }
}

fn optional_bool(value: Option<&Value>, location: &str) -> Result<Option<bool>, ToolLoweringError> {
    match value {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(ToolLoweringError::new(location, "value must be a boolean")),
        None => Ok(None),
    }
}

fn optional_bool_value(
    value: Option<&Value>,
    location: &str,
) -> Result<Option<Value>, ToolLoweringError> {
    match value {
        Some(value @ (Value::Bool(_) | Value::Null)) => Ok(Some(value.clone())),
        Some(_) => Err(ToolLoweringError::new(location, "value must be a boolean")),
        None => Ok(None),
    }
}

fn optional_string(
    value: Option<&Value>,
    location: &str,
) -> Result<Option<String>, ToolLoweringError> {
    match value {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(ToolLoweringError::new(location, "value must be a string")),
    }
}

fn optional_object(
    value: Option<&Value>,
    location: &str,
) -> Result<Option<Value>, ToolLoweringError> {
    match value {
        Some(value @ Value::Object(_)) => Ok(Some(value.clone())),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(ToolLoweringError::new(location, "value must be an object")),
    }
}

fn client_function_tool_names(tools: &Value) -> BTreeSet<String> {
    tools
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter(|tool| {
            tool.get("type")
                .and_then(Value::as_str)
                .map(str::trim)
                .is_none_or(|tool_type| {
                    tool_type.eq_ignore_ascii_case("function")
                        || tool_type.eq_ignore_ascii_case("custom")
                })
        })
        .filter_map(|tool| {
            non_empty_string(tool.get("name")).or_else(|| {
                ["function", "custom"]
                    .iter()
                    .find_map(|key| non_empty_string(tool.get(*key)?.get("name")))
            })
        })
        .map(ToOwned::to_owned)
        .collect()
}

fn raw_responses_tool(tool: &CanonicalToolDefinition) -> Option<&Map<String, Value>> {
    tool.extensions
        .get(OPENAI_RESPONSES_EXTENSION_NAMESPACE)
        .or_else(|| {
            tool.extensions
                .get(OPENAI_RESPONSES_LEGACY_EXTENSION_NAMESPACE)
        })
        .and_then(Value::as_object)
}

fn object_has_only_keys(object: &Map<String, Value>, allowed: &[&str]) -> bool {
    object.keys().all(|key| allowed.contains(&key.as_str()))
}

fn non_empty_string(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn is_valid_chat_tool_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= OPENAI_CHAT_TOOL_NAME_MAX_LEN
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn allocate_hashed_alias(namespace: &str, child_name: &str, used: &BTreeSet<String>) -> String {
    for nonce in 0_u64.. {
        let candidate = hashed_alias(namespace, child_name, nonce);
        if !used.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("u64 alias nonce space cannot be exhausted")
}

fn sanitize_chat_name_component(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-') {
                char::from(byte)
            } else {
                '_'
            }
        })
        .collect()
}

fn hashed_alias(namespace: &str, child_name: &str, nonce: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"aether-openai-namespace-tool\0");
    hasher.update(namespace.as_bytes());
    hasher.update(b"\0");
    hasher.update(child_name.as_bytes());
    hasher.update(b"\0");
    hasher.update(nonce.to_le_bytes());
    let digest = hasher.finalize();
    let digest_hex = digest
        .iter()
        .take(HASH_HEX_LEN / 2)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let semantic_budget =
        OPENAI_CHAT_TOOL_NAME_MAX_LEN - HASHED_ALIAS_PREFIX.len() - 1 - digest_hex.len();
    let mut semantic = sanitize_chat_name_component(child_name);
    semantic.truncate(semantic_budget);
    if semantic.is_empty() {
        semantic.push_str("tool");
        semantic.truncate(semantic_budget);
    }
    format!("{HASHED_ALIAS_PREFIX}{semantic}_{digest_hex}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn aliases(tools: Value) -> NamespaceToolAliases {
        let canonical = openai_responses_tools_to_canonical(Some(&tools))
            .expect("Responses tools should parse");
        NamespaceToolAliases::from_canonical_tools(&canonical)
    }

    fn discovered_message(specs: Value) -> crate::protocol::canonical::CanonicalMessage {
        crate::protocol::canonical::CanonicalMessage {
            role: crate::protocol::canonical::CanonicalRole::Tool,
            content: vec![CanonicalContentBlock::ToolResult {
                tool_use_id: "call_search".to_string(),
                name: Some(TOOL_SEARCH_FUNCTION_NAME.to_string()),
                output: Some(specs),
                content_text: None,
                is_error: false,
                extensions: BTreeMap::from([(
                    OPENAI_RESPONSES_EXTENSION_NAMESPACE.to_string(),
                    json!({"item_type": "tool_search_output", "execution": "client"}),
                )]),
            }],
            extensions: BTreeMap::new(),
        }
    }

    fn request_with(tools: Value, discovered: Option<Value>) -> CanonicalRequest {
        let mut request = CanonicalRequest::default();
        request.tools = openai_responses_tools_to_canonical(Some(&tools))
            .expect("Responses tools should parse");
        if let Some(discovered) = discovered {
            request.messages.push(discovered_message(discovered));
        }
        request
    }

    fn lowering(
        tools: Value,
        discovered: Option<Value>,
        target: ToolLoweringTarget,
    ) -> NamespaceToolAliases {
        NamespaceToolAliases::from_canonical_request(&request_with(tools, discovered), target)
            .expect("tool inventory should lower")
    }

    fn lowering_error(tools: Value, discovered: Option<Value>) -> ToolLoweringError {
        NamespaceToolAliases::from_canonical_request(
            &request_with(tools, discovered),
            ToolLoweringTarget::OpenAiChat,
        )
        .expect_err("tool inventory must fail closed")
    }

    #[test]
    fn namespace_aliases_keep_unique_child_names_and_reverse_them() {
        let aliases = aliases(json!([{
            "type": "namespace",
            "name": "mcp__reports",
            "description": "Reporting tools",
            "tools": [{
                "type": "function",
                "name": "vulnerability_report",
                "parameters": {"type": "object", "properties": {}}
            }]
        }]));

        assert_eq!(
            aliases.chat_name("mcp__reports", "vulnerability_report"),
            Some("vulnerability_report")
        );
        assert_eq!(
            aliases.responses_name("vulnerability_report"),
            Some(("mcp__reports", "vulnerability_report"))
        );
        assert_eq!(
            aliases.restore_tool_call("vulnerability_report"),
            ResponsesToolRestore::NamespaceChild {
                namespace: "mcp__reports".to_string(),
                name: "vulnerability_report".to_string(),
            }
        );
    }

    fn hosted_web_search(tools: Value, name: &str) -> bool {
        NamespaceToolAliases::from_report_context(&json!({
            "original_request_body": {"tools": tools}
        }))
        .emits_hosted_web_search_call(name)
    }

    #[test]
    fn hosted_web_search_call_is_reserved_for_undeclared_search_names() {
        let schema = json!({"type": "object", "properties": {"query": {"type": "string"}}});
        let cases = [
            ("no tools", json!([]), "web_search", true),
            (
                "hosted tool",
                json!([{"type": "web_search"}]),
                "web_search",
                true,
            ),
            (
                "hosted preview tool",
                json!([{"type": "web_search_preview"}]),
                "web_search_preview",
                true,
            ),
            (
                "function tool",
                json!([{"type": "function", "name": "web_search", "parameters": schema}]),
                "web_search",
                false,
            ),
            (
                "function tool named preview",
                json!([{"type": "function", "name": "web_search_preview", "parameters": schema}]),
                "web_search_preview",
                false,
            ),
            (
                "custom tool",
                json!([{"type": "custom", "name": "web_search"}]),
                "web_search",
                false,
            ),
            (
                "tool without type",
                json!([{"name": "web_search", "parameters": schema}]),
                "web_search",
                false,
            ),
            (
                "chat-shaped function tool",
                json!([{"type": "function", "function": {"name": "web_search", "parameters": schema}}]),
                "web_search",
                false,
            ),
            (
                "chat-shaped custom tool",
                json!([{"type": "custom", "custom": {"name": "web_search"}}]),
                "web_search",
                false,
            ),
            (
                "hosted and function tool together",
                json!([
                    {"type": "web_search"},
                    {"type": "function", "name": "web_search", "parameters": schema}
                ]),
                "web_search",
                false,
            ),
            (
                "unrelated function tool",
                json!([{"type": "function", "name": "lookup", "parameters": schema}]),
                "web_search",
                true,
            ),
            (
                "non-search name",
                json!([{"type": "web_search"}]),
                "lookup",
                false,
            ),
        ];

        for (label, tools, name, expected) in cases {
            assert_eq!(hosted_web_search(tools, name), expected, "{label}");
        }
    }

    #[test]
    fn namespaced_web_search_child_is_not_a_hosted_web_search_call() {
        assert!(!hosted_web_search(
            json!([{
                "type": "namespace",
                "name": "mcp__search",
                "description": "Search tools",
                "tools": [{
                    "type": "function",
                    "name": "web_search",
                    "parameters": {"type": "object", "properties": {}}
                }]
            }]),
            "web_search"
        ));
    }

    #[test]
    fn namespace_aliases_are_unique_bounded_and_prefix_safe() {
        let long_namespace = format!("namespace__{}", "n".repeat(120));
        let long_child = format!("aether_ns__{}", "c".repeat(120));
        let tools = json!([
            {
                "type": "function",
                "name": "shared",
                "parameters": {"type": "object"}
            },
            {
                "type": "namespace",
                "name": "first__namespace",
                "description": "First namespace",
                "tools": [{
                    "type": "function",
                    "name": "shared",
                    "parameters": {"type": "object"}
                }]
            },
            {
                "type": "namespace",
                "name": "second__namespace",
                "description": "Second namespace",
                "tools": [{
                    "type": "function",
                    "name": "shared",
                    "parameters": {"type": "object"}
                }]
            },
            {
                "type": "namespace",
                "name": long_namespace,
                "description": "Long namespace",
                "tools": [{
                    "type": "function",
                    "name": long_child,
                    "parameters": {"type": "object"}
                }]
            }
        ]);
        let aliases = aliases(tools);
        let names = aliases
            .tools
            .iter()
            .map(|tool| tool.chat_name.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(names.len(), 3);
        assert!(!names.contains("shared"));
        assert!(names.iter().all(|name| is_valid_chat_tool_name(name)));
        assert!(names.iter().all(|name| name.len() <= 64));
        for tool in &aliases.tools {
            assert_eq!(
                aliases.responses_name(&tool.chat_name),
                Some((tool.namespace.as_str(), tool.name.as_str()))
            );
        }
    }

    #[test]
    fn namespace_aliases_are_stable_across_tool_order_and_readable_collisions() {
        let first = json!([
            {
                "type": "function",
                "name": "b__c",
                "parameters": {"type": "object"}
            },
            {
                "type": "function",
                "name": "c",
                "parameters": {"type": "object"}
            },
            {
                "type": "namespace",
                "name": "a",
                "description": "A",
                "tools": [{
                    "type": "function",
                    "name": "b__c",
                    "parameters": {"type": "object"}
                }]
            },
            {
                "type": "namespace",
                "name": "a__b",
                "description": "AB",
                "tools": [{
                    "type": "function",
                    "name": "c",
                    "parameters": {"type": "object"}
                }]
            }
        ]);
        let second = json!([
            first[3].clone(),
            first[2].clone(),
            first[1].clone(),
            first[0].clone()
        ]);
        let first = aliases(first);
        let second = aliases(second);

        for identity in [("a", "b__c"), ("a__b", "c")] {
            let first_alias = first
                .chat_name(identity.0, identity.1)
                .expect("first alias");
            let second_alias = second
                .chat_name(identity.0, identity.1)
                .expect("second alias");
            assert_eq!(first_alias, second_alias);
            assert!(first_alias.starts_with(HASHED_ALIAS_PREFIX));
        }
    }

    #[test]
    fn namespace_children_preserve_nullable_fields_without_inheriting_parent_description() {
        let aliases = aliases(json!([{
            "type": "namespace",
            "name": "reports",
            "description": "Parent description",
            "tools": [{
                "type": "function",
                "name": "write_report",
                "description": null,
                "parameters": null,
                "strict": null
            }]
        }]));
        let chat_tool = aliases
            .tools_for_source(0)
            .next()
            .expect("namespace child")
            .to_openai_chat_tool();

        assert!(chat_tool["function"].get("description").is_none());
        assert!(chat_tool["function"].get("parameters").is_none());
        assert_eq!(chat_tool["function"]["strict"], Value::Null);
    }

    #[test]
    fn malformed_namespace_is_not_representable() {
        for raw in [
            json!({"type": "namespace", "name": "broken"}),
            json!({
                "type": "namespace",
                "name": "broken",
                "tools": [{"type": "function", "name": "missing_parent_description"}]
            }),
            json!({
                "type": "namespace",
                "name": "broken",
                "description": "Broken",
                "tools": {}
            }),
            json!({
                "type": "namespace",
                "name": "broken",
                "description": "Broken",
                "tools": [{"type": "function"}]
            }),
            json!({
                "type": "namespace",
                "name": "broken",
                "description": "Broken",
                "tools": [{
                    "type": "custom",
                    "name": "raw",
                    "format": {"type": "grammar", "syntax": "lark", "definition": "start: .*"}
                }]
            }),
            json!({
                "type": "namespace",
                "name": "broken",
                "description": "Broken",
                "tools": [{"name": "missing_type"}]
            }),
            json!({
                "type": "namespace",
                "name": "broken",
                "description": "Broken",
                "tools": [{
                    "type": "function",
                    "function": {"name": "nested"}
                }]
            }),
            json!({
                "type": "namespace",
                "name": "broken",
                "description": "Broken",
                "tools": [{
                    "type": "namespace",
                    "name": "nested",
                    "description": "Nested",
                    "tools": [{"type": "function", "name": "deep"}]
                }]
            }),
        ] {
            let aliases = aliases(json!([raw]));
            assert!(aliases.has_invalid_namespace_tools());
            assert!(aliases.is_namespace_tool(0));
            // An invalid namespace is never audited: the registry keeps its
            // generic extension validation for it.
            assert!(!aliases.tool_is_audited(0));
        }
    }

    #[test]
    fn deferred_function_is_hidden_until_discovered_and_materialized_once() {
        let tools = json!([
            {"type": "tool_search", "execution": "client", "description": "Find tools"},
            {
                "type": "function",
                "name": "hidden_lookup",
                "description": "Deferred lookup",
                "parameters": {"type": "object", "properties": {}},
                "defer_loading": true
            }
        ]);
        let undiscovered = lowering(tools.clone(), None, ToolLoweringTarget::OpenAiChat);
        assert_eq!(
            undiscovered.lowered_tools_for_source(ToolLoweringTarget::OpenAiChat, 1),
            Some(Vec::new())
        );
        assert!(undiscovered
            .discovered_tools(ToolLoweringTarget::OpenAiChat)
            .is_empty());
        assert_eq!(
            undiscovered.lowered_tools_for_source(ToolLoweringTarget::OpenAiChat, 0),
            Some(vec![json!({
                "type": "function",
                "function": {
                    "name": "tool_search",
                    "description": "Find tools",
                }
            })])
        );

        let discovered = json!([{
            "type": "function",
            "name": "hidden_lookup",
            "description": "Deferred lookup",
            "parameters": {"type": "object", "properties": {}},
        }, {
            "type": "function",
            "name": "brand_new",
            "description": "Discovered later",
            "parameters": {"type": "object", "properties": {"q": {"type": "string"}}},
        }]);
        let after = lowering(tools, Some(discovered), ToolLoweringTarget::OpenAiChat);
        // Declared deferred tools keep their declared declaration and are the
        // caller's passthrough responsibility once offered.
        assert_eq!(
            after.lowered_tools_for_source(ToolLoweringTarget::OpenAiChat, 1),
            None
        );
        assert_eq!(
            after.discovered_tools(ToolLoweringTarget::OpenAiChat),
            vec![json!({
                "type": "function",
                "function": {
                    "name": "brand_new",
                    "description": "Discovered later",
                    "parameters": {"type": "object", "properties": {"q": {"type": "string"}}},
                }
            })]
        );
        assert_eq!(
            after.restore_tool_call("tool_search"),
            ResponsesToolRestore::ClientToolSearch
        );
    }

    #[test]
    fn deferred_tool_without_client_tool_search_fails_closed() {
        let error = lowering_error(
            json!([{
                "type": "function",
                "name": "hidden",
                "defer_loading": true
            }]),
            None,
        );
        assert!(error.location().ends_with("defer_loading"), "{error}");
    }

    #[test]
    fn deferred_namespace_child_is_hidden_until_discovered() {
        let tools = json!([
            {"type": "tool_search", "execution": "client"},
            {
                "type": "namespace",
                "name": "mcp__reports",
                "description": "Reporting tools",
                "tools": [{
                    "type": "function",
                    "name": "write_report",
                    "parameters": {"type": "object", "properties": {}},
                    "defer_loading": true
                }]
            }
        ]);
        let undiscovered = lowering(tools.clone(), None, ToolLoweringTarget::ClaudeMessages);
        assert_eq!(
            undiscovered.lowered_tools_for_source(ToolLoweringTarget::ClaudeMessages, 1),
            Some(Vec::new())
        );
        assert!(undiscovered.tools_for_source(1).next().is_none());

        let discovered = json!([{
            "type": "namespace",
            "name": "mcp__reports",
            "description": "Reporting tools",
            "tools": [{
                "type": "function",
                "name": "write_report",
                "defer_loading": true
            }]
        }]);
        let after = lowering(tools, Some(discovered), ToolLoweringTarget::ClaudeMessages);
        let child = after.tools_for_source(1).next().expect("offered child");
        assert_eq!(child.chat_name, "write_report");
        assert_eq!(
            after.lowered_tools_for_source(ToolLoweringTarget::ClaudeMessages, 1),
            Some(vec![json!({
                "name": "write_report",
                "input_schema": {"type": "object", "properties": {}},
            })])
        );
    }

    #[test]
    fn default_functions_namespace_flattens_to_a_plain_identity() {
        let discovered = json!([{
            "type": "namespace",
            "name": "functions",
            "description": "Default namespace",
            "tools": [{
                "type": "function",
                "name": "get_shipping_eta",
                "description": "Shipping estimate",
                "parameters": {"type": "object", "properties": {"order": {"type": "string"}}},
                "defer_loading": true
            }]
        }]);
        let lowered = lowering(
            json!([{"type": "tool_search", "execution": "client"}]),
            Some(discovered),
            ToolLoweringTarget::ClaudeMessages,
        );
        assert!(lowered.tools_for_source(0).next().is_none());
        assert_eq!(
            lowered.discovered_tools(ToolLoweringTarget::ClaudeMessages),
            vec![json!({
                "name": "get_shipping_eta",
                "description": "Shipping estimate",
                "input_schema": {"type": "object", "properties": {"order": {"type": "string"}}},
            })]
        );
        assert_eq!(
            lowered.restore_tool_call("get_shipping_eta"),
            ResponsesToolRestore::Unchanged
        );
    }

    #[test]
    fn namespace_custom_child_is_lowered_to_a_text_function() {
        let tools = json!([{
            "type": "namespace",
            "name": "mcp__files",
            "description": "File tools",
            "tools": [{
                "type": "custom",
                "name": "apply_patch",
                "description": "Apply a patch"
            }]
        }]);
        let lowered = lowering(tools, None, ToolLoweringTarget::ClaudeMessages);
        let tool = lowered.tools_for_source(0).next().expect("custom child");
        assert_eq!(tool.chat_name, "apply_patch");
        assert!(lowered.custom_tool_is_function_lowered(Some("mcp__files"), "apply_patch"));
        let provider_tool = lowered
            .lowered_tools_for_source(ToolLoweringTarget::ClaudeMessages, 0)
            .expect("lowered custom child")
            .pop()
            .expect("custom tool");
        assert_eq!(provider_tool["name"], "apply_patch");
        assert_eq!(provider_tool["input_schema"], custom_tool_input_schema());
        assert!(provider_tool["description"]
            .as_str()
            .is_some_and(|description| description.contains("Apply a patch")
                && description.contains(CUSTOM_TOOL_INPUT_INSTRUCTION)));
        assert_eq!(
            lowered.restore_tool_call("apply_patch"),
            ResponsesToolRestore::CustomText {
                namespace: Some("mcp__files".to_string()),
                name: "apply_patch".to_string(),
            }
        );
    }

    #[test]
    fn declared_top_level_custom_stays_native_for_chat_and_lowers_for_claude() {
        let tools = json!([{"type": "custom", "name": "apply_patch", "description": "Patch"}]);
        let chat = lowering(tools.clone(), None, ToolLoweringTarget::OpenAiChat);
        assert_eq!(
            chat.lowered_tools_for_source(ToolLoweringTarget::OpenAiChat, 0),
            Some(vec![json!({
                "type": "custom",
                "custom": {"name": "apply_patch", "description": "Patch"},
            })])
        );
        assert!(!chat.custom_tool_is_function_lowered(None, "apply_patch"));
        assert_eq!(
            chat.restore_tool_call("apply_patch"),
            ResponsesToolRestore::Unchanged
        );

        let claude = lowering(tools, None, ToolLoweringTarget::ClaudeMessages);
        assert!(claude.custom_tool_is_function_lowered(None, "apply_patch"));
        assert_eq!(
            claude.restore_tool_call("apply_patch"),
            ResponsesToolRestore::CustomText {
                namespace: None,
                name: "apply_patch".to_string(),
            }
        );
        assert_eq!(
            claude.lowered_tools_for_source(ToolLoweringTarget::ClaudeMessages, 0),
            Some(vec![json!({
                "name": "apply_patch",
                "description": format!("Patch\n\n{CUSTOM_TOOL_INPUT_INSTRUCTION}"),
                "input_schema": custom_tool_input_schema(),
            })])
        );
    }

    #[test]
    fn custom_grammar_and_hosted_search_fail_closed() {
        for tools in [
            json!([{
                "type": "custom",
                "name": "apply_patch",
                "format": {"type": "grammar", "syntax": "lark", "definition": "start: .*"}
            }]),
            json!([{"type": "tool_search", "execution": "server"}]),
            json!([{"type": "tool_search"}]),
            json!([{
                "type": "tool_search",
                "execution": "client",
                "future_field": true
            }]),
        ] {
            lowering_error(tools, None);
        }
    }

    #[test]
    fn unsupported_discovered_tool_types_and_shapes_fail_closed() {
        for discovered in [
            json!([{"type": "web_search"}]),
            json!([{"type": "tool_search", "execution": "client"}]),
            json!([{"type": "function", "name": "bad name"}]),
            json!([{
                "type": "custom",
                "name": "apply_patch",
                "format": {"type": "grammar", "syntax": "regex", "definition": ".*"}
            }]),
            json!([{"type": "namespace", "name": "ns", "tools": []}]),
        ] {
            lowering_error(
                json!([{"type": "tool_search", "execution": "client"}]),
                Some(discovered),
            );
        }
    }

    #[test]
    fn flat_provider_name_collisions_between_identities_fail_closed() {
        let error = lowering_error(
            json!([
                {"type": "tool_search", "execution": "client"},
                {"type": "function", "name": "shared"},
                {"type": "custom", "name": "shared"}
            ]),
            None,
        );
        assert!(error.reason().contains("shared"), "{error}");
    }

    #[test]
    fn tool_search_result_text_names_the_offered_aliases() {
        let tools = json!([
            {"type": "function", "name": "shared"},
            {
                "type": "namespace",
                "name": "mcp__reports",
                "description": "Reporting tools",
                "tools": [{"type": "function", "name": "shared"}]
            }
        ]);
        let lowered = lowering(tools, None, ToolLoweringTarget::OpenAiChat);
        let alias = lowered
            .chat_name("mcp__reports", "shared")
            .expect("alias")
            .to_string();
        let text = lowered
            .tool_search_output_text(&json!([{
                "type": "function",
                "name": "shared"
            }, {
                "type": "namespace",
                "name": "mcp__reports",
                "description": "Reporting tools",
                "tools": [{"type": "function", "name": "shared"}]
            }]))
            .expect("search result text");
        assert_eq!(
            text,
            format!("These tools are now available to call: shared, {alias}")
        );
        assert_eq!(
            lowered
                .tool_search_output_text(&json!([]))
                .expect("empty search result"),
            "No tools matched the search."
        );
    }

    #[test]
    fn tool_search_output_text_rejects_unsupported_specs() {
        let lowered = lowering(
            json!([{"type": "tool_search", "execution": "client"}]),
            None,
            ToolLoweringTarget::OpenAiChat,
        );
        assert_eq!(
            lowered.tool_search_output_text(&json!([{"type": "web_search"}])),
            None
        );
    }

    #[test]
    fn report_context_reads_discovery_and_the_provider_target() {
        let report_context = json!({
            "provider_api_format": "claude:messages",
            "original_request_body": {
                "tools": [{"type": "tool_search", "execution": "client"}],
                "input": [
                    {"type": "tool_search_call", "call_id": "call_1", "execution": "client", "arguments": {"goal": "files"}},
                    {
                        "type": "tool_search_output",
                        "call_id": "call_1",
                        "execution": "client",
                        "tools": [{
                            "type": "namespace",
                            "name": "functions",
                            "description": "Default namespace",
                            "tools": [{
                                "type": "custom",
                                "name": "apply_patch",
                                "description": "Patch",
                                "defer_loading": true
                            }]
                        }]
                    }
                ]
            }
        });
        let lowered = NamespaceToolAliases::from_report_context(&report_context);
        assert!(lowered.custom_tool_is_function_lowered(None, "apply_patch"));
        assert_eq!(
            lowered.restore_tool_call("apply_patch"),
            ResponsesToolRestore::CustomText {
                namespace: None,
                name: "apply_patch".to_string(),
            }
        );

        let chat_context = json!({
            "provider_api_format": "openai:chat",
            "original_request_body": {
                "tools": [{"type": "custom", "name": "apply_patch"}]
            }
        });
        let chat = NamespaceToolAliases::from_report_context(&chat_context);
        assert!(!chat.custom_tool_is_function_lowered(None, "apply_patch"));
        assert_eq!(
            chat.restore_tool_call("apply_patch"),
            ResponsesToolRestore::Unchanged
        );
    }

    #[test]
    fn report_context_merges_leading_additional_tools_for_alias_parity() {
        let report_context = json!({
            "provider_api_format": "openai:chat",
            "original_request_body": {
                "input": [
                    {
                        "type": "additional_tools",
                        "role": "developer",
                        "tools": [{
                            "type": "namespace",
                            "name": "mcp__reports",
                            "description": "Reporting tools",
                            "tools": [{"type": "function", "name": "write_report"}]
                        }]
                    },
                    {"type": "message", "role": "user", "content": "hello"}
                ],
                "tools": [{"type": "function", "name": "plain"}]
            }
        });
        let lowered = NamespaceToolAliases::from_report_context(&report_context);
        assert_eq!(
            lowered.chat_name("mcp__reports", "write_report"),
            Some("write_report")
        );
        assert_eq!(
            lowered.restore_tool_call("write_report"),
            ResponsesToolRestore::NamespaceChild {
                namespace: "mcp__reports".to_string(),
                name: "write_report".to_string(),
            }
        );
    }

    #[test]
    fn real_responses_body_discovery_lowers_after_request_parsing() {
        let body = json!({
            "model": "gpt-5.6",
            "tools": [{
                "type": "tool_search",
                "execution": "client",
                "description": "Find tools",
                "parameters": {
                    "type": "object",
                    "properties": {"goal": {"type": "string"}},
                    "required": ["goal"]
                }
            }],
            "input": [
                {
                    "type": "tool_search_call",
                    "call_id": "call_search",
                    "execution": "client",
                    "arguments": {"goal": "shipping"}
                },
                {
                    "type": "tool_search_output",
                    "call_id": "call_search",
                    "execution": "client",
                    "tools": [{
                        "type": "namespace",
                        "name": "functions",
                        "description": "Default namespace",
                        "tools": [{
                            "type": "function",
                            "name": "get_shipping_eta",
                            "description": "Shipping estimate",
                            "parameters": {"type": "object", "properties": {}},
                            "defer_loading": true
                        }]
                    }]
                },
                {
                    "type": "function_call",
                    "call_id": "call_eta",
                    "namespace": "functions",
                    "name": "get_shipping_eta",
                    "arguments": "{\"order\":\"42\"}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_eta",
                    "output": "two days"
                }
            ]
        });
        let request = crate::formats::openai::responses::request::from_raw(&body)
            .expect("Responses body should parse");
        let lowered =
            NamespaceToolAliases::from_canonical_request(&request, ToolLoweringTarget::OpenAiChat)
                .expect("client tool search inventory should lower");

        assert_eq!(
            lowered.restore_tool_call(TOOL_SEARCH_FUNCTION_NAME),
            ResponsesToolRestore::ClientToolSearch
        );
        assert_eq!(
            lowered.discovered_tools(ToolLoweringTarget::OpenAiChat),
            vec![json!({
                "type": "function",
                "function": {
                    "name": "get_shipping_eta",
                    "description": "Shipping estimate",
                    "parameters": {"type": "object", "properties": {}},
                }
            })]
        );
        // `functions` is the default namespace and must pair with flat history.
        assert_eq!(
            lowered.tool_use_provider_name(Some("functions"), "get_shipping_eta"),
            Ok("get_shipping_eta".to_string())
        );
        assert_eq!(
            lowered.restore_tool_call("get_shipping_eta"),
            ResponsesToolRestore::Unchanged
        );
    }

    #[test]
    fn tool_choice_names_resolve_unambiguously_across_namespaces() {
        let lowered = lowering(
            json!([
                {"type": "function", "name": "plain"},
                {
                    "type": "namespace",
                    "name": "crm",
                    "description": "CRM tools",
                    "tools": [
                        {"type": "function", "name": "write_report"},
                        {"type": "function", "name": "shared"}
                    ]
                },
                {
                    "type": "namespace",
                    "name": "erp",
                    "description": "ERP tools",
                    "tools": [{"type": "function", "name": "shared"}]
                }
            ]),
            None,
            ToolLoweringTarget::OpenAiChat,
        );
        assert_eq!(
            lowered.resolve_tool_choice_name("plain"),
            LoweredToolChoiceName::Unchanged
        );
        assert_eq!(
            lowered.resolve_tool_choice_name("missing"),
            LoweredToolChoiceName::Unchanged
        );
        assert_eq!(
            lowered.resolve_tool_choice_name("write_report"),
            LoweredToolChoiceName::Alias("write_report".to_string())
        );
        // A namespace parent is not a callable provider tool.
        assert_eq!(
            lowered.resolve_tool_choice_name("crm"),
            LoweredToolChoiceName::Ambiguous
        );
        // The same child name in two namespaces cannot be disambiguated by a
        // Responses tool-choice name.
        assert_eq!(
            lowered.resolve_tool_choice_name("shared"),
            LoweredToolChoiceName::Ambiguous
        );
    }

    #[test]
    fn discovered_web_search_tools_are_never_reclassified_as_hosted_search() {
        let discovered_context = json!({
            "provider_api_format": "openai:chat",
            "original_request_body": {
                "tools": [{"type": "tool_search", "execution": "client"}],
                "input": [{
                    "type": "tool_search_output",
                    "call_id": "call_search",
                    "execution": "client",
                    "tools": [
                        {
                            "type": "function",
                            "name": "web_search",
                            "description": "Client record search",
                            "defer_loading": true,
                            "parameters": {"type": "object", "properties": {}}
                        },
                        {
                            "type": "custom",
                            "name": "web_search_preview",
                            "defer_loading": true
                        }
                    ]
                }]
            }
        });
        let discovered = NamespaceToolAliases::from_report_context(&discovered_context);
        // Comparable to a declared client tool: the discovered function/custom
        // tool owns the name, so the hosted call branch must never win.
        assert!(!discovered.emits_hosted_web_search_call("web_search"));
        assert!(!discovered.emits_hosted_web_search_call("web_search_preview"));
        // The discovered tools stay ordinary client tools on egress/restore.
        assert_eq!(
            discovered.discovered_tools(ToolLoweringTarget::OpenAiChat)[0]["function"]["name"],
            "web_search"
        );
        assert_eq!(
            discovered.restore_tool_call("web_search"),
            ResponsesToolRestore::Unchanged
        );
        assert!(discovered.custom_tool_is_function_lowered(None, "web_search_preview"));

        // Without a client tool of that name the same body still classifies an
        // undeclared web_search call as hosted search.
        let undeclared = NamespaceToolAliases::from_report_context(&json!({
            "provider_api_format": "openai:chat",
            "original_request_body": {
                "tools": [{"type": "tool_search", "execution": "client"}]
            }
        }));
        assert!(undeclared.emits_hosted_web_search_call("web_search"));
        assert!(undeclared.emits_hosted_web_search_call("web_search_preview"));
    }

    #[test]
    fn discovered_inventory_is_independent_of_declared_tool_order() {
        let discovered = json!([{
            "type": "namespace",
            "name": "mcp__files",
            "description": "File tools",
            "tools": [{
                "type": "custom",
                "name": "apply_patch",
                "description": "Patch"
            }]
        }]);
        let first = json!([
            {"type": "tool_search", "execution": "client"},
            {"type": "function", "name": "lookup"},
            {
                "type": "namespace",
                "name": "mcp__reports",
                "description": "Reporting tools",
                "tools": [{"type": "function", "name": "lookup"}]
            }
        ]);
        let second = json!([first[2].clone(), first[1].clone(), first[0].clone()]);
        let first = lowering(
            first,
            Some(discovered.clone()),
            ToolLoweringTarget::ClaudeMessages,
        );
        let second = lowering(second, Some(discovered), ToolLoweringTarget::ClaudeMessages);
        for identity in ["lookup", "apply_patch"] {
            assert_eq!(
                first.restore_tool_call(identity),
                second.restore_tool_call(identity)
            );
        }
        assert_eq!(
            first.chat_name("mcp__reports", "lookup"),
            second.chat_name("mcp__reports", "lookup")
        );
    }

    #[test]
    fn tool_use_provider_name_rejects_unknown_namespace_children() {
        let lowered = lowering(
            json!([{
                "type": "namespace",
                "name": "mcp__reports",
                "description": "Reporting tools",
                "tools": [{"type": "function", "name": "lookup"}]
            }]),
            None,
            ToolLoweringTarget::OpenAiChat,
        );
        assert_eq!(
            lowered.tool_use_provider_name(Some("mcp__reports"), "lookup"),
            Ok("lookup".to_string())
        );
        assert!(lowered
            .tool_use_provider_name(Some("mcp__reports"), "missing")
            .is_err());
        assert_eq!(
            lowered.tool_use_provider_name(None, "plain"),
            Ok("plain".to_string())
        );
    }

    #[test]
    fn tool_choice_can_detect_declared_but_unoffered_deferred_tools() {
        let lowered = lowering(
            json!([
                {"type": "tool_search", "execution": "client"},
                {"type": "function", "name": "hidden", "defer_loading": true},
                {"type": "function", "name": "visible"}
            ]),
            None,
            ToolLoweringTarget::OpenAiChat,
        );
        assert!(lowered.name_is_declared_but_unoffered("hidden"));
        assert!(!lowered.name_is_declared_but_unoffered("visible"));
        assert!(!lowered.name_is_declared_but_unoffered("tool_search"));
    }

    #[test]
    fn deferred_tools_are_audited_so_registry_can_skip_raw_fields() {
        let lowered = lowering(
            json!([
                {"type": "tool_search", "execution": "client"},
                {"type": "function", "name": "hidden", "defer_loading": true},
                {"type": "custom", "name": "patch", "defer_loading": true}
            ]),
            None,
            ToolLoweringTarget::OpenAiChat,
        );
        assert!(lowered.tool_is_audited(0));
        assert!(lowered.tool_is_audited(1));
        assert!(lowered.tool_is_audited(2));
    }

    #[test]
    fn request_without_declared_tools_reconstructs_client_search_inventory() {
        let discovered = json!([{
            "type": "namespace",
            "name": "functions",
            "description": "Default namespace",
            "tools": [{
                "type": "function",
                "name": "get_shipping_eta",
                "description": "Shipping estimate",
                "parameters": {"type": "object", "properties": {"order": {"type": "string"}}},
                "defer_loading": true
            }]
        }]);
        let lowered = lowering(
            json!([]),
            Some(discovered.clone()),
            ToolLoweringTarget::OpenAiChat,
        );
        assert!(lowered
            .lowered_tools_for_source(ToolLoweringTarget::OpenAiChat, 0)
            .is_none());
        assert!(lowered.chat_name("functions", "get_shipping_eta").is_none());
        assert_eq!(
            lowered.discovered_tools(ToolLoweringTarget::OpenAiChat),
            vec![json!({
                "type": "function",
                "function": {
                    "name": "get_shipping_eta",
                    "description": "Shipping estimate",
                    "parameters": {"type": "object", "properties": {"order": {"type": "string"}}},
                }
            })]
        );
        assert_eq!(
            lowered.restore_tool_call("get_shipping_eta"),
            ResponsesToolRestore::Unchanged
        );
        assert_eq!(
            lowered.tool_search_output_text(&discovered[0]["tools"]),
            Some("These tools are now available to call: get_shipping_eta".to_string())
        );
    }

    #[test]
    fn declared_default_namespace_children_are_emitted_from_the_flattened_table() {
        let tools = json!([{
            "type": "namespace",
            "name": "functions",
            "description": "Default namespace",
            "tools": [{
                "type": "function",
                "name": "get_shipping_eta",
                "parameters": {"type": "object", "properties": {}}
            }]
        }]);
        let lowered = lowering(tools, None, ToolLoweringTarget::OpenAiChat);
        assert_eq!(
            lowered.lowered_tools_for_source(ToolLoweringTarget::OpenAiChat, 0),
            Some(vec![json!({
                "type": "function",
                "function": {
                    "name": "get_shipping_eta",
                    "parameters": {"type": "object", "properties": {}},
                }
            })])
        );
        assert!(lowered.tools_for_source(0).next().is_none());
        assert_eq!(
            lowered.restore_tool_call("get_shipping_eta"),
            ResponsesToolRestore::Unchanged
        );
    }

    #[test]
    fn namespace_child_aliases_reserve_discovered_flat_names() {
        let discovered = json!([{"type": "function", "name": "lookup"}]);
        let lowered = lowering(
            json!([{
                "type": "namespace",
                "name": "mcp__reports",
                "description": "Reporting tools",
                "tools": [{"type": "function", "name": "lookup"}]
            }]),
            Some(discovered),
            ToolLoweringTarget::OpenAiChat,
        );
        let alias = lowered
            .chat_name("mcp__reports", "lookup")
            .expect("namespace alias");
        assert_ne!(alias, "lookup");
        assert_eq!(
            lowered.discovered_tools(ToolLoweringTarget::OpenAiChat)[0]["function"]["name"],
            "lookup"
        );
    }
}
