# Protocol & Stream Review: PR #881 & PR #840

## 1. PR #881: Canonical Stop Sequences Array Serialization

- **Source**: Upstream PR #881 (`fix(formats): always serialize stop as JSON array in write_openai_generation_config`)
- **Upstream PR Diff / URL**: `https://github.com/fawney19/Aether/pull/881` (commit `7776232f8f6b060fcd40a030ecc6497823e9ad49`)
- **Root Cause**:
  When clients send requests using the Claude protocol with a single stop sequence (e.g. `stop_sequences: ["</block>"]`), Aether's outbound conversion to OpenAI Chat Completions previously flattened single-element lists into a JSON string (`"stop": "</block>"`). Upstream providers and proxies with strictly-typed schema definitions (such as Go structs typing `Request.stop` as `[]string`, e.g., CodeBuddy 2API / wb2api) rejected these requests with HTTP 400: `json: cannot unmarshal string into Go struct field Request.stop of type []string`.
- **Exact Code Change**:
  In `crates/aether-ai/formats/src/protocol/canonical.rs` (`write_openai_generation_config`):
  - Removed single-element string flattening (`if values.len() == 1 { Value::String(...) }`).
  - Unified serialization of `config.stop_sequences` to always emit a JSON array (`Value::Array(...)`) when non-empty.
  - Omits `"stop"` when `values` is empty.
- **Verification & Test Details**:
  - Test command:
    ```bash
    CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUST_MIN_STACK=16777216 cargo test -p aether-ai-formats
    ```
  - Test result: **1,087 unit tests passed** (0 failed).
  - Focused tests added:
    - `protocol::canonical::tests::test_write_openai_generation_config_stop_always_array`: Validates single item, multiple items, and empty stop sequences.
    - `protocol::canonical::tests::claude_to_openai_chat_request_stop_sequences_always_array`: Validates end-to-end Claude-to-OpenAI conversion retaining `stop` as array.

---

## 2. PR #840: Stream Prefetch Handoff & Overflow Assessment

- **Source**: Upstream PR #840 (`fix(gateway): replay prefetch overflow bytes into stream rewriter`) / Issue #839 (`https://github.com/fawney19/Aether/pull/840`, commit `f3cfa971151017d08aea4d387da67439dc95caa8`)
- **Problem Analyzed in #840**:
  Upstream stream prefetch capture `provider_prefetched_body` was capped at `MAX_STREAM_PREFETCH_BYTES` (16 KiB). In upstream's architecture, the downstream pump task discarded the prefetch-phase stream rewriter / private normalizer and attempted to rebuild them from scratch by replaying `provider_prefetched_body`. When upstream SSE frames crossed the 16 KiB cap, bytes past the cap were dropped during pump task replay, emitting a corrupt SSE line to downstream clients. Upstream PR #840 fixed this symptom by adding a secondary `provider_prefetched_overflow: Vec<u8>` buffer and concatenating it with `provider_prefetched_body` for replay.
- **Superseded Reasoning (Covered by Local Commit `f18dad7ab`)**:
  - Local commit `f18dad7ab` ("fix: 修复 SSE 预读交接丢失完整分片") had already re-architected this handoff fundamentally:
    1. Rather than discarding state and reconstructing rewriters from replay buffers, `execute_stream_from_frame_stream_with_retry_scope` transfers the live stateful rewriters (`private_stream_normalizer` and `local_stream_rewriter`) directly into the background pump task via `into_owned()`.
    2. The background pump task processes incoming stream frames starting from where prefetch ended, with zero loss of parser state and no re-parsing.
    3. Prefetched chunks already processed during prefetch are streamed directly downstream via `build_sse_body_stream`.
    4. Telemetry logging capture is completely decoupled from stream execution parser continuity via `append_budgeted_stream_capture_bytes`.
  - Therefore, PR #840 is **fully covered and superseded** by the local architecture. Adding secondary overflow buffers and replay concatenation would introduce unnecessary allocation and duplicate parsing.
- **Exact Regression Tests**:
  Located in `apps/aether-gateway/src/execution_runtime/stream/execution_prefetch_handoff_tests.rs`:
  - `execution_runtime::stream::execution::prefetch_handoff_tests::responses_prefetch_handoff_preserves_full_fragments`
    - Tests 18 KB, 20 KB, and 75 KB wire payloads across chunk sizes 5,000, 6,000, 16,383, 16,384, 16,385, and full wire length.
  - `execution_runtime::stream::execution::prefetch_handoff_tests::responses_prefetch_handoff_preserves_partial_records_and_split_utf8`
    - Tests split frames across early semantic commits and multibyte UTF-8 code point boundaries across handoff.
  - `execution_runtime::stream::execution::prefetch_handoff_tests::private_normalizer_prefetch_handoff_preserves_full_fragments`
    - Tests Gemini private envelope normalizer preserving full fragments across 16 KiB boundaries.
  - `execution_runtime::stream::execution::prefetch_handoff_tests::passthrough_prefetch_handoff_preserves_original_bytes`
    - Tests passthrough mode handoff across 16 KiB chunk thresholds.
- **Test Status**:
  - All four handoff regressions passed in the coordinator workspace test run; that aggregate run later failed due to missing native PostgreSQL binaries and externally removed build artifacts.

---

## 3. Related Transport Contract Alignment: `SameFormatProviderRequestBodyInput.is_vertex`

- **File**: `crates/aether-provider/transport/src/antigravity/request.rs`
- **Context**: PR #890 introduced `is_vertex: bool` to `SameFormatProviderRequestBodyInput`.
- **Resolution**: Added `is_vertex: false` in the Antigravity test request builder (`antigravity/request.rs:1545`), aligning with Antigravity envelope expectations. Confirmed all other workspace initializers already provide `is_vertex`. Verified with `rustfmt --check`.

---

## 4. End-to-End Real Gateway Smoke Verification (#881 & #890)

Tested against the live running gateway process on `http://127.0.0.1:18089` with an in-process local mock upstream on port 19876.

### 4.1 #881: Outbound Stop Array Verification
- **Client Inbound**: `POST /v1/messages` (Claude Messages format)
  - Model: `smoke-claude-test`
  - Stop sequences: `["</block>"]`
- **Gateway Route**: Cross-format conversion from Claude Messages to OpenAI Chat format (`openai:chat`).
- **Mock Upstream Outbound Captured**:
  - Request path: `POST /v1/chat/completions`
  - Model: `gpt-4o-mock`
  - `stop` field: `["</block>"]` (strictly a JSON array, `Array.isArray(body.stop) === true`)
- **Client Response**: HTTP 200 with converted Claude message payload.

### 4.2 #890: Native Gemini functionResponse.id Preservation
- **Client Inbound**: `POST /v1beta/models/smoke-gemini-test:generateContent`
  - Payload contained conversation history with function calling:
    `parts[0].functionResponse = {"name": "lookup", "id": "call_abc_123", "response": {"output": "data_found"}}`
- **Gateway Route**: Native Gemini provider (`provider_type: "custom"`, `is_vertex: false`).
- **Mock Upstream Outbound Captured**:
  - Request path: `POST /v1beta/models/gemini-2.5-mock:generateContent`
  - Captured functionResponse: `{"name": "lookup", "id": "call_abc_123", "response": {"output": "data_found"}}`
  - Result: `functionResponse.id` is preserved (`id === "call_abc_123"`, not stripped).
- **Client Response**: HTTP 200 with candidate completion.
