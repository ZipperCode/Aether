# Implementation and delivery status

## Implemented

- Pool row/score/routing-allowlist/sticky hydration re-resolves each account's upstream model through the existing scheduler-core resolver before permission, quota and payload use.
- NULL model lists retain the row's configured default/provider mapping; explicit empty or incompatible lists remain denied. No model IDs fabricated, no auto-fetch toggles modified.
- Request operation and frozen per-format directive routing semantics are propagated through lazy/static materialization. Static image/video/files paths retain explicitly-disabled raw semantics; no per-page policy reload or temporary per-group map.
- Model-specific quota windows use each actual member target.
- Memory candidate listing mirrors PostgreSQL logical pool representative projection; expanded group reads retain actual account rows.
- Real loopback HTTP regressions cover first-account404 retries followed by account-specific mapped success and explicit empty-list no-send. Cursor boundaries cover raw custom suffixes and permission denial. Existing contract updated.

## Verified

See verification.md for exact checks and full-suite failures. Relevant pool tests, real HTTP scenarios, memory candidate tests, formatter and Clippy passed. Full gateway suite5602passed4failed3ignored. Missing local PostgreSQL tools and independent Kiro import assertion prevent reporting a fully green suite. No failed test suppressed or assertion re-pinned.

## Incident analysis

See research.md. Window20:09:54–21:07:12+08,271Gemini Flash requests,542Antigravity404attempts on1account, sustained key_model_disabled pool exhaustion;225fallback successes,46final503s. Signature/auth/TLS/quota failures in fallback providers are independent and outside this fix.

## Delivery boundary

No remote database/configuration/service changes or deployment. No real provider inference calls. The user explicitly requested committing after receiving the full verification results; commit the isolated repair without claiming the full suite passed. Production deployment remains outside this task.
