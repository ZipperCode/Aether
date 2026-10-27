# 修复发布 CI 阻塞并发布 v0.7.50

## Goal

Push the current `master` commit, obtain a green GitHub CI run for that exact commit, then publish the next stable Aether release tag.

## Background

- `origin/master` is already at `78ea533516e5d5525c119109b6053f17ba056fec`; the push was a no-op.
- The latest exact-commit Nightly Release run failed because `frontend/src/features/providers/components/provider-tabs/ModelCapabilityDialog.vue:818` mutates the `model` prop.
- The latest Rust CI run failed in `downstream_credentials_and_handshake_headers_do_not_reach_upstream` because parallel local PostgreSQL startup lost a port race and timed out after `127.0.0.1` was already in use.
- Origin's latest stable tag is `v0.7.49`; the requested next stable tag is `v0.7.50`.

## Requirements

- Fix the Vue prop mutation through the existing component API/state flow without weakening the lint rule or changing unrelated behavior.
- Make the local PostgreSQL test fixture robust against the observed ephemeral-port race while preserving isolated per-test databases and cleanup.
- Push the resulting commit to `origin/master`.
- Wait for the GitHub CI workflows required by the repository's release path to finish successfully for the exact pushed commit.
- Only after CI is green, create and push the annotated stable tag `v0.7.50` on that exact commit.

## Acceptance Criteria

- [x] Frontend lint and type/build checks pass for the changed component; full existing suite passed 266 files / 2231 tests. Actual dialog browser smoke saved a changed reference with a frozen model prop, preserved unrelated config, and emitted `saved` once.
- [x] Real local PostgreSQL smoke passed all four live fixture regressions, including forced bind collision and fixed-endpoint restart; full `responses_websocket_e2e` passed 13 tests. `cargo fmt --all --check` and targeted all-target Clippy with `-D warnings` passed.
- [x] GitHub Rust CI run [40604103970](https://github.com/ZipperCode/Aether/actions/runs/40604103970) completed successfully: all 19 jobs passed for `9d5a5b2c25f43d0ded1eab6fbbbdb5870d19cc57`.
- [x] Annotated remote tag `v0.7.50` was created only after that CI result and resolves to `9d5a5b2c25f43d0ded1eab6fbbbdb5870d19cc57`; tag-push Release Aether run [40605507824](https://github.com/ZipperCode/Aether/actions/runs/40605507824) started.

## Out of Scope

- No release of `tunnel-v*`, nightly tags, GitHub Pages, or unrelated workflow changes.
- No broad frontend lint cleanup beyond the blocking prop mutation.
- No changes to application behavior unrelated to test infrastructure.

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight task remains PRD-only; implementation is limited to two known blocking paths.
