# Preserve Anthropic tool streams across gateway keepalives

## Goal
Fix the approved, reproducible shared SSE-output defect that can interrupt native Anthropic tool calls when a gateway heartbeat is inserted inside an unfinished event.

## Confirmed background
- User observed failed/repeated/stopped tool calls through official BigModel GLM and explicitly approved implementation of the reviewed plan.
- Production image 0.7.33 revision c2c30e60d3543fbde19ebc6e1d9f083033649ee0 and local master 0d3ceb46508406052916936ec19ecb08188cf4c5 share unconditional periodic heartbeat insertion in build_sse_body_stream.
- Claude 2.1.236 reports JSON Parse error: Property name must be a string literal, then non-streaming fallback. Two errors followed first byte by about 15.3 and 44.9 seconds, matching the 15-second timer.
- Stored upstream/client events are captured before the heartbeat wrapper. Their equality does not establish final wire integrity. A minimal split-event experiment reproduces the parser failure; production wire A/B has not been performed.

## Requirements
- Insert gateway keepalives only between complete SSE events; skip ticks while a partially emitted event is open.
- Track both prefetched and subsequent emitted bytes, including split LF/CRLF separators. Retain progressive forwarding with constant additional memory.
- Preserve native Anthropic fields, tool argument bytes, IDs, reasoning/signatures and terminal behavior. No new conversion, retries, public API, configuration, schema or dependencies.
- Repair the shared owner so every current caller gets correct framing. Preserve existing OpenAI no-synthetic-control policy.
- Keep existing main-worktree Antigravity WIP intact. Implement in an isolated worktree and merge back to recorded master.

## Acceptance
- [x] A regression invokes the actual SSE output function and fails on the original unconditional heartbeat implementation.
- [x] Prefetched/live fragments, consecutive tool calls, escaped/UTF-8 content, LF/CRLF splits and message_stop are covered.
- [x] A local mock upstream validates final HTTP response bytes and complete tool-call/result correspondence without real-provider inference.
- [x] Scoped SSE tests, gateway Rust check and changed-file formatting pass; independent review completes.
- [x] Contract recorded, task-scoped changes committed and merged into master; original WIP preserved.

## Exclusions
No production deployment/restart/configuration changes, paid inference, credential changes or changes to the unrelated Antigravity task.
