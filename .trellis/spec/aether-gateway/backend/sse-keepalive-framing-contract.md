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
