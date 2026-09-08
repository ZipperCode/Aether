# Full upstream integration checkpoint

## Final implement checkpoint — 2026-09-08 14:33 Asia/Shanghai

- All delegated product scopes are returned; integrator is the only writer. Root alone owns PRD/design/implement/task status, specs/registry, journal and final Git integration.
- Product conflict markers/index: zero. HEAD and MERGE_HEAD still point at the two original parents; merge remains uncommitted.
- PASS already obtained: gateway production lib; contracts/data-contracts all-targets; data/usage-runtime all-targets; provider-transport/model-fetch all-targets; data no-default-features; frontend type-check + 423 targeted tests; full rustfmt; diff whitespace; API coverage generator; five workflow YAML files; four shell syntax checks; release supply-chain fixture; four read-only Compose configurations.
- Gateway all-targets retry `29889`: PASS, 7m00s, after fixture repair. Final `cargo check --workspace --all-targets --locked`: PASS, 7m43s in session `54627`. The same serial chain completed all ten small-crate filters: **17 passed, zero failed/ignored**, final process exit 0. No need to repeat the workspace compile in the independent check unless subsequent code changes invalidate it. Windows Unix-only helper/import warnings remain unmodified.
- Implementer is handing off resolved/staged product and evidence with no running build/test process. Gateway core behavior filters below are explicitly NOT yet executed and remain required in the exclusive check phase; compile is not their behavioral proof. Root-owned spec/registry updates and final commit/master integration are also pending by ownership, not implementation authorization.

## Git and scope

- Worktree: `C:/Users/Zipper/AppData/Local/Temp/aether-full-upstream-20260908` only.
- Baseline: `b0ad8ff7f7bdb1888abac8c06f47f61e76761431`; upstream: `c7e403b410139d12a6189dda9c2bdf0c7c80782e`; merge base: `7892aa94853461c1e634f7a5babbb1280128720f`.
- Executed real `git merge --no-commit --no-ff <upstream>`. Initial 229 conflict paths: apps 100, crates 85, frontend 35, root/CI 9. No commit, push, deployment, existing database, user `.env` or main checkout mutation.
- Accepted upstream deletion of 26 MySQL/SQLite modify/delete paths. The data leaf removed six remaining fork-only obsolete files. Other conflicts were resolved by hunk-level semantics; no blanket ours/theirs checkout.
- All leaf scopes returned; integrator owns the product tree for final machine checks. Root still owns task status, specs, journal and Git commit/master integration.

## Semantic result

- Auth: retain revision-only snapshot reads, atomic revision+value reads, singleflight and tombstones. Upstream string compare-and-set now advances revision in both PostgreSQL and gateway memory; v2 credential binding fields also reach the fork fingerprint read path.
- Candidate planning: retain `CandidateSchedulingContext` separating true Unix seconds from a request distribution seed, transport-free ranked snapshots and one-attempt lazy hydration. Adopt mandatory immutable resolved routing policy; retire old global scheduler-key fallback and obsolete routing `allowed_models` behavior.
- Model fetch: retain compact auth/model projections, one provider-wide whitelist reconcile per successful batch, exact Endpoint evidence and successful-endpoint-only binding replacement. Incorporate upstream native Codex catalog qualification and secret-safe management failures without reverting to full-key batch reads.
- Maintenance: retain shared process-wide admission gate, single-Key strong reads, OAuth credential isolation, quota projection fencing and one startup Pool rebuild.
- Pool: retain fresh balance threshold, persistent administrator-recoverable runtime quota block and separate catalog-state-unavailable diagnostic. Adopt upstream unconditional exclusion of known exhausted subscription quota; do not restore the retired skip switch as a bypass.
- Responses: frozen logical-turn identity uses upstream `ProviderOutboundRequestContext` (old Codex name is its alias); HTTP/WS retries restore that context. Preserve native unknown fields, private native IDs, precommit bare-error classification, visible Gemini thought and no post-commit provider splicing. Official ordinary Responses message IDs normalize through the shared helper; function `call_id` remains intact.
- Lifecycle: one upstream cancellation Guard with fork precise 429 admission settlement; one fork deferred failure response extension with actual fallback candidate attribution. Adopt per-candidate, post-admission first-byte budgets, preserving cost release and lazy same-Key retries.
- Management: retain exact Endpoint create/update/import/export mappings, bound mutation validation before batch writes, quota manual recovery, capability tests, batch settings and Nous OAuth. Full upstream VSCodex, entitlement/usage/operation/UI functionality remains present.
- Header persistence and root container runtime follow the explicitly approved upstream behavior. Temporary Compose parsing used fixture credentials and never started a container.
- Packaging: preserve `ZipperCode/Aether` / `master` and `ghcr.io/zippercode/aether`. Adapt nightly branch validation, release installer source-ref stamping and tunnel checkout to master. Keep GHCR provenance and all upstream build/package checks; do not add source-owner Docker Hub account/secrets or its dangling attestation. Restore Pages workflow functionality. Original frontend CI now installs VSCodex web dependencies before the build. Tunnel source/documentation moves from baseline 0.3.16 to upstream 0.3.17 per Root clarification; no tag/release created and application release identity is not downgraded.
- Deduplicated 28 additional same-name tests across gateway/core files: 26 equal after comment/whitespace normalization; two retained newer equivalent credential test setup (v2-aware decryption or explicit test OAuth credentials). Formats leaf separately removed 33 semantically duplicate tests. Removed obsolete legacy-scheduler test module, retaining upstream replacement behavior tests.

## Evidence files

- `data.md`, `provider.md`, `formats.md`, `frontend.md`, `system-import-export.md`, `execution.md`, `gateway-tests.md` provide exact delegated/manual resolution paths and contract mappings.
- Integrator resolved remaining gateway/planner/auth/maintenance/model-fetch/tunnel/root/CI conflicts directly. Final staged paths are reproducible with `git diff --cached --name-status`; initial conflicts were fixed in `ownership.md` and leaf receipts.

## Actual validation to date

- `cargo metadata --format-version 1 --no-deps` and same command with `--locked`: PASS.
- Owned Rust syntax/format pass over 810 files using `rustfmt --edition 2021 --config skip_children=true`, excluding concurrent leaf scopes: PASS. Later duplicate deletion whitespace was corrected by file-scoped rustfmt.
- `git ls-files -u`: 0 entries. Whole product marker search: no matches.
- `cargo check -p aether-data --no-default-features --locked`: PASS, 160 unused/dead-code warnings in the disabled SQL feature shape. Initial shell invocation failed because `set` introduced a trailing space in debug env; quoted per-process assignments fixed it without configuration changes.
- `cargo check -p aether-data -p aether-provider-transport -p aether-provider-pool -p aether-model-fetch -p aether-runtime-state --all-targets --locked`: first failed at three provider transport auto-merge interface errors (removed template field, two v2 decrypt binding arguments). Fixed the exact source.
- `cargo check -p aether-provider-transport -p aether-model-fetch --all-targets --locked`: PASS after those fixes.
- `python docs/api/generate_format_field_coverage.py --check`: PASS.
- Original `tests/compose_database_config_test.py`: Windows fixture strips Docker plugin discovery environment, so `docker compose` was not discovered (`unknown flag: --project-name`). Existing Docker Desktop direct `docker-compose.exe` v5.1.0 was then used for read-only parsing of all four fixture configurations: PASS; app/PG/Redis service sets and root/Postgres app fields verified. Temporary fixture files were removed. No daemon/service start.
- Frontend leaf: type-check PASS, 423 unique selected logic/state tests PASS, scoped ESLint zero errors (style warnings retained); no browser/visual/full frontend suite.
- Workspace all-targets compile and final formatting/checks currently in progress. No final success claim until result is recorded below.

### Subsequent machine checks

- Global `cargo fmt --all --check`, normal/staged diff whitespace checks, and all-product marker scan: PASS after targeted whitespace fixes; index unmerged count remains zero.
- Existing Docker Desktop Compose executable parses all four configurations; source `.env` was never created/read, and fixture directory was removed.
- Five workflow YAML files parse with existing `js-yaml`: build-tunnel 4 jobs, deploy-pages 3, nightly 11, release 7, rust-ci 17. `bash -n install.sh deploy.sh update.sh generate_keys.sh` and `tests/release_supply_chain_test.sh`: PASS using existing Git Bash at `D:/Program Files/Git/bin/bash.exe`.
- Windows native `boring-sys2`/BoringSSL, `boring2`, `tokio-boring2` and `wreq` built successfully through existing MSVC/NASM/CMake. No Windows dependency blocker remains.
- After workspace fixture errors, `cargo check -p aether-contracts -p aether-data-contracts --all-targets --locked`: PASS. Added missing request metadata field and scheduling CAS test import.
- After data/usage test errors, `cargo check -p aether-data -p aether-usage-runtime --all-targets --locked`: PASS. Migrated removed unfenced credential setter test to upstream CAS; restored the two-argument bodyless usage seed API.
- First gateway all-targets check returned lib 21 / lib-test 37 diagnostics (some duplicates across targets). Root-cause edits cover duplicate definitions/tests, stale imports, mandatory routing snapshot parameters, compact model-fetch outcome/state contracts, Nous session principal fields, new error enum variants and tunnel metadata fixture fields. Production-only gateway rerun begins after this batch; test target rerun follows once production passes.
- New integration regression `memory_system_config_string_cas_advances_revision` asserts successful CAS advances revision and failed stale CAS leaves it unchanged.
- First production-only gateway retry narrowed diagnostics to one obsolete AppState raw-key trait implementation (`state/integrations.rs`), removed after confirming the compact association projection already owns both real callers. The next production retry uses the same task-owned cache; no scope or environment changes.
- Actual filtered Rust tests (serial session54627): `cargo test -p aether-pool-core --lib pool_scheduler_always_ --locked` — 3 passed; `cargo test -p aether-provider-pool --lib runtime_quota_block_survives_model_specific_available_quota --locked` — 1 passed. Remaining filters continue in the same chain.
- Final serial filter results (all commands include `--lib` and `--locked`):
  - `cargo test -p aether-pool-core pool_scheduler_always_`: 3 passed.
  - `cargo test -p aether-provider-pool runtime_quota_block_survives_model_specific_available_quota`: 1 passed.
  - `cargo test -p aether-provider-pool balance_scheduling_`: 2 passed.
  - `cargo test -p aether-data model_fetch_candidates_are_lightweight_and_gate_metadata`: 1 passed.
  - `cargo test -p aether-data system_config_`: 3 passed (SQL shape/revision/tombstone checks, no database connection).
  - `cargo test -p aether-usage-runtime headers_to_json_`: 2 passed.
  - `cargo test -p aether-usage-runtime describing_request_bodies_`: 2 passed.
  - `cargo test -p aether-ai-formats reasoning_replay_removes_only_valid_aether_carriers`: 1 passed.
  - `cargo test -p aether-ai-formats official_message_normalization_is_idempotent_and_compact_stays_strict`: 1 passed.
  - `cargo test -p aether-ai-formats streams_gemini_thought_text_to_openai_responses_immediately`: 1 passed.

All target/caches remained under this worktree; existing user services/database and credentials were not used. No build/test process is intended to survive this handoff. Task-owned transient Compose fixture files were removed; ignored `target/`, frontend/VSCodex `node_modules/` remain for check-agent cache reuse and Root's final worktree cleanup.
- Pool-core subscription regression now asserts both values of the retained historical switch still exclude a known exhausted account, while its separate low-balance regression remains unchanged.
- `cargo check -p aether-gateway --lib --locked` second production retry: PASS in 4m20s, zero new diagnostics. Final workspace all-targets is now checking test/binary variants; Windows-only unused Unix helper/import warnings are recorded rather than changing unrelated platform code.

### Required gateway behavior filters for the exclusive check phase

Compilation is not behavior proof. In addition to the smaller crate regressions, the final check phase must execute (or explicitly report a concrete inability to execute) these existing/new gateway filters without running the entire gateway suite:

- `records_admission_timeout_once_as_429` — sync and stream terminal ownership/admission classification.
- `stream_candidate_watchdog_` — fresh per-candidate/post-admission first-byte budgets and watchdog/terminalization ownership.
- `memory_system_config_` — monotonic revision, tombstones and the newly integrated string CAS revision bump.
- `routed_ranking_hydrates_only_selected_candidates_in_fallback_order` — keep all 2,048 candidates transport-free until actual attempts.
- `same_format_responses_prefetch_retries_bare_error_before_committing_success` — native Responses precommit embedded-error behavior.

Use `cargo test -p aether-gateway --lib <filter> --locked` (task-owned target directory and fixture-only environment). These gateway behavior filters have not yet been run at this checkpoint; do not infer PASS from the production compile.

## Root-owned specification drift to reconcile

- `.trellis/config.yaml`: remove deleted data-mysql/data-sqlite package registrations.
- Root `AGENTS.md`: PostgreSQL-only support/smoke statements.
- Auth maintenance memory contract: remove all-SQL/live SQLite requirements and keep memory/PostgreSQL revision/projection coverage.
- Runtime quota block contract: replace four-backend CAS statements with memory/PostgreSQL.
- Balance scheduling contract: upstream now excludes known subscription exhaustion unconditionally; balance remains its own fact.
- Stream attempt lifecycle contract: new `AttemptCancellationGuard`, `mark_abandoned` and per-candidate post-admission first-byte budget replace duplicate Guard/absolute request deadline.
- Shared auth/model projections, exact Endpoint association, native Responses and logical identity contracts remain required.
