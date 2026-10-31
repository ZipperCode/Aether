# Implementation

1. Parent: pin reference algorithm/license; own provider-pool quota policy, pool behavioral tests, gateway HTTP quota harness, transport client-profile check and final docs/integration.
2. Admin slice: remove binding mutation/validation, safe projections, backup/config import/export fields and obsolete binding regressions. Preserve group IDs, disabled/group display projection and unrelated key behavior. Also fix grouped reset-only parsing using existing shared transport helper without changing exported signatures.
3. Frontend slice: remove binding editor, API/types, subscribers, helpers and obsolete specs; preserve account editing and quota display, including disabled rendering. No independent inference logic in Vue.
4. All workers skip builds, lint, format and tests mid-flight. Parent runs checks once slices integrate, fixes concrete failures, and runs loopback HTTP + browser smoke.
5. Update existing quota contract with source links and automatic semantics, remove throwaway smoke scaffolds, then commit and push only task-owned paths. Record session/archive task.

## Commands

- cargo test --locked -p aether-provider-pool
- cargo test --locked -p aether-admin
- RUST_MIN_STACK=16777216 cargo test --locked -p aether-gateway gateway_antigravity_ -- --nocapture
- RUST_MIN_STACK=16777216 cargo test --locked -p aether-gateway orchestration::antigravity_model_quota
- RUST_MIN_STACK=16777216 cargo test --locked -p aether-gateway --test architecture_guard
- frontend: npm run type-check; targeted current quota/editor Vitest files; npm run build; read-only eslint on changed files
- cargo fmt --all --check after scoped formatting

## Risk and rollback

Read stored group shape without administrative binding; disabled/zero-omission and reset semantics must survive the cutover. Do not use broad unknown-family fallback to poison other models. Removal is limited to last feature's binding contract, not general metadata CAS infrastructure. Rollback is a normal revert of this isolated commit; no DB migration or production action.

## Verification completed

- Shared transport fraction parser: 4 passed; provider-pool: 144 passed; admin: 349 passed.
- Gateway Antigravity regressions: 27 passed; model-quota orchestration: 12 passed; disabled-group projection: 1 passed; architecture guard: 209 passed.
- Model discovery regressions: 4 passed; gateway system import regressions: 43 passed.
- `cargo fmt --all --check` and provider-pool/transport `cargo clippy --locked --lib -- -D warnings` passed.
- Frontend type-check, six targeted suites, production build and read-only ESLint passed. Removed two incidental wording/forwarding tests after browser acceptance; retained behavioral coverage.
- Real loopback HTTP refresh and native Gemini sync/SSE covered ordinary and pool/sticky selection with no binding writes: dedicated Argon isolation, shared-only accounts, no cross-pool borrowing, Flash availability, both exhausted-window directions, partial reset, reset-only, disabled windows, all-blocked 503 with unchanged generation counts, positive refresh and elapsed-reset recovery.
- Chromium smoke exercised actual OAuth account editing, unsaved-close confirmation and ordinary save payload; quota dialog showed disabled quota without a bar and automatic group explanation. Desktop and 390px layouts had no horizontal overflow or browser errors. Temporary page, preview service and managed browser tab were removed/closed.
- Production deployment, real account requests/token refresh and storage migrations were not performed.
