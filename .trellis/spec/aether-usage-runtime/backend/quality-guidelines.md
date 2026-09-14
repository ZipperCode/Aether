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

### Waiting for asynchronous usage dispatch to drain

- `runtime::tests::wait_for_enqueue_dispatcher_to_drain` must wait for expected
  retry recoveries, zero retry pending, zero lifecycle-submission pending and
  zero ordered-lifecycle pending. Keep its existing five-second deadline and
  one-millisecond polling interval.
- A completed terminal call or an empty retry queue does not imply submission
  bookkeeping has finished: ordered completion can resolve before the outer
  worker joins `slot.execute()` and calls `record_processed`.
- Final-zero assertions must follow the complete drain condition. Do not
  remove assertions, add arbitrary sleeps or change production accounting to
  make a timing-sensitive test pass.
- `enqueue_dispatcher_drain_waits_for_lifecycle_and_ordered_completion` holds
  a real lifecycle worker and then an independent ordered completion. Poll
  the pinned helper future once in each held state, require `Pending`, release
  the fixture and require completion. This deterministically rejects the old
  retry-only condition without depending on scheduler delays.

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)
