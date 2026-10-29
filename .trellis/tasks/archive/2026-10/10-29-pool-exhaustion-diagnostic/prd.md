# Pool exhaustion diagnostic ownership

## Goal
Correct the misleading single-key model-disabled diagnosis on exhausted provider pools, review the repair, commit and push it, and verify GitHub CI.

## Confirmed incident
- Request `c3b46100-14de-497b-ae89-bccf0e99b9a2` on 192.168.2.212 returned HTTP 503 for `gemini-4-argon` at 2026-10-29 23:07:31 +08.
- The Antigravity pool summary contains 123 model-disabled, 30 cooldown, and 1 account-blocked members. Its representative key supports Argon tiers and had received upstream 429.
- `apps/aether-gateway/src/dispatch/pool_scheduler.rs:706-789` clones the representative identity and selects the most frequent member reason for the synthetic group row.

## Requirements
1. Persist exhaustion as a pool-level fact with `pool_group_exhausted`, no asserted member-key identity, and the existing bounded per-reason counts.
2. Show the pool context and counted member reasons in the admin timeline instead of attributing the aggregate reason to its representative key. Existing summary payloads must also render as pool summaries without rewriting stored history.
3. Preserve concrete single-key skips, scheduler eligibility, cooldown/quota behavior, real-attempt records, request-level diagnostics, user privacy, and HTTP failure classification.
4. Add behavior-level regression coverage, exercise an isolated running surface, independently review the changes, resolve findings, then commit and push the isolated task changes.
5. Check CI for the pushed commit and resolve failures introduced by this repair.

## Acceptance
- Mixed pool with a supporting-but-unavailable representative and more incompatible siblings produces a keyless group-exhaustion summary per exhausted logical group with exact member-reason counts, no upstream sends, and unchanged failure status. Existing separate planner passes remain separate candidate records.
- A pool that yields a real key does not gain a synthetic no-attempt summary.
- Admin timeline visibly identifies a pool summary, lists counted reasons, and does not show representative key/account metadata as if it were a rejected member.
- Ordinary per-key skips retain their identity and reason. Existing historical group payloads display correctly through their explicit group marker.
- Targeted validation and live smoke pass; independent review findings are resolved; pushed commit CI is observed to completion.

## Out of scope
No production configuration/deployment changes, cooldown resets, quota/model-policy changes, database migration, historical data writes, unrelated cleanup, or release tag.
