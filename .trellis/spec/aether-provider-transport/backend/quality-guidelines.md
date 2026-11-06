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

<!-- What level of testing is expected -->

(To be filled by the team)

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)

## Claude Code injected cache TTL

`apply_claude_code_body_mimicry` scans protocol cache markers in `tools`, `system`, and message content before rewriting the system blocks. If any client marker uses `ttl: "1h"`, the injected expansion block also uses `1h`; otherwise it retains `5m`. Upstream requires longer-TTL blocks before shorter-TTL blocks. Do not scan arbitrary tool arguments or schema data, and keep genuine Claude Code requests idempotent.

`injected_cache_ttl_follows_client_one_hour_markers_from_any_scope` covers all three scopes and repeated application; the existing system rewrite regression asserts the default `5m` path.
