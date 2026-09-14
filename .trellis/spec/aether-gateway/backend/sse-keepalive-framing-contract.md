# SSE Keepalive Framing

## 1. Scope / Trigger

`execution_runtime/stream/execution.rs::build_sse_body_stream` forwards arbitrary transport fragments. A timer firing between fragments must not insert a comment inside an SSE JSON value. This applies to every existing caller that enables gateway-generated keepalives, including native Anthropic tool streams.

## 2. Signatures

The private `build_sse_body_stream(prefetched_chunks, rx, filter_control_blocks, emit_keepalive, anthropic_message_stop_terminates_body, keepalive_interval)` signature is unchanged. `SseEventBoundary::observe(&[u8])` tracks framing with three booleans; no public API, configuration, schema or dependency is introduced.

## 3. Contracts

- Observe only bytes actually emitted after control filtering and terminal truncation, including prefetched bytes.
- `event_open` is true after a nonempty line begins and until an empty SSE line ends the record. LF, CR and CRLF are line endings; an LF following a CR is part of the same ending, including across chunks. Whitespace-only lines are not empty.
- Emit an initial heartbeat before any client bytes if existing behavior requests it. Emit periodic heartbeats only at a completed-record boundary; skip unsafe ticks without queuing them or delaying upstream fragments.
- Preserve exact non-heartbeat bytes, tools/IDs/arguments, thinking/signatures and native `message_stop` closure. Preserve the OpenAI policy that disallows synthetic SSE controls.
- Do not infer client-visible boundaries from the control filter's buffer length: the filter forwards partial records and may clear its size-limited buffer.
- Existing audit capture runs before the heartbeat wrapper. Captured JSON equality does not prove final HTTP wire integrity.

## 4. Validation & Error Matrix

| Situation | Required behavior |
| --- | --- |
| Idle before first data or between complete records | Existing heartbeat behavior |
| JSON, UTF-8 character, escape or record separator is partial | Forward immediately; skip heartbeat |
| Complete record followed by part of another in one chunk | Skip heartbeat until the second record ends |
| CR at end of one chunk, LF at start of next | Treat as one line ending |
| Native Anthropic `message_stop` with sender still open | End downstream body; no later heartbeat/data |
| Unsafe tick skipped | Existing tracing debug event `sse_keepalive_skipped_partial_event`; no raw payload |

## 5. Good / Base / Bad Cases

- Good: `data: {` then a delayed JSON suffix: keep streaming bytes, omit the intervening heartbeat.
- Base: `data: {}\n\n` then idle: a heartbeat is safe.
- Bad: `data: {` + `: aether-keepalive\n\n` + JSON suffix: client JSON parsing fails although upstream data and pre-wrapper audit are valid.

## 6. Tests Required

`execution_sse_body_tests.rs` exercises the real output function with prefetched/live fragments, filter enabled/disabled, LF/CRLF/CR, UTF-8/escaped arguments, buffer overflow heartbeat gating and native termination. The loopback HTTP test asserts final non-heartbeat bytes, parsed events, reasoning/signatures, consecutive tool calls and the next request's result IDs. The primary split-event regression must fail when the old unconditional output function is restored.

The overflow case proves heartbeat safety only. The pre-existing control filter can still drop an ordinary continuation after its >1 MiB buffer reset; general oversized-record forwarding is outside this change.

## 7. Wrong vs Correct

```text
Wrong: timer fires -> yield a comment regardless of previously emitted bytes.
Correct: observe emitted bytes -> timer fires -> yield only if no event is open.
```
