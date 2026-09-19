# Investigated code and validation pointers

- `apps/aether-gateway/src/execution_runtime/stream/commit_policy.rs`: `StreamCommitPolicy`, `StreamCommitGate`, `classify_generic_sse_record`, `value_has_semantic_content`. Responses opening events already return Pending, but text deltas fall through to SemanticEvent. Chat tests currently cover role-only opening, real text and tools.
- `apps/aether-gateway/src/execution_runtime/stream/execution.rs`: policy creation around 6958; observes commit gate at 7251; bounded semantic policies suppress the old NonError shortcut at 7320. Successful bytes flow through original prefetch buffers; do not reintroduce truncation fixed in v0.7.37. Existing test helpers `execute_generic_stream_precommit` and `execute_stream_precommit_for_format` around 9979.
- `apps/aether-gateway/src/tests/ai_execute/stream/chat_failover.rs`: real gateway multi-provider mock, deterministic routing and candidate/usage assertions. Inspect reuse before adding a fixture.
- `apps/aether-gateway/src/tests/ai_execute/stream_cli`: existing Responses request fixtures.
- The two adjacent captures (`3bbabaee`, `43eab184`) use response.created, response.in_progress, empty message item, empty output_text part, then output_text.delta with delta equal to a single U+0020 space. No live content or credentials are needed in the regression.
- Failed audit bodies retain the final error JSON instead of the complete preamble; do not describe the regression as replaying all raw bytes of c65c1e30.
- Existing local image `aether-gemini-builder:20260917`; Cargo cache volumes `aether-trellis-cargo-registry`, `aether-trellis-cargo-git`, `aether-trellis-target`. Parent verifies current availability and owns test processes.
