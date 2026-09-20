# PR #833 completion receipt

## Delivered

- Skipped existence uses candidate status independently of missing/blank reasons.
- Admin active polling now carries the same skipped flag/reasons as the list, reusing its existing candidate read and response override map.
- User list and active routes share candidate-derived state after authenticated usage scoping. Only `has_skipped_candidate` is exposed; provider names, provider keys and skipped reasons stay absent.
- `status=has_skipped_candidate` is applied to the existing user/time/search/API-format-scoped audit set before total/offset/limit. The frontend selects server paging for this new status.
- User desktop/mobile rows display generic markers even when the existing fallback flag is also true. Detailed administrator reasons never render in the user markers.
- Existing record-sync utilities preserve observed skipped facts and deduplicate reasons across list refreshes and active polling.

## Files changed by this implementer

- `apps/aether-gateway/src/handlers/public/support/user_me_usage.rs`
- `apps/aether-gateway/src/handlers/admin/observability/usage/summary_routes.rs`
- `apps/aether-gateway/src/tests/frontdoor/public_support.rs`
- `apps/aether-gateway/src/tests/control/admin/usage.rs`
- `frontend/src/api/me.ts`
- `frontend/src/api/usage.ts`
- `frontend/src/features/usage/components/UsageRecordsTable.vue`
- `frontend/src/features/usage/components/__tests__/UsageRecordsTable.spec.ts`
- `frontend/src/features/usage/composables/useUsageData.ts`
- `frontend/src/features/usage/composables/__tests__/useUsageData.spec.ts`
- `frontend/src/features/usage/utils/recordFilterPolicy.ts`
- `frontend/src/features/usage/utils/__tests__/recordFilterPolicy.spec.ts`
- `frontend/src/features/usage/utils/recordSync.ts`
- `frontend/src/features/usage/utils/__tests__/recordSync.spec.ts`
- `frontend/src/views/shared/Usage.vue`
- This receipt and `research/pr833-completion-plan.md`.

The root agent imported PR #833 itself; untouched PR files are not claimed as implementer edits. No index/commit mutations or Rust compilation were performed here.

## Verification performed

- PASS: 8 scoped Vitest files, **151 tests**, with committed-lock Vitest 4.1.11. Files: `recordFilterPolicy.spec.ts`, `recordSync.spec.ts`, `skipReason.spec.ts`, `status.spec.ts`, `useUsageData.spec.ts`, `UsageRecordsTable.spec.ts`, `HorizontalRequestTimeline.spec.ts`, and `views/shared/__tests__/Usage.record-filters.spec.ts`.
- PASS: non-mutating `npx eslint` across 20 PR/completion API, usage component/composable/utility/type and shared view paths.
- PASS: targeted `rustfmt --edition 2021 --config skip_children=true` on all four touched Rust files; scoped `git diff --check`.
- Full `npm run type-check` reached one error outside ownership: `ProviderFormDialog.vue:490` TS2741, missing `xai` in the `Readonly<Record<ProviderType, string>>` descriptions map. Root notified; its final rerun owns resolution. No usage-path type errors were reported.

## Rust checks handed to root

- New `gateway_users_me_usage_skipped_candidates_are_private_and_filtered_before_pagination`: reasonless/reasoned skips, no-skip rows, server total and offset beyond intervening rows, search/API-format combinations, empty page, user list/active consistency and foreign-user exclusion for discovery/explicit IDs.
- New `attempt_flags_report_skipped_candidates_without_reason_text`: `None`, empty and whitespace reasons preserve skipped existence without fallback.
- Extended `gateway_handles_admin_usage_active_locally_with_trusted_admin_principal`: reasonless skipped marker/reasons agree between active response and filtered list.
- Related filters: `users_me_usage`, `skipped_candidate`, existing admin usage candidate-flag/filter tests.

## Boundaries and remaining validation

Root must execute Rust regressions and repair/rerun the unrelated provider type-check error. No live diagnosis or browser screenshot acceptance was performed.

The derived user filter intentionally follows the existing administrator implementation: it materializes the selected scoped audit set and reads candidates per request. No truncation, new SQL/schema, retries or framework is added. Very large historical queries may warrant an indexed projection after measurement; the source marks this ceiling. Existing retry/fallback and model/client-family local filtering policy remains unchanged.
