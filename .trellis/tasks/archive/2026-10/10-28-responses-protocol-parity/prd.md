# Responses protocol compatibility

## Goal
Make Aether's existing Responses gateway handle the four source-backed compatibility gaps identified in the preceding read-only assessment. The user approved implementation of the recommended logic. Preserve native same-format traffic and current provider/account scheduling boundaries.

## Requirements
- R1: Every Aether-synthesized `response.failed` contains a valid accumulated Response (`id`, `object`, `model`, `created_at`, `status=failed`, `error`, `output`, and known usage). Emit one terminal, with monotonic sequence numbers, and no subsequent completion. Preserve incomplete tool arguments without manufacturing successful tool completion.
- R2: Claude signed thinking and redacted thinking survive synchronous and streaming Claude -> Responses -> Claude histories exactly and in order. Use a versioned Aether-owned carrier; remove only validated internal carriers when targeting real OpenAI, preserving real opaque ciphertext. Malformed carriers fail closed.
- R3: Convert Vertex `functionCall.partialArgs` (`jsonPath`, typed values, per-leaf and per-call `willContinue`) into correctly assembled tool argument deltas, including strings, nesting, arrays, sequential and parallel calls, terminal closure and errors. Existing complete `args` calls remain unchanged.
- R4: Responses client-executed tool search and deferred namespace/function definitions can run through Chat and Claude conversion, including discovered tools from tool_search_output, stable unambiguous function aliases, tool choice, stream/nonstream call identity restoration and subsequent tool-result replay. Preserve custom text-tool input/output shapes when lowered to function tools for Claude. Unknown or semantically unsupported grammar/hosted-tool combinations remain fail-closed.

## Evidence
The prior isolated executable smoke showed: OpenAIResponsesClientEmitter::emit_error emits no response; Claude signed thinking loses its signature and replay fails; redacted thinking is rejected; Gemini partialArgs is unsupported; tool_search declarations and deferred namespace functions are rejected. Ordinary text SSE sequence/usage and native compaction_trigger preservation succeeded.

## Acceptance criteria
1. R1 regressions exercise post-visible-content failure, accumulated usage, partial tool arguments, stable identity/timestamp, exactly one failed terminal and no completion.
2. R2 regressions execute real sync and stream conversions and replay the returned output with tool results into Claude, preserving signed/redacted blocks and tool pairing; native provider ciphertext is unchanged.
3. R3 regressions concatenate emitted argument deltas to expected JSON for nested/escaped/interleaved values, multiple calls and finish boundaries; malformed input produces a failed stream, never an empty successful call.
4. R4 regressions exercise search call -> search output with discovered functions -> invocation -> result across Chat/Claude and Responses, plus alias collisions, custom text input and unsupported grammar behavior.
5. Relevant crate tests, format checks, gateway compilation and an executable smoke using the actual stream rewrite and request conversion paths pass. Update protocol documentation after behavior is demonstrated.

## Out of scope
WebSocket multiplexing/steering, Response resource persistence, new public routes, speculative retries, schema weakening, invented tool results, global model lists, broad native wire rewriting and upstream API calls requiring live credentials.
