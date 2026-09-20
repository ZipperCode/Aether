# Integration verification

## Source and scope

- Baseline master: `e34c05e8970d46444c3e1480ef94384521fb1fad`, clean and three commits ahead of origin/master.
- Upstream main: `ba7c9f8b270cce63b0515299076b30129d7d64b4`; PR #833: `0486435f16b7db31add34133aa819e8477c81ea0`.
- Isolated branch/worktree and source provenance are recorded in `task.json` and `research/source-provenance.md`.
- Integrated upstream features, completed user/admin skipped-candidate views, fixed representative-Key rejection and real-Key filtering, and retained known persisted diagnostic codes.

## Environment

- Rust 1.95.0 in task-owned Docker container `aether-upstream-check-20260920`, existing `aether-gemini-builder:20260917` image.
- Worktree mounted read-only at `/build`; existing Cargo registry/git and `/target` caches. Cargo uses locked/offline dependencies, four build jobs, incremental/debug disabled, 16 MiB test stacks and the existing clang/lld flags.
- Frontend has a worktree-local `npm ci --ignore-scripts --no-audit --no-fund` installation, including lockfile Vitest 4.1.11. The temporary junction was removed without changing the original checkout's dependency tree.

## Completed checks

- Six affected Rust package suites: **2,150 passed**, zero failed/ignored. Formats 1,026; OAuth 74; Provider Pool 105; provider transport 539; usage runtime 369; video-task core 37. `research/rust-packages.log` records execution. These precede the later candidate diagnostic-code addition.
- Upstream frontend: **10 suites / 143 passed** (`research/frontend-upstream-tests.log`).
- PR #833 completion: **8 suites / 151 passed**, recorded by the implementer.
- Independent reviewer: **4 suites / 85 passed**, including the new empty-user-group regression and final reason labels. These overlap the PR suites and are not additional unique coverage.
- Final frontend type-check and non-mutating scoped ESLint passed. The changed non-PR frontend scope has nine existing template-style warnings but no errors; reviewer-owned paths pass with zero warnings.
- Frontend production bundling passed, including the embedded VSCodex web assets. The standard `npm run build` prehook hit existing Windows `spawnSync npm.cmd EINVAL`; its same commands were executed manually (`npm --prefix aether-vscodex/web ci`, web build, copy generated assets, then `vite build`). No build script was changed and generated files are untracked/ignored outputs.
- Full Rust formatting and staged diff checks passed. Format field-coverage generation check passed in the format implementer receipt.
- Independent source review is complete; detailed boundaries and corrections are in `research/integration-check.md`.

## Combined integration results

- The combined Cargo run compiled the complete gateway test target and executed 639 selected gateway tests: 636 passed, three source-scanning architecture checks failed. All new PoolGroup/actual-Key, stale score, user skip pagination/isolation, persisted reason, user-group, memory xAI lifecycle and selected SSE/signature checks passed.
- The three architecture failures were corrected without relaxing ownership or alias guards: shared source readers normalize CRLF, the positive fixture uses `claude:messages`, and the scheduler assertion enforces the current shared diagnostic predicate and boolean delegation.
- All **209 existing architecture tests** were then compiled directly as the unchanged test module using `rustc --test` with `CARGO_MANIFEST_DIR=/build/apps/aether-gateway`: 209 passed. This reuses the actual assertions and helpers, not rewritten equivalents. A temporary wrapper containing only `#[path = ".../architecture/mod.rs"] mod architecture;` was deleted after execution. Results are in `research/architecture-final.log`; these overlap 206 previously passing tests and are not additional unique tests.
- Combined non-gateway filters: admin 69 passed; data runtime 23 passed; data contracts 7 passed; PostgreSQL 138 passed and 13 ignored; Pool core 9 passed. The initial broad `usage` run also passed 74 data-runtime cases. No ignored database case is counted as passed.
- The initial scheduler-core filter selected zero tests; that is not validation. The compiled scheduler-core suite was explicitly run afterward: **96 passed**. Administrator statistics were separately run: **8 passed**.
- Three additional gateway tests passed: both imported provider-aware Antigravity schema normalization tests and `provider_key_limit_rejects_concurrent_guard_until_release`.
- After the gateway run, changes are limited to the architecture helpers/assertions, a canonical protocol value in the new test fixture and moving two existing URL functions above test modules to satisfy Clippy. No runtime behavior was changed by these final corrections.

## Final gate

PASS: affected-package `cargo clippy --locked --offline --lib --tests --no-deps -- -D warnings` completed with exit 0 in 8m43s on the final source. It covers gateway, formats, data contracts/runtime/PostgreSQL, admin, usage runtime, provider Pool/transport, OAuth, video-task core, model-fetch and testkit. `research/rust-clippy-final.log` is the local raw log. All identified integration failures are fixed; no required code or validation work remains before committing and integrating the verified tree.

## Delivery

Product merge commit `377ca367f362577688974918f9fc6470e4bc33cb` has the original master and exact upstream main as parents. Original `D:/Project/GitHub/Aether` was fast-forwarded to this commit after verifying its baseline and clean worktree; both original local commits and upstream ancestry are retained. Subsequent commits only archive this task and record the session. No remote push or deployment occurred.

## Corrected validation findings

- A new grounding test initially omitted the `compact` argument to Responses `to_raw`; supplied `false`.
- The imported xAI gateway fixture lacked fork `routing_facts`; it now follows existing video fixtures with the default value.
- The representative-key empty-map branch required a named binding to satisfy Rust temporary lifetime rules.
- Two existing PostgreSQL SQL-source assertions assumed LF-only checkout. They now normalize CRLF before asserting the same expressions; no production SQL was changed for these failures.
- The provider form's exhaustive label map lacked xAI; the label is now present.
- Reviewer fixed empty-group loading-state retention and preserved five existing scheduling reason codes through sanitization/count projection, retaining unknown-string sanitization.
- Clippy found two imported `items_after_test_module` violations in Chat/Responses normalization. Existing functions were moved intact above the test modules; no allow attribute or implementation change was introduced.

## Validation limits

- No production/provider request, server restart, deployment, push, tag or release was performed.
- The user's production incident has no supplied trace; the demonstrated code defects and local regressions do not prove a live request was repaired.
- Ignored database integration cases and the explicitly excluded xAI PostgreSQL lifecycle case do not certify a real database. No browser screenshot acceptance was run.
- Whole-frontend ESLint is not claimed: an initial overbroad read-only reviewer command found an unchanged `ModelCapabilityDialog.vue:818` `vue/no-mutating-props` error outside this diff.
