# Codex HTTP Responses Relay Contract

## 1. Scope / Trigger

Use this contract when changing OpenAI Responses handling for downstream OpenAI Codex clients, including request conversion, provider transport, HTTP SSE finalization, compaction, headers, errors, or coverage documentation.

The supported Codex HTTP relay surface is intentionally narrower than the full OpenAI Responses resource API:

- `POST /v1/responses`
- `POST /v1/responses/compact`
- HTTP SSE returned by `POST /v1/responses`

Responses WebSocket mode, `previous_response_id`, retrieve/delete/cancel/input-items/input-tokens resource operations, and Aether-owned Response persistence are separate product work. Do not infer them from the general OpenAI API reference.

The client baseline was verified against OpenAI Codex commit `3929c99a97d1aa0fb8000903a4b57b24fbabe742`. Re-check the current Codex source before expanding or removing fields because this external contract can drift.

## 2. Signatures

```http
POST /v1/responses
Content-Type: application/json
Accept: text/event-stream

POST /v1/responses/compact
Content-Type: application/json
```

Current Codex HTTP inference sends `store=false`, `stream=true`, a model, and the complete input for the turn. Current remote compaction sends a model and complete input and consumes the returned `output` array.

## 3. Contracts

### Create request

Preserve these current Codex fields and all unknown same-format JSON fields:

- `model`, `instructions`, `input`
- `tools`, `tool_choice`, `parallel_tool_calls`
- `reasoning`
- `store`, `stream`, `stream_options`
- `include`
- `service_tier`, `prompt_cache_key`
- `text`
- `client_metadata`

For a native `openai:responses` provider, copy the JSON object and apply only documented provider transport edits. The API coverage matrix is an audit artifact, never a runtime allowlist.

For a cross-format provider, map only semantics represented by the canonical model. Unsupported material input, tool, structured-output, or reasoning semantics must fail closed rather than disappear.

### Compact request and response

Preserve `model`, `input`, `instructions`, `tools`, `parallel_tool_calls`, `reasoning`, `service_tier`, `prompt_cache_key`, and `text`. A native provider response preserves `output` and unknown response fields. Do not replace remote compaction with local summarization or stored Response state.

### Headers and credentials

Forward only headers allowed by the existing provider transport policy. Strip downstream `Authorization`, API-key variants, `Cookie`, `Proxy-Authorization`, hop-by-hop headers, and Aether-owned internal headers. Inject the selected provider credential at provider egress.

### HTTP SSE and errors

Native same-format SSE preserves event names, event order, JSON fields, and unknown future events. Usage and error observers may inspect bytes but must not rewrite them. Preserve upstream HTTP error status/body and request identifiers through the existing HTTP response boundary.

For native `openai:responses` SSE, classify the first complete body/event before committing downstream HTTP 2xx. If that first body is an embedded error and no output is client-visible, preserve the real error status or use the existing candidate-failover path. Do not expose a bare `{ "error": ... }` object as a successful Response: successful Responses require an `id`, while a post-commit `response.failed` event requires a complete Response object.

Aether stores normal usage/audit records only. It does not store Response bodies or create `response_id` affinity for this Codex HTTP surface.

## 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Missing/invalid Aether authentication | Reject locally before provider execution. |
| Admission or quota failure | Return the existing local error; do not call upstream. |
| Native Responses unknown request/response field | Preserve the JSON value. |
| Native Responses unknown SSE event/field | Preserve the emitted bytes and ordering. |
| Cross-format material semantic cannot be represented | Return a structured conversion/terminal error; never silently drop it. |
| Upstream HTTP 4xx/5xx | Preserve status and error body through the generic HTTP boundary. |
| Upstream HTTP 2xx + first native Responses body is an embedded error | Detect before committing downstream 2xx; preserve the error or retry an eligible next candidate. |
| Terminal SSE error after client-visible output | Return the same stream; do not splice a second provider stream. |
| Retryable terminal policy error before additional client-visible output | Follow the existing configured failover policy. |
| `store=false` | Forward unchanged; create no Aether Response persistence. |

## 5. Good / Base / Bad Cases

- Good: Codex sends the current full create payload plus a future field to a native Responses provider; the future field and opaque SSE event survive unchanged.
- Base: Codex sends a compact payload to a native provider; Aether returns the provider `output` array without local state.
- Bad: a cross-format provider cannot represent a new input/tool item, and Aether silently removes it to keep the request running.
- Bad: downstream credentials or `x-aether-*` identity headers reach provider egress.
- Bad: a provider returns HTTP 200 plus a bare Responses error body, and Aether commits 200 before classifying it, causing clients to deserialize the error as a successful Response without `id`.
- Bad: a general OpenAI resource endpoint is added solely because it exists in the official reference, without a downstream product requirement and provider-affinity design.

## 6. Tests Required

Keep focused regressions on the shared paths:

- `same_format_responses_body_preserves_opaque_extension_fields`: current Codex create fields plus an unknown field remain equal after native request construction.
- `same_format_headers_cannot_restore_credentials_or_internal_headers`: downstream credentials/internal headers are absent and provider credentials are present.
- `falls_back_to_body_json_for_openai_responses_same_family_sync_payload`: compact `output` and unknown response fields remain equal.
- `rejects_openai_responses_same_family_error_body_json`: success finalization does not consume 4xx/5xx error bodies.
- `prefetched_codex_cyber_policy_violation_stops_failover_by_default`: opaque SSE and terminal error bytes remain ordered and unchanged.
- `prefetched_codex_cyber_policy_violation_retries_when_system_setting_is_enabled`: the no-extra-output retry boundary remains intact.
- `same_format_responses_prefetch_retries_bare_error_before_committing_success`: a first bare Responses error is classified before HTTP commit and returns the existing candidate-retry signal.

Run the API coverage generator in check mode and `cargo fmt --all --check`. Broaden crate tests only when a focused regression exposes a shared-contract risk.

## 7. Wrong vs Correct

### Wrong

```text
Official Responses has seven HTTP resource methods
-> add all seven routes
-> persist Response bodies or provider affinity in Aether
```

This confuses the general OpenAI platform with the current Codex HTTP consumer and turns a relay into an application-state service.

### Correct

```text
Inspect the current Codex HTTP client
-> support create + compact + HTTP SSE
-> preserve native unknown fields/events
-> fail closed for unsupported cross-format semantics
-> plan stateful resource APIs separately if a real consumer requires them
```

For native Responses streaming failures, the correct boundary is:

```text
inspect the first complete event before downstream 2xx
-> embedded error with no visible output: preserve non-2xx or retry
-> valid Responses event: commit and preserve the original stream bytes
```
