# 支持 Antigravity 账号套餐识别与展示

## Goal

Allow administrators to distinguish Antigravity Free, Pro, and Ultra accounts in provider account management.

## Requirements

- Use authoritative Antigravity account information; consult the public AntigravityManager implementation for tier detection semantics.
- Carry account tier through existing OAuth/quota metadata and admin API paths without a database migration or changes to scheduling.
- Display the account tier alongside existing account identity in the relevant list and detail surfaces using existing badge conventions.
- Do not classify missing, failed, or unrecognized tier information as Free.
- Preserve all existing quota and other-provider behavior.

## Acceptance Criteria

- [x] Free, Pro, and Ultra accounts can be distinguished in Antigravity account management.
- [x] A refresh updates account tier through the existing account metadata path.
- [x] Unknown or unavailable tier evidence does not display a false Free classification.
- [x] Regression checks cover tier selection precedence and unknown information; the actual UI is smoke-verified.

## Out of Scope

- Tier-based scheduling, filtering, billing, or account entitlement changes.
- Unrelated UI redesign, database schema changes, or fetching real account credentials for verification.
