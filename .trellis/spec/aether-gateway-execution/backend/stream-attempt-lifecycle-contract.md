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
// 同格式 Chat/Responses 在交付响应前分类首个完整正文或事件。
StreamCommitPolicy::FirstClassifiedBody;
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
- Same-format OpenAI Chat and Responses SSE use `FirstClassifiedBody`, not
  response-header-only commitment. A first provider error follows the existing
  failover/stop policy before response handoff; do not increase retry counts to
  compensate for a prematurely committed stream.
- SSE classification consumes complete records using the existing boundary
  parser. Comments, `id`, `retry`, and other control-only records do not commit
  a response. Join a record's `data` fields before JSON classification; a split
  error record remains pending. Keep LF, CRLF and CR record boundaries working.
- Inspection must not rewrite successful upstream bytes or unknown fields.
  Preserve the existing prefetch limits, first-byte budget and terminal owner.
- First-upstream-event accounting is independent of HTTP commitment. The first
  `Data` frame, including an empty frame, records first-byte time and nonterminal
  streaming state during prefetch. Reuse the existing event-recording owner;
  response handoff must not initialize the same event again. An early error may
  still fail over before client-visible output and settle through its one owner.
- Archived `upstream_response` status/headers may be reconstructed from a
  terminal provider error. Do not infer original wire status or commitment
  timing from those normalized fields alone.
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
| Chat first event is an error, including fragmented/multiline SSE | Honor failover and reach the next eligible provider before handing off a response. |
| Only SSE comments, id or retry records arrive | Keep waiting within the existing prefetch budget. |
| First Data arrives before any visible text | Persist first-byte/streaming while HTTP response remains uncommitted. |
| Provider explicitly stops on the first error status | Return that error without contacting the next provider. |
| Client closes after a complete terminal event | Preserve completed/failed terminal state, not cancellation. |

## 5. Good / Base / Bad Cases

- Good: candidate one exhausts its first-byte budget; candidate two receives
  its own full budget after admission rather than failing immediately.
- Base: a candidate reaches its normal terminal write and disarms the guard;
  dropping the completed future performs no second write.
- Good: provider A fails twice under sticky retry, B returns HTTP 200 with a
  first-event error, and C is actually called and completes; usage belongs to C.
- Bad: assuming `fixed_order` guarantees HTTP call order in a Chat simulation.
  Chat still selects within a ranked target window by in-flight/preselect
  pressure. Use an independent `UpstreamTargetAdmission` and deterministic
  target pressure for strict-order fixtures, not process-wide environment edits.
- Good: Gemini emits visible thought text and then reports a malformed function
  call; the client receives the thought followed by one complete failure event.
- Bad: an old request-wide deadline immediately times out later candidates,
  or two drop guards race to settle the same candidate and usage record.

## 6. Tests Required

- Gateway lifecycle:
  `gateway_settles_stream_attempt_when_client_disconnects_before_first_byte`.
- `chat_stream_failover_*`: assert actual HTTP call counts `[2, 1, 1]` for
  first-error/split-error success; `[2, 1, 0]` for visible-output and explicit
  stop cases, exact success bytes including unknown fields, final provider
  usage/token attribution, and every attempted candidate's terminal state.
- `execution_runtime::stream::error::tests` covers complete-record boundaries,
  control-only records, multiline data, incomplete errors and a normal first
  event preceding a later error. Keep same-format Responses regressions green.
- `execute_execution_runtime_stream_records_first_stream_event_before_visible_text`
  drives response execution concurrently, asserts first-byte/streaming before
  releasing visible data, and asserts that the HTTP task is not yet complete.
  Pair with `execute_execution_runtime_stream_records_first_data_as_streaming_before_terminal_telemetry`.
  Never await the precommitted response before releasing the event it requires.
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

```text
Wrong: HTTP 200 headers -> hand off Chat response -> first error -> no next provider.
Correct: classify complete first Chat event -> retry error or deliver original success bytes.
```
