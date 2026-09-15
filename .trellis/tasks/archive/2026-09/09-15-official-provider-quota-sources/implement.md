# Implementation

- [x] Extend typed source contract and inspect every Rust literal/match.
- [x] Correct provider parsers/regional origins, Kimi wallet and MiniMax adapter.
- [x] Replace Zhipu exclusive fallback with per-source aggregation/persistence.
- [x] Correct scheduling evidence and invalidate caches on eligibility transitions.
- [x] Update frontend types, preset wiring, shared grouped display and sibling preservation.
- [x] Publish capability/unit evidence document.
- [x] Review cross-layer behavior and remove obsolete production logic.
- [x] Run frontend type-check, targeted rustfmt, git diff --check, static contract fan-out and sample replay. No new unit tests or large compilation.

Record validation results below. Gateway compilation is unrun by user constraint.


## Validation completed

- Frontend: `npm run type-check` passed. Five existing focused Vitest files passed, 95/95 cases; only obsolete assertions were updated, no new tests were added.
- Frontend: targeted ESLint passed with no new warnings. The wider changed-file check retains three pre-existing Vue formatting warnings outside the changed logic.
- Rust: targeted `rustfmt --check --config skip_children=true` passed for 25 changed files; no gateway/workspace compilation was run.
- A temporary standalone Rust replay under `/tmp/aether-quota-validation` includes current on-disk parser/contract/scheduling modules and extracted unchanged pure gateway merge/response-decoding functions, linking cached dependencies. This is not a full gateway build or integration test.
- Replayed DeepSeek multi-currency/missing fields, regional Moonshot/SiliconFlow, OpenRouter null/no remaining/period/BYOK, Kimi 10^8 versus 10^2 scales and cross-currency rejection, Zhipu/Z.ai credit/percent/tool scope, and MiniMax old/new counts/boost/excluded/unlimited/unknown currency.
- Replayed successful personal/team coexistence, partial permission failure preserving old values and success timestamps, terminal not-applicable clearing old values without backoff, regional history isolation, multi-plan identity, reset expiry, stale/unknown scheduling, precise 1.0 boundary, and manual-block field preservation.
- `git diff --check`, changed contract literal fan-out and MiniMax registration/domain checks passed.
- The initial implementation used samples only. No commits, no new unit tests, no large module build. Previously dirty guide index, preflight guide and `.omc/` were preserved. Authorized live verification followed below.

## Authorized live verification follow-up

- Queried 9 user-provided API Keys through 11 official quota GET requests. Credentials stayed in process memory, were not written to the repository, and were not retried across regions. No inference requests were made.
- Six Keys returned usable quota data: DeepSeek, OpenRouter, Zhipu, Z.ai and two Kimi Keys. Zhipu personal plan and account balance were merged while its empty team response remained independently unknown/failed.
- Moonshot CN returned HTTP 401; MiniMax CN returned business status 2049 inside HTTP 200. MiniMax business authentication rejection now maps to query permission denied rather than a generic parser failure, without invalidating model calls.
- SiliconFlow CN returned HTTP 410 / business code 20092. Official release notes confirm retirement on 2026-08-14 and no announced replacement. Removed the retired CN request before reading credentials; persisted an unsupported source even on the first refresh. International requests remain independently available from their configured origin.
- Kimi live responses included exact `usages` ratios and two monthly windows missing from older samples. Valid exact ratios now replace duplicate legacy windows; monthly total/Code windows are shown with unknown scheduling scope. Missing or malformed exact ratios retain valid legacy data.
- Live Kimi responses did not include wallet or tier fields. Wallet scales remain covered by official source and temporary sample replay, not by these live Keys.
- Sanitized live response replay and its report are kept outside the repository under `/tmp/aether-quota-live`; no personal balances or identifiers were added to task artifacts.
- Follow-up verification passed: frontend type-check and focused ESLint, 25-file Rust formatting and diff checks, current parser/merge/frontend live response replay, retired first/history-source handling without errors/backoff, no credential read for retired requests, Kimi malformed exact-ratio fallback and observation-only monthly windows. Static call-order review confirms the retired check precedes gateway transport loading/decryption. The credential-holding query process was terminated after requests completed.

## Delivery

Provider capabilities, authentication, field units and primary evidence: `docs/api/official-provider-quota.md`.
The user subsequently authorized committing, pushing and verifying GitHub CI. Publish only the quota change and its task artifacts; preserve unrelated dirty guides and .omc in the original worktree.

- Final cross-layer audit: official subscription refresh no longer creates persistent account quota scores that outlive reset/staleness. Temporary replay passed all three providers for success/error/backoff and confirmed only legacy quota-refresh-owned scores are retired; unrelated hard states and the existing strong runtime scheduling-block guard remain intact.

## Publication preflight

- Replayed the 44 task product/spec/document files onto the latest origin/master in an isolated worktree without conflicts.
- Original unrelated guide/index changes and .omc remain outside the commit scope.
- Credentials were scanned before staging publication artifacts; real keys and account identifiers are absent.
- Gateway compilation and the full test matrix are delegated to GitHub Rust CI as requested; no large local compile is run.
