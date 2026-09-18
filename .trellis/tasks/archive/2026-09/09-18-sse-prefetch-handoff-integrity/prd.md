# 修复预读交接导致的 SSE 数据缺失

## Goal

Preserve every byte consumed by gateway prefetch when handing an upstream SSE response to live forwarding. Valid upstream JSON must remain complete downstream regardless of transport fragmentation.

## Requirements

- R1: Parser/normalizer/usage-observer restoration receives the complete consumed prefetch data, including the final fragment crossing 16 KiB.
- R2: Preserve the independent 16 KiB inspection/commit budget, existing audit capture limits, and prompt release of replay buffers after restoration.
- R3: Preserve event order, tool descriptions, unknown fields, UTF-8, terminal events and usage without missing or duplicate output.
- R4: Cover Responses compatibility rewriting, the shared private-normalizer path and a passthrough control through real gateway execution.
- R5: No public API/config/dependency/schema changes. Do not change malformed-JSON passthrough or the independent 1 MiB control-filter issue. No push, release or deployment.

## Acceptance Criteria

- [x] A new gateway regression fails against the old truncating implementation and passes after the fix.
- [x] Approximately 18 KB, 20 KB and 75 KB legal events survive 5,000, 6,000 and 16,384 byte fragments and boundary-offset splits.
- [x] Complete event plus partial next event, split UTF-8, and normal termination retain content/order exactly with no duplication.
- [x] Private-normalizer and passthrough coverage pass; precommit failover, first-byte accounting and capture-budget regressions remain green.
- [x] Relevant Rust checks, formatting and scoped review complete with an evidence receipt.
- [x] Local Chinese-message work commit complete: `f18dad7ab957e72dd971489c4c3093d42230e85f`.

## Notes

- User approved this plan explicitly on 2026-09-18. Baseline: master at dd66cac6f, clean working tree.
- Deployment identity and request-specific routing remain separate evidence gaps; a source regression does not prove the production incident's attribution.
