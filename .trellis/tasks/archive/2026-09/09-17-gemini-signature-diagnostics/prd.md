# Gemini signature rejection and diagnostic retention
## Goal
Restore Gemini conversations rejected because historical thought signatures cannot be decoded, and retain complete diagnostic bodies for large requests.
## Authorization
The user approved the preceding three-step repair recommendation with "进行修复". Local implementation, relevant verification and commits are authorized. No push, release, production deployment, server configuration changes, client history deletion or paid generation replay.
## Deliverables
- R1: .trellis/tasks/archive/2026-09/09-17-antigravity-base64-recovery — targeted error-triggered Antigravity signature recovery.
- R2: .trellis/tasks/archive/2026-09/09-17-usage-full-body-retention — integrate upstream PR #830, commit 5842c7232ecd6cdfa95994da9afaac14f5a8deb0.
## Acceptance
- R1 rejects only the confirmed error family and preserves unrelated payloads and existing retry/accounting contracts.
- R2 retains oversized FULL bodies when bounded direct persistence is available and preserves truthful truncation otherwise.
- Relevant regressions pass; working changes remain scoped; local commits and remaining production validation are reported.
