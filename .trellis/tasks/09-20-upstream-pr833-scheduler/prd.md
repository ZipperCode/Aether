# Upstream integration and skipped-candidate diagnostics

## Goal
Integrate upstream main and PR #833 into local master, preserving existing fork fixes. Investigate the reported symptom: eligible provider keys are skipped and requests report no candidates.

## Requirements
- R1: Merge upstream ba7c9f8b270cce63b0515299076b30129d7d64b4 into baseline e34c05e8970d46444c3e1480ef94384521fb1fad. Preserve local features, manual quota blocks, quota refresh, Gemini signature recovery, complete audit bodies, SSE framing/prefetch fixes and disabled GitHub Pages.
- R2: Integrate PR #833 at 0486435f16b7db31add34133aa819e8477c81ea0: skip indicators, reason descriptions, filters and desktop/mobile timeline. Complete its documented ordinary-user filtering gap without exposing other users' records or administrator-only details.
- R3: Trace eligible-key/no-candidate behavior, especially stale inactive Pool scores and candidate fallback corrected by upstream #824. Fix only demonstrated remaining defects, preserving eligibility, quota, auth and strong reads.
- R4: Validate affected packages and frontend paths, independently review the integration, commit in Chinese and merge back into original master without discarding its three unpublished commits.

## Acceptance
- Both upstream feature sets are integrated and conflict resolutions compile while retaining fork behavior.
- A regression proves eligible active keys remain discoverable despite stale inactive scores; fixed-order behavior remains deterministic.
- Admin and ordinary-user skip indicators/filtering work across desktop and mobile, with user isolation preserved.
- Recent SSE prefetch, Gemini recovery and complete-body regressions remain present and relevant focused checks pass.
- Main worktree ends clean with local commits. No push, release, deployment, server mutation or unrelated open PR import.

## Evidence and limits
The baseline is clean and ahead of origin/master by three commits. Preflight identifies provider, formats, usage and frontend conflicts. No production request ID is supplied; local regression evidence must not be presented as a proven production diagnosis.
