# Antigravity private tool Schema normalization

## 1. Scope / Trigger

Public Gemini `generateContent` requests routed to Antigravity are adapted to a private v1internal envelope. The authorized 2026-09-20 upstream integration imports PR #832 and supersedes the earlier `$schema`-only cleanup: private tool schemas are normalized to the supported wire subset, including Claude union compatibility. Ordinary public Gemini passthrough remains unchanged.

## 2. Signatures

`build_antigravity_safe_v1internal_request(...) -> AntigravityRequestEnvelopeSupport` calls `normalize_antigravity_function_declaration_parameters(&mut Map<String, Value>, &str) -> Result<(), ()>` in both input branches. The private `schema` module owns `normalize_tool_parameters`, `normalize_claude_unions` and request-wide `SchemaBudget` (4096 charged nodes and 1 MiB serialized expansion budget). Exhaustion returns `ToolSchemaBudgetExceeded`; no configuration or database migration is added.

## 3. Contracts

- Accept both `functionDeclarations` and `function_declarations`. Existing `parameters` wins over aliases; otherwise `parametersJsonSchema` wins over `parameters_json_schema`. Preserve the selected definition rather than replacing it with an empty schema.
- The existing private-wire mapping ends in `parameters`. The public Gemini fields are not equivalent aliases: `parameters` is a constrained Schema, while `parametersJsonSchema` carries JSON Schema. Public discovery does not establish that private cloudcode accepts `parametersJsonSchema` directly.
- Antigravity-aware conversion sets `FormatContext.preserve_gemini_tool_schemas` to defer normalization until private transport; ordinary conversion and context reset do not inherit that flag.
- Normalize actual Schema positions, not arbitrary tool argument data. Retain supported property names and literal values while dropping unsupported dialect/extension fields. Resolve local references with cycle/depth guards and a shared expansion budget; merge reference siblings without contradictory object-spread overrides.
- The private subset can relax unsupported constraints. Only Claude targets additionally fold string-literal unions into equivalent enums or retain unsupported mixed-union guidance in descriptions. Do not choose an arbitrary branch; tool execution must still validate its original schema.
- Keep the existing error-triggered Gemini signature recovery contract. Private Claude replay removes unsigned historical thinking while retaining signed thinking, text and tools; public Gemini histories are unaffected.
- Do not mutate the input body, double-wrap native envelopes, or modify ordinary Gemini same-format traffic.

## 4. Validation & Error Matrix

| Input | Required result |
| --- | --- |
| Parameter Schema contains `$schema` | Remove that metadata before private transmission |
| Nested supported Schema contains dialect/unsupported keywords | Normalize that Schema node to the private subset |
| Property name or retained literal value uses `$schema` | Preserve the name/data; do not treat it as a keyword |
| Existing `parameters` and aliases coexist | Keep existing precedence; clean selected Schema only |
| Shared expansion budget exhausted | Reject private envelope with `ToolSchemaBudgetExceeded` |
| Claude mixed union unsupported on private bridge | Preserve alternative guidance without selecting a branch |
| Same request uses a public Gemini provider | Preserve public wire schema semantics |

## 5. Good / Base / Bad Cases

- Good: `parametersJsonSchema: {"$schema":"dialect","type":"object","properties":{"q":{"type":"string"}}}` becomes private `parameters` without the root metadata, retaining `q`.
- Base: supported private schemas remain stable under repeated normalization.
- Bad: leaking private normalization into public Gemini, rewriting literal data as schema, selecting one union branch, or allowing unbounded reference expansion.

## 6. Tests Required

Inline `antigravity::request` and `antigravity::schema` tests cover twelve tools, alias precedence, all source formats, public/private isolation, both envelope branches, immutability, reference budgets and Claude-only unions. Retain exact corrupted/Base64 signature recovery tests and `same_format_gemini` coverage. `antigravity_combined_client_conversion_preserves_schemas_until_transport` verifies provider-aware deferral end to end.

## 7. Wrong vs Correct

Wrong: apply private wire normalization during every Gemini conversion or treat a relaxed private schema as proof that arbitrary tool arguments are valid.

Correct: defer Antigravity tool schema handling to its bounded private transport owner, retain original validation at tool execution, and preserve ordinary public Gemini behavior.

## Claude cache-control candidate preparation

### 1. Scope / Trigger

Claude clients may attach `cache_control` to system, tools and historical content. Strict Claude-to-Gemini conversion correctly has no lossless cache equivalent, but Antigravity compatibility must deliberately discard that metadata before validation. This is separate from schema normalization and signature-error recovery.

### 2. Signatures

`aether_ai_formats::api::prepare_antigravity_claude_cache_control_body(body: &Value, client_api_format: &str, provider_type: &str, provider_api_format: &str, provider_config: Option<&Value>) -> Cow<Value>` owns preparation. Gateway candidate construction invokes it before standard conversion. No schema migration or upstream cache-management call is introduced.

### 3. Contracts

- Require explicit Antigravity identity, Claude Messages source and Gemini GenerateContent destination. `provider.config.antigravity_cache_control_compat` is a boolean, enabled when omitted; `false` preserves strict rejection.
- Borrow out-of-scope/disabled/unmarked requests. Clone only a request containing protocol markers, keeping original audit/fallback data intact.
- Remove protocol metadata at root, system, tool declaration, message/content blocks and nested tool-result content-block arrays. Object-valued tool results are arbitrary business data, not protocol blocks; preserve their cache_control fields even when another real marker causes cloning. Never descend into tool arguments, JSON schemas, document/image sources or arbitrary business objects. Preserve signatures byte-for-byte.
- Preserve original request as body-rule placeholder input; diagnostics for failed conversion inspect the prepared body so removed markers do not obscure later failures.
- Do not weaken universal conversion or native same-format passthrough. Claude TTL/breakpoint and pricing semantics are not guaranteed; real upstream usage alone determines cache accounting.

### 4. Validation & Error Matrix

| Condition | Result |
| --- | --- |
| Scoped provider and compatibility enabled | Remove markers before strict conversion |
| Explicit false | Original strict cache-control rejection |
| Other provider/source/destination | Borrow unchanged; existing policy applies |
| Business field named cache_control | Preserve schema/argument value |
| Native Claude fallback after preparation | Original cache markers retained |

### 5. Good / Base / Bad Cases

- Good: tool declaration marker disappears while `input_schema.properties.cache_control` and `tool_use.input.cache_control` survive.
- Base: an unmarked request remains borrowed.
- Bad: recursive global key deletion, mutation of shared fallback input, fake `cachedContent` resource IDs, or globally allowing Claude cache extensions through Gemini validation.

### 6. Tests Required

Standard-matrix regressions cover scope/toggle, nested tool-result markers, business-data preservation and strict/native isolation. Transport `antigravity_claude_cache_control_compat_preserves_tool_history_and_fallback` exercises adaptive thinking, effort, context controls, signed tool history and final private envelopes in stream/non-stream modes. Native conversion can merge adjacent same-role messages; assert preserved semantic blocks rather than incidental message grouping.

### 7. Wrong vs Correct

Wrong: add a post-conversion body rule to remove source markers; strict conversion has already rejected the request.

Correct: prepare a candidate-local source body before validation, preserve the original source for audit/rules/fallback, and retain strict checks elsewhere.
