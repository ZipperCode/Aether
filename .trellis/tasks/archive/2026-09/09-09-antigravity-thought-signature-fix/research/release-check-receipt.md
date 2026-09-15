# Antigravity release check receipt

## Scope and result

- Reviewer: `antigravity_release_check`, 2026-09-15, `D:/Project/GitHub/Aether`.
- Branch: `master`; dispatch HEAD: `72cdcf6d86a43d9a69b70f04b46472c739a752e0`; final inspection HEAD: `74d7670f6e5fbd9c2ba8f1246ab3a5418174dab0` (main session owns Git).
- Reviewed all **17 dirty Rust source files**, including the untracked recovery module, against check.jsonl, PRD, design, implement, route receipt, final implementation checkpoint and the three package quality/spec documents.
- Source review: **通过 after self-fixes**. Formatting/whitespace: **通过**. Compilation, Clippy and runtime regression execution: **无法验证 locally** because the authorized Docker builder is absent; GitHub exact-SHA CI remains required before a tag.
- Only existing source scope and this receipt were written. No commits, push, tag, service changes, live inference, host Rust compilation or Docker containers were made by this reviewer.

## Findings (fixed)

1. **Ordinary stream requests copied provider bodies into recovery snapshots.**
   - `attempt_cancellation.rs::refresh_signature_recovery` ran after every stream response, even without recovery.
   - Added a compact-diagnostic gate before any copy; removed the unused plan parameter and updated the stream callers.
2. **Cancellation retained the repaired body in the wrong usage field.**
   - Putting `provider_request_body` in `request_metadata` did not update `UsageEventData.provider_request_body`, leaving the original pending capture.
   - Store the repaired value in its actual capture field only after recovery; retain original-client capture ownership. Candidate and watchdog keep the existing compact recovery diagnostic.
   - Added one regression, `antigravity_signature_cancellation_snapshot_keeps_actual_capture`, checking both the no-recovery/no-copy case and recovered candidate/watchdog/body snapshots.
3. **Admin error capture used the pre-recovery body.**
   - `provider_query_execute_antigravity_test_candidate` used a stale local body on recovery transport failure.
   - Read the final plan body before branching on execution success/failure. Ordinary failures without a recovery diagnostic continue propagating through the original outer error handling.
4. **Sync forced-terminal snapshots could preserve an older recovery outcome.**
   - Refresh the terminal guard after the final response is observed and after resolving the remaining OAuth-retry deadline, so later failure/cancellation retains the latest result or deadline exhaustion.
5. **Formatting and small test hygiene.**
   - Formatted only the 17 changed Rust files with `skip_children=true`; moved the new recovery module's test block after production items.
   - Removed one unused admin test import. Used the existing sync tunnel request header frame to assert identical authorization on the two sends.

## Review coverage

- **Gate and rewrite:** explicit Antigravity envelope plus Gemini format; actual HTTP 400 + exact INVALID_ARGUMENT message; no broad JSON traversal; public/native contents, aliases, input immutability, missing/already-compatible no-op.
- **Execution:** first body stays original; one candidate-local repair allowance; repair uses the same provider, endpoint, key, URL, model and authentication; OAuth allowance stays separate.
- **Disposition:** Provider scope is selected within the existing RetryNextCandidate branch; StopLocalFailover remains terminal; ordinary error policy is retained.
- **Routes:** direct and authenticated embedded-tunnel stream/sync, plus fixed-target admin execution. Local sync tunnel uses the shared sender and response-start callback.
- **Lifetime:** intermediate rejection precedes terminal/health/pool failure owners; original candidate ID and lease ownership are retained; shared remaining deadline, report context, actual final body and cancellation/watchdog projections were inspected.
- **Persistence:** fixed diagnostic fields survive both data-contract persistence and read projection; no public DTO, database schema, frontend, template, configuration or dependency changes were needed.
- These are source conclusions. Runtime cardinality/settlement, transport and OAuth assertions remain to be executed on the final SHA in GitHub CI.

## Verification

| Command | Exit | Result / count |
| --- | ---: | --- |
| `git rev-parse --show-toplevel`, branch, HEAD and status inspection | 0 | Correct repository and existing dirty scope |
| `python ./.trellis/scripts/get_context.py --mode packages` | 0 | Package context loaded |
| `docker context show` | 0 | `desktop-linux` |
| `docker image ls -a --no-trunc --format '{{.ID}}\|{{.Repository}}:{{.Tag}}\|{{.Size}}'` | 0 | No authorized builder or matching image ID |
| `docker volume inspect aether-trellis-cargo-registry aether-trellis-cargo-git aether-trellis-target` | 0 | All three existing cache volumes present |
| `docker image inspect aether-gateway-builder:trellis-check --format '{{.Id}}'` | 1 | `No such image`; no rebuild requested |
| `rustup run 1.95.0 rustfmt --version` | 0 | `rustfmt 1.9.0-stable (59807616e1 2026-04-14)` |
| `rustup run 1.95.0 rustfmt --edition 2021 --config skip_children=true --check <17 changed Rust files>` before fix | 1 | Existing scoped formatting drift |
| `rustup run 1.95.0 rustfmt --edition 2021 --config skip_children=true <17 changed Rust files>` | 0 | Scoped mechanical formatting |
| Same `rustfmt --check` after fixes | 0 | **通过**, 17 files |
| `git diff --check` after fixes | 0 | **通过**, only Git LF/CRLF normalization notices |
| Docker Cargo / Clippy / tests | Not run | **无法验证**, missing authorized image; final validation delegated to exact-SHA GitHub CI |

Current source contains **10 gateway tests** matching `antigravity_signature` (9 existing plus 1 new) and **1 data-contract regression**; the provider `antigravity::request` suite was previously recorded as **8 tests**. These are selection/source counts, **not current passing counts**.

Historical route/final-implementation receipts recorded nonzero results: **8 provider + 1 data-contract + 9 gateway passed**, with gateway `cargo check --locked -p aether-gateway --lib` exit 0. Those results predate this review's fixes and do not certify this fingerprint.

## Reviewer-changed paths

- `apps/aether-gateway/src/execution_runtime/antigravity_signature.rs`
- `apps/aether-gateway/src/execution_runtime/attempt_cancellation.rs`
- `apps/aether-gateway/src/execution_runtime/stream/execution.rs`
- `apps/aether-gateway/src/execution_runtime/sync/execution.rs`
- `apps/aether-gateway/src/execution_runtime/transport_failure.rs`
- `apps/aether-gateway/src/executor/candidate_loop.rs`
- `apps/aether-gateway/src/handlers/admin/provider/query/models/model_test.rs`
- `apps/aether-gateway/src/handlers/admin/provider/query/models/model_test/tests.rs`

## Source fingerprint

Manifest: sorted path + one space + raw-file SHA-256, joined with LF without a trailing newline.
Aggregate SHA-256: `c1b76515d20a4dd843ff5ba3ba091fe14fc94e53f7e3ba3798ca24cce57ca5ce`.

| Source path | SHA-256 |
| --- | --- |
| `apps/aether-gateway/src/execution_runtime/antigravity_signature.rs` | `c5fea80b80ceebe375880241b8325acf0574367da98224b8b1305421ef88db3c` |
| `apps/aether-gateway/src/execution_runtime/attempt_cancellation.rs` | `3cc8239e255093c981647078a111588015773ca66120d663b67c23173539ddee` |
| `apps/aether-gateway/src/execution_runtime/mod.rs` | `3bc6addba4468e6ce6b4860d2ff5a88497699a8e38724401428bf527c12e4214` |
| `apps/aether-gateway/src/execution_runtime/stream/execution.rs` | `672a687f6618b0ea2ba4472bd543d52af6aedba814b2c20a3d72836a3abe3927` |
| `apps/aether-gateway/src/execution_runtime/stream/mod.rs` | `26fa3e4ea532e81c7f73c5403690e7e6b2556e20e8a8c8352391769485703e7b` |
| `apps/aether-gateway/src/execution_runtime/sync/execution.rs` | `e6bcd85f2f6798356a0eac2980c13a520e2d4de13f9dc0d8b7153f2bed11682b` |
| `apps/aether-gateway/src/execution_runtime/transport_failure.rs` | `5501373a8021098f89e551c95b87de6f69c9a96d4fddb047046c21c2a97ccf6a` |
| `apps/aether-gateway/src/execution_runtime/transport.rs` | `5973d9918879138e887f0ddee5c7ad347078573c637cceb20a2c2d7d76e3630a` |
| `apps/aether-gateway/src/executor/candidate_loop.rs` | `fe20d0daf20d892942d0faf459675baf0423ac69e025ea707e5f0847a7b96227` |
| `apps/aether-gateway/src/handlers/admin/provider/query/models/model_test.rs` | `8856ae9fd1d9c34b6eff8b21f08db650238bed849a96322634d88e5aea595c62` |
| `apps/aether-gateway/src/handlers/admin/provider/query/models/model_test/capability_test.rs` | `4d9af18f7e7ae7b05d11ff6785248254760bc856884a2e22d525b511692ae0bb` |
| `apps/aether-gateway/src/handlers/admin/provider/query/models/model_test/summary.rs` | `2555a6ec0d2cbdd5ec7bb5909f2e321e10a475f6cb79bb3c43e7e3690ff80d3e` |
| `apps/aether-gateway/src/handlers/admin/provider/query/models/model_test/tests.rs` | `79bee3dc868279efe92c16e66b59d904f7678d63664d973b3de5e47ff82377ff` |
| `apps/aether-gateway/src/request_candidate_runtime.rs` | `7dc00fa66c264ae3cf1c233a5bb08f928da10980ca44a2ac5c40469898fb9d20` |
| `crates/aether-data/contracts/src/repository/candidates/types.rs` | `ef1f6ff3589074fb1ea3a818fd9d237b94dbd4f8f37cbb1f694727b81ed42742` |
| `crates/aether-provider/transport/src/antigravity/mod.rs` | `11ac97527923f842a3e636e79c6d60794b49114fe9793fc43ec59d355a1d1e2d` |
| `crates/aether-provider/transport/src/antigravity/request.rs` | `826af6b6a87a616fa500a2932d9170d9d147adb13ccef2098dfea965b197c60f` |

## Findings (not fixed) / limitations

- No known remaining blocking source finding within the assigned diff.
- **Clippy / TypeCheck / tests: 无法验证 locally** due to missing builder. Do not claim a quality-gate or release PASS until all required GitHub Rust CI jobs succeed for the final candidate SHA.
- The new snapshot regression and admin transport-error capture fix have not been executed locally. Existing success/rejection direct/tunnel tests plus full required GitHub checks must run on the new source.
- No production replay, deployment, remote service or credential verification was performed. This review makes no live Antigravity-success claim.
