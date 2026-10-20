# Antigravity account tier design

## Boundaries and flow

Quota refresh performs one best-effort `loadCodeAssist` request per account using the existing model-fetch plan builder and quota executor. Parse only authoritative project and tier fields; continue existing quota probes with a cached project when discovery fails. Avoid an additional hydration call when discovery has already run. Use the same proxy override and bounded execution conventions as other quota probes.

Pure parsing belongs in `crates/aether-admin/src/provider/quota.rs`. Store normalized known `free|pro|ultra` or null in `upstream_metadata.antigravity.subscription_tier`. An explicit successful response without known evidence clears stale tier classification; a failed request preserves the prior observation. Prefer paidTier over currentTier, using nonempty ID before name. Do not infer membership from allowedTiers, a menu of available upgrades, or from available models. Preserve unknown values as unknown instead of declaring Free.

The admin metadata sanitizer must allow the normalized tier field. Keep `status_snapshot.quota.plan_type` null: `derive_plan_tier` in `crates/aether-provider/pool/src/plan.rs:20-29` consumes it for scheduling. Frontend uses a shared resolver over `subscription_tier` instead. Do not write scheduling-sensitive metadata `plan_type` or change provider-pool behavior. Reuse existing list/mobile/quota dialog badges and account type helpers. No database migration, new dependency, or redesign.

## Reference

- https://github.com/lbjlaq/Antigravity-Manager/blob/main/src-tauri/src/modules/quota.rs
- https://github.com/lbjlaq/Antigravity-Manager/blob/main/src-tauri/src/models/quota.rs
- Reference precedence: paidTier before currentTier; aliases ultra/helium, free/starter, pro/premium/advanced. `standard-tier` is not a known mapping.
- Deliberate safer difference: absent/unrecognized tier evidence is unknown, not Free; allowed upgrade options do not establish an active entitlement.

## Compatibility and failure behavior

Existing accounts populate tier on the next normal quota refresh. Missing metadata remains unknown. Discovery failure must not fail an otherwise valid quota refresh with an existing project. Preserve other metadata namespaces and current quota/suspension semantics. Do not expose raw responses or credentials.
