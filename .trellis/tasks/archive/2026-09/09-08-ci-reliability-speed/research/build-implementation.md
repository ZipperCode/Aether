# Build watch implementation receipt

## Scope and change

- Owner: build implementer; only `apps/aether-gateway/build.rs`, `tests/gateway_build_watch_test.py`, and this receipt were written.
- Replaced the nonexistent linked-worktree `../../.git/HEAD` assumption with Git-native `rev-parse --git-path` resolution for HEAD and the current symbolic branch. Existing loose refs are watched as exact files, including the linked worktree's common Git directory.
- Per Root's acceptance clarification, a packed-only branch temporarily watches its existing `packed-refs` and nearest existing parent directory. Creating the loose branch ref causes one rerun, after which only the exact ref file is watched. No permanent refs-directory or recursive `.git` watch was added.
- Source archives keep the explicit `build.rs` watch and the four existing version/build-type environment watches.
- Version precedence is unchanged: `AETHER_BUILD_VERSION` → `AETHER_VERSION` → `GITHUB_REF_NAME` → gateway `git describe` → Cargo package version. Leading `v` normalization, whitespace trimming, tunnel-tag exclusion, `--dirty`, and build-type default `source` are retained.

## Before/after evidence

- Before editing `build.rs`, `python tests/gateway_build_watch_test.py` executed the actual old script in dependency-free temporary Cargo packages. Normal checkout's second check was Fresh (0 script executions); linked worktree's second unchanged check ran the script again (1 execution, expected 0), and Cargo reported `the file ../../.git/HEAD is missing`. The regression exited 1 as intended. The fixture temp directory was cleaned on that failure.
- After the fix, the same command passed 23 Cargo checks in 17.86 seconds on native Windows with Rust 1.95.0. Eight unchanged checks observed 0 script executions and `Fresh aether-build-watch-fixture`; the remaining 15 input-changing/initial checks observed exactly 1 execution each. Tests read Cargo's saved build output to verify the actual version/build type, rather than just matching source strings.
- Coverage: normal checkout and linked-worktree unchanged inputs and branch advancement; packed-refs then a new loose ref; detached HEAD changes and advancement; no-Git source archive stability; all explicit version precedence levels, version changes, tunnel-tag exclusion, and build-type changes. The final fixture also asserts packed-branch advancement returns to file-only watches without `packed-refs`.
- Final rerun after adding that exact-watch assertion: all 23 checks passed in 18.47 seconds, exit 0; temporary-directory inventory was empty afterward.
- The temporary package has no dependencies and uses `cargo check --offline -vv`; this did not compile the real gateway, workspace, frontend, or database tests. Rust found its installed MSVC linker automatically; no VS setup scripts, installs, or global environment/configuration changes were needed.

## Checks and limits

- `rustfmt --check apps/aether-gateway/build.rs`: exit 0.
- Existing `admin_system.rs` build-script source contract patterns: all preserved, checked directly without compiling the gateway architecture suite.
- `git diff --check`: exit 0 (only normal LF/CRLF conversion warnings).
- No main-repository Git state mutations, staging, commits, push, CI dispatch, remote calls, service processes, or persistent temporary build directories.
- No end-to-end GitHub CI timing or full-gateway compile claim. Dirty-tree/index/arbitrary tag-change invalidation remains outside this task; existing description semantics are preserved without expanding their watch scope.
