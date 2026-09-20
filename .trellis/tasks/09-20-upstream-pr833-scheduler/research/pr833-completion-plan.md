# PR #833 completion plan

Source: `upstream/pr-833` at `0486435f16b7db31add34133aa819e8477c81ea0`; product reads in the integration worktree.

- `summary_routes.rs` derives skipped existence from nonempty reasons. Derive it directly from `RequestCandidateStatus::Skipped`; keep reason collection independent. Add the same candidate facts to the existing admin active-response override map.
- `user_me_usage.rs` omits skipped facts from both list and active responses. Reuse candidate reads and the existing active override map for both routes; expose only a boolean after authenticated-user usage scoping. Keep provider names, provider keys and raw skip/error diagnostics absent.
- The PR's local-only user filter sees only the initially loaded 100 rows. Support `status=has_skipped_candidate` in the user route before pagination, preserving time, search and API-format constraints. Reuse the administrator's derived-filter approach over the scoped audit set; no SQL/schema changes. Its known ceiling is per-request candidate reads across the selected time range; add an indexed projection only if measured usage warrants it.
- The PR's two table marker sites are admin-only. Add generic user markers to the existing mobile model/desktop type cells. Keep detailed tooltips admin-only. Switch only the new filter to server pagination; preserve current retry/fallback policy.
- Extend existing frontend user/admin active types, polling and list merging so newly observed skip facts survive list refreshes.

Validation: a route-level fixture covers reasonless and reasoned skips, no-skip rows, cross-page selection/search/API-format filters, list/active equality and other-user exclusion; admin inline regression covers reasonless skip independence. Focused Vitest covers user markers/privacy, filter policy, API paging parameters and sparse refresh. Root owns Rust execution; frontend type-check is coordinated after edits stabilize.
