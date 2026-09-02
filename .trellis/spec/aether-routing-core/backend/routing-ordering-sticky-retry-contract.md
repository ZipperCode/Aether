# Routing Ordering and Lazy Sticky Retry Contract

## 1. Scope / Trigger

Use this contract when changing routing-group JSON, scheduler ordering, model or
Key priority, candidate materialization, Pool expansion, or fixed/dynamic
attempt loops. It prevents legacy global settings from overriding a resolved
request policy and prevents retry budgets from multiplying the candidate list.

## 2. Signatures

```rust
pub struct RoutingDefaultPolicy {
    pub priority_mode: RoutingSetPriorityMode,
    pub scheduling_mode: RoutingSchedulingMode,
    pub keep_priority_on_conversion: bool,
    pub sticky_key_attempts: u32,
}

pub struct RoutingModelPolicy {
    pub key_priority_overrides: BTreeMap<String, i32>,
    pub key_priority_overrides_by_format:
        BTreeMap<String, BTreeMap<String, i32>>,
    // existing fields omitted
}

pub const DEFAULT_STICKY_KEY_ATTEMPTS: u32 = 2;
pub const STICKY_KEY_ATTEMPTS_REPORT_FIELD: &str = "sticky_key_attempts";

pub(crate) fn next_same_key_retry_index(
    identity: ExecutionAttemptIdentity,
    sticky_key_attempts: Option<u32>,
) -> Option<u32>;
```

Rule actions extend the persisted JSON contract:

```json
{"type":"set_scheduling","sticky_key_attempts":2}
{"type":"set_key_priority","key_id":"key-1","priority":0,"api_format":"openai:responses"}
```

No database migration is involved; routing configuration remains opaque JSON.

## 3. Contracts

- A resolved `ResolvedRoutingPolicy` is the only request-level source for
  `priority_mode`, `scheduling_mode`, `keep_priority_on_conversion`, and
  `sticky_key_attempts`. Do not OR or merge legacy global values into it.
- Without a resolved policy, read the enabled system-default routing group's
  `default_policy`; only then fall back to legacy system-config keys.
- Startup best-effort creates and publishes a system-default group from legacy
  values when routing storage is writable and no default exists. Missing or
  read-only storage must not prevent gateway startup.
- Key priority precedence is format-scoped override, format-agnostic override,
  then catalog priority. Format keys are trimmed and compared case-insensitively;
  planner matching may also recognize existing API-format aliases.
- `sticky_key_attempts` is the total attempt count on the first-ranked
  candidate. `0` and `1` mean no same-Key retry; the default `2` means one
  retry. Every later candidate gets one attempt.
- A Pool group may retry only `candidate_index == 0` and
  `pool_key_index == 0`. Its encoded retry index must remain below
  `POOL_KEY_RETRY_INDEX_STRIDE == 100`.
- Materialize and persist exactly one initial attempt per candidate. Derive a
  same-Key retry only after a candidate-scoped failure, with a fresh candidate
  ID and incremented retry index.
- Fixed and dynamic loops must apply skip/admission/quota checks to derived
  attempts, preserve the response-producing plan/context for deferred
  exhaustion, and drain/mark unused attempts on return or error.
- Provider and Endpoint `max_retries` fields remain readable/writable for
  compatibility but no longer expand local execution attempts. Do not add the
  removed Provider form control back as a second retry authority.

## 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Enabled system-default group has valid `default_policy` | Use it; ignore conflicting legacy values. |
| Default group missing/disabled/invalid | Fall back to legacy values, then stable defaults. |
| Format-specific Key override exists | Use it before the global Key override. |
| Blank `api_format` in action | Treat as format-agnostic Key override. |
| `sticky_key_attempts` missing | Deserialize as `2`. |
| Budget `0` or `1` | Execute the initial attempt only. |
| First candidate fails with candidate scope | Derive the next same-Key attempt if budget remains. |
| Later candidate or later Pool Key fails | Advance; never derive a same-Key retry. |
| Derived attempt is quota/admission blocked | Skip it through the existing guard; do not bypass the guard. |
| Routing storage is read-only during bootstrap | Log and continue with legacy fallback. |

## 5. Good / Base / Bad Cases

- Good: `openai:chat` gives `key-a` priority `0`, while
  `claude:messages` gives the same Key priority `3`; each request sees its own
  format-specific order.
- Base: old JSON has no new fields; it loads with two total sticky attempts and
  the existing global Key priorities.
- Good: the first candidate fails once, a fresh same-Key retry succeeds, and no
  fallback candidate was pre-persisted as a retry slot.
- Bad: applying the legacy conversion-priority switch after policy resolution,
  pre-expanding N retries per candidate, or using the last failure instead of
  the deferred response's plan to build exhaustion.

## 6. Tests Required

- `aether-routing-core`: serialization/defaults, rule override, and
  format-scoped priority precedence.
- `aether-ai-serving`: one attempt per candidate and preservation of deferred
  exhaustion context.
- Gateway: system-default bootstrap/fallback, attempt index/Pool stride,
  dynamic-loop cleanup, sync/stream failover, Pool balance/runtime quota, and
  `openai_image_sync_heartbeat_retries_sticky_key_lazily_before_failover`.
- Frontend: routing config normalization, independent per-format Key maps,
  hidden Provider override preservation, sticky-attempt clamping, and type-check.

## 7. Wrong vs Correct

### Wrong

```rust
let keep_priority = policy.keep_priority_on_conversion || legacy_global_flag;
for retry_index in 0..policy.sticky_key_attempts {
    attempts.push(build_attempt(candidate, retry_index));
}
```

### Correct

```rust
let ordering = SchedulerOrderingConfig::from_routing_policy(policy);
let attempts = vec![build_attempt(candidate, 0)];
// After a candidate-scoped failure only:
let retry = next_same_key_retry_attempt(&attempt);
```
