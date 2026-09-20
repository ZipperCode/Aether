# Implementation and verification

1. [x] Verify clean baseline, remote heads and merge conflict inventory.
2. [x] Record reviewed artifacts and real contexts; user authorized integration and clarified the symptom.
3. [x] Start task and merge upstream without automatic commit.
4. [x] Resolve disjoint conflicts with Trellis implementers; root handles usage persistence/integration and reviews auto-merges intersecting recent fork fixes.
5. [x] Independently trace candidate selection and retain/add regressions for demonstrated availability defects.
6. [x] Integrate PR #833 into the coherent merge tree, finish ordinary-user filtering/authorization; exact PR provenance is recorded for the eventual verified merge commit.
7. [x] Run focused Rust tests/checks for formats, transport, pool/scheduler, usage and gateway integration; frontend type-check, non-mutating scoped ESLint and relevant Vitest. Reuse Linux builder/cache with stable flags.
8. [x] Dispatch independent Trellis check, repair findings and rerun only affected checks.
9. [x] Update specs/receipts and commit as 377ca367f; original master fast-forwarded cleanly with both parent ancestries retained. Complete the task archive and session journal in the following bookkeeping commits.

## Validation boundaries
Do not claim full CI, DB smoke or live diagnosis without execution. Do not restart original service containers. Preserve quota/auth denial cases alongside availability regressions. Do not run npm run lint because it mutates files.
