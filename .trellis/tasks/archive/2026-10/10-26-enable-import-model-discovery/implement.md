# Implementation Plan

1. Update the batch Key import form default and its behavior test.
2. Update the pure batch-record builder so omitted import settings enable model discovery while an explicit setting still overrides it; extend unit coverage for omitted and explicit `false` cases.
3. Add an explicit import-mode parameter to the shared OAuth new-key provisioning seam. Pass it only from refresh-token import and OAuth batch import creation paths (including Kiro); leave ordinary OAuth authorization/device/cookie provisioning unchanged.
4. Extend gateway OAuth import coverage to assert `auto_fetch_models` on newly created records and preserve duplicate replacement behavior.
5. Run formatting/type checks and targeted tests; perform a real import-path smoke run if the repository test harness exposes the needed in-memory gateway setup.
6. Review diff, update the PRD checkboxes/spec only if a durable contract changed, then commit the isolated change with a Chinese message.

## Validation Commands

- `cd frontend && npx vitest run src/features/providers/components/__tests__/provider-key-batch-import-behavior.spec.ts`
- `cd frontend && npm run type-check`
- `cargo test -p aether-admin validates_and_applies_auto_fetch_models_settings batch_import_key_record_persists_auto_fetch_models_settings`
- `cargo test -p aether-gateway gateway_imports_admin_provider_oauth_refresh_token_locally_with_trusted_admin_principal gateway_batch_imports_admin_provider_oauth_locally_with_trusted_admin_principal`
- `cargo fmt --all --check`
