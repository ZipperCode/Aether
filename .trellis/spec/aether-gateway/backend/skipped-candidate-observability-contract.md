# Skipped candidate observability

## 1. Scope / Trigger
Upstream PR #833 introduces candidate-skip indicators and timeline reasons.
The 2026-09-20 integration completes ordinary-user data, pagination and active
polling. These fields describe scheduling decisions; they do not change policy.

## 2. Signatures
- Admin usage records and active responses: `has_skipped_candidate: boolean`
  and `skipped_candidate_reasons: string[]`.
- `GET /api/users/me/usage` and `/api/users/me/usage/active`: only the boolean
  skip indicator is exposed to ordinary users.
- `status=has_skipped_candidate` selects usage records with skipped candidates.

## 3. Contracts
- Existence is determined by `RequestCandidateStatus::Skipped`, independently
  of whether `skip_reason` is missing, blank or known to the frontend.
- Admin reasons are trimmed and deduplicated in candidate order. They remain
  separate from attempted retry/fallback facts; a skip is not a failed send.
- Candidate persistence and diagnostic-count projection recognize the exact
  codes `key_quota_exhausted`, `key_balance_below_minimum`,
  `pool_key_quota_exhausted`, `pool_balance_below_minimum` and
  `pool_key_state_unavailable`. Do not collapse these known facts into
  `unclassified_skip`; unknown arbitrary strings still use that sanitized
  fallback. Frontend labels use the same codes and distinguish manual recovery,
  insufficient balance and infrastructure failure.
- Authenticate and scope usage rows to the current user before reading their
  candidates. Ordinary-user list, explicit-ID active polling and discovery
  cannot expose other users' records, provider/key names or raw skip reasons.
- Apply the selected user skip filter after existing user/time/search/API-format
  constraints but before total/offset/limit. Do not filter only the first loaded
  100 rows. Reuse the admin-derived filter pattern; large historical ranges keep
  its current per-request candidate-read cost, with no silent truncation.
- List and active polling agree; existing record merging preserves observed
  skip facts through sparse refreshes. Desktop/mobile users see generic markers;
  only administrators see detailed reasons. Existing retry/fallback filtering
  remains unchanged.
- In `maybe_execute_sync_via_local_image_decision`, drain candidate preparation
  before building a synchronous JSON heartbeat shell. If no execution attempt
  remains, return the existing `NoPath` outcome before committing HTTP 200.
  The ordinary proxy runtime-miss finalizer then records one failed usage with
  the original trace ID, skipped candidates and configured request capture.
  Preserve existing heartbeat behavior for nonempty attempts.
- In `record_failed_usage_for_runtime_miss_request`, preserve diagnostic and
  candidate model precedence. If both lack a model, use the nonempty string
  `model` from the original request JSON already parsed by that writer. Reuse
  the same parsed value for capture; do not enable ImageDecision diagnostics,
  broaden candidate metadata or depend on body capture being enabled. Missing
  or invalid model fields retain the existing unknown result.
- Keep `failure_diagnostic.kind` and `failure_diagnostic.source` in the bounded
  persistence/admin diagnostic projection. These locate the internal rejection
  when breakpoint is `$`; ordinary-user projection stays unchanged. A skipped
  candidate with `started_at=NULL` is not an upstream send or response.
- Responses stream plan rejection is a typed categorical failure, not a body
  failure inferred from `None`. Distinguish missing headers/body, plan identity,
  incomplete auth and URL. Persist via the existing direct candidate skip seam;
  it bypasses the bulk pool-membership filter and records request-level counts.
- A PoolGroup count is a logical candidate count, not the number of attempted
  accounts. Keep ordinary pool-row suppression and the yielded-key exhaustion
  guard; record rejection where a concrete plan or quarantined candidate drops.
- Exhausted pools that yielded no member persist one synthetic skipped candidate:
  `skip_reason="pool_group_exhausted"`, `key_id=null`, and the existing object
  `extra_data.pool_group_exhaustion={scanned_keys,budget_scanned_keys,skip_reason_counts}`.
  Member-reason frequencies describe the group, never its representative Key.
  Keep every member reason in request-level diagnostics for HTTP classification;
  do not replace those facts with the synthetic summary reason.
- When request diagnostics are disabled (for example ImageDecision), the persisted
  capacity classifier reads the marked group's nonempty `skip_reason_counts`.
  HTTP 429 requires positive integer counts and every member reason in the
  existing capacity set. Mixed noncapacity, unknown, empty or malformed group
  facts retain HTTP 503; `pool_group_exhausted` alone is never a capacity reason.
- The admin timeline recognizes the explicit group object, including historical
  rows with a representative Key and plurality reason. Show pool exhaustion and
  positive integer member counts, without representative account/auth/Key metadata
  or per-member scheduling details. Do not infer group scope from a missing Key.
  Existing history is not rewritten; concrete per-Key records stay unchanged.
- Runtime quarantine drops record `endpoint_capability_quarantined` once at the
  dropping gate. The quarantine cache stores membership, not the original reason;
  do not invent its cause or change quarantine scope/TTL to improve diagnostics.
- Responses `Exhausted` means at least one attempt executed: update only the
  terminal reason to `execution_runtime_candidates_exhausted`, retaining skip
  counts and routing metadata. `NoPath` retains planner rejection diagnostics.

## 4. Validation & Error Matrix
| Input | Result |
| --- | --- |
| Skipped candidate with no reason | Boolean true; empty admin reasons |
| Only attempted failed/retried candidates | No skip indicator inferred |
| Matching skip beyond first loaded page | Server includes it with correct total/offset |
| Explicit active IDs include another user's row | Foreign row is absent |
| Ordinary-user record | Boolean only; no provider/key or reason disclosure |
| Candidate data source unavailable | Preserve existing data-error behavior; do not fabricate a reason |
| All image candidates skipped with JSON heartbeat enabled | Real failure status and one failed usage; no empty HTTP 200 shell |
| Stored failure_diagnostic has internal kind/source | Preserve in admin diagnostics through repeated projection |
| No-plan failure has an explicit JSON model but no diagnostic/candidate model | Record the declared model under both basic and full capture |
| Pool key lacks a complete auth pair | No upstream send; categorical `transport_auth_unavailable` skipped diagnostic |
| Candidate dropped by runtime quarantine | Count `endpoint_capability_quarantined` once, not a silent plan miss |
| Planning skips followed by actual execution exhaustion | Preserve skips and classify execution exhaustion |
| Mixed unavailable and model-incompatible pool, no yielded Key | Keyless `pool_group_exhausted` row with exact counts; no upstream send; original HTTP classification |
| Historical group summary still names a representative Key | Timeline shows group diagnosis/counts, no attributed account metadata |
| Pool yielded a concrete member or ordinary single-Key skip | Keep real identity/reason; no extra synthetic no-attempt summary |
| Diagnostics-disabled image pool, only concurrency/RPM skips | Persisted member counts preserve HTTP 429 without selecting a representative reason |

## 5. Good / Base / Bad Cases
Good: a successful request that skipped an earlier Key shows a skip marker even
without failover. Base: a single successful attempted candidate has no skip.
Bad: infer skip existence from nonempty reason text or expose raw reasons to users.

## 6. Tests Required
- `gateway_users_me_usage_skipped_candidates_are_private_and_filtered_before_pagination`
  covers reasonless/reasoned skips, pages, combined filters, active equality and
  cross-user exclusion.
- `attempt_flags_report_skipped_candidates_without_reason_text` and the admin
  active route regression preserve reason-independent status.
- Data-contract `scheduling_skip_reasons_survive_persistence_and_count_projection`
  preserves known codes/counts while sanitizing unknown sensitive strings.
- Frontend `UsageRecordsTable`, `useUsageData`, `recordFilterPolicy`, `recordSync`,
  `skipReason`, `status`, timeline and shared Usage filter suites cover both form
  factors, generic-user privacy, server paging and sparse updates.
- Image-route regression must enable JSON heartbeat, skip every candidate and
  assert non-200 error, one failed usage correlated with `x-trace-id`, skipped
  candidate evidence, no upstream send and configured capture behavior. Keep
  existing successful and attempted-failure heartbeat regressions passing.
- Include explicit request model assertions for skipped-only and no-candidate
  image requests. Candidate failure details remain on the candidate; preserve
  the existing minimal usage diagnostic projection instead of copying them.
- Candidate persistence regression preserves kind/source across repeated
  sanitization while unrelated metadata and ordinary-user output remain filtered.
- Exercise real HTTP Responses pool failover and all-key failures; separately
  cover pre-send plan rejection and capacity skips. A constructor serialization
  assertion is not proof of candidate persistence or terminal classification.
- Use an isolated real gateway + loopback upstream for before/after smoke:
  missing pool-key authentication must move from an anonymous plan miss to a
  categorized skipped trace without sending upstream. Verify exact Responses
  endpoint selection with Compact/Search bindings present.
- `responses_pool_exhausted_supporting_representative_persists_keyless_group_summary`
  exercises real HTTP with a supporting cooldown member and incompatible siblings:
  assert 503, zero sends, keyless group scope and exact distinct member counts.
  Timeline regressions cover historical/current group payloads, emitted selection,
  tooltips, empty groups and an ordinary per-Key control; inspect desktop/mobile.
- `gateway_image_sync_pool_capacity_exhaustion_returns_429_and_mixed_pool_returns_503`
  exercises diagnostics-disabled image HTTP: no upstream sends, keyless group
  counts, pure capacity 429 and mixed model/capacity 503. Persisted-context tests
  reject empty, invalid, unknown and nonpositive counts instead of accepting a
  historical plurality reason as proof of pure capacity.

## 7. Wrong vs Correct
Wrong: `has_skipped_candidate = !reasons.is_empty()` or filter the visible page.
Correct: derive the boolean from candidate status, authorize records first and
paginate the filtered result; keep detailed diagnostics administrator-only.
Wrong: assign the most frequent pool member reason to its representative Key.
Correct: persist a keyless pool summary; render counted reasons as group facts.
