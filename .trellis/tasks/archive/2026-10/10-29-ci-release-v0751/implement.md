# Implementation

1. Reuse CI run 40800632381 failure output and inspect existing package/root-seam conventions.
2. Repair three independent slices: transport test lint; Gateway dependency seams; WebSocket quota failover fixture/runtime cause.
3. After all handoffs, format only changed Rust files, run affected transport Clippy/test, Gateway integration targets and real WebSocket end-to-end scenarios; run full CI-equivalent checks where applicable.
4. Record verified scope, commit scoped repairs and task artifacts, push master. Wait for exact pushed SHA Rust CI aggregate success; diagnose concrete failures without blind reruns.
5. Freeze the verified repair commit for exact-SHA CI and tagging. Archive and journal only after tag publication; bookkeeping must never move the tag target to an unverified commit. If a docs-only HEAD needs a run, dispatch `rust-ci.yml` at `master` explicitly and verify its exact SHA.
6. Check remote next-version absence, create/push annotated v0.7.51 at successful SHA, observe release run and report actual publication state.

## Known proof

- GitHub Rust CI 40800632381 at 99362bb83: Frontend, Gateway Clippy, Data/adapter tests, Workspace Rest tests, Format and shell security fixtures passed; Workspace Rest Clippy, Integration Scenarios and Gateway integration architecture target failed.
- No new tag was created while CI failed.

## Applied repair and local evidence

- Planner format/transport imports now use the existing permitted root seams; architecture guards are unchanged.
- Tool-history assertions iterate the actual lookup parts with an exact three-call count. The obsolete Compact bypass test row is removed; current Compact coverage remains.
- Both synthetic Codex OAuth WebSocket accounts now have their own fetched-model prerequisite. No production quota/discovery logic or failover assertions changed.
- Gateway Clippy passed. Full Gateway lib/bin nextest: 5,766 passed, 3 skipped.
- Additional Gateway integration targets: 210 passed, including all dependency and retired-format guards.
- All integration scenario bins/tests: 31 passed, including all 13 Responses WebSocket and both tunnel E2E cases.
- Provider transport nextest: 606 passed. Final Workspace Rest Clippy `--all-targets` and full Rust formatting passed after all source adjustments and smoke cleanup.
- Independent loopback runtime: a standalone gateway plus disposable PostgreSQL/mock WS upstream was driven by an external stdlib RFC 6455 client. Three client turns completed; account one had one send/one 429, account two had three sends/three completions; continuation reused its socket and a fresh client socket excluded account one. Four settled audit rows recorded one 429 and three billed 200s. Service exited 0 and all throwaway sources/config/results were removed.
- Repair commit `a5d9fb546a93c90cd1d1115b14595cf0874d530d` was pushed to `origin/master`. Rust CI run `40805947219` completed successfully: all 19 jobs, including frontend, real Postgres smoke, architecture/integration and aggregate `check`, succeeded.
- Only after that success, annotated `v0.7.51` was created and pushed. Remote tag object `46f6805cdee52ece64b3d72d6d27bb4fbcab42d2` resolves to the exact green SHA above.
- Automatic tag-push `Release Aether` run `40807046371` succeeded on that same SHA: all eight jobs passed, including Linux amd64/arm64 builds, multi-arch GHCR publication/provenance and GitHub Release assets.
- Stable non-draft `v0.7.51` was published at `2026-10-29T02:52:56Z` with both Linux tarballs, VSCodex VSIX, Sigstore provenance, installer and SHA256SUMS (six assets). Release: https://github.com/ZipperCode/Aether/releases/tag/v0.7.51 . No production deployment occurred.
