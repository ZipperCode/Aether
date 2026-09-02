# Research: selected-upstream Git integration map

- Query: Build a Git-level integration map for selectively synchronizing upstream `fawney19/Aether` features 1, 3, 4, and 5 onto fork baseline `50c96d060`, while excluding VSCodex, entitlement revocation, the generic usage template, Nightly work, formatting-only work, and unrelated changes.
- Scope: internal
- Date: 2026-09-02

## Findings

### Repository snapshot and method

- Task worktree: `C:\Users\Zipper\AppData\Local\Temp\aether-sync-selected-upstream-20260902`.
- Fork baseline, task `HEAD`, and `origin/master` all resolve to `50c96d060442fb1b612a27c587b91dec4f79a613`.
- `upstream/main` resolves to `cae9aa4134b6bfd4b21dab0c535186232002ed34`.
- Merge base is `7892aa94853461c1e634f7a5babbb1280128720f`; divergence is 119 fork-only commits and 62 upstream-only commits.
- The upstream-only side contains 36 non-merge commits and 26 merge commits. Merge commits are topology evidence only and must not be integrated.
- Exact order below comes from `git rev-list --reverse --topo-order --no-merges 50c96d060..upstream/main`.
- `git cherry 50c96d060 upstream/main` marks all 36 non-merge commits `+`: none has an exact patch-equivalent in the fork. This does **not** prove absence of semantic overlap after the fork's refactors.
- Stable patch IDs expose two upstream-internal duplicate pairs:
  - `fa8e443f7` = `5b6fce1a7` (`040e9a86c0e5c3b6327d286e5cb4280c0dfa9dbb`).
  - `7ae984df4` = `e2154629c` (`64899ead1e38210a9bb455a6024a204aecd76bc4`).
- A non-mutating raw-baseline `git apply --check` probe found only 7 of the 27 selected patches clean in isolation (`4da8c57fe`, `5b6fce1a7`, `64e572533`, `f0b0064f3`, `5bcdcca78`, `88d2b002b`, `7ae984df4`); 20 report at least one conflict. This is a lower-fidelity, non-cumulative probe: applying selected predecessors can remove dependency-context failures, but fork-overlap conflicts remain likely.

### Exact upstream-only non-merge classification

Classification is exhaustive and preserves the Git topological order. `selected` means take the complete upstream patch unless conflict resolution must preserve a fork contract. `prerequisite/subset` means integrate only the named hunk/path subset. `duplicate` means do not take that SHA because the chosen twin carries the same stable patch. `formatting-only` means reproduce formatting at final validation rather than cherry-picking the commit.

| # | Commit | Classification | Rationale / integration treatment |
|---:|---|---|---|
| 1 | `4da8c57fe332d65fd1c01979f205de424238cc87` | selected | R4 foundation: Gemini/Responses compatibility and Antigravity model-fetch alignment. |
| 2 | `8cdfa338e5fa132a21beef5f770b041bfa7f403c` | selected | R4: pair Gemini tool-call history when upstream IDs are absent. |
| 3 | `c4b4dfa996dcd8ce8ff92cb425c6883a492aa90f` | selected | R4: preserve thought signatures through canonical, sync, and stream conversions. |
| 4 | `dd2958a458a582d77803a8e57c9aa3f23672dd7b` | prerequisite/subset | R2 UI persistence dependency only: extract updated-Key return/snapshot hunks from `KeyFormDialog.vue`, `OAuthKeyEditDialog.vue`, `ProviderDetailDrawer.vue`, and the `provider-key-concurrent_limit.spec.ts` assertions. Exclude all settlement changes plus `EndpointFormDialog.vue` and `endpoint-form-dialog-layout.spec.ts`; those are unrelated quota/accounting and layout fixes. |
| 5 | `5687dad17717adc7c326650eb01dfd87681574ed` | selected | R4 follow-up: keep the signature helper test-scoped. Must follow `c4b4dfa996`. |
| 6 | `fa8e443f7b3ce3bc860586c67f05e97810a7a27a` | duplicate | Same stable patch as `5b6fce1a7`; skip this branch copy and use the first-parent-integrated PR #757 copy below. |
| 7 | `5b6fce1a77d8260b151b4f08e3fef8b177a5c1ec` | selected | R4: degrade Responses reasoning summaries for Chat; chosen duplicate representative. |
| 8 | `64e57253310ae21bc470dea3ee69a5a42e1a0c37` | selected | R4: sanitize Gemini tool schemas. |
| 9 | `f0b0064f3de581bf46dda15fcc487e0968dbc815` | selected | R4: enable mixed Gemini tool calls. |
| 10 | `83098f98b6a4b5d2de751cd8c2bda879089ad7f0` | selected | R4: model-gate mixed Gemini behavior and connect Antigravity request handling. |
| 11 | `5bcdcca7849598844262b39083bdbc89c1918cac` | selected | R4: normalize Responses `additional_tools` into Chat. |
| 12 | `1bc2287baa0019f4968468b276336c17a1414976` | selected | R4: normalize mixed tools for same-format Gemini providers. |
| 13 | `9837ce119702934da1785d5c7d1fc83e69922258` | selected | R4: emit the Gemini schema field expected by Antigravity. |
| 14 | `36daba7a34d68c4dca27aa993ffca39bedbd321a` | selected | R4: align Antigravity tool-schema wire fields. |
| 15 | `b35364d7fd9f31e8e95667f2a29f3f8c47972784` | selected | R4: normalize the private search tool name. |
| 16 | `56395945c025c03331bb863b6095644180d62e5f` | selected | R4: restrict Responses compaction candidates to Responses-capable providers. |
| 17 | `4dbf98163e33631ed2f06013ca4b1bb86f228bc6` | excluded | Explicitly out of scope: generic Provider Usage API template. No dependency on R1-R4 was found. |
| 18 | `9631b229b3ac07d5ecccb7990268717a72d086a2` | selected | R2: Key concurrency enforcement across execution paths plus cache-affinity single-account/LRU modes and related admin/UI payloads. |
| 19 | `2fe2600021df7a5a971d1e5d6ec94bcf852ff4aa` | selected | R2: isolate quota exhaustion by model and compact account presentation. |
| 20 | `ee55f46962c397fade75bd5d7150d2e788f8ca9d` | selected | R2: restore Antigravity quota progress presentation. |
| 21 | `57abb20778b19423debe35dde26a3447d5db09cb` | selected | R2: carry and display Antigravity quota reset times. |
| 22 | `a39048eccea368a49e7a36dafb673bef55271046` | selected | R3: stabilize Codex logical identity across retries and WebSocket paths. |
| 23 | `3e540ce589648f24823e7b84e637b1e2aa4ce33e` | selected | R3 dependency/fix: route Codex context through the provider-transport facade. |
| 24 | `ef7caa40e7ffeaf0c62dc99eed8dc53a8dfd95b1` | excluded | Explicitly out of scope: Nightly publication workflow, CI trigger, README, and installer edits. |
| 25 | `d07dc863761533a6b66540344943f7bfe9adca3f` | selected | R3: generalize Codex fingerprint convergence beyond OAuth while retaining Codex/provider and Agent Identity guards; accompanying UI wording/tests are part of that behavior. |
| 26 | `6c71f87589d9e47124e77359054931afae08dd56` | selected | R2: align Antigravity quota summaries across provider and Pool views. |
| 27 | `633363e190415943c37792946c4a63acefcf3408` | selected | R3: classify Pool saturation and reject malformed Gemini calls before downstream stream commit. |
| 28 | `3d87bbf230c5919bd2b21210a304dbeece1cf754` | formatting-only | Pure Rust formatting of files changed by `633363e19`; skip the SHA and run workspace formatter once after conflict resolution. |
| 29 | `30a75832f854fd0c2eff15648777bcb9a7b708b8` | excluded | Explicitly out of scope: VSCodex gateway/frontend/sidecar/extension/deployment integration. Never resolve shared-file conflicts by taking this commit's parent tree wholesale. |
| 30 | `88d2b002be8f5147a61c76014b0ff55c3998bfcd` | selected | R3: ignore no-op Responses `ping` stream events. |
| 31 | `7ae984df4ba66387d6aba548c68ad05499781c35` | selected | R3: apply DeepSeek compatibility only from provider type, valid DeepSeek host, or DeepSeek model evidence; chosen duplicate representative. |
| 32 | `611c29f1f5ef83fc1006d5e2cff0c239c38f0777` | excluded | Explicitly out of scope and dependent on excluded features: installs VSCodex web dependencies in Nightly. |
| 33 | `e2154629caf89359e3f594acc5c746106c9a7983` | duplicate | Same stable patch as `7ae984df4`; skip the later copy carried by PR #777. |
| 34 | `144a28f544a88102466cfc2c12854f53bd6a3b7c` | excluded | Explicitly out of scope: plan-entitlement revocation and associated billing/data/UI changes. |
| 35 | `415b2da81bdd307a808cce231a89d9fd711d78a8` | selected | R1: make routing profiles the policy source, bootstrap a system-default group from legacy config, and add per-format Key priority. |
| 36 | `7323d41fbec5a18fbdc102e50822dd8a6bc29641` | selected | R1: move same-Key retry budget to `sticky_key_attempts`, default 2; retry only the first candidate and lazily advance failover candidates. |

Totals: **27 selected**, **5 excluded**, **1 prerequisite/subset**, **2 duplicate**, **1 formatting-only** = 36.

### Recommended integration strategy and exact batches

Use individual non-merge cherry-picks in the following selected-topological order, with one local integration commit for the `dd2958a45` subset. Stop for focused validation after each batch. Do not merge `upstream/main` and then revert: that would first introduce all 62 upstream commits (including the five hard exclusions and 26 merge commits), then force risky negative conflict resolution across 119 fork-only commits. Do not manually re-create all selected behavior either; patch extraction is justified only for the single mixed commit.

1. **Protocol foundation**
   - `4da8c57fe332d65fd1c01979f205de424238cc87`
   - `8cdfa338e5fa132a21beef5f770b041bfa7f403c`
   - `c4b4dfa996dcd8ce8ff92cb425c6883a492aa90f`
   - selected subset of `dd2958a458a582d77803a8e57c9aa3f23672dd7b`
   - `5687dad17717adc7c326650eb01dfd87681574ed`
   - `5b6fce1a77d8260b151b4f08e3fef8b177a5c1ec`
   - `64e57253310ae21bc470dea3ee69a5a42e1a0c37`
   - `f0b0064f3de581bf46dda15fcc487e0968dbc815`
   - `83098f98b6a4b5d2de751cd8c2bda879089ad7f0`
   - `5bcdcca7849598844262b39083bdbc89c1918cac`
   - `1bc2287baa0019f4968468b276336c17a1414976`
   - `9837ce119702934da1785d5c7d1fc83e69922258`
   - `36daba7a34d68c4dca27aa993ffca39bedbd321a`
   - `b35364d7fd9f31e8e95667f2a29f3f8c47972784`
   - `56395945c025c03331bb863b6095644180d62e5f`
2. **Provider Pool and quota chain**
   - `9631b229b3ac07d5ecccb7990268717a72d086a2`
   - `2fe2600021df7a5a971d1e5d6ec94bcf852ff4aa`
   - `ee55f46962c397fade75bd5d7150d2e788f8ca9d`
   - `57abb20778b19423debe35dde26a3447d5db09cb`
3. **Identity, UI summary, and runtime compatibility**
   - `a39048eccea368a49e7a36dafb673bef55271046`
   - `3e540ce589648f24823e7b84e637b1e2aa4ce33e`
   - `d07dc863761533a6b66540344943f7bfe9adca3f`
   - `6c71f87589d9e47124e77359054931afae08dd56`
   - `633363e190415943c37792946c4a63acefcf3408`
   - `88d2b002be8f5147a61c76014b0ff55c3998bfcd`
   - `7ae984df4ba66387d6aba548c68ad05499781c35`
4. **Routing policy last**
   - `415b2da81bdd307a808cce231a89d9fd711d78a8`
   - `7323d41fbec5a18fbdc102e50822dd8a6bc29641`

The ordering intentionally keeps upstream prerequisites in front of their follow-ups, preserves the selected portion of the observed topology, and leaves the two broadest routing patches until the protocol/runtime surfaces they consume have settled. Run `cargo fmt --all` only after the final conflict resolution; do not cherry-pick `3d87bbf23`.

### Dependency and conflict hotspots

- **Format registry/canonical core:** `crates/aether-ai/formats/src/formats/registry.rs`, `protocol/canonical.rs`, `formats/gemini/generate_content/request.rs`, and `formats/openai/chat/stream.rs` are changed repeatedly by the protocol batch and also diverged in the fork. Resolve behavior/test by behavior/test; taking either full side risks losing fork-specific Responses fidelity.
- **Antigravity sequence:** the raw-baseline patches `9837ce119`, `36daba7a3`, and `b35364d7f` fail to apply even though the fork has no merge-base-relative edits to their only file. This is predecessor-context dependency, not fork overlap; apply `83098f98b` and `1bc2287ba` first, then keep the three Antigravity commits ordered.
- **Pool scheduler and execution:** `dispatch/pool_scheduler.rs`, stream/sync execution, `provider_pool_demand.rs`, and `frontend/src/views/admin/PoolManagement.vue` overlap fork quota scheduling work and recur in `9631b229b`, `2fe260002`, `633363e19`, `415b2da81`, and `7323d41fb`. Preserve the fork's balance/runtime-quota facts and cache invalidation contracts while adding concurrency permits, per-model quota, saturation, and routing policy.
- **Codex identity:** `ai_serving/planner/decision_input.rs`, `ai_serving/transport.rs`, `handlers/proxy/mod.rs`, and provider-transport exports overlap fork Codex/Responses work. `a39048ecc` -> `3e540ce58` -> `d07dc8637` is one chain; do not independently rename helpers before the first two commits are resolved.
- **Model capability test:** `7ae984df4` changes `handlers/admin/provider/query/models/model_test.rs`, which the fork's v0.7.26 model-capability feature also changed. Preserve the local route, saved-reference, and buffered-body contracts; add only DeepSeek evidence propagation.
- **Routing tail:** both routing commits were authored on a parent tree already containing excluded VSCodex/Nightly/entitlement work. Their own path sets contain none of the exclusive VSCodex, Nightly, entitlement, or generic-usage implementation paths, so no excluded commit is a proven functional prerequisite. Shared-file conflict context must still be resolved hunk-wise, never by accepting the upstream parent tree.
- **Mixed commit `dd2958a45`:** whole-commit cherry-pick would silently import quota settlement semantics and a dialog layout fix. Its selected frontend snapshot hunk should be committed locally with an `Upstream-subset: dd2958a45...` provenance note.

### Code patterns anchoring the map

- Routing owns the retry contract at `upstream/main:crates/aether-routing-core/src/model.rs:41-56`; the lazy attempt loop consumes it at `upstream/main:apps/aether-gateway/src/orchestration/attempt.rs:161-203`.
- The system-default routing group is bootstrapped in `upstream/main:apps/aether-gateway/src/state/routing_profiles.rs:13-80`, while the read precedence and legacy fallback live at `upstream/main:apps/aether-gateway/src/scheduler/config.rs:156-230`.
- Key-level concurrency is held by a runtime permit at `upstream/main:apps/aether-gateway/src/provider_pool_demand.rs:341-456` and its release behavior is covered at `provider_pool_demand.rs:711`; this is why UI-only persistence from `dd2958a45` is a subset dependency rather than the enforcement implementation.
- Cache affinity selects `single_account` or `lru` in `upstream/main:apps/aether-gateway/src/dispatch/pool_scheduler.rs:1865-1884`, with behavior checks at `pool_scheduler.rs:2250-2281`.
- Per-model quota isolation is centralized in `upstream/main:crates/aether-provider/pool/src/quota.rs:35`, with an Antigravity regression at `pool/src/lib.rs:849`.
- Malformed Gemini calls are rejected before commit at `upstream/main:apps/aether-gateway/src/execution_runtime/stream/commit_policy.rs:616-658`; Responses ping is asserted empty at `upstream/main:crates/aether-ai/formats/src/formats/shared/stream_core/format_matrix.rs:1370-1379`.
- DeepSeek evidence is deliberately limited to type/validated host/model in `upstream/main:apps/aether-gateway/src/ai_serving/planner/standard/deepseek.rs:3-36`, with hostile-host cases beginning at `deepseek.rs:290`.
- Responses compaction candidate filtering starts at `upstream/main:apps/aether-gateway/src/ai_serving/planner/candidate_source.rs:89-122`; thought signatures are reconstructed at `upstream/main:crates/aether-ai/formats/src/formats/gemini/generate_content/request.rs:487-550`.

### Final path-based scope audit

Treat `50c96d060442fb1b612a27c587b91dec4f79a613` as the immutable audit base.

1. Build an **allowed-path set** from `git diff-tree --no-commit-id --name-only -r` over the 27 selected SHAs, then add only these four `dd2958a45` subset paths:
   - `frontend/src/features/providers/components/KeyFormDialog.vue`
   - `frontend/src/features/providers/components/OAuthKeyEditDialog.vue`
   - `frontend/src/features/providers/components/ProviderDetailDrawer.vue`
   - `frontend/src/features/providers/components/__tests__/provider-key-concurrent_limit.spec.ts`
2. Compare `git diff --name-only 50c96d060..HEAD` against that set. Any extra path fails the audit unless it is individually recorded as a minimal compile/test dependency with an R1-R4 mapping. Do not silently widen the set.
3. Independently assert zero baseline-to-final changes under these hard-deny paths:
   - `aether-vscodex/**`, `frontend/scripts/sync-vscodex.mjs`, `frontend/src/api/vscodex.ts`, `frontend/src/views/user/VscodeControl.vue`, and gateway `*vscodex*` files.
   - `.github/workflows/**`, `README.md`, `install.sh`, `.dockerignore`, `.env.example`, root `.gitignore`, and VSCodex-related Docker/package wiring.
   - `crates/aether-admin/src/provider/ops/architectures/usage_api.rs` and the generic-usage additions to `provider/ops/{actions.rs,verify.rs,architectures/mod.rs}`.
   - `apps/aether-gateway/src/handlers/admin/users/**`, billing/settlement adapters and repository contracts touched by `144a28f54`, `frontend/src/api/users.ts`, `UserPlanDialog.vue`, and `frontend/src/views/admin/Users.vue`.
   - The excluded `dd2958a45` paths: all three `crates/aether-data/adapters/*/src/settlement.rs`, `EndpointFormDialog.vue`, and `endpoint-form-dialog-layout.spec.ts`.
4. Because shared allowed files can still receive excluded hunks, scan the added diff (`git diff -U0 50c96d060..HEAD`) for `vscodex`, `revoke_user_plan_entitlement`, `revoke_user_billing_entitlement`, `/billing/entitlements/`, and generic architecture id `usage_api`. Any hit requires proof it pre-existed the baseline; otherwise fail.
5. Verify provenance:
   - neither duplicate twin pair appears twice in the integrated ancestry/diff;
   - none of the five excluded SHAs or 26 upstream merge SHAs is an ancestor of the integration result;
   - the formatting-only SHA is absent;
   - the local subset commit cites `dd2958a45` and changes only the four approved paths/hunks.
6. Re-run the path audit after formatter/tests. Formatter-created changes outside the allowed path set are unexpected scope expansion, not automatically acceptable.

### Integration requests

- **Implementer:** integrate only the ordered batches above; keep one conflict-resolution note per selected SHA when fork behavior wins; use patch extraction only for `dd2958a45`; never use whole-tree `ours`/`theirs` resolution on shared hotspots.
- **Implementer:** if a selected patch will not compile without a non-selected upstream hunk, stop that batch and record the exact compiler error, source SHA/path/hunk, and R1-R4 link before adding the minimum dependency. This is the only allowed scope exception.
- **Checker:** run the fail-closed allowed-set/deny-set audit before functional validation, then specifically compare local model-capability, Responses, balance scheduling, runtime quota block, and routing behavior against the relevant specs listed below.

### Files found

| Path | Description |
|---|---|
| `.trellis/tasks/09-02-sync-selected-upstream-features/prd.md` | Authoritative R1-R5 scope and AC1-AC7 exclusions/acceptance. |
| `.trellis/workflow.md` | Requires persisted research and later implement/check/spec/commit phases. |
| `crates/aether-ai/formats/src/formats/registry.rs` | Central conversion registry and highest-frequency protocol conflict hotspot. |
| `crates/aether-ai/formats/src/formats/gemini/generate_content/request.rs` | Gemini schema, mixed-tool, ID/signature reconstruction chain. |
| `crates/aether-provider/transport/src/antigravity/request.rs` | Ordered Antigravity wire-schema/search-name follow-up patches. |
| `apps/aether-gateway/src/provider_pool_demand.rs` | Key concurrency permit acquisition/release and saturation boundary. |
| `apps/aether-gateway/src/dispatch/pool_scheduler.rs` | Cache affinity, model quota, failure, and routing-policy convergence hotspot. |
| `apps/aether-gateway/src/ai_serving/planner/decision_input.rs` | Shared Codex/DeepSeek/routing decision context hotspot. |
| `apps/aether-gateway/src/orchestration/attempt.rs` | Lazy first-candidate same-Key retry implementation. |
| `frontend/src/views/admin/PoolManagement.vue` | Fork-overlapping Pool quota/concurrency/Antigravity presentation hotspot. |
| `frontend/src/views/admin/RoutingProfiles.vue` | Routing policy, priority, and sticky-attempt configuration UI. |

### Related specs

- `.trellis/spec/guides/cross-layer-thinking-guide.md`
- `.trellis/spec/guides/code-reuse-thinking-guide.md`
- `.trellis/spec/aether-gateway/backend/index.md`
- `.trellis/spec/aether-gateway/backend/quality-guidelines.md`
- `.trellis/spec/aether-routing-core/backend/index.md`
- `.trellis/spec/aether-provider-pool/backend/index.md`
- `.trellis/spec/aether-provider-pool/backend/balance-scheduling-contract.md`
- `.trellis/spec/aether-provider-pool/backend/runtime-quota-block-contract.md`
- `.trellis/spec/aether-ai-formats/backend/index.md`
- `.trellis/spec/aether-ai-formats/backend/codex-http-responses-contract.md`
- `.trellis/spec/aether-provider-transport/backend/index.md`

### External references

- No external documentation or live web source was needed. All conclusions use the refreshed local Git objects for `upstream/main`, fork refs, task PRD, Trellis workflow, specs, and source snapshots. The upstream GitHub PR numbers are inferred from locally stored merge subjects and are topology labels, not independently fetched claims.

## Caveats / Not Found

- No exact fork-side patch duplicate was found by `git cherry`; semantic equivalents may still exist under different code shape and must be judged during hunk resolution.
- The isolated `git apply --check` probe is not a cumulative cherry-pick simulation, so its 7-clean/20-conflict split predicts hotspots but not the final number of manual resolutions.
- No excluded feature was found to be a functional prerequisite of R1-R4. The routing commits' parents contain excluded work, so compile/test evidence—not parent ancestry—must govern any claimed dependency exception.
- This research made no product-code, ref, index, or worktree-state changes and performed no integration.
