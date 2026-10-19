# Integration verification

## Inputs and decisions

- Original master: `047cd3f3e4c5674c6d0304bc4911284028f15e46`.
- Upstream main: `9453e6b3255d2831490a51b959252b7341358c77` (24 incoming commits, including merges).
- Integrated open PR #881 `7776232f8f6b060fcd40a030ecc6497823e9ad49` and #885 `0fd0c5050451f24dcc9a1ddce565e02741d67f15` from `fawney19/Aether`.
- #840 `f3cfa971151017d08aea4d387da67439dc95caa8`: superseded by local stateful prefetch handoff `f18dad7ab`; all four handoff regressions passed in the workspace run. No redundant overflow buffer added.
- #886 excluded: financial/referral workflow rewrite with incomplete upstream PostgreSQL integration verification.

## Completed checks

- Frontend type-check, all 266 Vitest files / 2,225 tests, production build passed. Subsequently removed incidental legacy-phrase wording test; remaining seven i18n behavior tests passed.
- Read-only ESLint of touched frontend API/usage/system settings surfaces passed.
- Formats package: 1,087 tests passed, including stop sequence boundaries and Claude conversion.
- Pool core: 27 focused tests passed.
- Final `cargo fmt --all --check` passed.
- Gateway `python3 tools/ci.py clippy-gateway` passed with warnings denied.
- `python3 tests/ci_contract_test.py` and `python3 tests/gateway_build_watch_test.py` passed.
- Actual gateway executable build passed.
- Tracked PostgreSQL live regressions: all 50 passed, including health timeline totals, dashboard customer billing and full-body persistence.
- Final isolated `cargo test -p aether-data --lib live_table_maintenance_skips_missing_tables_and_vacuums_existing -- --ignored --nocapture` passed (one test), proving analyzed relation statistics and missing relation skip. Earlier aggregate attempt was interrupted after this test passed by a SIGKILL on an unrelated filtered integration target.
- Final isolated data/runtime PostgreSQL `cargo clippy --all-targets --locked -- -D warnings` passed.

## Actual runtime/browser smoke

- Rebuilt gateway port 18089 used disposable PostgreSQL 18 database `aether_smoke`; production bootstrap applied 86 tracked migrations and seven backfills. Actual `db status` reported schema/backfills current. No production database touched.
- Health HTTP 200; admin login HTTP 200 after providing required random `JWT_SECRET_KEY`. Initial 503 without this launch prerequisite was configuration, not a code defect.
- Browser signed in through actual login dialog, opened `/admin/system`, displayed compaction off / body size 0 defaults.
- Saved 64 KB cap and enabled Base64 compaction through UI. Both PUTs HTTP 200; reload retained values; PostgreSQL confirmed them. Restored defaults 0/false afterward.
- Actual dashboard, usage and health monitor pages rendered without browser runtime errors, including populated records/cards after protocol requests. PII-hit rendering is covered by existing behavior tests, not claimed as browser proof.
- Actual Claude `/v1/messages` request returned HTTP 200; captured outbound OpenAI `stop` was a JSON array containing the single supplied stop sequence.
- Actual native Gemini request returned HTTP 200; captured outbound `functionResponse.id` remained `call_abc_123`. See `protocol-review.md` for paths and values. Local mock remained in memory, with no permanent mock/scaffold files.
- Closed browser tab, gateway process and frontend preview, and removed disposable Redis container after smoke.

## Failures corrected and remaining verification

- Initial workspace compilation: fixed missing `is_vertex` in Antigravity test constructor; removed source-only SQL assertions rather than repinning implementation details.
- First ignored PostgreSQL run: 35 passed / 15 failed. Fourteen failures were fixture preparation errors (raw SQL without migration ledger replayed old view migrations); prepared fresh `aether_regression` through real `db prepare`. One analytics fixture lacked its API key FK owner; fixed seed and cleanup. Corrected run: 50/50 passed.
- Workspace run reached gateway tests: 5,586 passed, three ignored, three failed because native `initdb`/`postgres`/`pg_ctl` were unavailable. No claim of gateway suite pass.
- During later workspace/doc/lint targets, repository `target/` disappeared externally. Missing dependency artifacts and SIGKILL interrupted aggregate checks. No agent ran target deletion/clean. Rebuilt in isolated `/tmp/aether-oct19-verify` with two compiler jobs and two test threads, explicitly excluding two native PostgreSQL-only auth tests and supplying Docker PostgreSQL for the xAI test.
- Restricted isolated workspace result: all package suites and doc tests passed except `responses_websocket_e2e`. Gateway: 5,587 passed, three ignored, two explicitly filtered. xAI PostgreSQL lifecycle passed.
- `responses_websocket_e2e`: 13 failures, twelve missing native `initdb` and one `AddrInUse` while reserving a port before database startup. Its harness unconditionally starts `ManagedPostgresServer` and does not accept the Docker test URL. Native `initdb`, `postgres`, `pg_ctl` are not installed on this host; searched existing installations and PATH. No dependency installed or test harness bypass added.
- User explicitly authorized committing and pushing despite the documented local environment limitations, then publishing a new tag only after GitHub CI succeeds for that exact commit. Publish target: `ZipperCode/Aether` master; next patch tag follows remote release history. Disposable database containers and isolated build directory were removed after verification.
