# Integration review receipt

Worktree: `C:/Users/Zipper/.codex/worktrees/sync-upstream-pr833/Aether`.
Baseline: `e34c05e8970d46444c3e1480ef94384521fb1fad`; upstream: `ba7c9f8b270cce63b0515299076b30129d7d64b4`; PR #833: `0486435f16b7db31add34133aa819e8477c81ea0`.

## Findings fixed by reviewer

1. `frontend/src/views/admin/UserStats.vue` and `__tests__/UserStats.empty-group.spec.ts`: switching from an in-flight user summary to an empty group scope invalidated the old request but left summary/trend loading set forever. Reset the three loading refs in the existing empty-selection branch. The mounted Vue regression failed before the fix (two loading indicators remained), then passed; it also rejects the late old-user response.
2. `crates/aether-data/contracts/src/repository/candidates/types.rs`: five existing fork scheduling reasons were absent from the diagnostic allowlist, so persistence and pool exhaustion count projection collapsed them into `unclassified_skip`. Added exactly `key_quota_exhausted`, `key_balance_below_minimum`, `pool_key_quota_exhausted`, `pool_balance_below_minimum`, and `pool_key_state_unavailable`. The new `scheduling_skip_reasons_survive_persistence_and_count_projection` regression checks repeated projection, distinct count identity, and continued sanitization of an unknown sensitive category. Root owns its Cargo execution.
3. `frontend/src/features/usage/utils/skipReason.ts` and its existing test: added those five reason labels, distinguishing manual recovery from low balance and unavailable state, and corrected the unclassified sentinel to the backend's `unclassified_skip`. No new scheduling policy or ordinary-user diagnostics were added.

## Review conclusions

- Formats/Antigravity: checked private schema deferral and both private envelope paths, Claude-only unsigned-thought cleanup, Gemini grounding part/offset handling, Responses long-call pairing/raw-reasoning output, and exact local signature-rejection recovery. The merged private schema behavior is covered by the updated spec; public Gemini passthrough remains separate.
- Provider/OAuth/xAI: registrations retain local Nous/official quota adapters; xAI device polling remains behind the existing principal, session and Provider identity fence. xAI host selection preserves Gemini CountTokens routing, provider credential ownership and existing quota persistence. No new concrete integration defect found here.
- User-group statistics: group membership becomes explicit `user_ids`, including an empty set that cannot broaden to all users; summary/time-series/leaderboard readers propagate the scope through memory and PostgreSQL ownership. Administrative route checks remain in place.
- Routing UI: multi-model Provider filtering uses the resolved model union, preserves an explicitly empty scope, and merges hidden Provider overrides when reordering visible rows.
- Usage retention: the runtime production body-capture path has no baseline delta; runtime changes are test-fixture synchronization. Complete-body persistence and original byte/prefetch regressions remain present.
- PR #833: both user search and ordinary list bind the authenticated user before reading candidates. Skipped filtering occurs before total/offset/limit. Explicit-ID active requests recheck ownership; discovery polling binds the same user. User payloads contain only the boolean, while admin list/active payloads preserve the allowlisted reasons. Desktop/mobile markers remain private and record merging retains observed skip facts.
- Scheduler: representative Key runtime facts no longer reject a PoolGroup; Provider quota/concurrency and request admission remain. Real strongly read Keys pass shared concurrency/health/RPM checks before active-probe fallback and window truncation. Existing manual quota, account/auth, balance, cooldown and catalog failure reasons retain precedence. Sticky and normal paging use that same path. The existing 128-record accounting window and atomic execution admission are unchanged.
- Gateway compile evidence surfaced `E0716` for the temporary empty map in the representative branch; root was notified and owns that correction, alongside the imported xAI fixture's missing `routing_facts`.

## Verification

- PASS: mounted `UserStats.empty-group.spec.ts` after a demonstrated failing run.
- PASS: final scoped Vitest run of `skipReason.spec.ts`, `UsageRecordsTable.spec.ts`, `HorizontalRequestTimeline.spec.ts`, and `UserStats.empty-group.spec.ts`: **4 files / 85 tests**. This overlaps earlier PR tests and must not be summed as unique coverage.
- PASS: reviewer-owned four frontend files with non-mutating `eslint --max-warnings 0`.
- PASS: stable non-PR changed frontend scope (29 files), non-mutating ESLint: 0 errors, 9 template-style warnings in OAuthAccountDialog/ProviderFormDialog. No `--fix` was used.
- PASS: frontend `npm run type-check` after all reviewer changes, including the UserStats regression and skip-label/test additions (exit 0).
- PASS: scoped rustfmt on candidate contract changes and scoped `git diff --check`.
- Reused root evidence: `rust-packages.log` has formats 1,026, OAuth 74, Provider Pool 105, transport 539, usage runtime 369, video-task core 37 passing tests. These precede the reviewer's diagnostic allowlist edit. `frontend-upstream-tests.log` has 10 files / 143 passing tests; the PR completion receipt records its separate 8-file / 151-test run.
- Root must finish the gateway/Data-contract and corrected PostgreSQL source-test runs. The earlier `rust-usage-data.log` contains two CRLF-sensitive SQL-source assertion failures, subsequently corrected by root; it is not final PASS evidence. Ignored PostgreSQL integration tests do not certify a real database.

## Findings not changed / boundaries

- An initial incorrectly scoped read-only ESLint invocation also scanned unchanged frontend code and reported `ModelCapabilityDialog.vue:818` (`vue/no-mutating-props`) plus style warnings. This pre-existing file is outside the integration diff and was not edited; do not report whole-frontend ESLint as green.
- Root owns the updated persistence-code/skip-observability/Pool specs, all Cargo invocations, index, commits and final integration. No source staging, commit, workspace formatter, live provider request, database smoke or browser screenshot acceptance was performed by this reviewer.
- No further concrete source defect was found in the reviewed final PR/scheduler paths. Production causality remains unconfirmed without a supplied live request trace.

Memory was used only to locate the historical exact-signature and complete-body boundaries; current task artifacts, specs, source and logs were checked directly.

## Architecture follow-up

Root's combined gateway run compiled and passed the new scheduler/PR833 regressions, but reported three architecture failures in `rust-integration.log`. Reviewer resolved their concrete source causes:

- `tests/architecture/mod.rs`: normalize CRLF to LF in both `read_workspace_file` and `read_workspace_module_tree`. The existing Provider query/strategy routes already delegate to their correct owners. Read-only reproduction found raw CRLF in each file; the original multiline LF assertion was false before normalization and true afterward. No production delegation or ownership assertion was removed.
- `tests/frontdoor/public_support.rs`: change the new PR833 fixture's positive `claude:chat` format to canonical `claude:messages`. The retired-format allowlist and scanner are unchanged. An exact equivalent read-only scan covered 2,804 source files with zero unwhitelisted retired-alias occurrences after this fix.
- `tests/architecture/runtime_and_security.rs`: replace the stale `candidate_is_selectable_with_runtime_state` symbol assertion with checks for the actual `aether_scheduler_core` diagnostic invocation, absence of a local diagnostic redefinition, and boolean selectability deriving from the same gateway diagnostic result. Existing state-trait, affinity and local-ownership prohibitions remain; no compatibility wrapper was introduced.

Scoped `rustfmt --check` and `git diff --check` pass on these three files. Read-only source checks confirm both route delegations and scheduler predicate convergence. All follow-up sources are stable; root owns Clippy and the final architecture/gateway rerun. No Cargo, Git index change or running-process mutation was performed here.
