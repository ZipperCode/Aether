# Execution lifecycle merge receipt

## Scope

- Worktree: `C:/Users/Zipper/AppData/Local/Temp/aether-full-upstream-20260908`.
- Parents: local `b0ad8ff7f7bdb1888abac8c06f47f61e76761431`, upstream `c7e403b410139d12a6189dda9c2bdf0c7c80782e`; merge base `7892aa94853461c1e634f7a5babbb1280128720f`.
- Original six-file ownership plus Root-approved lock for `attempt_cancellation.rs`.
- Previous formats ownership was relinquished before this assignment; no formats writes during this assignment.

## Manually resolved product paths

1. `apps/aether-gateway/src/execution_runtime/stream/execution.rs`
2. `apps/aether-gateway/src/execution_runtime/sync/execution.rs`
3. `apps/aether-gateway/src/execution_runtime/transport_failure.rs`
4. `apps/aether-gateway/src/executor/candidate_loop.rs`
5. `apps/aether-gateway/src/executor/outcome.rs`
6. `apps/aether-gateway/src/executor/mod.rs`
7. `apps/aether-gateway/src/execution_runtime/attempt_cancellation.rs` (upstream addition, explicitly lock-granted for shared Guard integration).

## Single terminal owner

- Upstream `AttemptCancellationGuard` is the only pre-response stream cancellation owner. Removed local `StreamAttemptTerminalGuard` and its duplicate forced-terminal helper.
- The Guard is armed once the attempt has recorded Pending, retaining the upstream compact bodyless usage seed, candidate snapshot, request identity and diagnostics.
- Added `AttemptCancellationGuard::fail_and_disarm(&GatewayError)`: ownership is taken exactly once; AdmissionTimeout preserves HTTP 429, `gateway_admission_timeout`, the internal gate and queue budget. Other aborts use a fixed diagnostic instead of serializing arbitrary internal errors.
- Forced failure settlement moves into an independent Tokio task before awaiting persistence, so dropping the waiter cannot interrupt candidate plus usage settlement. Repeated failure/disarm/drop calls cannot resubmit the taken ArmedAttempt.
- Successful stream construction disarms the Guard and transfers terminal ownership to the existing response-body finalizer. Dropping a still-armed attempt settles Cancelled/499 without estimating absent tokens.
- Watchdog progress now has one `abandoned` fact and the upstream `mark_abandoned()/abandoned()` API. The watchdog claims ownership before dropping execution, preventing Guard cancellation from overwriting timeout. Once `terminal_started` is observed, the watchdog waits for terminalization instead of cancelling it.
- Sync retains its existing single `SyncAttemptTerminalGuard`; precise AdmissionTimeout 429 classification is combined with upstream sanitized generic abort messages.

## Candidate lifecycle and accounting

- Adopted upstream per-candidate first-byte timeout after upstream execution admission. Removed request-level timing state and absolute-deadline argument from the stream loop and watchdog.
- The former `stream_candidate_retry_does_not_reset_an_expired_request_first_byte_budget` test is intentionally superseded by upstream fresh-budget and post-admission-budget regressions, matching the explicitly approved new semantics.
- Preserved candidate-level admission timeout handling, Pool lease release, timeout effects, credential-scoped retry, lazy same-Key retry creation, dynamic candidate ordering, and Provider transfer/usage reservation tracking.
- Resolved automatic merge's duplicate sync gate acquisition: `execute_sync_candidate_with_admission` owns the single permit. Cost reservation runs only inside the admitted future; the wrapper records actual gate-held timing, and execution errors release active plan costs.
- Removed an automatically duplicated `next_same_key_retry` invocation so one candidate failure derives at most one same-Key retry.
- Preserved quota evidence effects and their separation from health/adaptive/OAuth effects. HTTP 200 quota envelopes do not run success-only finalizers or success effects; manual scheduling recovery remains outside this write scope.
- Preserved native Responses embedded-error classification before 2xx commit, immediate visible Gemini thought handling, no candidate switch after client-visible output, and complete same-stream terminal failures.

## One deferred failure entry

- Retained fork `DeferredUpstreamResponse { exhaustion }` as the sole response extension owning deferred failure accounting.
- Both static and dynamic candidate loops attach exhaustion built from the actual fallback plan and context. The shared AI attempt loop already preserves the same tuple; no new compatibility path was introduced.
- Retained `take_deferred_upstream_exhaustion(&mut Response)` and `record_failed_usage_for_deferred_upstream_response(state, exhaustion, started_at, client_status_code, client_headers, execution_path)`.
- Removed duplicate upstream `DeferredUsageContext`, `attach_deferred_usage_context`, and `record_failed_usage_for_deferred_response`. They cloned a second full plan and introduced another potential Failed usage write.
- Preserved planned candidate attribution, actual returned status/headers, body-capture Unavailable semantics and routing diagnostic metadata. Candidate error type uses the upstream sanitizer; retained error text is taken from the attributed persisted failed candidate.
- Exact interface was sent to integrator, who owns proxy/orchestration terminal callers. No unresolved cross-file API request remains.

## Tests preserved/adapted for unified validation

- `stream_attempt_guard_records_admission_timeout_once_as_429` now targets the unified Guard and still asserts a single accepted terminal event.
- `sync_attempt_guard_records_admission_timeout_once_as_429`
- `armed_guard_settles_a_dropped_attempt_as_cancelled`
- `settling_a_dropped_attempt_respects_disabled_request_body_capture`
- `guard_stands_down_when_the_watchdog_abandons_the_attempt`
- `disarmed_guard_leaves_the_attempt_pending`
- `stream_candidate_watchdog_claims_timeout_before_dropping_execution`
- `stream_candidate_watchdog_does_not_cancel_started_terminalization`
- `stream_candidate_watchdog_failover_gets_fresh_first_byte_budget`
- `stream_candidate_watchdog_same_provider_retries_get_fresh_first_byte_budget`
- `stream_candidate_watchdog_starts_first_byte_budget_after_admission`
- Existing target/execution admission timeout and lease-release regressions.
- Existing native bare Responses error, malformed Antigravity thought-then-failure and quota envelope regressions remain present.
- Tunnel before-first-data/after-body-start tests combine upstream authenticated tunnel fixtures with the fork strong-Key catalog; removed duplicate setups that otherwise replaced the authenticated state. Redirect regression keeps the catalog-enabled setup.

## Actual checks and handoff

- Ran `rustfmt --edition 2021 --config skip_children=true` and the corresponding `--check` only on the seven owned files: passed.
- Scoped `git diff --check`: passed.
- Scoped marker search: no unresolved conflict markers.
- Scoped test-name scan: no duplicate test names. Adjacent identical conditional/assignment block scan found no remaining duplicated blocks after the same-Key retry cleanup.
- Deleted Guard/deferred/timing symbol scan: no residual references in the owned files; cross-file deferred caller changes were coordinated with integrator.
- No Cargo build or test execution in this assignment, per concurrent shared-root ownership. Scope syntax success is not claimed as type-check/test success; unified compilation and targeted regressions remain required.
- No main-checkout writes, services, databases, secrets, commits, task-state changes or temporary processes. Only this evidence file was added outside the seven product files.

## Specification follow-up for Root

`.trellis/spec/aether-gateway-execution/backend/stream-attempt-lifecycle-contract.md` still describes the old absolute request-level timeout and old Guard name. Update it to per-candidate post-admission budgets and the unified AttemptCancellationGuard API during specification integration; this assignment did not own that file.
