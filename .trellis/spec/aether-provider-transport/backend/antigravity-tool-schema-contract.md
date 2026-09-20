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
