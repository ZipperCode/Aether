# Quality Guidelines

> Code quality standards for backend development.

---

## Overview

<!--
Document your project's quality standards here.

Questions to answer:
- What patterns are forbidden?
- What linting rules do you enforce?
- What are your testing requirements?
- What code review standards apply?
-->

(To be filled by the team)

---

## Forbidden Patterns

<!-- Patterns that should never be used and why -->

(To be filled by the team)

---

## Required Patterns

<!-- Patterns that must always be used -->

(To be filled by the team)

---

## Testing Requirements

- **Test runner stack size**: Integration scenarios (`aether-integration-tests`, e.g. `responses_websocket_e2e`) instantiate in-process Aether gateways, temporary PostgreSQL stores, mock upstream servers, and multi-turn WebSocket pipelines. Because debug-profile async futures hold unoptimized nested state across await points, test runner threads must be allocated at least 16 MiB stack space (`RUST_MIN_STACK=16777216`). The CI workflow `check_integration_scenarios` and local test invocations must set `RUST_MIN_STACK=16777216` to prevent test thread stack overflow aborts.
- **Complete failure collection**: Tests must never be suppressed, ignored, or retried to mask stack overflow or async timing issues. All tests collect and report their results deterministically.

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)
