# Technical Design

## Scope and boundaries

This task strengthens the existing standard-format conversion pipeline. It does not add a new API format, persistence schema, provider type, or vendor management surface.

The implementation stays within four existing ownership boundaries:

1. `crates/aether-ai/formats`: canonical parsing/emission, cross-format validation, and Gemini stream framing.
2. `apps/aether-gateway/src/execution_runtime/stream`: HTTP response headers for the selected Gemini stream wire mode.
3. `apps/aether-gateway/src/handlers/public/support/models`: public Gemini model projections.
4. `docs/api`: generated field-coverage declarations.

Same-format runtime request forwarding remains source-native. Cross-format conversion remains opt-in and fail-closed.

## 1. Gemini generation fields

### Canonical mapping

Extend the existing Gemini generation parser/emitter to map:

| Gemini | Canonical |
| --- | --- |
| `presencePenalty` | `generation.presence_penalty` |
| `frequencyPenalty` | `generation.frequency_penalty` |
| `responseLogprobs` | `generation.logprobs` |
| `logprobs` | `generation.top_logprobs` |

The mapping uses the official Gemini field types and preserves camelCase/snake_case input aliases consistently with the current parser.

Update `GEMINI_MAPPED_GENERATION_CONFIG_KEYS` so these fields are not duplicated inside provider extensions. Remove the stale target validation that claims Gemini cannot represent them. Existing source-target validation continues blocking genuinely unrepresentable fields.

Provider-only generation fields such as speech/image/translation configuration stay extension-preserved for same-format paths and fail closed across formats until an audited semantic mapping exists.

## 2. Response extension safety and selected mappings

### Safety invariant

For every cross-format response conversion:

```text
provider response -> canonical response -> source-specific extension audit -> target response
```

The source-specific audit runs only when source and target differ. Same-format response reconstruction and native runtime relay remain unchanged.

### Gemini response policy

- Preserve existing mappings for content, thinking, function calls/results, stop reason, cache/reasoning usage, images/files/audio.
- Preserve extended `FunctionResponse` behavior: fields without a target equivalent fail closed.
- Map Gemini web grounding/citation data to OpenAI Responses URL annotations only when the source contains a valid text segment plus a resolvable web URI/title.
- Do not synthesize citations when spans or source identity are incomplete.
- For OpenAI Chat or Claude targets, unconsumed `groundingMetadata`, `citationMetadata`, `promptFeedback`, safety ratings, logprob payloads, and provider-only usage details fail closed with the exact source field path.
- Any remaining top-level, candidate-level, content-block, or usage extension not explicitly consumed by the target fails closed.

### Claude response policy

- Preserve existing thinking, redacted thinking, tool, cache-token, media, stop-reason, and same-format extension behavior.
- Map a Claude citation only where an exact target annotation representation exists; otherwise fail closed instead of discarding it.
- Server-tool blocks remain same-format raw blocks and fail closed when the target has no lossless equivalent.
- Top-level response extensions (`container`, `stop_details`, provider-only fields), text-block extensions, and usage extensions are audited with explicit safe lists.
- `output_config.format` is parsed into canonical response-format semantics when it is an official JSON schema shape; unsupported output configuration remains provider-scoped and fail-closed cross-format.

The validator reports `LossyConversionBlocked` or `UnsupportedField` through the existing error contract. It does not introduce a new error type.

## 3. Gemini function-call identity

Request-history parsing keeps a per-name FIFO queue of unmatched function-call IDs while walking `contents` in order.

- Explicit `FunctionCall.id` is preserved and registered.
- A missing ID receives a deterministic ID containing message and part position, avoiding collisions between turns.
- A `FunctionResponse` with an explicit ID keeps it.
- A response without an ID consumes the earliest unmatched call ID for the same function name.
- If no matching call exists, use a deterministic response fallback ID and keep the current fail-safe behavior.

The response parser and streaming state keep their current independent identity behavior; only multi-turn request/history correlation changes.

## 4. Gemini stream wire mode

### Selection

Introduce one shared resolver based on sanitized report context:

- Public `/v1/models/*:streamGenerateContent` and `/v1beta/models/*:streamGenerateContent` with `alt=sse` -> SSE.
- The same public routes without `alt=sse` -> streaming JSON array.
- `/v1internal:streamGenerateContent` -> existing SSE behavior.

### Emitter

`GeminiClientEmitter` receives a mode at construction:

- SSE mode keeps `data: <GenerateContentResponse>\n\n`.
- JSON-array mode emits `[` before the first response, commas between responses, and `]` on finish. An empty stream emits `[]`.
- The response objects themselves are identical across modes, preserving stable response ID, tool argument aggregation, finish reason, media parts, and usage.
- If a conversion error occurs after the array has started, emit the structured Gemini error as the final array element and close the array so the client receives valid JSON.

### Rewriter and headers

- Wrap same-format Gemini SSE records directly when the client requested JSON-array mode, preserving the raw response object; converted traffic (or traffic requiring model/envelope edits) continues through the standard stream rewriter.
- Set `Content-Type: application/json` for JSON-array mode and `text/event-stream` for SSE mode.
- Apply the same mode to sync-JSON-to-stream bridging.
- Do not change OpenAI, Claude, Gemini Interactions, or private `/v1internal` framing.

## 5. Gemini model catalog projection

Replace the two duplicated static Gemini model builders with one shared projection.

The projection publishes only facts supported by current Aether data:

- `name`, `baseModelId`, `displayName`, and a gateway-owned description.
- `generateContent` for a model that passed the existing generation routability filter.
- `streamGenerateContent` because Aether can either use a streaming candidate or bridge an eligible sync response through the existing standard stream matrix.
- `countTokens` only when the model has an eligible native `gemini:generate_content` candidate whose provider type is not a private adapter that rejects this operation.

Remove fabricated universal values for version, input/output token limits, temperature, maxTemperature, topP, and topK. No new metadata schema is invented. If a future task establishes authoritative model metadata fields, the projection can add them then.

List and detail routes use the same projection so their capability declarations cannot drift.

## 6. Documentation and compatibility

- Update the field-coverage generator so the four Gemini generation fields are `mapped` where semantics are supported.
- Regenerate `format-field-coverage-matrix.md`.
- Preserve the documented distinction among native runtime handling, canonical roundtrip, mapped cross-format behavior, and lossy blocking.
- Existing tests that assert the old model directory payload may be adjusted only to the new contract; no new test files or suites are introduced.

## Risks and mitigations

| Risk | Mitigation |
| --- | --- |
| Over-blocking existing conversions | Audit provider extensions by source and location with small explicit safe lists; same-format exits before validation. |
| JSON-array stream becomes invalid on EOF/error | Array framing is owned by one stateful emitter and always closed by `finish`/`emit_error`. |
| Same-format Gemini loses provider fields while changing the envelope | Use the raw JSON-array wrapper for an exact same-format path; reserve canonical rewriting for conversions and required model/envelope edits. |
| Model directory advertises unsupported countTokens | Derive it from exact native Gemini candidate eligibility, not the cross-format generation catalog alone. |
| Catalog changes invent a new metadata contract | Omit unavailable numeric fields instead of guessing. |

## Rollback

All changes are code/document projections with no persistent migration. Rollback is limited to the touched format, stream-response, model-projection, and generated documentation files.
