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

## 同步图片响应转流的单次转换边界

- `stream_pump` 仅在实际完成同步 JSON→客户端 SSE 后向内部帧头写入 `x-aether-bridged-client-sse: 1`；读取上游响应时先剥除同名头，不能相信上游声明已转换。
- relay 读取标记后立即移除，不能将内部标记暴露给客户端或写入上游响应报告。标记仅用于跳过 provider normalizer、rewriter 与 observer；终态仍消费帧泵的 EOF summary。
- “帧泵已转换”与“relay 已消费原 JSON 并自行输出替代 SSE”是两个状态。前者不能触发 Data 帧跳过，否则 HTTP 200 会得到空 body。
- 不通过 `upstream_is_stream=false` 加 SSE Content-Type 猜测格式；上游可以无视请求的流模式，只有实际转换动作能建立该证据。
- 原生 `image_generation.*` / `image_edit.*` 事件保持块字节、未知字段、顺序与每张图片的 completed；`n>1` 不能因第一张完成而截断后续图片。Responses→Images 转换规则保持原样。
- 回归须覆盖本机真实 HTTP 图片同步响应→下游 SSE、内部标记不泄露、伪造标记不能禁用转换、分片/CRLF 原生事件与多图片终态保真；仅 runtime stub 计划断言不能证明最终客户端字节正确。

## Long-stream cleanup and terminal evidence

### 1. Scope / Trigger
Changes to stale usage cleanup, request lifetime guards, or candidate finalization.

### 2. Signatures
`UsageWriteRepository::heartbeat_active_request(request_id, now_unix_secs)` updates the database-owned `usage.request_metadata.runtime_heartbeat_unix_secs`. `request_lifecycle::with_usage_heartbeat` binds its task to the existing cloneable admission permit.

### 3. Contracts
- Client disconnect cancels upstream execution by default for HTTP and Responses WebSocket requests. Missing `cancel_on_client_disconnect` uses `true`; explicit `false` opts into background completion and must survive serialization in routing snapshots.
- Heartbeats follow actual execution, including detached draining when cancellation is explicitly disabled; the last owner drop aborts the task. They do not touch usage timing snapshots, billing or terminal status.
- Cleanup uses its existing abandonment grace against a fresh heartbeat; request age alone cannot finalize a live stream. No stream total-duration limit is implied.
- `streaming` and HTTP 200 are not completion evidence. Only terminal candidate success with `stream_completed=true` can recover success; cleanup never manufactures candidate success.
- Accepted terminal failed/cancelled stream usage may repair only its exact legacy success candidate without completion evidence. Genuine success, other candidates, and stale lifecycle updates retain existing immutability.
- Stream read errors retain typed timeout/reset-or-closed/decode/protocol categories before formatting. Operational logs contain only fixed categories and IDs, not error-chain text, URLs or payloads.
- `direct_stream_terminal_diagnostics` reports bounded SSE category counts, byte counts, last content progress and terminal observation. Heartbeat events do not advance content progress. Diagnostic classification must not expand the terminal detector's accepted protocol markers or change forwarded bytes.

### 4. Validation & Error Matrix
| Condition | Result |
| --- | --- |
| Fresh heartbeat, old creation time | Preserve pending/streaming |
| Heartbeat ceases beyond abandonment grace | Existing abandoned-request failure cleanup |
| Explicit completed candidate | Recover usage, do not fabricate candidate end time |
| Legacy false success followed by real stream failure | Repair serving candidate in accepted usage transaction |
| Malformed heartbeat value | No SQL cast failure or batch poisoning |

### 5. Good / Base / Bad Cases
Good: a stream running for hours remains active while owned. Base: terminal updates stop heartbeat writes. Bad: removing streaming-as-success while leaving creation-only failure cleanup merely changes false success into false failure.

### 6. Tests Required
`live_stream_cleanup_preserves_heartbeat_and_reclaims_abandoned_requests` exercises PostgreSQL with old live, abandoned, completed and malformed-metadata rows. `live_stream_terminal_repairs_only_legacy_false_success_candidate` preserves genuine/nonstream success and unrelated candidates. `usage_heartbeat_guard_follows_body_and_detached_execution_lifetime` checks cancellation, EOF and background ownership.

### 7. Wrong vs Correct
Wrong: `status=streaming` means completed, or increase the timeout to hide the bug. Correct: separate live execution evidence, terminal outcome evidence and abandoned-request cleanup.
