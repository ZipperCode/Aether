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

## 7. Wrong vs Correct
Wrong: `has_skipped_candidate = !reasons.is_empty()` or filter the visible page.
Correct: derive the boolean from candidate status, authorize records first and
paginate the filtered result; keep detailed diagnostics administrator-only.
