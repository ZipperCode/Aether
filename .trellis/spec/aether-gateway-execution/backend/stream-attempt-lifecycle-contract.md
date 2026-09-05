# Stream Attempt Lifecycle Contract

## 1. Scope / Trigger

Use this contract when changing streamed candidate retries, first-byte
timeouts, downstream cancellation, usage settlement, stream commit policy, or
protocol terminal events. One request may try several candidates, but every
candidate attempt must reach exactly one durable terminal state.

## 2. Signatures

```rust
struct StreamAttemptTerminalGuard { /* attempt and usage settlement state */ }

let request_first_byte_deadline = request_first_byte_started_at + timeout_duration;
let candidate_budget = request_first_byte_deadline.saturating_duration_since(now);
```

The guard is armed while a candidate is pending and is explicitly disarmed only
after normal terminal persistence. Dropping an armed guard schedules the same
cancelled settlement path.

## 3. Contracts

- The first-byte timeout is one absolute request-level deadline. Candidate
  failover consumes the remaining budget; a retry must not restart the clock.
- `StreamAttemptTerminalGuard` is the single fallback owner for an abandoned
  pending attempt. Do not add a second cancellation guard or an independent
  drop-based terminal writer.
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
  terminal writer. Started terminalization must be allowed to finish.

## 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Candidate one consumes most of the first-byte budget | Candidate two receives only the remaining time. |
| Retry begins after the absolute deadline | Fail immediately with first-byte timeout. |
| Client disconnects before first byte | Persist one cancelled candidate and one cancelled usage outcome. |
| Normal terminal path completes | Disarm the guard; no duplicate drop settlement. |
| Non-empty Gemini thought is emitted | Commit that candidate and forbid later failover. |
| Signature-only Gemini control part arrives | Do not commit solely for that part. |
| Provider fails after visible output | Emit the protocol failure terminal in the same stream. |
| Client closes after a complete terminal event | Preserve completed/failed terminal state, not cancellation. |

## 5. Good / Base / Bad Cases

- Good: candidate one times out after using 80% of the first-byte budget;
  candidate two has only the final 20% and cannot extend the request deadline.
- Base: a candidate reaches its normal terminal write and disarms the guard;
  dropping the completed future performs no second write.
- Good: Gemini emits visible thought text and then reports a malformed function
  call; the client receives the thought followed by one complete failure event.
- Bad: each retry receives a fresh first-byte timeout, or two drop guards race to
  settle the same candidate and usage record.

## 6. Tests Required

- Gateway lifecycle:
  `gateway_settles_stream_attempt_when_client_disconnects_before_first_byte`.
- Candidate loop:
  `stream_candidate_retry_does_not_reset_an_expired_request_first_byte_budget`
  and watchdog/terminalization ordering regressions.
- Stream execution: guard drop settles once, normal disarm avoids duplicates,
  and `malformed_antigravity_function_call_streams_thought_then_fails_in_band`.
- Usage runtime: cancelled/bodyless attempts retain identity and terminal usage
  facts without estimating request or partial-response tokens.
- Integration transport: HTTP/1 and h2c truncated SSE tests publish the partial
  body before closing the connection and produce a deterministic partial-body
  error.

## 7. Wrong vs Correct

### Wrong

```text
candidate one times out
-> start a fresh full first-byte timer
-> drop path and watchdog both write cancellation
```

### Correct

```text
compute one request deadline
-> pass only remaining time to each candidate
-> keep one armed terminal guard
-> normal terminal write disarms it; abandonment settles once on drop
```
