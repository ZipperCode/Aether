# Design

## Boundaries
Protocol repairs live in `crates/aether-ai/formats`. The gateway planner expands scoped Responses history before request conversion and pins a tool-only identity projection in report context; gateway changes are limited to this existing continuation boundary and its exports/tests. Registry remains the strict cross-format audit boundary. Same-format Responses bypasses canonical conversion.

## Reasoning lifecycle
Allocate the synthetic reasoning item ID from `reasoning_output_index`. A completed reasoning item remains open until both its text and carrier metadata are available; item-level maps reconcile done snapshots once. Response validation rejects Claude-owned carriers for Gemini before emission. Stream emitter errors become the same accumulated failed Response path as provider parse errors.

## Vertex lifecycle
Track active partial calls per candidate as a set, not a single overwrite slot. Idless fragments require exactly one active owner. Parse RFC 9535 quoted path keys with Unicode escapes and surrogate validation. A candidate finish validates only its own calls; overall stream completion waits for remaining candidates.

## Tools and history
Build ordinary-name collision counts from declared and discovered flat candidates. Apply the same lowered identity/type mapping to `allowed_tools`. Pin declared and discovered tool definitions from the effective expanded body in `openai_responses_tool_identity_context`; do not copy conversation content. Sync/SSE restoration consumes the pin, with scoped history expansion for direct library callers lacking it. Missing history fails closed. A pin restores the current response even after history expiry, but is never enough to save a full continuation transcript. Keep existing Chat-only history persistence scope.

## Failure semantics
Any checked carrier rejection after visible output invokes `emit_error` with retained state. Failed hosted search output uses an unsuccessful status, never `completed`. Preserve one terminal event and known usage.