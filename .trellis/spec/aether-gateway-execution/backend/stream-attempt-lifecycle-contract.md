# Stream Attempt Lifecycle Contract

## 1. Scope / Trigger

Use this contract when changing streamed candidate retries, first-byte
timeouts, downstream cancellation, usage settlement, stream commit policy, or
protocol terminal events. One request may try several candidates, but every
candidate attempt must reach exactly one durable terminal state.

## 2. Signatures

```rust
struct AttemptCancellationGuard { /* compact pending-attempt settlement state */ }

async fn fail_and_disarm(&mut self, error: &GatewayError);
// Start a fresh candidate timer only after execution admission succeeds.
let timeout_duration = resolve_stream_candidate_watchdog_timeout(plan, report_context);
```

The guard is armed while a candidate is pending. A successful stream handoff
disarms it and transfers terminal ownership to the response-body finalizer;
explicit failure takes its armed state once. Dropping a still-armed guard
schedules the cancelled settlement path unless the watchdog already owns it.

## 3. Contracts

- Each candidate gets a fresh first-byte budget after execution admission.
  Earlier candidates and admission queue time do not consume that budget;
  failover may therefore extend total request elapsed time.
- `AttemptCancellationGuard` is the single fallback owner for an abandoned
  pending attempt. Do not add a second cancellation guard or an independent
  drop-based terminal writer.
- `fail_and_disarm` preserves admission timeout as Failed/429 with void usage.
  It takes terminal ownership once and awaits a separately spawned settlement
  task; cancelling the waiter must not interrupt durable failure settlement.
- The drop path settles candidate status and usage once, including cancellation
  before any body bytes. Bodyless usage still carries the request/candidate
  identity required by the terminal write; it must not invent token usage.
- Candidate selection may continue only before client-visible output. Once a
  protocol-visible event is emitted, a later terminal provider error stays in
  that stream and is rendered as the complete client-format failure terminal.
- A non-empty Gemini `thought` is client-visible; a signature-only part is not.
  Tool-call content is visible even when marked as thought metadata.
- A downstream close after a complete client-visible terminal event is treated
  as completed. A close before a terminal event remains cancelled unless an
  explicit terminal failure already owns the outcome.
- The existing stream watchdog observes lifecycle state but does not race the
  terminal writer. It calls `mark_abandoned()` before dropping execution;
  the guard respects `abandoned()`. Started terminalization must finish.
- Deferred upstream failure has one owner: `take_deferred_upstream_exhaustion`
  feeds `record_failed_usage_for_deferred_upstream_response` with the actual
  fallback plan. Do not attach a second usage context or write the same failure
  through another accounting path.

## 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Candidate one consumes most of its first-byte budget | Candidate two receives its own full configured budget. |
| Execution admission takes time before retry | Start the candidate timer after admission succeeds. |
| Admission times out | Persist one Failed/429 outcome, not Cancelled/499. |
| Client disconnects before first byte | Persist one cancelled candidate and one cancelled usage outcome. |
| Response-body finalizer takes ownership | Disarm the pre-response guard; do not duplicate settlement. |
| Watchdog times out before dropping execution | Mark abandoned first; guard must not overwrite timeout. |
| Non-empty Gemini thought is emitted | Commit that candidate and forbid later failover. |
| Signature-only Gemini control part arrives | Do not commit solely for that part. |
| Provider fails after visible output | Emit the protocol failure terminal in the same stream. |
| Client closes after a complete terminal event | Preserve completed/failed terminal state, not cancellation. |

## 5. Good / Base / Bad Cases

- Good: candidate one exhausts its first-byte budget; candidate two receives
  its own full budget after admission rather than failing immediately.
- Base: a candidate reaches its normal terminal write and disarms the guard;
  dropping the completed future performs no second write.
- Good: Gemini emits visible thought text and then reports a malformed function
  call; the client receives the thought followed by one complete failure event.
- Bad: an old request-wide deadline immediately times out later candidates,
  or two drop guards race to settle the same candidate and usage record.

## 6. Tests Required

- Gateway lifecycle:
  `gateway_settles_stream_attempt_when_client_disconnects_before_first_byte`.
- Candidate loop: `stream_candidate_watchdog_failover_gets_fresh_first_byte_budget`,
  `stream_candidate_watchdog_same_provider_retries_get_fresh_first_byte_budget`,
  `stream_candidate_watchdog_starts_first_byte_budget_after_admission`, plus
  watchdog ownership and terminalization ordering regressions.
- Stream execution: guard drop settles once, normal disarm avoids duplicates,
  both `records_admission_timeout_once_as_429` regressions preserve Failed/429,
  and `malformed_antigravity_function_call_streams_thought_then_fails_in_band`.
- Tests asserting captured error bodies must explicitly seed
  `request_record_level = "full"`. The production default is `Basic` and strips
  bodies; that capture policy must not be changed merely to satisfy a fixture.
- Usage runtime: cancelled/bodyless attempts retain identity and terminal usage
  facts without estimating request or partial-response tokens.
- Integration transport: HTTP/1 and h2c truncated SSE tests publish the partial
  body before closing the connection and produce a deterministic partial-body
  error.

## 7. Wrong vs Correct

### Wrong

```text
candidate one times out
-> reuse an expired request-wide deadline for candidate two
-> drop path and watchdog both write cancellation
```

### Correct

```text
admit each candidate and start its configured first-byte budget
-> keep one armed pre-response cancellation guard
-> hand off to the body finalizer or take explicit failure ownership once
-> watchdog marks abandonment before drop; no duplicate terminal write
```
