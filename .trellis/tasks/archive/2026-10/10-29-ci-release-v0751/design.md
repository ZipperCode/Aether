# Design

## Independent repair slices

- Transport test lint: iterate existing contents with `enumerate()` rather than indexing a constant range; preserve exact assertions.
- Gateway dependency ownership: use the established `ai_serving/transport.rs` and format root seams, migrate every violating planner callsite without compatibility aliases or additional allowlists.
- WebSocket quota failover: trace existing two-account integration fixture and real-key admission after quota feedback. Repair the established fixture prerequisite if its account discovery evidence is absent; change production only if evidence proves a runtime defect. Never authorize an unsupported model to satisfy the test.

## Integration

One integration owner runs checks only after edit handoffs. No sibling workers run builds, lint, tests or formatters. Reuse the CI failure logs as failing-before proof; do not rerun known failures unchanged.

## Release invariant

Select the exact pushed commit only after its Rust CI aggregate succeeds. Create an annotated next patch tag, never retag or force. Tag push triggers existing release automation. No production mutation.
