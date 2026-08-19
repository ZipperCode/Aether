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

- A Key is below the minimum only when the snapshot is `fresh`, has
  `kind = "balance"`, is explicitly finite (`unlimited` is missing or
  `false`), contains a non-empty `balances` array, and every entry has a
  non-empty `unit` plus a finite parseable `available` value `<= 1.0`.
- Currency values are compared in their upstream units. Do not sum or convert
  currencies. Any valid balance `> 1.0` keeps the Key eligible.
- Missing, stale, null, unknown, malformed, non-finite, empty, or ambiguously
  unlimited data is fail-open.
- Balance eligibility is independent of `skip_exhausted_accounts`.
  Subscription exhaustion remains controlled by that switch.
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

No database, HTTP, frontend, or user-configuration contract is introduced.

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
| subscription quota | not a balance fact | use existing exhaustion switch |

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
- Pool-core tests must assert balance is unconditional while subscription
  exhaustion still follows `skip_exhausted_accounts`.
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

