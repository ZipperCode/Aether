# Quality Guidelines

## Fixed stream error classification

`MsgType::StreamError` carries a bounded UTF-8 message, not a JSON object with
an `error_code` field. The sender's existing `safe_stream_error_message`
projection must preserve the trusted static `response_too_large` category
produced by both declared-length and streamed-size collectors. Keep generic
fallback for unknown upstream text; do not raise body limits or expose raw
diagnostics to make the message more specific.

Both `declared_oversized_response_emits_stable_stream_error` and
`streamed_oversized_response_emits_stable_stream_error` must keep their original
wire-message assertions. Gateway's separate peer-error projection is a distinct
contract and is not implicitly changed by a sender-side registration fix.

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

<!-- What level of testing is expected -->

(To be filled by the team)

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)
