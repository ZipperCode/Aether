# Antigravity account entitlements and explicit onboarding

## Goal
Make existing Antigravity accounts acquire and display authoritative paid tiers, use Google entitlement identifiers consistently in model/quota discovery, and provide an explicit administrator action to initialize accounts lacking a companion project.

## Scope and requirements
- R1: Preserve normalized display tier and raw paid/current tier IDs from loadCodeAssist. Missing evidence remains unknown; failed refresh preserves previous observations and exposes a distinct query-failure state.
- R2: Reuse account observations across quota refresh, request-time project hydration and model discovery. Existing projects must not prevent missing/stale account observations from being refreshed; bound refresh frequency and keep usable cached projects on best-effort discovery failure.
- R3: Both model discovery and quota fetchAvailableModels use observed raw paid tier, then observed current tier, then no entitlement. Exclude free-tier/legacy-tier and duplicates. Retry only entitlement-specific invalid/precondition refusals; do not retry authentication failures, throttles or transport errors as entitlement fallback.
- R4: Add explicit administrator-only project initialization for Antigravity accounts lacking a project, with visible UI action and confirmation that it initializes the upstream account. Reuse loadCodeAssist, then onboardUser only if still needed, with bounded polling/cancellation and existing proxy/origin/timeout protections. Never onboard during chat or background reads.
- R5: Keep display-only tier independent from scheduling-sensitive plan fields. Preserve account-specific model permissions, quota and signature/cache behavior.

## Acceptance
- Old project-only accounts can acquire Pro/Ultra without re-import; all current account badge surfaces continue rendering canonical tier or Unknown.
- Failed account-info query is distinguishable from a successful unknown-tier response; previous successful tier survives failures.
- Paid/current/no-entitlement sequence is consistent in both discovery paths and stops on unrelated failures.
- Explicit initialization persists the resulting project and account observations; repeated action on an initialized account does not issue onboardUser; polling terminates on deadline.
- Relevant behavior regressions and a real local mock-upstream HTTP/UI smoke are exercised without real credentials or upstream account writes.

## Non-goals
Ollama, speech/audio, unrelated protocol extensions, blanket 429 rewriting, synthetic history repair, database schema migration, tier-based scheduling changes and live Google account operations.

## Evidence
- state/integrations.rs:88-150 skips discovery for an existing project and saves project only.
- handlers/admin/provider/oauth/quota/antigravity.rs:252-385 discovers paid tier on quota refresh only.
- crates/aether-provider/pool/src/providers/antigravity.rs:64-95 sends project only.
- crates/aether-model-fetch/src/transport.rs:217-265 sends project only.
- Magpie fa22bc64676e230a71b45a40e7fee2ca750bb4dd internal/provider/google.go:328-342,663-741,947-974 provides reference behavior; retain Aether stricter failure boundaries.
