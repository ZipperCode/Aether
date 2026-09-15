# Balance-Aware Key Scheduling Contract

## 1. Scope / Trigger

This contract applies whenever a provider Key exposes `status_snapshot.quota`
with `kind = "balance"`, or when ordinary candidate, Pool, sticky, refresh, or
cache code consumes that balance state. It prevents stale balance data,
subscription switches, and shortcut paths from producing inconsistent Key
eligibility.

## 2. Signatures

```rust
pub const PROVIDER_POOL_MINIMUM_SCHEDULABLE_BALANCE: f64 = 1.0;

pub fn provider_pool_key_balance_below_minimum(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
) -> bool;

pub struct PoolMemberSignals {
    pub balance_below_minimum: bool,
    // existing fields omitted
}

pub struct CandidateRuntimeSelectabilityInput {
    pub balance_below_minimum: bool,
    // existing fields omitted
}
```

The corresponding internal skip reasons are
`key_balance_below_minimum` and `pool_balance_below_minimum`.

## 3. Contracts

- For legacy snapshots without query sources (subject to the official-provider
  exceptions below), a Key is below the minimum only when the snapshot is `fresh`, has
  `kind = "balance"`, is explicitly finite (`unlimited` is missing or
  `false`), contains a non-empty `balances` array, and every entry has a
  non-empty `unit` plus a finite parseable `available` value `<= 1.0`.
- Currency values are compared in their upstream units. Do not sum or convert
  currencies. Any valid balance `> 1.0` keeps the Key eligible.
- Missing, stale, null, unknown, malformed, non-finite, empty, or ambiguously
  unlimited data is fail-open.
- Balance eligibility is independent of `skip_exhausted_accounts`. Known
  subscription exhaustion is also excluded unconditionally; the historical
  switch is not a bypass for either fact after the full upstream merge.
- Ordinary candidates and real Pool Keys consume the same shared balance
  fact. A PoolGroup representative must not project its balance onto the
  whole Pool.
- Sticky Pool Keys must pass through the shared Pool scheduler. Shortcut code
  must not reimplement quota or balance filters.
- `ObservationOnly` balance providers enter account self-check automatically.
  Existing interval/concurrency settings apply; defaults remain 60 minutes
  and four concurrent checks. Refresh failure must not write Pool hard-state.
- When the derived low-balance boolean changes, invalidate candidate page and
  resolved candidate caches. An unchanged boolean keeps catalog-only
  invalidation.

The fixed monetary threshold does not introduce a user-configurable policy.

### Official query sources

Official quota snapshots may add `sources[]`; balances and windows associate
with their metadata using `source_id`, without duplicating the numeric arrays.
For these snapshots the source product and per-source status take precedence
over the legacy top-level `kind`/freshness compatibility fields.

- Only `ok` + `fresh` sources with product `account_balance` or
  `key_spending_limit`, known currency units, and valid amounts participate in
  the existing 1.0 monetary threshold. An eligible source with missing values,
  a failed/unknown source, or applicable coding/extra-usage funding makes that
  monetary inference unknown. `unsupported` and `not_applicable` sources are
  not applicable constraints.
- Independent applicable plan sources must all be proven exhausted before
  projecting account-wide exhaustion. A plan is constrained by any of its
  account/key windows with an explicit exhausted state and a future reset.
  Tool/MCP/model-specific, excluded and unlimited windows do not establish
  that fact. Reset deadlines are reevaluated when scheduling.
- Mixed account balance, plan and extra-usage sources do not prove an upstream
  funding order; keep the account-wide decision unknown rather than allowing
  one exhausted component to poison every alternative. MiniMax remains
  observation-only, including its monetary query results.
- Legacy OpenRouter derived remaining amounts and Zhipu error-derived amounts
  are not trusted for the monetary threshold until a normal refresh creates
  source evidence. Legacy Zhipu/Z.ai/Kimi Coding subscription snapshots also
  lack the required product/source evidence and do not establish new blocks.
- Query failures must not be presented as OAuth/inference authentication
  expiration. Refresh writes and frontend application preserve all
  `status_snapshot` siblings, particularly the independent manual-recovery
  `scheduling` block.
- The subscription refresh worker must not duplicate official source evidence
  into a persistent account `QuotaExhausted` score. Such a duplicate outlives
  source staleness and window resets. Normal refresh results clear only scores
  tagged as old `quota_refresh_health` exhaustion; runtime manual blocks remain
  protected by the existing projection lock and strong scheduling-state check.

The additive source API and upstream evidence are documented in
`docs/api/official-provider-quota.md`. No database migration is required.

## 4. Validation & Error Matrix

| Input state | `balance_below_minimum` | Scheduling result |
|---|---:|---|
| fresh, one balance `0` or `1` | true | skip Key |
| fresh, any balance `> 1` | false | keep eligible |
| fresh, all currencies `<= 1` | true | skip Key |
| stale or missing freshness | false | fail-open |
| empty balances or missing/empty unit | false | fail-open |
| missing/invalid/non-finite amount | false | fail-open |
| `unlimited = true`, null, or invalid | false | fail-open |
| known exhausted subscription quota | not a balance fact | exclude regardless of the historical switch |

## 5. Good / Base / Bad Cases

- Good: a fresh CNY `0.5` plus USD `0.9` snapshot skips the Key and continues
  background refresh.
- Base: a fresh CNY `0.5` plus USD `2.0` snapshot keeps the Key eligible.
- Bad input: a stale `0.1`, an invalid `unlimited` value, or a missing unit is
  not trusted to block traffic.
- Recovery: fresh low -> fresh active and fresh low -> stale both invalidate
  candidate caches so the next selection observes the new state.

## 6. Tests Required

- Provider-pool unit tests must assert `0`, `1`, `1.0001`, multi-currency,
  stale, empty, missing unit/amount, malformed/non-finite values, unlimited,
  and subscription behavior.
- Pool-core tests must assert balance and known subscription exhaustion are
  both excluded for either value of the historical `skip_exhausted_accounts`.
- Scheduler tests must assert ordinary Key filtering and that PoolGroup
  representatives do not inherit Key balance state.
- Gateway tests must assert sticky fallback/seen/skip evidence/scan budget,
  ObservationOnly automatic refresh and failure semantics, plus low->active,
  low->stale, and low->low cache behavior.

## 7. Wrong vs Correct

### Wrong

```rust
// A shortcut duplicates one condition and bypasses the rest of Pool policy.
if sticky_key.balance <= 1.0 {
    return next_key();
}
```

### Correct

```rust
// Derive the fact once, then send sticky and ordinary Pool Keys through the
// same scheduler that owns all eligibility and audit behavior.
let balance_below_minimum =
    provider_pool_key_balance_below_minimum(key, provider_type);
let scheduled = schedule_pool_page_candidates(singleton_candidate, context);
```
