# Design

## Boundaries
Changes belong to aether-ai-formats; keep gateway orchestration, native passthrough, account-scoped scheduling and provider persistence unchanged. Reuse canonical conversion, existing namespace aliases, report_context.original_request_body and the standard stream rewriter. Do not copy upstream implementations or add dependencies.

## Response lifecycle and Claude replay
The core implementation owner owns all shared canonical, registry, Claude parser/emitter, Responses request/response/stream and stream-matrix files. Reuse terminal_response to build failed snapshots without validating incomplete arguments as successful tool completion. Retain accumulated usage before terminal errors, emit once, and make subsequent finish/emit calls inert.

Claude replay uses a bounded versioned Aether-owned encoded carrier for original thinking/signature or redacted_thinking blocks. Decode only structurally valid owned carriers; reject invalid owned envelopes on conversion and strip only decoded valid carriers before native OpenAI egress. Ordinary provider ciphertext remains opaque. Preserve multiple thinking blocks and ordering in sync and streaming output, including reasoning-only/redacted-only blocks. Stream metadata may use a narrowly typed canonical event when existing events cannot preserve the original block; migrate every consumer exhaustively.

## Vertex partial arguments
A separate focused module owns a per-call assembler and JSON-path decoding. Preserve complete args semantics and identify continuing calls with explicit ids or the active per-candidate call, never with a stream chunk's transient part index. Emit safe append-only deltas when possible; buffer until safe for non-depth-first path order/interleaved leaves. Reject conflicting containers, duplicate scalar values, incomplete strings or unterminated calls rather than synthesize an empty success. Bound depth/index/frame growth consistently with current parser limits. Shared canonical response auditing changes are integrated by the core owner, not concurrently edited.

## Tool discovery and lowering
An isolated tool adaptation owner extends the existing namespace alias module or adds an adjacent module. Define a request-local mapping from Responses tool identity (namespace/name/custom/search) to collision-safe target function name using original_request_body plus discovered tool_search_output tools. No global cache or persistent state. Only client execution tool_search is lowered; hosted search and custom grammars without equivalent semantics remain rejected. Deferred functions are not offered until present in a search result; searched definitions are then offered with reversible identity. Search itself is a function whose returned calls become tool_search_call and whose corresponding result becomes tool_search_output; custom text tools use a one-string input schema and preserve the original raw string on return. Tool choice resolves against the same mapping. Restore response shapes in sync, stream and replay. Core owner integrates shared registry/canonical/emitter changes from explicit tool API contracts.

## Ownership and integration
- Core owner: R1/R2 and every edit to protocol/canonical.rs, protocol/stream.rs, formats/registry.rs, openai/chat/stream.rs, Claude request/response/stream and Responses request/response/mod; integrates narrowly specified helper hooks for tools and Vertex.
- Vertex owner: gemini/generate_content/stream.rs and new adjacent partial argument module/tests only.
- Tools owner: openai/namespace.rs and new adjacent tool adaptation modules/tests only. Publishes helper API and integration instructions to core owner.
- Parent: planning, integration review, docs, executable smoke, final once-afterward checks and commit. No worker runs build/lint/tests/formatters mid-flight.

## Compatibility
No public API removal, resource persistence or schema migration. Internal enum additions are exhaustive. Native Responses fields/events, real encrypted_content, original ids and compaction_trigger remain unchanged except existing documented transport edits. Aether carriers are scoped to their origin protocol and never passed as real vendor ciphertext.
