# Design

## Contract

ProviderQuotaSnapshotContract gains sources: Vec<ProviderQuotaSource> (serde default and omit empty). Keep the current schema version and legacy fields for additive compatibility. Each source has id, label, product, scope, optional region/plan_id/plan_name/plan_tier/currency_source, query_status, freshness and refresh_state. Query statuses: not_queried, ok, unsupported, permission_denied, not_applicable, error. Freshness: fresh/stale/unknown. Balances/windows gain optional source_id; windows also gain optional unlimited/is_included. Original arrays remain the only value storage; no duplicate nested arrays.

Use source ids balance, key_limit, subscription (renamed personal/team by Zhipu targets), and extra_usage for Kimi monetary wallet/monthly-cap data. Products: account_balance, key_spending_limit, coding_plan, token_plan, extra_usage. Copy plan identity only from upstream evidence. No sources means legacy. Old ambiguous official plan flags are not independent scheduling facts.

## Ownership

Provider-pool parsers own fixed origins, authentication requests, wire semantics, decimal scaling and source identity. Gateway owns bounded query execution and per-source outcomes/merge/freshness, retaining its transport/proxy/TLS/backoff. Shared provider-pool helpers own scheduling: do not collapse independent plans or Kimi extra usage into any-window-exhausted, do not use MCP as a model constraint, keep MiniMax observation-only. Frontend utilities own numeric/units/source projection; components consume it. Clamp bar geometry only, not boosted percentage text.

## Compatibility

No database or route changes. Do not infer old source/tier/unit or recalculate historical values. Refresh failures retain only matching source data as stale. Preserve scheduling/account/model_probe/OAuth and unknown siblings on frontend quota merge. Keep query authentication separate from inference/OAuth status.

## Evidence

Use the official references in the approved user plan: DeepSeek docs; Moonshot CN/global balance.md; SiliconFlow current release notes/global user info; OpenRouter /key and /credits permissions; zai-org/zai-coding-plugins; MoonshotAI/kimi-code managed-usage.ts; MiniMax-AI/cli endpoints/types/quota/output. SiliconFlow's 2026-08-14 CN retirement supersedes its old OpenAPI. Authorized live Kimi responses additionally establish exact usages ratios and monthly windows. Document extra-cloud-credential and unconfirmed capabilities rather than implementing guesses.
