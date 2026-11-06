# Execution

1. Confirm clean index/worktree, fetch upstream, inspect merge preview and contracts.
2. Activate task; merge --no-commit --no-ff upstream/main. Resolve assigned conflict domains, preserving local invariants and new upstream behavior. Check auto-merges and relocated callers.
3. Run frontend typecheck/build, frontend behavior suite (shared i18n/path refactor), Rust focused referral/wallet/module/provider-query plus affected local regressions, fmt and diff checks. Observe local UI; no real providers or financial writes. Fix only merge regressions.
4. Verify ancestry and no unresolved files. Commit normal merge and push current origin/master; report any hook/push failure without rebase or force.
5. Read actual PR diffs for #893/#897/#898/#899, compare to merged symbols/tests, and report coverage, porting risks and recommended acceptance. Do not cherry-pick them.
