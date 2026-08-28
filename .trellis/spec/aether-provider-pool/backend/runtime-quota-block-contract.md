# Runtime Key Quota Block Contract

## 1. Scope / Trigger

This contract applies when an upstream response indicates that one provider
credential has exhausted a non-resettable balance or quota. It spans response
classification, credential-fenced persistence, ordinary and Pool scheduling,
Pool score projection, administrator recovery, and the two management views.

The runtime block is independent from `is_active`, `status_snapshot.quota`,
OAuth invalidation, health/circuit state, and resettable rate-limit cooldowns.

## 2. Signatures

```rust
pub struct ProviderCatalogKeySchedulingStateCasUpdate {
    pub key_id: String,
    pub expected_encrypted_api_key: Option<String>,
    pub expected_encrypted_auth_config: Option<String>,
    pub expected_auth_type: String,
    pub expected_scheduling: Option<serde_json::Value>,
    pub scheduling: Option<serde_json::Value>,
    pub updated_at_unix_secs: Option<u64>,
}

pub fn provider_pool_key_runtime_quota_blocked(
    key: &StoredProviderCatalogKey,
) -> bool;

POST /api/admin/endpoints/keys/{key_id}/clear-quota-exhausted
```

Frontend provider configuration extends `FailoverRulesConfig` with:

```ts
quota_exhaustion_patterns?: FailoverRuleItem[]
```

## 3. Contracts

- Persist the state only at `status_snapshot.scheduling`; do not add a database
  column, change `is_active`, mutate `quota.exhausted`, or delete the Key.
- Confirmed state has `code = "quota_exhausted"`, `blocked = true`, and
  `requires_manual_recovery = true`. Suspected state has
  `code = "quota_suspected"` and does not block.
- The object records `source`, `confidence`, `confirmation_count`, HTTP status,
  upstream error code, a bounded sanitized reason, and first/latest observation
  timestamps. An internal credential fingerprint may fence weak observations,
  but API payload builders must remove it.
- HTTP 402, exact built-in error codes, and administrator patterns are strong
  evidence. Weak message fallbacks require two consecutive observations for
  the same credential. A non-quota HTTP terminal clears suspicion; transport
  failures neither advance nor clear it.
- Administrator patterns are evaluated first. HTTP 402 and exact built-in
  non-resettable quota codes remain strong evidence even when an intermediary
  also attaches a reset header or field. A reset deadline suppresses only weak
  or otherwise ambiguous quota text; transient rate-limit codes remain in the
  existing cooldown path.
- Confirmed state is never cleared by success, quota refresh, credential edit,
  restart, health recovery, or cache rebuild. Only the administrator endpoint
  clears it.
- The scheduling CAS must compare the encrypted API key, encrypted auth config,
  auth type, and previously observed scheduling object. Implement it for
  memory, SQLite, MySQL, and PostgreSQL.
- Serialize quota-evidence scheduling CAS, runtime-quota Pool projection, score rebuild,
  active-probe membership, and administrator recovery with the same
  provider/Key runtime lock. Every writer that could derive `Available`,
  `Cooldown`, or another hard state must strongly re-read scheduling while
  holding that fence; a confirmed block either suppresses the derived write or
  reasserts `QuotaExhausted` with source `runtime_quota_exhaustion`.
- If the runtime lock backend is unavailable, quota evidence must still persist
  the credential-fenced scheduling CAS. Skip only the derived Pool projection;
  candidate strong reads enforce the source fact immediately and a later score
  rebuild reconciles the projection.
- Background quota probe and account self-check selection must remove confirmed
  runtime blocks before upstream work. Their result and active-probe writers
  must repeat the strong check under the projection fence to cover a block
  installed while the probe was in flight.
- Success and non-quota terminal paths must strongly read the Key before
  deciding that no suspicion exists; a local cache-negative shortcut can miss
  a suspicion written by another gateway and incorrectly make two weak signals
  consecutive. Suspicion clearing uses the credential-fenced scheduling CAS
  without the Pool projection lock because it cannot downgrade a confirmed
  block and the CAS safely linearizes against a concurrent second weak signal.
- Ordinary candidates, sticky promotion, Pool real Keys, active probes, and
  Pool score rebuilds consume the same persisted fact. Candidate admission and
  sticky/Pool materialization use strong Key reads so another gateway's block
  cannot remain hidden behind the local catalog cache. Use diagnostic reasons
  `key_quota_exhausted` and `pool_key_quota_exhausted`, and project Pool scores
  to `PoolMemberHardState::QuotaExhausted` with source
  `runtime_quota_exhaustion`.
- Structured HTTP/SSE/Responses WebSocket terminals feed the shared evidence
  state machine. A protocol leg whose provider frames are intentionally opaque
  cannot originate new evidence, but its candidate admission must still honor
  a block learned through any other Endpoint or protocol for the same Key.
- If a Pool catalog strong read fails or omits a requested Key, fail closed with
  `pool_key_state_unavailable`; never report that infrastructure failure as
  `pool_key_quota_exhausted`.
- `pool_key_quota_exhausted` is reserved for the administrator-recoverable
  runtime fact. Existing resettable Provider hard-quota signals retain their
  previous skip reason and recovery semantics.
- A quota-classified failure records the candidate failure and quota evidence,
  invalidates candidate/affinity caches, releases any Pool lease, and retries
  with credential scope. It must not train adaptive 429, increment health,
  open a circuit, write OAuth invalidation, or enter runtime auto-delete.
- HTTP 200 quota error envelopes are still failures: skip health/adaptive/Pool
  success effects and success-only response finalizers even when an existing
  stop rule prevents replaying the request on another candidate.
- Recovery is idempotent and returns `{ key_id, cleared, message }`. It clears
  only scheduling quota state, rebuilds the Pool score, and preserves manual
  disablement, OAuth, cooldown, health, and unrelated hard states.

## 4. Validation & Error Matrix

| Observation or action | State/result |
|---|---|
| HTTP 402, with or without reset metadata | confirm on first observation |
| exact non-resettable quota code, with or without reset metadata | confirm on first observation |
| configured regex/status rule | confirm on first observation |
| weak quota text, first observation | `quota_suspected`, schedulable |
| weak quota text, second consecutive observation | `quota_exhausted`, blocked |
| non-quota HTTP terminal after suspicion | clear suspicion |
| transport error after suspicion | leave suspicion unchanged |
| weak/ambiguous signal with reset deadline | no scheduling state; use cooldown |
| stale response from replaced credential | CAS/fingerprint miss; no write |
| clear endpoint for missing Key | HTTP 404 |
| clear endpoint with no quota state | HTTP 200, `cleared = false` |
| clear endpoint with confirmed state | HTTP 200, `cleared = true` |

## 5. Good / Base / Bad Cases

- Good: a Sub2API `API_KEY_QUOTA_EXHAUSTED` response immediately blocks that
  Key, skips all remaining plans with the same Key id, and tries another Key.
- Base: a normal `rate_limit_exceeded` 429 with `Retry-After` keeps using the
  existing cooldown and never installs a manual block.
- Good recovery: an administrator clears the quota state while a manually
  disabled Key stays disabled and an auth-invalid Pool score stays auth-invalid.
- Bad: setting `is_active = false`, relying only on a transient Pool score, or
  clearing the block on the next successful request.

## 6. Tests Required

- Classifier assertions: New API, One API, and Sub2API exact codes; HTTP 402;
  weak two-hit confirmation; intervening non-quota response; reset headers and
  body fields; transient rate-limit codes; configured HTTP 200 error envelopes.
- Repository assertions: CAS success/conflict on all four backends, including
  credential replacement and concurrent suspicion increments.
- Scheduling assertions: ordinary, Pool, sticky, cached, active-probe, restart,
  score rebuild, and credential-scope current-request filtering.
- Recovery assertions: idempotency, Pool score rebuild, cache invalidation, and
  preservation of manual disablement/OAuth/cooldown/health state.
- Frontend assertions: confirmed and suspected labels plus confirmation-gated
  recovery actions in Provider Detail and Pool Management.

## 7. Wrong vs Correct

### Wrong

```rust
// A score-only or activation mutation can auto-recover or destroy admin intent.
key.is_active = false;
score.hard_state = PoolMemberHardState::QuotaExhausted;
```

### Correct

```rust
// Persist one credential-fenced scheduling fact, then derive every scheduling
// and Pool projection from that fact until an administrator clears it.
let blocked = provider_pool_key_runtime_quota_blocked(key);
signals.runtime_quota_hard_blocked = blocked;
```
