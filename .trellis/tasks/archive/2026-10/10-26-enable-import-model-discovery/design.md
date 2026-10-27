# Technical Design

## Scope

Change only import-created credentials:

- Provider API Key batch import defaults `auto_fetch_models` to `true`, while an explicit `false` from the existing import form remains authoritative.
- OAuth refresh-token/single import and OAuth batch import (including the Kiro batch path) create new OAuth keys with `auto_fetch_models = true`.
- Existing keys updated/replaced by OAuth duplicate handling keep their existing model-discovery setting.
- Normal OAuth authorization/device flows and manual key creation keep their current defaults.

## Data Flow

1. `ProviderKeyBatchImportDialog.vue:createDefaultSettings` initializes the import form with `auto_fetch_models: true`.
2. `batchImportPoolKeys` sends the setting through `PoolKeySettingsPatch` to `AdminAppState::build_admin_pool_batch_import_response`.
3. `aether-admin::build_admin_pool_batch_import_key_record` initializes batch-import records with discovery enabled, then applies the optional settings object so explicit `false` still wins.
4. OAuth single import and OAuth batch import call the shared OAuth provisioning helper with an import-only flag.
5. `create_provider_oauth_catalog_key` writes the flag to the new `StoredProviderCatalogKey`; duplicate update paths remain unchanged.
6. Existing model-fetch runtime already selects active keys with `auto_fetch_models`, so no runtime or schema change is needed.

## API / Compatibility

- No database migration or payload field addition; `auto_fetch_models` already exists in the API and persistence contract.
- The shared OAuth creation function gains an explicit import-mode argument (or equivalent named internal seam) so normal OAuth flows do not inherit the import default accidentally.
- Existing callers are migrated together; no compatibility shim or deprecated overload remains.

## Testing

- Frontend behavior test: default batch import payload contains `auto_fetch_models: true`; toggling the existing switch off sends `false`.
- `aether-admin` unit test: batch-import record defaults to enabled when settings omit the field and respects explicit `false`.
- Gateway OAuth import tests: newly created single and batch OAuth keys persist `auto_fetch_models = true`; duplicate replacement tests continue to validate existing-key behavior.
- Run targeted frontend Vitest and gateway/admin tests plus type-check/build checks appropriate to changed layers.
