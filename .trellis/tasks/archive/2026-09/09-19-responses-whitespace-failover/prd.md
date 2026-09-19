# Responses leading-whitespace failover

## Goal

Keep a Responses request eligible for provider failover while upstream has sent only opening events and empty or whitespace-only text. The user explicitly requested a fix after the investigation of `c65c1e30-8499-4928-946e-016c36da0492`.

## Evidence

- Live v0.7.37 request: `gpt-5.6-sol`; Codex was skipped, xmapi was attempted once. HTTP 200 was returned at 23:02:47.281 on 2026-09-19, before the 503 failure recorded at 23:02:56.431. Later providers had Responses bindings. No stop-transfer rule or transfer limit explained the stop.
- Adjacent xmapi responses began with created/in-progress, an empty message/content part, and `response.output_text.delta` with a single space before real model output.
- `execution_runtime/stream/commit_policy.rs::classify_generic_sse_record` treats a Responses text delta, including whitespace, as a semantic event. The exact failed request's initial SSE records were not retained; the whitespace trigger is supported by adjacent captures and current source, not an exact live replay.

## Requirements

- R1: While uncommitted, opening events and empty/whitespace-only text deltas do not commit downstream success. Reuse existing prefetch deadlines and byte limits.
- R2: A retryable error after that preamble reaches the existing next-candidate path, with correct failed/success candidate and usage attribution.
- R3: A successful response retains the original upstream bytes, including leading whitespace and unknown fields, exactly once.
- R4: Meaningful text, tool calls/arguments, and legitimate terminal events retain their existing commit boundary. Errors after such output remain in the same stream.
- R5: No new setting, dependency, retry mechanism, logger, or unbounded buffering. No production data/configuration changes.

## Acceptance

- A gateway-level fixture calls provider A and then B when A sends opening events plus whitespace and then a retryable 503; the client receives only B's successful stream and usage belongs to B.
- Tests cover fragmented whitespace events, successful leading whitespace preservation, and no failover after real text/tool output; meaningful existing precommit, first-byte, and replay regressions remain green.
- The actual new regression fails against the old production logic, then passes after the fix.
- Independent review, relevant formatting/lint checks, and a local Chinese-language commit complete the task. Push, release and deployment are separate.

## Scope

The current repair owns the stream commit boundary for the c65c1e30 symptom. Earlier model-discovery/binding and usage-audit storage defects are separate and are not silently bundled into this change.
