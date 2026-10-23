# Design

## Ownership
- Provider transport owns pure Antigravity account parsing and protocol policy in `antigravity/account.rs`; admin no longer owns a second parser. Move existing parser tests with the implementation and migrate every caller; no compatibility re-export.
- Model-fetch owns discovery execution and model outcome metadata. Gateway owns persistence and admin mutations. Provider pool owns quota request construction. Frontend consumes safe display projections.

## Shared interface (coordinate all implementers)
Export from `aether_provider_transport::antigravity`:
- `AntigravityCodeAssistInfo { project_id: Option<String>, subscription_tier: Option<String>, paid_tier_id: Option<String>, current_tier_id: Option<String> }`.
- `parse_antigravity_code_assist_response(&Value) -> Option<AntigravityCodeAssistInfo>`; existing paid-ID-before-name normalization unchanged. Raw IDs come only from actual ID/string, never tier names.
- `antigravity_account_observation_update(info: Option<&AntigravityCodeAssistInfo>, now: u64) -> Value`: root `antigravity`; success includes nullable canonical tier, nullable raw IDs, `account_info_status: "success"`, `account_info_checked_at`, `account_info_updated_at`; optional project only when present. Failure includes only `account_info_status: "error"` and checked timestamp so merge preserves last successful evidence.
- `antigravity_account_observation_due(metadata: Option<&Value>, now: u64) -> bool`: missing observation immediately due; success 15-minute TTL; failed attempt 60-second cooldown. Future timestamps treated as stale. It must not require a known canonical tier, since successful unknown is valid.
- `antigravity_entitlement_candidates(metadata: Option<&Value>) -> Vec<Option<String>>`: raw paid ID then current ID then None, skipping blanks/free-tier/legacy-tier and duplicates. Never infer IDs from canonical tier.
- `is_antigravity_entitlement_rejection(status: u16, body: &Value) -> bool`: only 400/412 with invalid/precondition entitlement/userTier/tier evidence; auth/rate/transport failures never advance entitlement fallback.
- `build_antigravity_fetch_available_models_body(project_id: &str, tier: Option<&str>) -> Value`.

## Execution
Quota refresh is explicit and always refreshes account observations. Request hydration refreshes due observations even with a cached project, persists full merged observations; best-effort failure returns usable cached transport. Model discovery refreshes due observations and attaches them to returned model metadata without erasing other namespaces. Both fetchAvailableModels executors use shared entitlement order; current host failover remains separate from entitlement fallback.

## Explicit initialization
Administrator mutation performs discovery, skips onboardUser for existing projects, otherwise sends selected default allowed tier to the configured Antigravity origin and polls at most five times with a short delay and total deadline. Never infer subscription from allowed tiers; those are bootstrap choices only. Action has UI confirmation and is never invoked by chat or background reads. Reuse existing authentication/proxy/origin guards. No real account initialization in verification.

## Security and compatibility
Keep normalized tier display-only; no scheduling plan field changes. Safe admin projection allows only typed observation fields and raw tier IDs (not raw responses). No schema migration. Never log tokens, project payloads or unbounded upstream errors. Account status changes do not invalidate usable credentials. Preserve original reasoning/cache/schema paths.
