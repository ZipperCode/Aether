# 修复 Antigravity 客户端身份与逐账号模型发现

## Goal

Discover the actual models available to every Antigravity account, including eligible Gemini 4 accounts, without inventing model entries or borrowing another account's permissions.

## Background

- User approved the proposed repair and explicitly requires removing first-account-only discovery because account tiers differ.
- `crates/aether-provider/transport/src/antigravity/auth.rs:111-131` ignores the configured client version and emits a fixed VS Code-style identity.
- `apps/aether-gateway/src/handlers/admin/provider/query/models/mod.rs:959-965` stops after the first successful Antigravity account. Provider-level cached results can therefore hide later-account models.
- `crates/aether-model-fetch/src/strategy.rs:1576-1627` preserves upstream model IDs but ignores `disabled: true`.
- Official model documentation restricts Gemini 4 Argon to App/CLI/IDE extensions and eligible account entitlements: https://antigravity.google/docs/models.

## Requirements

1. Use one supported Antigravity client identity consistently for account initialization, discovery, and generation. Existing supported version overrides must not silently be ignored.
2. Query every eligible account in the requested selection and return the deduplicated union of its actual discovered models, independent of ordering or tier. A failed account must not discard successful results from other accounts.
3. Keep each account's model cache and serving eligibility isolated; a provider-wide union must never become an individual account's permission set.
4. Exclude upstream-disabled model entries from discovered models and associated quota metadata.
5. Remove obsolete first-account provider-cache behavior so upgrades cannot continue presenting that incomplete catalog.
6. Never supplement Gemini 4 from static catalogs, aliases, quota group names, or manually constructed entries.

## Acceptance Criteria

- [x] A later account's exclusive model appears in provider-wide and selected-account results even when the first account succeeds or has an empty catalog.
- [x] Single-account results remain restricted to that account; duplicates across accounts appear once in the provider response.
- [x] Partial failures retain successful account models and report existing warnings/errors correctly.
- [x] `disabled: true` entries are absent while enabled, previously unknown upstream IDs remain discoverable.
- [x] Discovery and generation use the same supported client family/version, including configured overrides.
- [x] Focused regression checks and an executable no-credential HTTP smoke cover the changed behavior. Live Google account verification remains unexecuted because no authorized account request was made.

## Out of Scope

- Manual model additions, account entitlement bypass, subscription changes, frontend redesign, database migrations, and unrelated scheduling changes.
- New automatic remote version-refresh infrastructure unless required by an existing identity mechanism.
