# Review and release v0.7.42

## Goal
Review integration from 47681f9ec through the merged upstream and seven priority PRs, fix confirmed defects, commit and push to origin/master, then publish the next version tag only after GitHub CI passes for that exact commit.

## Requirements
- Preserve existing local protocol, authorization, scheduling, quota, and frontend contracts.
- Confirm review findings against executable behavior; fix only task-related defects.
- Keep user changes isolated; do not rewrite history or force push.
- Use Docker PostgreSQL for tests requiring PostgreSQL; do not install host PostgreSQL.
- Observe GitHub CI for the exact pushed SHA. Never tag a failing or unverified commit.
- Latest observed release is v0.7.41; planned tag v0.7.42, subject to remote collision check.

## Acceptance
- Review findings addressed or evidence-backed dismissed.
- Relevant verification and runtime smoke pass.
- origin/master contains reviewed commits.
- Exact-SHA GitHub CI succeeds before tag publication.
- New tag published following existing release workflow; report observed release status.

## Review findings and fixes
- Preserve custom system instructions after recognized Claude identity prefixes, including inline continuations and multiple blocks. Genuine CLI requests remain unchanged.
- Reconcile expired Codex/Claude metadata-backfilled quota exhaustion with normalized windows while preserving future exhaustion and flags-only refusal.
- Start read-only profile cache appliers on frontdoor-only processes independently of singleton background network workers; preserve pins and rollback protection.
- Keep a pending search debounce alive across local pagination-only changes.

## Verification receipts
- Claude mimicry module: 11 passed.
- Quota snapshot family: 34 passed.
- Frontdoor startup cache convergence: 1 passed.
- Frontend type-check and full suite: 238 files, 1,891 passed.
- Full pre-push backend gate and remote CI/release receipts pending.
