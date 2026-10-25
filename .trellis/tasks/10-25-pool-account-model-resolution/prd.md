# Pool account model resolution

## Goal
Fix pool failover after the first Antigravity account disables model fetching, and classify subsequent Gemini Flash requests on 192.168.2.212.

## Evidence
Request dd5360f9-df10-44c4-8535-ca14708d08a7 at 2026-10-25 20:09:54+08 selected bare gemini-3.8-flash. Its first account had allowed_models=NULL; 155 other active accounts held fetched suffixed model IDs. Both first-account attempts returned upstream404. At20:09:56.093 pool_group_exhausted logged key_model_disabled157. dispatch/pool_scheduler.rs::pool_candidate_from_row clones the group's selected model while replacing only key fields. Transport eligibility then compares exact model names.

## Requirements
- Re-resolve the upstream model per pool member using existing scheduler mapping rules, endpoint and request-operation scope before permission checks and payload construction.
- Cover first-account model fetching disabled followed by enabled accounts; never bypass explicit account restrictions or fabricate model IDs.
- Cover pool score, normal page, sticky/catalog candidate paths that hydrate accounts.
- Inspect subsequent gemini-3.8-flash requests read-only and separate same-cause pool failures, upstream errors, and final request outcomes.
- No production deployment, credential disclosure, model calls to real providers, schema changes, or remote configuration mutations.

## Acceptance
- Deterministic regression fails before the fix, passes after, and proves fallback sends an account-listed suffixed model instead of rejecting all later accounts.
- Provider mapping priority and endpoint/operation restrictions remain intact; explicit empty allowed list stays denied.
- Real local HTTP smoke exercises mixed-account failover and observes upstream model/account selection.
- Relevant Rust tests/checks pass; documentation explains per-account resolution and incident findings include bounded time range and counts.
