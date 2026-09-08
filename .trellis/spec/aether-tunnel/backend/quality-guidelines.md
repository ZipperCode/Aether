# Quality Guidelines

## Fixed stream error classification

### 1. Scope / Trigger

Apply when changing bounded-body rejection or the sending-side stream error projection.

### 2. Signatures

`MsgType::StreamError` carries a bounded UTF-8 message, not a JSON object with
an `error_code` field. `safe_stream_error_message(&str) -> &'static str` owns
the sender's fixed message projection.

### 3. Contracts

The projection must preserve the trusted static `response_too_large` category
produced by both declared-length and streamed-size collectors. Keep generic
fallback for unknown upstream text; do not raise body limits or expose raw
diagnostics to make the message more specific.

### 4. Validation & Error Matrix

| Input | Wire output |
| --- | --- |
| Known `response_too_large` rejection | Exact fixed `response_too_large` text |
| Unknown upstream diagnostic | Existing bounded generic projection |

### 5. Good / Base / Bad Cases

- Good: declared and accumulated size rejection retain the same fixed category.
- Base: unrelated known stream failures retain their existing projection.
- Bad: a valid fixed internal code is replaced by generic text or raw diagnostics.

### 6. Tests Required

Both `declared_oversized_response_emits_stable_stream_error` and
`streamed_oversized_response_emits_stable_stream_error` must keep their original
wire-message assertions. Gateway's separate peer-error projection is a distinct
contract and is not implicitly changed by a sender-side registration fix.

### 7. Wrong vs Correct

```text
Wrong: weaken the two expected wire values or raise the configured body limit.
Correct: register the exact internal category in the existing sender projection.
```

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
