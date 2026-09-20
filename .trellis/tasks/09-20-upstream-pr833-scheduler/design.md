# Integration design

- Use isolated checkout C:/Users/Zipper/.codex/worktrees/sync-upstream-pr833/Aether, branch codex/sync-upstream-pr833-20260920. Original master stays at its initial HEAD until validation completes.
- Merge upstream normally and resolve conflicts semantically, retaining required upstream additions and fork contracts. Root owns Git index/commits and integration; disjoint implementers own formats/Antigravity, provider/OAuth and frontend files. Workers do not stage, commit or run workspace-wide formatters.
- Apply PR #833 to the coherent upstream merge tree before committing; record its exact source SHA in the final merge commit. This avoids creating an unvalidated intermediate commit and permits one combined verification pass. Skip status is observability, not scheduling policy. Reuse candidate repositories and existing authorization to complete ordinary-user projection/filtering.
- Investigate catalog/score paging, hard filters, strong key admission and failover. Preserve legitimate quota/auth/manual blocks and infrastructure errors; do not weaken safeguards to produce candidates.
- Retain source history and exact PR provenance. Fast-forward original master only when baseline ancestry and worktree state are safe. No new dependency or database migration is planned. Production confirmation remains separate.
