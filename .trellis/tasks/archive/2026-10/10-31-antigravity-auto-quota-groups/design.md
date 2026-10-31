# Automatic Antigravity quota groups

## Reference

Behavior reviewed from Antigravity-Manager 0067fe045ddd77f4462ea8e2c25384839d869f32 modules/quota.rs:535-636 and proxy/token_manager.rs:5450-5580. Its current CC BY-NC-SA license prevents copying its implementation into this codebase as an unrestricted component.

Port the equivalent MIT-licensed family selection from OmniRoute merged PR #18026, merge revision 848f2573cf3d32a91f93bacf5b4ab3148dffa1c6:
- https://github.com/cheaper-inference/OmniRoute/blob/848f2573cf3d32a91f93bacf5b4ab3148dffa1c6/open-sse/services/antigravityQuotaFamily.ts
- https://github.com/cheaper-inference/OmniRoute/blob/848f2573cf3d32a91f93bacf5b4ab3148dffa1c6/open-sse/services/usage/antigravityWeeklyQuota.ts

Reuse Aether's existing typed runtime and deadline checks rather than importing the reference server framework. Preserve MIT attribution for translated source. Core transfer: Argon-specific family recognized before Gemini; dedicated Argon aggregate windows win; absence selects shared Gemini aggregates; third-party windows remain isolated. Family classification is protocol quota interpretation, not model routing or a list of available models.

## Ownership and flow

- provider-pool quota.rs owns the account-local automatic group resolver and calls existing per-window exhaustion/deadline logic. Replace antigravity_bound_group_exhausted; no new persistent map or per-request cache.
- aether-admin quota.rs retains group IDs/bucket IDs and disabled fields. Extend its fraction read to reuse the existing transport zero-omission helper, including normalized snake_case aliases without unnecessary allocations.
- The already selected provider model is evaluated; account model discovery and reasoning variant resolution remain untouched.
- Existing group snapshots and UI remain the display source. Remove the bindings-only projection fields and editor instead of maintaining two parallel policies.
- Existing metadata namespace antigravity_quota_bindings is not read or written after cutover. No destructive historical storage migration is required.
- Quota and generation requests retain the same transport endpoint. The existing summary request builder already falls back to the `antigravity/cli/` identity and preserves supplied identity headers; no transport-profile change is needed.

## Boundaries

Aether retains enabled-window OR blocking, exact-model blocking and reset reevaluation instead of collapsing two windows into one percentage. Reference model synthesis, plan-based entitlement assumptions, arbitrary-group fallback, and new round-robin/EDF policy are outside the authorized quota change.

## Verification

Use existing loopback HTTP fixture with account-local groups and no binding PUT. Observe exact account/model execution counts, admin persisted metadata, sync/SSE content and blocked-all 503. Compile/run a direct policy probe only if needed. Browser smoke uses actual account editor and quota dialog with controlled non-secret fixture. No live account access.
