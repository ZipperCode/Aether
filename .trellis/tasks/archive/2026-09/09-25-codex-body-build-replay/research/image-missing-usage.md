# Research: Native Images empty heartbeat response and missing usage

- Query: Trace native `/v1/images/edits` when every candidate is skipped before execution; explain HTTP 200, absent request ID, and missing usage; identify the smallest existing finalization path to reuse.
- Scope: Internal source investigation, with live read-only evidence supplied by the main session.
- Date: 2026-09-25
- Repository: `D:/Project/GitHub/Aether`.
- Baseline supplied by dispatcher: `8e765877e`; v0.7.39 source `22124fe59`. No Git operation was performed by this researcher.

## Findings

### Conclusion and repair boundary

The observed no-upstream failure is explained by the **request-level image heartbeat shell accepting an empty vector of executable attempts**. The relevant entry is `maybe_execute_sync_via_local_image_decision` in `apps/aether-gateway/src/executor/orchestration.rs:1367`, not the similarly named per-attempt `build_openai_image_sync_json_heartbeat_response` in `execution_runtime/sync/execution.rs:1923`.

After candidate materialization finishes, return `Ok(LocalExecutionRequestOutcome::NoPath)` if `attempts.is_empty()`, before constructing the request-level shell at `orchestration.rs:1400`. This returns ownership to the existing proxy no-plan failure path. It can preserve the failed request's usage, routing context and real HTTP 503 without any new postcommit accounting owner, fabricated execution plan, or retry.

Do not change nonempty heartbeat behavior as part of this repair. Do not call both the existing proxy failure writer and a new writer in the heartbeat serializer.

### Live evidence supplied by the main session

- Requests `ce2a2d23-da93-4c11-9058-fed464682657` and `525b1b29-b692-41b6-8b13-6f188d5eee27`, around 13:09 +08 on September 25, each have one Codex candidate with status `skipped`, reason `provider_request_body_missing`, `started_at = NULL`, and no usage row.
- Docker plaintext logs show buffered bodies of 1,290,549 / 1,290,464 bytes, path `/v1/images/edits`, final HTTP 200, `execution_path=execution_runtime_sync`, `request_id=-`, `local_execution_runtime_miss_reason=none`, Pool exhaustion, and no upstream request.
- Successful requests using the previously enabled custom fallback have usage rows.
- The main session subsequently recovered a related 13:06 successful-fallback body with `n=1`, `size=1536x1024`, `output_format=png`, and no extra fields/images. The exact Codex projector rejection is a separate implementation agent's scope; it is not needed to explain this missing-usage defect.
- These production observations were supplied by the main session; this researcher did not query production or replay a paid request.

### Exact code path

1. **A candidate exists before its body is constructible.** `build_local_image_sync_attempt_source_for_kind` returns a source for a nonzero candidate count (`ai_serving/planner/specialized/image.rs:134`, `:175`). This count is not the count of successfully built execution plans.
2. **Body failure persists a skipped candidate.** `resolve_local_openai_image_candidate_payload_parts` calls image normalization (`image/request.rs:131`). A normalization failure records `provider_request_body_missing` with source `openai_image_request_normalize` (`:140-156`). A later native request-body projection failure records the same skip reason with source `codex_openai_images_request_contract` (`:214-230`). The source field is needed to distinguish these failures; the generic skip reason alone cannot.
3. **No execution attempt is yielded.** `LocalOpenAiImageSyncAttemptSource::next_execution_attempt` continues past failed builds and returns `None` when exhausted (`image.rs:263-270`). `build_sync_attempt` returns `None` when the candidate payload cannot be built (`image.rs:350-362`). Skipped diagnostic persistence goes through `image/support.rs:444` and the shared candidate persistence wrapper (`planner/candidate_materialization.rs:2400`); it does not start an upstream execution.
4. **The heartbeat branch unconditionally wraps the empty vector.** With `routing_execution_policy.enable_cf_heartbeat=true`, `maybe_execute_sync_via_local_image_decision` drains the source into a vector (`orchestration.rs:1392-1399`), then always returns `Responded(build_openai_image_sync_heartbeat_shell_response(...))` (`:1400-1410`). There is no empty-vector check.
5. **HTTP 200 and missing control request ID are decided immediately.** `build_openai_image_sync_heartbeat_shell_response_with_executor` obtains its optional request ID only from `attempts.first()` (`orchestration.rs:1154-1157`). It builds HTTP 200 with a whitespace-heartbeat stream (`:1169-1172`) and attaches that optional ID (`:1189`). An empty vector therefore creates a response without a control request ID. It starts the background executor only afterward (`:1194`).
6. **An empty loop is NoPath, not Exhausted.** `run_ai_attempt_loop` has no `last_attempted` for an empty vector and returns `AiAttemptLoopOutcome::NoPath` (`crates/aether-ai/serving/src/attempt_loop.rs:130-138`, `:183-185`). The gateway maps it directly to `LocalExecutionRequestOutcome::NoPath` (`executor/candidate_loop.rs:168`).
7. **The only image background failure writer cannot run.** `execute_openai_image_sync_heartbeat_attempts` calls `record_failed_usage_for_exhausted_request` only for `Exhausted` (`orchestration.rs:1259-1269`); `NoPath` passes through at `:1271`. The shell's separate deferred-upstream writer requires a `Responded` outcome containing an exhaustion extension (`:1211-1221`), which also does not exist here.
8. **The serializer only produces bytes.** `openai_image_sync_heartbeat_final_bytes` maps `NoPath` to generic JSON error bytes (`orchestration.rs:1282-1285`). The generated body contains `error.code=503` and `error.upstream_status=503` (`:1350-1357`), but the outer HTTP status remains 200. This synthetic `upstream_status` is not evidence that an upstream was called.
9. **Proxy finalization has already taken the success-shaped branch.** The proxy sees `Responded`, clears the runtime-miss diagnostic, and returns via `EXECUTION_PATH_EXECUTION_RUNTIME_SYNC` (`handlers/proxy/mod.rs:2184-2213`). That explains `local_execution_runtime_miss_reason=none` and why the normal failure writer is unreachable despite the persisted skip.

### Existing failure finalization to reuse

`record_failed_usage_for_runtime_miss_request` (`executor/outcome.rs:487`) is the existing helper for a request that never produced an executable plan. Its only executable production caller is the proxy no-exhaustion branch (`handlers/proxy/mod.rs:2495`). `executor/mod.rs:24` is a re-export, not a second accounting owner.

When the image handler returns `NoPath`, the proxy can continue its normal routing/fallback policy, then finalizes a genuine miss as follows:

- Takes the runtime-miss diagnostic and loads persisted candidate context (`proxy/mod.rs:2370-2373`).
- Computes the public error detail and routing failure path (`:2387-2410`).
- If there is no execution exhaustion, calls `record_failed_usage_for_runtime_miss_request` with the actual `trace_id`, original headers and buffered body (`:2495-2506`).
- Builds the normal local error response (`:2509-2517`). `local_execution_runtime_miss_status(false)` is HTTP 503 (`:2860-2864`); only the independently identified capacity case maps to 429.

The helper selects an executed candidate if one exists, otherwise a routing candidate (`outcome.rs:503-506`); reconstructs model/provider/auth/format information; writes `status_code=503`; preserves request capture according to existing policy; and calls `record_terminal_event_direct(UsageEventType::Failed, request_id, data)` (`:527-623`). It does not require or invent a started execution. The existing all-skipped integration test confirms the resulting usage can be `failed` / `void` and linked to a still-skipped candidate.

`record_failed_usage_for_exhausted_request` (`outcome.rs:327`) requires a `LocalExecutionExhaustion` built from an attempted plan. Its executable callers are the normal proxy exhaustion branch (`proxy/mod.rs:2485`), the standard text heartbeat exhaustion helper (`orchestration.rs:894`), and image heartbeat attempted-exhaustion handling (`orchestration.rs:1260`). It is not the right helper to call directly for the empty vector.

### Request identity nuance

Returning `NoPath` restores usage identity under the real `trace_id`, and normal response construction preserves `x-trace-id` (`api/response.rs:183`). It does **not by itself guarantee** that the access log's distinct `request_id` field changes from `-`:

- The access log reads `x-aether-control-request-id` (`crates/aether-gateway/frontdoor/src/middleware/access_log.rs:124-130`).
- `build_local_http_error_response` calls the normal response builder without `attach_control_metadata_headers` (`api/response.rs:369-414`).
- If the control request ID header is explicitly included in the repair scope, reuse `attach_control_metadata_headers(response, Some(trace_id), None)` at the existing final failure response boundary. Its implementation is `api/response.rs:233-240`. Do not fabricate a successful/started execution candidate ID.

The core empty-vector guard should not be expanded into a new request-ID fallback scheme in the heartbeat body serializer.

### Similar names and all relevant callers

| Symbol | Callers / scope | Implication |
| --- | --- | --- |
| `maybe_execute_sync_via_local_image_decision` | `executor/sync_path.rs` image sync step | Shared native image sync routing entry; applies to edits and generations. |
| `build_openai_image_sync_heartbeat_shell_response` | Image sync entry at `orchestration.rs:1401`; diagnostic propagation test at `:2045` | Production caller already knows whether attempts is empty; place the guard there. |
| `build_openai_image_sync_heartbeat_shell_response_with_executor` | Wrapper at `:1114`; deferred-usage test at `:2117` | Preserve background cancellation, diagnostics propagation, deferred usage and nonempty behavior. |
| `execute_openai_image_sync_heartbeat_attempts` | Shell executor; retry/failover/sticky-limit tests | Existing attempted-exhaustion writer stays unchanged. |
| `build_openai_image_sync_json_heartbeat_response` | Only `execute_execution_runtime_sync_impl` at `execution_runtime/sync/execution.rs:2233` | Per-attempt layer already receives an `ExecutionPlan`; cannot explain a body-build skip that never yielded a plan. |
| `execute_execution_runtime_sync_impl` | Plain sync wrapper (`:2158`), retry-scope wrapper (`:2189`), per-attempt heartbeat background (`:1952`) | Pending usage starts at `:2294-2299`, after an actual plan and provider-key admission. The observed all-skipped request never reaches it. |
| `set_local_openai_image_execution_exhausted_diagnostic` | Non-heartbeat image sync (`orchestration.rs:1425`) and image stream (`:1501`) | Do not blindly convert a no-plan skip into attempted-execution exhaustion or overwrite the existing skip diagnostic. |

### Existing tests and recommended bounded regression

Existing tests located/read:

- `executor/orchestration.rs:2027` — `openai_image_sync_heartbeat_no_path_returns_json_error_body`: only asserts the serializer's error type and synthetic 503 field; it does not assert HTTP status, skipped-candidate identity, or usage. This explains the coverage gap.
- `orchestration.rs:1980`, `:1999`, `:2016` — successful body preservation and in-band error wrapping.
- `orchestration.rs:2037` — `openai_image_sync_heartbeat_propagates_request_diagnostics_to_terminal_usage`.
- `orchestration.rs:2078` — `openai_image_sync_heartbeat_records_deferred_usage_for_shell_response_once`.
- `orchestration.rs:2180`, `:2234`, `:2324` — candidate failover, lazy sticky retry, and provider transfer limit.
- `tests/usage/local.rs:1847` — `gateway_records_failed_usage_when_all_local_claude_cli_candidates_are_skipped`: existing full-route pattern for HTTP 503, `failed` / `void` usage under original trace ID, routing skip reason, one skipped candidate, and zero upstream hits (assertions `:2071-2159`).
- `tests/usage/local.rs:1229` — `gateway_records_failed_usage_for_claude_runtime_miss_without_execution_exhaustion`.
- `tests/ai_execute/sync/image.rs:756`, `:1185`, `:1657` — native Codex image gateway setup, local-upstream execution, and key allowlist rejection fixtures available for image regression reuse.
- `execution_runtime/sync/execution.rs:5603` — per-attempt heartbeat hides internal error details; preserve this independent safety behavior.

Smallest meaningful new regression: drive a real local gateway request to `/v1/images/edits` with `enable_cf_heartbeat=true`, one candidate deliberately rejected during body construction, and no working fallback. Assert HTTP 503 before any whitespace shell; existing trace identity; zero upstream/execution calls; exactly one terminal failed/void usage row linked to the request and skipped candidate; skipped candidate remains unstarted; capture policy unchanged. Use a rejection that remains invalid after the separate projector fix, or the test will stop covering the empty-attempt case. A successful nonempty heartbeat regression should retain HTTP 200 and unchanged successful JSON. If broad native Images coverage is desired, parameterize the no-plan assertion across edits and generations rather than adding a new abstraction.

### Files found

- `apps/aether-gateway/src/executor/orchestration.rs` — request-level image and standard-text heartbeat orchestration and inline tests.
- `apps/aether-gateway/src/executor/candidate_loop.rs` — maps the shared attempt loop to local response/no-path/exhaustion outcomes.
- `crates/aether-ai/serving/src/attempt_loop.rs` — empty/non-attempted loop returns NoPath; attempted failures construct exhaustion.
- `apps/aether-gateway/src/executor/outcome.rs` — no-plan and exhausted-request failed usage writers and persisted routing context.
- `apps/aether-gateway/src/handlers/proxy/mod.rs` — single normal HTTP failure finalization after local paths fail.
- `apps/aether-gateway/src/ai_serving/planner/specialized/image.rs` — image attempt source and skip-on-body-build behavior.
- `apps/aether-gateway/src/ai_serving/planner/specialized/image/request.rs` — native image normalization and request-body projection diagnostics.
- `apps/aether-gateway/src/ai_serving/planner/specialized/image/support.rs` — image skipped-candidate persistence wrapper.
- `apps/aether-gateway/src/ai_serving/planner/candidate_materialization.rs` — shared diagnostic persistence.
- `apps/aether-gateway/src/execution_runtime/sync/execution.rs` — separate per-plan heartbeat and execution lifecycle.
- `apps/aether-gateway/src/api/response.rs` — trace/control request ID headers and local HTTP error response builder.
- `crates/aether-gateway/frontdoor/src/middleware/access_log.rs` — access-log request ID comes from control request header.
- `apps/aether-gateway/src/tests/usage/local.rs` — integration patterns for all-skipped failed usage.
- `apps/aether-gateway/src/tests/ai_execute/sync/image.rs` — native Images integration fixtures.

### Related specs

- `.trellis/workflow.md` — persist research; stay within the authorized phase and role.
- `.trellis/spec/aether-gateway-execution/backend/stream-attempt-lifecycle-contract.md` — one terminal owner, bodyless identity, no duplicate deferred-failure usage, and capture tests explicitly selecting `request_record_level=full` when asserting bodies. This is a stream contract, so its ownership rule is supporting guidance rather than a direct synchronous empty-shell specification.
- `.trellis/spec/aether-gateway/backend/quality-guidelines.md` — specialized image/file/video paths may use static materialization; tests must respect bounded body collection. Do not refactor materialization into a new generic abstraction for this guard.
- `.trellis/spec/aether-routing-core/backend/routing-ordering-sticky-retry-contract.md:126-134` — keep image heartbeat lazy sticky retry coverage intact.
- Gateway execution/frontdoor and usage-runtime indexes were read. Several generic documents remain templates; no direct native-image empty-heartbeat contract was found. Main/implementer should add the narrow rule: an empty executable-attempt set must remain a no-plan outcome before HTTP heartbeat commitment, and the existing request failure path owns terminal usage.

### External references / versions

No external documentation was needed or consulted. Findings are from current local source reached with CodeGraph first, then narrow source reads for uncovered ranges. Baseline/release identifiers and production evidence are dispatcher-supplied, not independently reverified here.

## Caveats / Not Found

- No builds, tests, paid upstream calls, source edits, or Git operations were performed; this report is source evidence and a repair recommendation, not a passing implementation result.
- This report explains loss of usage and HTTP status after a construction skip. It does not establish the precise malformed/rejected field of the Codex image body; that is being fixed separately from the main session's recovered body evidence.
- Returning NoPath deliberately retains ordinary routing/fallback policy. With an eligible later path it may execute that path; with no usable path it reaches existing failed-usage/503 finalization. It must not bypass user policy by forcing failure or manufacturing a new fallback.
- The current fixed body-build public detail helper recognizes only `provider_request_body_build_failed`, not `provider_request_body_missing` (`executor/outcome.rs:178-193`). The existing generic runtime-miss/skip diagnostics will therefore determine the latter's public message; changing this message is a separate concern.
- A heartbeat that was legitimately committed with a nonempty attempt cannot later change its wire HTTP status. This repair prevents an already-known no-plan failure from committing a shell; it is not a postcommit HTTP-status redesign.
- The request-ID distinction above is material: preserving the actual usage request ID and trace header does not automatically populate the separate control request ID header.
