# 修复 Antigravity 运行时额度反馈跨模型阻断

## Goal

Keep Antigravity Gemini and Claude quota availability independent after an upstream runtime quota error. Stop repeating generation against the exhausted requested model while preserving serving through the same account's unrelated available model quota and through other eligible accounts.

## Background

Current fetched-model quota admission is exact-model/account-scoped and refresh/reset-aware. Runtime feedback is not: `orchestration/effects.rs::record_pool_error_effect` sends HTTP 429 quota errors to account-wide KV cooldown and account-scoped `PoolMemberHardState::QuotaExhausted`. This can suppress unrelated Claude serving after a Gemini `RESOURCE_EXHAUSTED` response. No database manual-recovery scheduling ban is needed for resettable upstream quota.

## Requirements

- R1: Antigravity runtime model-quota exhaustion must be learned as scoped, resettable evidence and enforced before the next affected generation request in ordinary, real Pool and sticky paths, including cached candidates and multiple gateway instances.
- R2: Unrelated available model quotas on the same account must remain schedulable immediately. No account-wide cooldown, exhausted Pool hard state, adaptive RPM reduction, or shared health penalty may be introduced solely by the scoped model-quota failure.
- R3: Use the actual serving account and upstream model identity. Apply wider quota scope only when explicit upstream evidence establishes the shared bucket; do not invent model lists, infer groups from name prefixes, equal fractions or display labels, or borrow another account's model catalog.
- R4: Honor upstream reset/deadline evidence and successful persisted quota refresh; failed/stale refresh and unrelated model success must not clear active scoped exhaustion. Preserve credential-generation fences and existing manual scheduling bans.
- R5: Preserve existing behavior for non-Antigravity Providers, authentication failure, account suspension, explicitly configured account-level bans/rules and genuine rate-limit/account-level failures. Do not simply suppress the error or skip all quota feedback.
- R6: Keep existing API/database schema and fetched model catalog ownership. Prefer existing runtime and persistence primitives; do not mutate production configuration/data or deploy without separate authorization.

## Acceptance Criteria

- [x] An isolated real localhost HTTP scenario fails before the fix on Gemini-quota feedback contaminating Claude or repeated affected-model generation.
- [x] After a native upstream Gemini quota error, a subsequent affected request skips that account and serves an eligible sibling; Claude on the same account still completes HTTP 200 without waiting for account cooldown.
- [x] Reverse quota isolation works and ordinary, Pool/sticky, sync and native streaming paths consume the same scoped fact.
- [x] Positive refresh or elapsed reset restores only the affected scoped restriction; failed refresh, unrelated success and credential replacement races retain correct source/fence behavior.
- [x] No whole-account cooldown/QuotaExhausted/health/adaptive penalty arises from the scoped error; genuine account restrictions and other Provider behavior retain regression coverage.
- [x] Related Rust suites, formatting, Clippy and real HTTP smoke pass; affected executable contracts are updated and the change is committed/pushed.

## Out of Scope

- Production deployment or remote business writes, broad rate-limit redesign, generic model-resolution changes, built-in model lists, schema migrations and unrelated UI changes.
