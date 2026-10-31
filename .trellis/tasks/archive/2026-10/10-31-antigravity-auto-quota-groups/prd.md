# 复刻 Antigravity 自动额度组调度

## Goal

Replace manual per-account quota binding with the automatic quota-family selection shipped by reference proxies. Existing and newly added accounts require no quota mapping configuration.

## Requirements

- R1: Follow the referenced Argon/Gemini/Claude grouping behavior: dedicated Argon windows take precedence; use Gemini shared windows only when the account has no dedicated Argon group. Exhaustion must never trigger cross-pool borrowing.
- R2: Evaluate the selected account's actual upstream model after dynamic thinking resolution. Do not synthesize models, change model availability, infer availability from subscription tiers, or change account rotation.
- R3: Five-hour and weekly windows are cumulative constraints. An exhausted active window blocks its associated models until positive refresh or the relevant reset deadlines expire. Disabled windows are not consumable quota. Preserve exact-model exhausted guards and shared-KV feedback.
- R4: Normalize reset-only grouped quota with the existing upstream zero-omission rule; malformed explicit values stay unknown. Preserve disabled flags and grouped display.
- R5: Remove the superseded manual-binding API, backup/config fields, UI editor and dead helpers/tests. Do not create provider templates or a replacement settings namespace. Existing stored legacy maps have no effect; do not mutate production storage.
- R6: Keep quota requests aligned to the generation endpoint and use the upstream client profile that exposes Argon groups where the existing transport permits it. No automatic production token refresh, generation, deployment or database migration.

## Acceptance Criteria

- AC1: An account with dedicated Argon exhausted and generic Gemini positive is skipped for Argon but continues serving Flash; a shared-only account serves Argon with positive Gemini quota.
- AC2: With no binding writes, ordinary and pool/sticky sync/SSE paths switch away from depleted accounts, produce 503 with zero upstream executions when all applicable accounts are depleted, and recover after refresh/reset.
- AC3: Short-window reset cannot unblock a still-exhausted weekly window; disabled short windows do not mask weekly depletion. Unknown groups do not become invented model quotas.
- AC4: Reset-only grouped responses are correctly persisted and used in the real admin-refresh HTTP path.
- AC5: Account edit UI no longer offers binding controls; existing quota groups and not-applicable display remain functional. Type-check, relevant regressions and browser smoke pass.
- AC6: All obsolete binding consumers are removed, documentation reflects the automatic contract, and isolated verified changes are committed/pushed.

## Out of Scope

Model synthesis, new static model catalogs, purchased-credit fallback, changing subscription routing, adopting reference load-balancing algorithms, production account consumption/deployment, or replacing the existing distributed quota feedback mechanism.
