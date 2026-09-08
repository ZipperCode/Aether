# Research: Current CI/build topology and local validation parity

- Query: Identify evidenced build invalidation, duplicated compilation/testing, and local/CI command drift; recommend minimal changes without deleting features or required gates.
- Scope: mixed; current local files are authoritative for this report, remote run failures/timings belong to the other researcher.
- Date: 2026-09-08
- Baseline supplied by parent: local `798ba9d5c8166febab1ab3c56b29f93f17d0e6ee`, remote master `876a4fb5a95a95d9ccd6493b818176ae53e87eeb`; no Git operations, builds, product edits, DB operations, or CI dispatch performed here.

## Findings

### Files found and applicable instructions

| File | Purpose / relevant evidence |
| --- | --- |
| `.github/workflows/rust-ci.yml` | Main reusable PR/push CI, required aggregate gate, Rust/cache/frontend/DB commands. |
| `.github/workflows/nightly.yml` | Reuses Rust CI, then adds all-feature checks, doctests, repository health, nightly frontend and release artifacts. |
| `.github/workflows/release.yml` | Tag classification, frontend/build/package/image/publication; preserve owner `zippercode/aether`, version propagation and release authorization. |
| `.github/workflows/build-tunnel.yml` | Separate target matrix and locked release builds for tunnel; not an ordinary Rust CI optimization target. |
| `Cargo.toml:44-47,145-158` | Bare Cargo selects only gateway; local dev/test debug is `line-tables-only`; release uses thin LTO and 8 codegen units. |
| `rust-toolchain.toml:1-2` | Repository compiler pinned to 1.95.0. |
| `apps/aether-gateway/Cargo.toml:9-17,27` | Default features empty; optional jemalloc/testkit; data dependency explicitly enables all-drivers. |
| `apps/aether-gateway/build.rs:4-59` | Build-version selection and hardcoded Git HEAD invalidation. |
| `apps/aether-gateway/src/tests/architecture/admin_system.rs:3-77` | Existing source contract preserves explicit version inputs, git-describe selection and tunnel-tag exclusion. |
| `crates/aether-data/runtime/Cargo.toml:10-13` | Default postgres, postgres driver, all-drivers alias to postgres. |
| `crates/aether-testing/integration/Cargo.toml:15-17` | Integration crate enables gateway testkit, unlike package-only gateway checks. |
| `crates/aether-testing/testkit/Cargo.toml:11,18` | Testkit feature adds gateway dependency and testkit feature. |
| `frontend/package.json:7-19` | Build invokes prebuild synchronization; plain build does not run type checking; lint mutates files. |
| `frontend/scripts/sync-vscodex.mjs:43-75` | Every frontend prebuild runs VSCodex web build and copies its output. |
| `Makefile:1,9,580-598` | Existing command front door is Bash-based and has only dev/DB targets, no CI target. |
| `tests/release_supply_chain_test.sh:60-64` | Existing cheap workflow source assertion checks mutable action references; does not resolve Git objects. |
| `tests/compose_database_config_test.py` | Existing stdlib Python fixture convention suitable for a small CI command-contract check. |

No `.github/AGENTS.md`, `.config/nextest.toml`, `.config/` directory, root `scripts/`, or current reusable full-CI script was found in the searched locations. Gateway module `AGENTS.md` was read. CodeGraph was consulted first; its broad build query returned unrelated product symbols, so current exact configuration/build files were read directly.

### Current job graph and exact repeated command families

Most leaf jobs start independently: neither formatting nor shell fixtures gate expensive Rust jobs today.

```text
frontend ---------------------------\
fmt ---------------------------------|
clippy_gateway + clippy_data + rest -> clippy --\
gateway + data + driver matrix + rest + adapter + integration -> test -> check
postgres smoke -> data_db_smoke ----------------/
shell_security --------------------------------/
```

The final `check` requires all six aggregate/leaf branches (`rust-ci.yml:635-657`). `clippy` requires all three lints (`257-273`); `test` requires all six test branches (`515-538`). Preserve their result semantics.

| Job | Command and relevant flags |
| --- | --- |
| Format | `cargo fmt --all --check` (`153`). |
| Clippy gateway | `cargo clippy -p aether-gateway --lib --bins --examples -- -D warnings` (`180`). |
| Clippy data | `cargo clippy -p aether-data --all-targets -- -D warnings` (`214`). |
| Clippy rest | workspace excluding gateway, data, integration; `--all-targets -- -D warnings` (`248`). |
| Gateway tests | Two sequential `cargo nextest run -p aether-gateway --lib` / `--bins`, identical stack=16 MiB and mold flags (`305-319`). |
| Data tests | `cargo nextest run -p aether-data`, required local PG tests true (`355-360`). |
| Data feature matrix | `cargo check -p aether-data --no-default-features --features postgres` and `all-drivers` (`369-397`). |
| Rest tests | workspace excluding gateway/data/integration (`434`). Includes postgres adapter because it is a workspace member. |
| Adapter test | `cargo nextest run -p aether-data-postgres` again (`443-473`). |
| Integration | `cargo test -p aether-integration-tests --bins --tests` with PostgreSQL binaries in PATH (`482-510`). |
| PG smoke | Eight test invocations of same all-feature data lib: two migrations, five exact lifecycle names, one export (`578-612`). These should reuse the same local test binary within that job; command count is not eight full builds. |

### Cause map: observed versus inferred

1. **Observed current bug: invalid worktree watch path.** `build.rs:9` emits `../../.git/HEAD` unconditionally. In a linked worktree `.git` is a file, so this watch target does not exist. Parent supplied prior local evidence: repeated unchanged Cargo invocations recompiled gateway, after an initial 18m46 build, with repeats around 1m41–4m18. This timing was not rerun here and is not GitHub ordinary-checkout evidence. A normal checkout also keeps symbolic `HEAD` unchanged while the referenced branch moves; source archives have no `.git`. Fix the owner, not each Cargo command.

2. **Observed parity gap: local minimal commands do not cover CI contracts.** CI sets incremental=0 and both debug profiles=0 (`rust-ci.yml:76-80`), while Cargo defaults retain line tables locally. CI requires PG in data tests and has dedicated real PG smoke. Workspace/all-target/all-feature paths additionally activate dev dependencies and testkit (`integration/Cargo.toml:15-17`), so passing package-only checks does not establish those paths. Native Windows MSVC is also not identical to Linux/mold; do not label a native-only result full CI parity.

3. **Observed cache environment mismatch.** All Rust leaves use `shared-key: rust-ci-${{ runner.os }}` with no explicit per-family key, save-if or workspace-crate option. Gateway tests set `RUSTFLAGS=-C link-arg=-fuse-ld=mold` only on compilation steps (`310,318`), after cache restoration (`287-291`), so the cache action cannot hash that step-only value when choosing its key. Its documented defaults hash CARGO/RUST-prefixed environment variables and omit workspace crates. Same shared key does not prove same artifacts, feature set or linker; concurrent cache consumers are not a same-run build-artifact sharing system. Cache hit rates and save conflicts require run logs from the other researcher.

4. **Observed setup redundancy, not proven compiler drift.** Test jobs omit `with.toolchain`; the pinned dtolnay action defaults to stable and runs `rustup default stable`. But repo `rust-toolchain.toml` takes precedence over rustup default, so actual repo Cargo still selects 1.95.0 absent another override. Pin setup to 1.95.0 to avoid installing an unused moving stable, not because tests were proven to use the wrong compiler.

5. **Observed trigger blind spots.** Push/PR path filters (`rust-ci.yml:9-37,39-67`) omit `rust-toolchain.toml` and `aether-vscodex/**` although both affect builds. If a shared CI script/fixture is introduced, add its paths too. This is CI reliability, not feature reduction.

6. **Observed duplicate work.** Adapter package is tested by both rest and dedicated adapter job. Keep the dedicated gate and exclude that package from rest, or move gate ownership deliberately; first ensure whole-workspace-only feature union is not lost for that package. Nightly calls full Rust CI (`nightly.yml:61-65`) then repeats frontend type-check and unit tests (`148-154`), in addition to required lint and a versioned build (`144-161`). Release frontend explicitly builds VSCodex (`release.yml:83-87`), then frontend prebuild does it again (`frontend/package.json:10`, `sync-vscodex.mjs:64`). The last duplication can be removed by keeping dependency install and relying on the existing prebuild owner; no new abstraction is necessary.

7. **Not all build work is avoidable.** Gateway lib, gateway unit-test harness and executable tests are different compiler outputs; Clippy metadata cannot substitute for linked test binaries. Integration enables gateway testkit; nightly all-features includes jemalloc and is deliberately different. Release is another optimization profile and CPU/OS target. The existing sccache supports neither system-linker output caching nor arbitrary metadata-only Clippy reuse according to its Rust docs. Do not promise that adding cache/shards removes these costs.

8. **Already present protections/accelerators.** sccache is already installed on Rust compile jobs; mold is already used for gateway tests. Rust CI cancels superseded runs in its event/workflow/ref group (`69-71`). Release and nightly intentionally do not cancel in progress. Keep those policies. No evidence supports paid runners, new cache service, or disabling checks.

### Recommended minimal implementation set (choose with remote-run evidence)

**Convergence after parent supplied historical job measurements:** gateway lib compilation took 6m07–7m26, its actual suite 397–400 seconds, then bins compiled another 3m10–3m20 and ran in about one second. Target cache restored about 252 MB under the same key and post-save reported up-to-date; sccache reported 0 hits / 1 miss / 3 crate-type non-cacheable requests. These are parent/parallel-GH-researcher observations, not local measurements by this agent. Therefore gateway build+suite is the primary path; adapter duplication and unused toolchain installation are secondary, not the principal speed claim. Exact run/job provenance belongs in the companion GH research artifact.

**Priority:** combine gateway target planning (D), repair build invalidation (A), and provide repeatable parity checks (B); make only the evidenced environment correction from C. Do not expand this iteration into all speculative cache/sharding options.

#### A. Fix build-version invalidation at its owner

- Affected product file: `apps/aether-gateway/build.rs` only; preserve version precedence and emitted build type/version.
- Resolve Git-owned watch paths using Git's path resolution rather than assuming `.git` is a directory; only emit existing paths. Ensure a stable `build.rs` fallback prevents whole-package default scanning when Git is absent. Preserve symbolic/detached HEAD changes and branch-ref changes; do not introduce a generic Git metadata framework or new crate.
- Minimal regression: a tiny stdlib fixture that compiles only the build script or a minimal dummy Cargo package using it, not gateway. Cover normal checkout, linked-worktree layout, absent Git, unchanged rerun, and changed version input. Existing architecture contract remains intact; avoid triggering gateway test compilation merely to assert this source contract.
- Dependency: agreement on exactly which existing version semantics must be preserved. `--dirty` currently exists, but HEAD-only watch was already incomplete for dirty/index/tag changes; do not silently broaden to recursive `.git` watching.

#### B. One local/CI command owner plus cheap first gate

- Existing front door to reuse: `Makefile`; existing reusable workflow: `rust-ci.yml`; existing Python/shell fixture conventions under `tests/`.
- Minimal cross-platform implementation when native Windows support is required: one stdlib Python command dispatcher under existing `tools/`, called by thin Makefile targets and CI steps, with a dry-run/list mode that prints exact commands without compiling. Avoid a second Bash-only owner and a second PowerShell-only owner.
- Affected files: `tools/ci.py` (new, name illustrative), `tests/ci_contract_test.py` (new), `Makefile`, `.github/workflows/rust-ci.yml`; one short documented invocation in `README.md` if needed. Preserve each current feature/test filter and environment requirement. Include separate cheap, backend, DB and frontend stages rather than claiming cheap means full coverage.
- Cheap checks reuse formatting and existing fixture commands; do not invoke `npm run lint` because it fixes files. Make expensive jobs depend on cheap formatting/config checks only, not the entire frontend build, and keep final `check` failing when prerequisites are skipped/failed. This improves failure latency/wasted work; it may slightly delay a successful build and needs actual timing.
- Add missing trigger paths for toolchain, VSCodex and the new owner/test. A fixture can assert required gates/commands/trigger paths, dry-run argument arrays, and fail propagation without any Rust build.

#### C. Align cache fingerprint/setup before adding infrastructure

- Affected file: `.github/workflows/rust-ci.yml`; shared runner environment owner if B is implemented.
- Put gateway `RUSTFLAGS` at job scope before cache restoration so the existing action hashes the actual compilation environment. Use explicit 1.95.0 setup for all Rust leaves. Preserve incremental/debug values and mold rather than changing them opportunistically.
- Do not enable `cache-workspace-crates`, cache-on-failure, or a unique key for every feature by default: each changes cache size/save behavior, and checkout mtimes can still invalidate restored workspace artifacts. Inspect current restore/save logs first; only split cache families or designate a saver when demonstrated collision/coverage loss warrants it.
- Minimal verification: YAML/command-contract fixture checks ordering, scope and identical lib/bin flags. No full compile required to verify configuration; speed gain remains unmeasured.

#### D. Plan and run both gateway test targets in one invocation

- Affected file: `rust-ci.yml` gateway job; shared command owner and regression from B if implemented.
- Replace the two identical-environment steps with `cargo nextest run -p aether-gateway --lib --bins` under the existing stack/mold/profile settings. Preserve the `test_gateway` job and all aggregate gate names/results. This runs the same two target families through one Cargo plan and one nextest execution plan, and avoids a second Cargo/build-script/fingerprint planning boundary after the long lib suite.
- **Important limit:** normal library, unit-test library harness and binary harness remain distinct artifacts. Combining flags does not eliminate all 3m10–3m20 of bin compilation; Cargo may schedule independent compile work together, but the saved time is not measured. A one-invocation change is low scope; promising the entire observed bin step as saved time would be false.
- Minimal fixture verifies the combined argument array contains both `--lib` and `--bins`, there is exactly one gateway nextest invocation, env stays unchanged, and the final gate still requires gateway success. No gateway rebuild is needed to validate command wiring. A next CI run must report compile and suite timings before claiming speed improvement.
- Secondary duplicate cleanup (adapter included twice, nightly repeated frontend checks, release repeated VSCodex build) is documented above but is not the gateway critical-path fix. Defer it if it expands this iteration; no gate or necessary feature coverage should be removed to manufacture speed.

### Nextest archiving/partitioning decision

Official nextest supports `cargo nextest archive --archive-file ...`, then execution on other machines; source must be same revision, nextest should match version, archives must be explicitly transferred, and source fixtures are not bundled automatically. This could prevent repeated test compilation only if multiple consumers truly need the same binaries/features/profile/target. Current gateway lib and bins already run sequentially on one runner, not duplicate shards. Adding sharding plus build/upload/download jobs now has no evidenced payoff and would expand orchestration. Defer until remote timing shows execution, not build/link, is dominant or same binaries are actually compiled in multiple jobs. Do not unify all workspace features merely to create one archive; that changes package-only coverage.

### External references, versions and verification

All references are primary sources, read live on 2026-09-08. `web.run` failed with provider HTTP 503; read-only official HTTPS requests were used instead. No credentials were extracted.

- [Cargo build-script change detection](https://doc.rust-lang.org/cargo/reference/build-scripts.html#rerun-if-changed): watches mtimes, recursive scans for directories, `build.rs` can disable default package scanning. Missing worktree-path timing is parent-supplied local evidence, not directly measured by this doc.
- [Pinned dtolnay action source](https://github.com/dtolnay/rust-toolchain/blob/4360b52568e2003a75bf9bc1d59f33a8e3fc893c/action.yml): omitted toolchain defaults stable; action installs it and sets rustup default.
- [Rustup precedence](https://rust-lang.github.io/rustup/overrides.html): repository toolchain file outranks default toolchain.
- [Resolved rust-cache README](https://github.com/Swatinem/rust-cache/blob/6323deb102c322ba6fcbdcafc7e3dddab59af2b6/README.md): shared-key/environment hashing, workspace crate omission, save-if and cache-workspace-crates behavior. This is the commit peeled from the exact current workflow tag object.
- [rust-cache annotated tag API](https://api.github.com/repos/Swatinem/rust-cache/git/tags/49a0bdc70d2e1b713ca9e2869b211fcce03d3c1c): workflow's 40-hex pin is an annotated tag object, which points to commit `6323deb102c322ba6fcbdcafc7e3dddab59af2b6`. Raw content/commits endpoints return 404/422 for the tag-object ID; this alone is NOT proof Actions cannot resolve it. Parent was explicitly corrected after this distinction was established. Do not blindly change pins based on raw-content failure.
- [sccache Rust limitations](https://github.com/mozilla/sccache/blob/main/docs/Rust.md): linker-invoking crate outputs are not cached, incremental must be off, supported emit restrictions.
- [nextest archiving primary documentation source](https://github.com/nextest-rs/nextest/blob/main/site/src/docs/ci-features/archiving.md): build once/partition execution is supported; same-revision source and archive transfer requirements. Live website TLS request failed, official source succeeded.

### Related specs and acceptance boundaries

- `.trellis/workflow.md`: research persists in this task; role writes nowhere outside research; research does not authorize product edits or release actions.
- `.trellis/spec/aether-gateway/backend/index.md` and `quality-guidelines.md`: read; generic quality sections still partly placeholders, so current module AGENTS and executable architecture tests provide concrete build/version constraints.
- `apps/aether-gateway/AGENTS.md`: specific gateway Clippy/test target contract and preservation of architecture tests.
- `tests/release_supply_chain_test.sh`: immutable action/image and attestation contract; do not remove assertions for convenience.
- Parent PRD requires all existing necessary gates, exact-SHA release process, no paid runner/permissions/secret changes and no push/dispatch.

## Caveats / Not Found

- No runtime savings measured; no builds, tests, database operations or remote dispatch executed by this researcher.
- Existing prior local timing and local/remote SHAs are supplied handoff context, not remeasured here.
- Root release workflow preflight (`release.yml:21-65`) validates tags, not exact-SHA CI itself. Exact-SHA CI-before-tag is an established release procedure / parent acceptance requirement; preserve it rather than incorrectly claiming an existing YAML check enforces it.
- Branch protection settings, live cache hit/save behavior, peak memory, failure causes and historical timings are not this researcher's scope; use the parallel GH evidence before final optimization selection.
- No evidence supports removing user-facing features. Large gateway/test trees impose real cost, but the confirmed waste is invalidation, command drift and repetition.
