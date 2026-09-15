# Official provider quota sources and accurate plan display

## Goal and approval

The user approved the full implementation plan on 2026-09-15. Improve all existing seven official API-key providers, add MiniMax, support documented regional origins, identify only key-visible plans, and fix quota-derived false scheduling blocks. No further implementation approval is needed.

## Requirements

- Extend the existing quota snapshot with optional sources and source_id on balances/windows; retain legacy fields and routes.
- Independently collect applicable Zhipu personal/team/account queries. At least one successful source makes a key refresh successful; other failures remain visible and retain only their own historical values as stale.
- Distinguish unknown, unsupported, permission denied, not applicable and transient errors. Missing/error payloads must never become zero or unlimited quota.
- Preserve decimal precision and provider-specific units. DeepSeek keeps per-currency balances; Moonshot and SiliconFlow use CN/global origins and regional currencies. Live verification confirmed SiliconFlow CN retired its balance endpoint on 2026-08-14: record unsupported without reading/sending credentials, while preserving the separately documented global capability. OpenRouter /api/v1/key means a key spending limit, not account balance; only limit_remaining is authoritative.
- Kimi supports api.kimi.com/api.kimi.ai, weekly/short quotas as proportions, booster wallet divided by 100000000 and priceInCents divided by 100. Prefer valid exact usages ratios over rounded legacy counters; display actual monthly total/Code windows with unknown scheduling scope. Keep reported plan identities only.
- Zhipu/Z.ai TOKENS_LIMIT is percentage-only, CREDIT_LIMIT is points, and MCP exhaustion does not block all model calls.
- MiniMax follows its official CLI: sk-api-* uses account/query_balance, other subscription keys use v1/token_plan/remains, on the matching API origin. Preserve unknown currency, percentage-guided usage_count interpretation, weekly boosts above 100%, excluded buckets and unlimited windows. Keep MiniMax observation-only.
- Drawer/list/pool use consistent grouped presentation. Fix null-to-zero, granted-as-total, decimal strings, blanket Token labels and query-failed-as-expired. Quota updates preserve scheduling and all sibling snapshot state.
- Keep the existing 1.0 monetary minimum and administrator-only runtime block recovery. Only current, applicable, unambiguous evidence restricts scheduling; ambiguous alternatives/extra usage remain unknown.

## Acceptance and boundaries

Verify provider examples, partial/stale results, missing/zero/large decimals, exact wallet scales, regional selection, OpenRouter reset/BYOK/unlimited and MiniMax boosts/excluded buckets. Deliver a capability/evidence document. Initial implementation prohibited commits and live credential use. The user subsequently authorized read-only quota queries with provided Keys, then explicitly requested commit, push and verification of GitHub CI. Do not add unit tests, run large local builds, introduce SQL migrations or make inference calls. Use frontend type checking, targeted format/static checks and temporary sample replay locally; full compilation and test jobs run in GitHub CI. Preserve the pre-existing dirty guides index, ci-compile-preflight guide and .omc.
