# Research: Same-format request byte fidelity and hot path

- Query: Trace the current request path from frontdoor buffering through same-format planning and `ExecutionPlan` transport; identify what the latest master already implements, what OpenAI-specific paths still miss, and the smallest safe optimization that preserves request semantics.
- Scope: internal
- Date: 2026-08-28

## Findings

### Current call path

1. The proxy frontdoor buffers the request once, then normalizes encoded bodies before routing. `buffer_and_normalize_request_body` collects the Axum body and calls the bounded header/body normalizer (`apps/aether-gateway/src/handlers/proxy/body_buffer.rs:181`, `apps/aether-gateway/src/handlers/proxy/body_buffer.rs:211`). The normalizer returns the original `Bytes` when no content encoding is present; for encoded input it returns decoded bytes and removes `Content-Encoding`/`Content-Length` (`apps/aether-gateway/src/headers.rs:386`, `apps/aether-gateway/src/headers.rs:391`, `apps/aether-gateway/src/headers.rs:397`). Exact-byte fidelity therefore refers to the normalized/decoded HTTP entity body, not the original compressed wire representation.
2. JSON parsing is shared by sync and stream entry paths. `parse_direct_request_body` parses JSON directly from the decoded slice and deliberately does not create base64 for JSON; non-JSON input gets base64 (`crates/aether-ai/formats/src/formats/shared/request.rs:20`, `crates/aether-ai/formats/src/formats/shared/request.rs:24`, `crates/aether-ai/formats/src/formats/shared/request.rs:32`). The gateway wrapper decodes only for JSON, then delegates to that parser (`apps/aether-gateway/src/ai_serving/planner/common.rs:32`, `apps/aether-gateway/src/ai_serving/planner/common.rs:36`).
3. Both local sync and stream execution clone request parts, clone the parsed `serde_json::Value`, and capture a copy of the decoded bytes in `OriginalRequestPayload` (`apps/aether-gateway/src/executor/sync_path.rs:50`, `apps/aether-gateway/src/executor/sync_path.rs:54`, `apps/aether-gateway/src/executor/sync_path.rs:62`; `apps/aether-gateway/src/executor/stream_path.rs:55`, `apps/aether-gateway/src/executor/stream_path.rs:62`, `apps/aether-gateway/src/executor/stream_path.rs:70`). The extension is then visible to every candidate planned from those cloned parts.
4. `OriginalRequestPayload` stores the parsed value and decoded bytes behind `Arc`; construction deep-copies the passed JSON clone into a new `Arc<Value>` and copies the byte slice into `Arc<[u8]>` (`crates/aether-ai/serving/src/payload_fidelity.rs:9`, `crates/aether-ai/serving/src/payload_fidelity.rs:16`). Its guard returns base64 only when the captured bytes are non-empty and the terminal provider JSON is equal to the original parsed JSON; this preserves whitespace and key order without trusting a heuristic (`crates/aether-ai/serving/src/payload_fidelity.rs:23`, `crates/aether-ai/serving/src/payload_fidelity.rs:26`, `crates/aether-ai/serving/src/payload_fidelity.rs:30`). Focused positive and changed-body tests already exist (`crates/aether-ai/serving/src/payload_fidelity.rs:45`, `crates/aether-ai/serving/src/payload_fidelity.rs:63`).
5. The generic same-format candidate planner already evaluates byte reuse after provider routing policy and operation-specific body invariants have finalized that candidate (`apps/aether-gateway/src/ai_serving/planner/passthrough/provider/family/payload.rs:281`, `apps/aether-gateway/src/ai_serving/planner/passthrough/provider/family/payload.rs:286`, `apps/aether-gateway/src/ai_serving/planner/passthrough/provider/family/payload.rs:293`). It additionally rejects raw reuse for compatibility adaptation, redaction, compatibility edits, explicit content encoding, or enabled/unknown gzip policy (`apps/aether-gateway/src/ai_serving/planner/passthrough/provider/family/payload.rs:352`, `apps/aether-gateway/src/ai_serving/planner/passthrough/provider/family/payload.rs:361`). Its tests cover exact bytes and every eligibility gate (`apps/aether-gateway/src/ai_serving/planner/passthrough/provider/family/payload.rs:628`, `apps/aether-gateway/src/ai_serving/planner/passthrough/provider/family/payload.rs:653`).
6. The generic standard and Gemini plan builders already preserve the selected base64 body for both sync and stream. They use `resolve_ai_passthrough_sync_request_body` at `apps/aether-gateway/src/ai_serving/planner/standard/plan_builders.rs:65`, `apps/aether-gateway/src/ai_serving/planner/standard/plan_builders.rs:152`, `apps/aether-gateway/src/ai_serving/planner/standard/gemini/plan_builders.rs:62`, and `apps/aether-gateway/src/ai_serving/planner/standard/gemini/plan_builders.rs:134`. The resolver gives non-empty base64 precedence over JSON and emits the existing `RequestBody.body_bytes_b64` representation (`crates/aether-ai/serving/src/attempt_plan.rs:89`, `crates/aether-ai/serving/src/attempt_plan.rs:93`, `crates/aether-ai/serving/src/attempt_plan.rs:103`). Standard sync/stream regressions already assert that behavior (`apps/aether-gateway/src/ai_serving/planner/standard/plan_builders.rs:222`, `apps/aether-gateway/src/ai_serving/planner/standard/plan_builders.rs:247`).
7. `RequestBody` remains the serialized execution contract: `json_body`, `body_bytes_b64`, and `body_ref` (`crates/aether-contracts/src/plan.rs:63`). `ExecutionPlan` embeds it and remains serde-serializable for runtime/tunnel boundaries (`crates/aether-contracts/src/plan.rs:133`, `crates/aether-contracts/src/plan.rs:152`). At egress, `build_request_body` serializes JSON or decodes base64, and only applies gzip/zstd when the JSON variant is used (`apps/aether-gateway/src/execution_runtime/transport.rs:2557`, `apps/aether-gateway/src/execution_runtime/transport.rs:2560`, `apps/aether-gateway/src/execution_runtime/transport.rs:2562`, `apps/aether-gateway/src/execution_runtime/transport.rs:2570`). The same builder feeds direct sync, direct stream, local-tunnel stream, and local-tunnel sync (`apps/aether-gateway/src/execution_runtime/transport.rs:795`, `apps/aether-gateway/src/execution_runtime/transport.rs:859`, `apps/aether-gateway/src/execution_runtime/transport.rs:996`, `apps/aether-gateway/src/execution_runtime/transport.rs:1135`).

### Confirmed gaps on current master

| Area | Current state | Gap |
| --- | --- | --- |
| Generic same-format planner | Exact decoded bytes are captured, equality-guarded, base64-encoded only after candidate finalization, and consumed by standard/Gemini builders. | No missing generic request-fidelity mechanism was found. Reimplementing it would duplicate existing code. |
| Dedicated Gemini plan builders | Both sync and stream already consume `provider_request_body_base64`. Same-format standard-family candidates are delegated to the generic same-format planner before the cross-format family path (`apps/aether-gateway/src/ai_serving/planner/standard/family/payload.rs:46`, `apps/aether-gateway/src/ai_serving/planner/standard/family/payload.rs:56`). | No functional gap found. A Gemini-specific regression is absent, but the implementation is already wired. |
| OpenAI Chat plan builders | Sync and stream always call `RequestBody::from_json`, ignoring any exact base64 already present on the decision (`apps/aether-gateway/src/ai_serving/planner/standard/openai/plan_builders/sync.rs:117`, `apps/aether-gateway/src/ai_serving/planner/standard/openai/plan_builders/stream.rs:132`). | An exact body emitted by the generic planner is discarded at this boundary. |
| OpenAI Responses plan builders | Sync and stream likewise always call `RequestBody::from_json` (`apps/aether-gateway/src/ai_serving/planner/standard/openai/plan_builders/sync.rs:199`, `apps/aether-gateway/src/ai_serving/planner/standard/openai/plan_builders/stream.rs:232`). | Same loss for Responses/compact plans. |
| Dedicated OpenAI candidate planners | Chat rebuilds and finalizes every same-format candidate body, including model, stream, body-rule and compatibility edits (`apps/aether-gateway/src/ai_serving/planner/standard/openai/chat/decision/request.rs:416`, `apps/aether-gateway/src/ai_serving/planner/standard/openai/chat/decision/request.rs:483`, `apps/aether-gateway/src/ai_serving/planner/standard/openai/chat/decision/request.rs:514`). Responses similarly builds then performs candidate-aware finalization (`apps/aether-gateway/src/ai_serving/planner/standard/openai/responses/decision/request.rs:272`, `apps/aether-gateway/src/ai_serving/planner/standard/openai/responses/decision/request.rs:475`, `apps/aether-gateway/src/ai_serving/planner/standard/openai/responses/decision/request.rs:565`, `apps/aether-gateway/src/ai_serving/planner/standard/openai/responses/decision/request.rs:607`). Both decision payloads currently set `provider_request_body_base64: None` (`apps/aether-gateway/src/ai_serving/planner/standard/openai/chat/decision/payload.rs:248`, `apps/aether-gateway/src/ai_serving/planner/standard/openai/responses/decision/payload.rs:291`). | Even after fixing the builder drop, the ordinary dedicated native OpenAI path still has no exact-byte candidate value to consume. |
| Transport JSON construction | `build_request_body` clones the full terminal `Value` and then serializes the clone (`apps/aether-gateway/src/execution_runtime/transport.rs:2560`, `apps/aether-gateway/src/execution_runtime/transport.rs:2561`). | The deep clone is unnecessary; `serde_json::to_vec` only needs `&Value`. Serialization itself remains necessary for modified/cross-format JSON. |

### Exact hot-path work and whether it is avoidable

- Avoidable now: one deep `Value` clone in `build_request_body` for every JSON execution. Borrowing `plan.body.json_body.as_ref()` removes it without changing bytes, errors, compression, retries, or contracts.
- Avoidable for unchanged native OpenAI requests: JSON reserialization in all four OpenAI sync/stream plan-builder exits. The existing equality guard can select exact bytes after all candidate-specific body changes.
- Currently required by the serialized contract: base64 encoding in `OriginalRequestPayload::body_bytes_base64_if_unchanged` followed by base64 decoding in `build_request_body`. Removing those operations requires a new raw-byte execution representation or custom serialization and would expand the contract/tunnel change surface.
- Currently paid once per direct JSON request: the entry-path `body_json.clone()` plus byte-slice copy into `OriginalRequestPayload`. They are real allocations, but removing them cleanly requires changing ownership across parsing, request extensions, and candidate planners. This is not the smallest safe change.
- Semantically required: parsing JSON, per-candidate body construction, model/stream normalization, body/header rules, redaction, Codex/DeepSeek edits, and final equality comparison. These operations determine whether raw reuse is legal and must not be skipped.

### Recommended minimal change set

1. In the four OpenAI plan-builder exits, replace direct `RequestBody::from_json(...)` construction with the already-existing `resolve_ai_passthrough_sync_request_body(...)`. This preserves an exact `provider_request_body_base64` produced by any generic or serialized decision and falls back to the same JSON body otherwise.
2. Add one shared OpenAI plan-building helper, used by both `sync.rs` and `stream.rs`, to fill a missing exact body from `parts.extensions().get::<OriginalRequestPayload>()` only when all of the following hold:
   - client/provider API formats are alias-equivalent;
   - the final per-candidate provider JSON equals the captured original JSON (the existing raw-byte equality guard);
   - no explicit content encoding is active and request gzip is definitively absent/disabled, because raw `body_bytes_b64` bypasses transport compression;
   - an exact base64 value supplied by the decision is not already present.
   Evaluate this after candidate routing/body finalization and immediately before moving the body into `RequestBody`. That placement covers Chat/Responses sync+stream together, preserves per-candidate isolation, and automatically rejects model, stream, prompt-cache, body-rule, redaction, or compatibility edits when they change JSON.
3. In `build_request_body`, borrow `json_body` instead of cloning it before `serde_json::to_vec`. This is the smallest independent shared optimization and applies to every execution transport caller.

This approach changes no public DTO or wire shape. Existing base64 remains the fallback when an `ExecutionPlan` is serialized or relayed through a tunnel. Report-context construction should continue to inspect the final JSON before it is moved, so audit/usage semantics do not depend on which wire representation is selected.

### Explicit non-changes

- Do not add a new `RequestBody` variant, raw `Arc<[u8]>` field, byte registry, `body_ref` interpretation, dependency, config switch, or custom serde contract.
- Do not remove JSON parsing or per-candidate body finalization; byte reuse is an egress choice, not a bypass around validation/policy.
- Do not share a mutable body or encoded string across candidates. Each candidate must independently pass the final equality/encoding guard.
- Do not change model mapping, `stream` enforcement, prompt-cache injection, header filtering/auth injection, redaction, body rules, response encoding, or retry/failover order.
- Do not change WebSocket framing/continuation behavior. The investigated optimization is the HTTP sync/SSE request body path; absence of `OriginalRequestPayload` must continue to fall back to JSON.
- Do not change `ExecutionPlan`/`RequestBody` serialization or tunnel relay metadata/body contracts.

### Tests and measurement evidence to add/run

Existing focused evidence to retain:

- `OriginalRequestPayload` exact/changed-body unit tests (`crates/aether-ai/serving/src/payload_fidelity.rs:45`, `crates/aether-ai/serving/src/payload_fidelity.rs:63`).
- Generic eligibility-gate tests (`apps/aether-gateway/src/ai_serving/planner/passthrough/provider/family/payload.rs:628`, `apps/aether-gateway/src/ai_serving/planner/passthrough/provider/family/payload.rs:653`).
- Standard sync/stream exact-body plan tests (`apps/aether-gateway/src/ai_serving/planner/standard/plan_builders.rs:222`, `apps/aether-gateway/src/ai_serving/planner/standard/plan_builders.rs:247`).

Add the smallest regressions:

- OpenAI Chat sync and stream plan-builder tests: a decision containing both JSON and exact base64 must produce `RequestBody { json_body: None, body_bytes_b64: Some(...) }`.
- OpenAI Responses sync and stream equivalents, including compact plan selection.
- One shared negative test proving cross-format, changed model/stream/body, or enabled gzip selects JSON rather than original bytes.
- One frontdoor-to-captured-upstream test for native OpenAI Chat and one for native OpenAI Responses/SSE using deliberately non-canonical whitespace/key order; assert byte-for-byte request equality. Use two candidates or a retry fixture only if it can prove each candidate independently chooses its own finalized body without emitting client-visible stream data first.
- A focused `build_request_body` unit assertion for JSON output and base64 exact output; the borrow-only clone removal should not alter either result.

Suggested validation order (serial, and verify non-zero matched tests):

```text
cargo test -p aether-ai-serving payload_fidelity -- --nocapture
cargo test -p aether-gateway exact_request_body -- --nocapture
cargo test -p aether-gateway openai_chat -- --nocapture
cargo test -p aether-gateway openai_responses -- --nocapture
cargo fmt --all --check
```

No dedicated benchmark harness is needed. Use the existing `frontdoor_stream_parse` and `direct_build_body` stage observations around the same payload corpus before/after (`apps/aether-gateway/src/executor/stream_path.rs:54`, `apps/aether-gateway/src/execution_runtime/transport.rs:858`). Record payload size, candidate count, sync/SSE mode, sample count, and p50/p95; allocation profiling is the only reliable proof for the removed deep clone because millisecond stage counters may round small payloads to zero.

### Risks

- Raw-body precedence must be unambiguous: when exact base64 is selected, `json_body` must be `None`, matching the existing resolver contract. Carrying both would make transport precedence and usage capture harder to reason about.
- Never select raw bytes while gzip/zstd is requested. `build_request_body` compresses only `json_body`; choosing base64 in that state would send an uncompressed body under a compressed header.
- Equality must be checked against the final candidate body, after routing policy and every mutation. Checking the source body earlier can undo model/body/header-dependent policy.
- Empty JSON bodies are intentionally ineligible in the current guard. Preserve that behavior unless a separate requirement defines exact empty-body semantics.
- The exact bytes are normalized decoded bytes. Tests must not assert preservation of the client's original compressed octets after frontdoor decoding.

### Files found

- `apps/aether-gateway/src/handlers/proxy/body_buffer.rs` — bounded request buffering and normalization entry.
- `apps/aether-gateway/src/headers.rs` — request content-decoding and header normalization.
- `crates/aether-ai/formats/src/formats/shared/request.rs` — shared JSON/non-JSON parsing and initial base64 policy.
- `apps/aether-gateway/src/executor/sync_path.rs` — sync `OriginalRequestPayload` insertion and decision path.
- `apps/aether-gateway/src/executor/stream_path.rs` — stream/SSE insertion and decision path.
- `crates/aether-ai/serving/src/payload_fidelity.rs` — exact parsed-value/raw-byte guard.
- `apps/aether-gateway/src/ai_serving/planner/passthrough/provider/family/payload.rs` — existing generic same-format eligibility and base64 selection.
- `apps/aether-gateway/src/ai_serving/planner/standard/plan_builders.rs` — standard sync/stream raw-body consumption and tests.
- `apps/aether-gateway/src/ai_serving/planner/standard/gemini/plan_builders.rs` — Gemini sync/stream raw-body consumption.
- `apps/aether-gateway/src/ai_serving/planner/standard/openai/chat/decision/request.rs` — dedicated OpenAI Chat candidate body finalization.
- `apps/aether-gateway/src/ai_serving/planner/standard/openai/chat/decision/payload.rs` — Chat decision DTO construction; base64 currently absent.
- `apps/aether-gateway/src/ai_serving/planner/standard/openai/responses/decision/request.rs` — dedicated Responses/compact candidate finalization.
- `apps/aether-gateway/src/ai_serving/planner/standard/openai/responses/decision/payload.rs` — Responses decision DTO construction; base64 currently absent.
- `apps/aether-gateway/src/ai_serving/planner/standard/openai/plan_builders/sync.rs` — OpenAI Chat/Responses sync `RequestBody` construction.
- `apps/aether-gateway/src/ai_serving/planner/standard/openai/plan_builders/stream.rs` — OpenAI Chat/Responses stream `RequestBody` construction.
- `crates/aether-ai/serving/src/attempt_plan.rs` — existing base64-over-JSON body resolver.
- `crates/aether-contracts/src/plan.rs` — serialized `RequestBody` and `ExecutionPlan` contract.
- `apps/aether-gateway/src/execution_runtime/transport.rs` — final JSON serialization/base64 decode, compression, direct and tunnel callers.

### External references and history

- No external library or API lookup was needed; this is an internal ownership/performance path and adds no dependency.
- The current executable contract is `.trellis/spec/aether-ai-formats/backend/codex-http-responses-contract.md`. It requires native unknown-field preservation, credential isolation, unchanged native SSE bytes, and fail-closed cross-format conversion. Historical task notes corroborate that same-format OpenAI Responses must copy/preserve unknown JSON fields; all such claims above were re-verified against current source rather than taken from history alone.

### Related specs

- `.trellis/spec/aether-ai-formats/backend/codex-http-responses-contract.md` — native request/SSE fidelity, headers, errors, retry boundary, and focused tests.
- `.trellis/spec/aether-ai-formats/backend/index.md` — links the current Codex HTTP relay contract.
- `.trellis/spec/aether-gateway/backend/index.md` — links the same cross-layer contract from gateway ownership.
- `.trellis/spec/aether-provider-transport/backend/index.md` — links the native passthrough/credential/tunnel side of the contract.

## Caveats / Not Found

- The task `prd.md` is still the generated `TBD` skeleton, so the dispatch's confirmed goal/constraints were treated as the authoritative research query. Implementation should first persist the reviewed requirements/design in normal Trellis flow.
- No product code, spec, task status, manifest, benchmark, test, or git state was changed or executed during this research-only subtask.
- CodeGraph was used first as required, but its symbol search did not surface `OriginalRequestPayload` accurately; all line evidence in this file was then read from the current on-disk source with targeted `rg` queries.
- No current test was found that proves native OpenAI Chat/Responses request whitespace/key-order equality end to end. Existing coverage proves the lower-level guard and standard planner path, not the dedicated OpenAI builder path.
