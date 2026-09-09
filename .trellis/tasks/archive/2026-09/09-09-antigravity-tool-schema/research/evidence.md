# Evidence for the bounded fix

## Verified incident

Deployment 192.168.2.212, aether-app 0.7.32, revision 7eaea44b082d2cdd1e4b0133ee04c83fb7b703ca. Local Antigravity request source blob matches v0.7.32. At 2026-09-09 11:10:36.748 CST, trace 930a9417-b194-4763-b631-5fc58253870b entered public Gemini `/v1beta/models/gemini-3.8-flash:streamGenerateContent?alt=sse`. No original request bodies were retained.

The candidate record (not final usage body) retained Google `400 INVALID_ARGUMENT` with 12 field violations. First description:

```text
Invalid JSON payload received. Unknown name "$schema" at 'request.tools[0].function_declarations[0].parameters': Cannot find field.
```

Do not embed credentials, real prompts, or large production bodies in tests/task files.

## Existing path and contract

`resolve_local_same_format_provider_candidate_payload_parts` -> Gemini same-format body builder -> `build_antigravity_safe_v1internal_request` -> private v1internal URL. Both ordinary source body and an existing `{request:{contents:...}}` envelope invoke `normalize_antigravity_function_declaration_parameters`.

Current normalizer aliases `parametersJsonSchema` and `parameters_json_schema` into `parameters`, with existing `parameters` taking precedence. This mapping is existing private-wire behavior and is covered by tests; do not blindly reverse it. The bug is that this mapping forwards incompatible Schema metadata unchanged. Public Gemini's fields are not equivalent: `parameters` uses the constrained Schema type, while `parametersJsonSchema` is arbitrary JSON. Public discovery cannot prove private cloudcode supports the latter field.

Official source consulted: https://generativelanguage.googleapis.com/$discovery/rest?version=v1beta (Schema has no `$schema`).

## 9Router reference and deliberate exclusions

Inspected decolua/9router commit `eb712ca821f0ba6bc41043fbd14494c5af5daba5`:
- `open-sse/executors/antigravity.js:239-261` calls `cleanJSONSchemaForAntigravity` before private upstream transmission.
- `open-sse/translator/formats/gemini.js:6-36` deletes `$schema`, but also deletes refs/definitions; lines 259-280 select only one union branch. Do not copy these lossy behaviors.
- `open-sse/translator/request/gemini-to-openai.js:52-69` ignores `parametersJsonSchema`; do not copy its empty-schema fallback or OpenAI pivot.

Keep the real schema structure, only adapt the demonstrated incompatible metadata. Any recursive traversal must distinguish Schema objects from `properties`/definition maps and literal data (`default`, `examples`, `const`, enum values).

## Bug Analysis: private Schema metadata

### 1. Root Cause Category
Cross-layer contract plus test coverage gap: public Gemini tool schema content was forwarded into private Antigravity `parameters` without removing the rejected dialect metadata. An intentional field mapping was mistaken for sufficient schema adaptation.

### 2. Why Fixes Failed
No earlier product fix was attempted in this task. The first build failed for missing NASM in process PATH, not code behavior; the actual RED test subsequently reproduced the twelve-tool defect.

### 3. Prevention Mechanisms
Completed: shared-boundary metadata removal, synthetic public-Gemini-to-private-envelope regression, schema-versus-literal boundary assertions, independent targeted tests/Clippy, and the executable package contract.

### 4. Systematic Expansion
Both existing-envelope and public-body callers use the same fixed boundary. Ordinary Gemini remains unchanged. Complex JSON Schema translation remains explicitly outside this fix; no lossy converter, new architecture, monitoring, or fallback was added.

### 5. Knowledge Capture
Recorded in `.trellis/spec/aether-provider-transport/backend/antigravity-tool-schema-contract.md` and linked from its index. This is Aether-owned runtime code, not a Trellis template implementation. User approved the phase 3.4 commit after the independent quality gate passed.
