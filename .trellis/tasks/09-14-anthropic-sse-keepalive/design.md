# Design

The shared build_sse_body_stream in apps/aether-gateway/src/execution_runtime/stream/execution.rs forwards arbitrary byte fragments and currently inserts timer comments regardless of framing. Observe the bytes actually yielded after filtering/truncation, not raw input and not audit snapshots. Permit the initial heartbeat before any bytes and periodic heartbeats only when the emitted stream is at an event boundary. A tick during a partial event is skipped, not queued.

Use a small local incremental boundary state with constant memory and correct CR/LF/CRLF semantics, reusing existing framing knowledge. Do not buffer or parse complete JSON/events, reset the timer to hide the bug, or disable Anthropic thinking/tools. Keep the wrapper signature and stream/cancellation/terminal policies stable. Cover the existing filter's partial forwarding and overflow behavior; its buffered.is_empty() alone is not sufficient proof of a client-visible boundary.

Both runtime paths call the same wrapper. Same-format Anthropic stays native. No public interface, configuration, schema, dependency or provider-specific rewriting changes. Existing audit captures remain unchanged; validation must consume the final Body/HTTP response to observe injected comments.

Isolated branch codex/anthropic-sse-keepalive starts at 0d3ceb46508406052916936ec19ecb08188cf4c5. The main tree has 22 pre-existing changed/untracked entries. Integration must record and restore any temporarily stashed overlapping WIP and verify unrelated files byte-for-byte. No force cleanup or remote push.
