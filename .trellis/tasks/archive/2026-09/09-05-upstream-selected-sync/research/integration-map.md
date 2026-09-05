# Research: selected upstream integration map

- Query: Map every non-merge upstream patch after `ddcbeb3ae`, its dependencies, conflicts, preservation checks, integration order, and validation gates for selected categories 1/3/4/5.
- Scope: internal
- Date: 2026-09-05

## Findings

### Repository state and comparison boundary

- Current `master`/`HEAD`: `5afc3e96d70b1b6d0926af88c35e7b107f8f0242`; `origin/master`: `c51adc205a44c10fcb64ddc4cddae03c8ed82b86`; the local branch is ahead by five commits and must not be rewritten.
- Current cached `upstream/main`: `27b0381a9add065ed24d3df75c98cd6a1ef45afa`; `ddcbeb3ae99e4145c2a56ad261ab1d73b1c6bfdb` is its ancestor. The long-lived fork merge base remains `7892aa94853461c1e634f7a5babbb1280128720f` (`master...upstream/main = 157/121`).
- `ddcbeb3ae..upstream/main` contains 20 commits total and 13 non-merge patches. `git cherry -v master upstream/main ddcbeb3ae` reports `+` for all 13. Stable patch IDs are recorded in the classification table below; no exact patch is already absorbed.
- The five local-only commits are: product fix `c465e50a` (Endpoint evidence loading), spec `f2b37a8b`, task evidence `1b6bc7d3`, task archive `38a4f274`, and journal `5afc3e96`. Only `c465e50a` changes product files: `frontend/src/api/admin.ts`, `BatchAssignModelsDialog.vue`, and its loading regression test.

### Patch classification

| Order | SHA | Stable patch-id | Classification | Direct apply to current tree | Core scope |
|---:|---|---|---|---|---|
| 1 | `fe8ff268` | `1c36d9f9` | selected functional (3) | conflict | Antigravity OAuth email identity; Codex reset-credit local consume/merge/UI projection |
| 2 | `c5ae9c2c` | `77450c41` | selected functional (3), depends on `fe8ff268` branch order | conflict | Import routable Antigravity quota-discovered model IDs into the catalog |
| 3 | `c8d1ae3e` | `d29a66df` | test-only support for `fe8ff268` | clean alone | Preserve the expanded reset-credit fixture when credential generation rejects a reservation |
| 4 | `ba11a722` | `49463eb8` | test-only support for selected routing/runtime contracts | conflict | Align gateway fixtures/assertions with current routing contracts; inspect item-by-item, do not treat as product dependency |
| 5 | `dabaeb8d` | `18ddd85c` | excluded | missing file | Nightly image-owner workflow only; `.github/workflows/nightly.yml` is absent locally |
| 6 | `1eb2d10d` | `2478a21c` | selected functional (1) | conflict | Filter per-model Provider ordering by GlobalModel ID; retain full list for unified/global-key modes |
| 7 | `66d6c17d` | `13945c7f` | selected functional (5) | conflict | Commit on non-empty Gemini reasoning; emit structured terminal error and usage across stream formats |
| 8 | `14744abd` | `ce125df0` | selected functional (5), logical follow-up to `66d6c17d` | clean alone | Strip Aether Gemini signature carriers before replay to OpenAI/Codex Responses |
| 9 | `9282cce1` | `a4c81534` | selected functional (4), overlaps an existing local guard | conflict | Settle pre-first-byte dropped attempts; compactly preserve request-body capture descriptors; avoid watchdog race |
| 10 | `20699564` | `61cee920` | selected functional (4), depends on the candidate-loop/watchdog shape from `9282cce1` | conflict | Share one first-byte deadline across candidate retries |
| 11 | `86f7cc0d` | `b4381c6b` | test/build-only support | clean alone | Rust 1.95 integer-lint suffix in a gateway unit test |
| 12 | `344b3031` | `d8528e90` | test-only support for category 4 | clean alone | Make the mock h2c truncated SSE ordering deterministic |
| 13 | `57cdef4b` | `a125e511` | selected functional (4) | clean alone | Fail closed when cross-format sync finalization sees truncated/incomplete products |

### Critical semantic overlap already found

- Current `master` already has `StreamAttemptTerminalGuard` in `apps/aether-gateway/src/execution_runtime/stream/execution.rs` (introduced by local `60377958`, Endpoint exact binding/failure isolation). It is armed after pending candidate/usage rows are written and its `Drop` schedules 499/cancelled settlement unless the watchdog already claimed terminal ownership.
- Therefore `9282cce1` must not be layered on top as a second cancellation owner. Its novel pieces must be reconciled into the existing single-owner path: body-capture-describing seed rather than a body-clearing empty seed, compact capture rather than cloning both request bodies for every live attempt, and explicit watchdog abandonment/claim ordering.
- Current routing editor already has a documented `globalModelId` prop and resolved-ID computed value, but `loadProviders()` still always requests all providers. `1eb2d10d` supplies the missing query/race behavior under a differently named upstream prop (`modelId`); adapt it to the local `globalModelId` contract instead of renaming/reverting the local API.

## Caveats / Not Found

- This file is being completed from the current local refs without fetching or changing any Git ref. Remote freshness is the parent task's confirmed `upstream/main=27b0381a9` boundary.
- Final per-file conflict map, preservation test names, ordered validation commands, and spec context are appended after targeted source/spec inspection.
