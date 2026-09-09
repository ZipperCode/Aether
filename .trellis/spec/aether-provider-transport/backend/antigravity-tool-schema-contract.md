# Antigravity tool Schema metadata

## 1. Scope / Trigger

Public Gemini `generateContent` requests routed to an Antigravity provider are adapted to a private v1internal envelope; they are not byte-for-byte passthrough. Google rejected the observed request because twelve tool `parameters` schemas contained `$schema`. This contract applies only to the Antigravity transport boundary, including existing v1internal envelopes.

## 2. Signatures

`build_antigravity_safe_v1internal_request(...) -> AntigravityRequestEnvelopeSupport` calls `normalize_antigravity_function_declaration_parameters(&mut Map<String, Value>)` in both input branches. The private helper `remove_antigravity_schema_dialect(&mut Value)` removes the demonstrated incompatible metadata. Public signatures, formats, and configuration are unchanged.

## 3. Contracts

- Accept both `functionDeclarations` and `function_declarations`. Existing `parameters` wins over aliases; otherwise `parametersJsonSchema` wins over `parameters_json_schema`. Preserve the selected definition rather than replacing it with an empty schema.
- The existing private-wire mapping ends in `parameters`. The public Gemini fields are not equivalent aliases: `parameters` is a constrained Schema, while `parametersJsonSchema` carries JSON Schema. Public discovery does not establish that private cloudcode accepts `parametersJsonSchema` directly.
- Remove `$schema` only from actual Schema objects. Traverse child Schema positions: named schema maps, schema arrays/combinators, and single-schema keywords; do not interpret arbitrary nested JSON as a Schema.
- Preserve business property/definition names `$schema`, literal data in defaults/examples/enum/const, unknown extensions, `$ref`, constraints, and all composition branches. Preserving these structures does not claim new upstream support for them.
- Do not mutate the input body, double-wrap native envelopes, or modify ordinary Gemini same-format traffic.

## 4. Validation & Error Matrix

| Input | Required result |
| --- | --- |
| Parameter Schema contains `$schema` | Remove that metadata before private transmission |
| Nested `properties` / array item / combination Schema contains `$schema` | Remove metadata at the nested Schema node |
| Property name or literal value uses `$schema` | Preserve the name/data |
| Existing `parameters` and aliases coexist | Keep existing precedence; clean selected Schema only |
| Missing parameters or non-object Schema | Preserve existing behavior; do not add placeholders |
| Other upstream-incompatible keyword | No speculative rewrite; this fix is not a complete dialect converter |

## 5. Good / Base / Bad Cases

- Good: `parametersJsonSchema: {"$schema":"dialect","type":"object","properties":{"q":{"type":"string"}}}` becomes private `parameters` without the root metadata, retaining `q`.
- Base: a Schema without `$schema` retains all content.
- Bad: removing the property named `$schema` from `properties`, deleting `$ref`, or selecting one `oneOf` branch changes tool semantics and is forbidden.

## 6. Tests Required

Inline tests in `crates/aether-provider/transport/src/antigravity/request.rs` cover the synthetic twelve-tool incident, parameter/declaration spellings, public-Gemini preservation before private wrapping, existing envelope and input immutability, nested schema/data distinction, and alias precedence. Run `cargo test -p aether-provider-transport --lib antigravity::request` plus relevant `same_format_gemini` coverage and scoped `rustfmt --check`.

## 7. Wrong vs Correct

Wrong: rename arbitrary JSON Schema to `parameters` and assume identical formats, recursively delete every JSON key named `$schema`, or copy a lossy third-party schema converter wholesale.

Correct: keep the current private field contract and remove only the confirmed incompatible metadata at schema-aware positions in the Antigravity owner. Preserve public Gemini and every unrelated tool-definition semantic.
