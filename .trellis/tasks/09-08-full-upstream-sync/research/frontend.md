# Frontend merge receipt

## Ownership and baseline

- Sole write scope: `frontend/**` and this report; no main-checkout, backend, root manifest, task-state, commit, deployment, database, or service writes.
- Worktree: `C:/Users/Zipper/AppData/Local/Temp/aether-full-upstream-20260908`.
- Fork baseline: `b0ad8ff7f7bdb1888abac8c06f47f61e76761431`; upstream: `c7e403b410139d12a6189dda9c2bdf0c7c80782e`; common ancestor: `7892aa94853461c1e634f7a5babbb1280128720f`.
- Initial frontend conflicts: 35. Conflicts were resolved per hunk and affected caller, not by wholesale ours/theirs selection.
- Applied trellis-before-dev, Ponytail, and trellis-check instructions directly; no recursive agents.

## Semantic integration and retained fork contracts

- Provider Model association remains on the default aggregate-discovery path in `BatchAssignModelsDialog.vue`: Provider/open/session fences, pending-save guard, exact case-insensitive matching, upstream model ID, de-duplicated `endpoint_ids`, and empty-discovery batch inference remain. Existing `BatchAssignModelsDialog.loading.spec.ts` passed, including default path and stale-session cases.
- Model capability testing remains wired from `provider-tabs/ModelsTab.vue` to `ModelCapabilityDialog.vue` and the existing provider-query API. `ProviderModelConfig extends ModelConfig` preserves `capability_test_reference` alongside upstream typed model config; `ModelCreate.endpoint_ids` and `ModelUpdate.endpoint_bindings` remain. `model-test-request.spec.ts` passed.
- Global routing retains `include_whitelist=false` lightweight reads and typed `mapping-preview` pagination. Exact GlobalModel ID filtering, async request generation fencing, and format-independent Key priorities remain in `RoutingPriorityPolicyEditor.vue`. Upstream `buildRoutingProviderSummaryQuery` is reused.
- Runtime quota confirmation/recovery remains in Provider Detail and both Pool table/mobile action paths, using `clearQuotaExhausted`; confirmation explicitly preserves manual disablement, OAuth, cooldown, and health. `FailoverRulesDialog.vue` retains `quota_exhaustion_patterns`.
- `mergePoolKeyQuotaSnapshots` preserves `scheduling` and `model_probe` while adopting upstream required OAuth/account defaults. Added one focused regression: `preserves runtime scheduling and model-probe state during quota refresh`.
- Exact decimal strings, generic quota cards, Nous quota fields, ambiguous Zhipu quota/model-probe evidence, and error-unavailable presentation remain. The Nous `exhausted_reason` field was verified in the backend quota producer.
- Adopted upstream Antigravity account-level `quota_group` windows and localized weekly / 5-hour names. Raw model entries remain in the quota dialog's test-model menu. Removed the obsolete two-family summary helper and its dedicated test because the upstream target explicitly supersedes that presentation.
- Adopted upstream Provider card/table toggle, drag ordering, responsive controls and navigation. Retained fork Provider batch basic/endpoint actions and custom-provider conversion confirmation. Provider save no longer resurrects upstream-retired billing/monthly quota UI.
- Retained raw full request-detail Header rendering and adopted upstream separate raw body loading, worker parsing, and virtualized body rendering. No masking/substitution was introduced.
- Adopted upstream VSCodex route/navigation/API and authentication/navigation flow; targeted API and navigation/security/login/client tests passed.
- Removed obsolete `PriorityManagementDialog.vue` after confirming no remaining callers; upstream replaces priority management with the new direct ordering/routing flows. Its fork-only delta was a comment.
- Upstream retires RoutingGroupConfig.allowed_models. Integrator confirmed this is the final backend target, so obsolete whitelist UI/functions/test were removed; per-model Provider filtering and ordering remain.

## Cross-agent contract confirmations

Confirmed with full_merge_integrator:

1. Retain exact Endpoint association, capability config/API, scheduling/model_probe snapshots, lightweight routing and mapping-preview.
2. Retire routing-group `allowed_models`, not Provider/model association and auth access controls.
3. Provider batch fields `max_retries`, `stream_first_byte_timeout`, `request_timeout`, `keep_priority_on_conversion` remain consumed in backend create/update handlers; retained all four batch actions.
4. No pending frontend/backend contract decision remains. Backend compilation/integration is owned by the integrator.

## Actual verification

- `npm ci --ignore-scripts --no-audit --no-fund`: passed, 434 packages installed only in worktree `frontend/node_modules`; no lockfile hand edits.
- `npm run type-check`: initial run exposed merge-generated duplicate declarations, Axios unknown responses, shared DTO/test typing, and upstream-retired references; repaired. Subsequent and final runs exited 0.
- Non-mutating scoped ESLint on 33 existing original-conflict files exited 0: 0 errors, 124 warnings, mostly retained Vue formatting/multiple-component warnings. Merge-created indentation in EndpointFormDialog and ProviderTableHeader was subsequently corrected. No automated lint rewrite was run.
- An initial ESLint invocation referenced nonexistent `eslint.config.ts` (ENOENT); corrected to the repository's `eslint.config.js`. A no-target invocation caused by workdir-relative Git pathspec was interrupted without modifying files or claiming a result.
- `git diff --check -- frontend`: passed. No conflict markers remain in `frontend/src`.
- Node `v24.18.0`; not identical to CI Node 22.
- 21 selected test files, **423 unique tests passed** across the following batches; no whole-frontend suite or build was run.

### Batch A: 12 files, 156 tests

```text
src/api/__tests__/global-model-routing.spec.ts
src/api/__tests__/usage-contract.spec.ts
src/api/__tests__/dashboard-body-loading.spec.ts
src/api/__tests__/vscodex.spec.ts
src/features/routing/__tests__/routingPolicy.spec.ts
src/features/routing/__tests__/providerQuery.spec.ts
src/features/providers/components/provider-tabs/__tests__/model-test-request.spec.ts
src/features/providers/utils/__tests__/batchEdit.spec.ts
src/utils/__tests__/providerKeyQuota.spec.ts
src/utils/__tests__/oauth-icons.spec.ts
src/features/usage/utils/__tests__/body-document.spec.ts
src/features/usage/utils/__tests__/body-document-engine.spec.ts
```

### Batch B: 5 files, 23 tests

```text
src/features/providers/components/__tests__/BatchAssignModelsDialog.loading.spec.ts
src/features/providers/components/__tests__/ModelMappingDialog.spec.ts
src/features/routing/components/__tests__/RoutingPriorityPolicyEditor.spec.ts
src/features/pool/utils/__tests__/poolQuotaRefresh.spec.ts
src/features/pool/utils/__tests__/poolKeyBatchSettings.spec.ts
```

The initial batch had 22 passes and one ModelMappingDialog failure. The failing file was rerun alone after fixing explicit forceRefresh propagation and empty-result manual-fetch state; its 6 tests passed. Successful sibling files were not rerun.

### Batch C: 4 files, 244 tests

```text
src/layouts/main-layout/__tests__/navigation.spec.ts
src/utils/__tests__/navigationSecurity.spec.ts
src/features/auth/utils/__tests__/loginRedirect.spec.ts
src/api/__tests__/client.spec.ts
```

Commands used `npm run test:run -- <listed paths>`. Component-based files were existing logic/state/payload regressions, not visual or browser validation.

## Initial conflict paths (35)

- `frontend/src/api/endpoints/global-models.ts`
- `frontend/src/api/endpoints/pool.ts`
- `frontend/src/api/endpoints/types/model.ts`
- `frontend/src/api/endpoints/types/statusSnapshot.ts`
- `frontend/src/api/usage.ts`
- `frontend/src/features/pool/components/PoolKeyQuotaPanel.vue`
- `frontend/src/features/pool/components/PoolSchedulingDialog.vue`
- `frontend/src/features/pool/components/__tests__/PoolKeyDisplayPanels.spec.ts`
- `frontend/src/features/pool/components/__tests__/PoolSchedulingDialog.cache-affinity.spec.ts`
- `frontend/src/features/pool/utils/__tests__/poolKeyBatchSettings.spec.ts`
- `frontend/src/features/pool/utils/poolQuotaRefresh.ts`
- `frontend/src/features/providers/components/AntigravityQuotaDialog.vue`
- `frontend/src/features/providers/components/EndpointFormDialog.vue`
- `frontend/src/features/providers/components/KeyFormDialog.vue`
- `frontend/src/features/providers/components/ModelMappingDialog.vue`
- `frontend/src/features/providers/components/OAuthKeyEditDialog.vue`
- `frontend/src/features/providers/components/PriorityManagementDialog.vue`
- `frontend/src/features/providers/components/ProviderBatchActionDialog.vue`
- `frontend/src/features/providers/components/ProviderDetailDrawer.vue`
- `frontend/src/features/providers/components/ProviderFormDialog.vue`
- `frontend/src/features/providers/components/ProviderMobileCard.vue`
- `frontend/src/features/providers/components/ProviderQuotaProgressRow.vue`
- `frontend/src/features/providers/components/ProviderTableHeader.vue`
- `frontend/src/features/providers/components/__tests__/AntigravityQuotaDialog.spec.ts`
- `frontend/src/features/providers/utils/antigravityQuota.ts`
- `frontend/src/features/routing/__tests__/routingPolicy.spec.ts`
- `frontend/src/features/routing/components/RoutingPriorityPolicyEditor.vue`
- `frontend/src/features/routing/utils/routingPolicy.ts`
- `frontend/src/i18n/messages.ts`
- `frontend/src/utils/__tests__/oauth-icons.spec.ts`
- `frontend/src/utils/oauth-icons.ts`
- `frontend/src/views/admin/PoolManagement.vue`
- `frontend/src/views/admin/ProviderManagement.vue`
- `frontend/src/views/admin/RoutingProfiles.vue`
- `frontend/src/views/admin/__tests__/RoutingProfiles.allowed-models.spec.ts`

## Manual resolution / repair paths (45)

Includes semantic deletions and original conflicts resolved without a remaining final-content delta. Automatically merged upstream files are not listed here.

- `frontend/src/api/endpoints/global-models.ts`
- `frontend/src/api/endpoints/keys.ts`
- `frontend/src/api/endpoints/pool.ts`
- `frontend/src/api/endpoints/types/model.ts`
- `frontend/src/api/endpoints/types/statusSnapshot.ts`
- `frontend/src/api/usage.ts`
- `frontend/src/components/common/__tests__/AlertDialog.spec.ts`
- `frontend/src/features/models/components/ModelMappingsTab.vue`
- `frontend/src/features/pool/components/__tests__/PoolKeyDisplayPanels.spec.ts`
- `frontend/src/features/pool/components/__tests__/PoolSchedulingDialog.cache-affinity.spec.ts`
- `frontend/src/features/pool/components/PoolAccountBatchDialog.vue`
- `frontend/src/features/pool/components/PoolKeyQuotaPanel.vue`
- `frontend/src/features/pool/components/PoolSchedulingDialog.vue`
- `frontend/src/features/pool/utils/__tests__/poolKeyBatchSettings.spec.ts`
- `frontend/src/features/pool/utils/__tests__/poolQuotaRefresh.spec.ts`
- `frontend/src/features/pool/utils/poolQuotaRefresh.ts`
- `frontend/src/features/providers/components/__tests__/AntigravityQuotaDialog.spec.ts`
- `frontend/src/features/providers/components/__tests__/ModelMappingDialog.spec.ts`
- `frontend/src/features/providers/components/AntigravityQuotaDialog.vue`
- `frontend/src/features/providers/components/EndpointFormDialog.vue`
- `frontend/src/features/providers/components/KeyFormDialog.vue`
- `frontend/src/features/providers/components/ModelMappingDialog.vue`
- `frontend/src/features/providers/components/OAuthKeyEditDialog.vue`
- `frontend/src/features/providers/components/PriorityManagementDialog.vue`
- `frontend/src/features/providers/components/ProviderBatchActionDialog.vue`
- `frontend/src/features/providers/components/ProviderDetailDrawer.vue`
- `frontend/src/features/providers/components/ProviderFormDialog.vue`
- `frontend/src/features/providers/components/ProviderMobileCard.vue`
- `frontend/src/features/providers/components/ProviderQuotaProgressRow.vue`
- `frontend/src/features/providers/components/ProviderTableHeader.vue`
- `frontend/src/features/providers/utils/__tests__/antigravityQuotaSummary.spec.ts`
- `frontend/src/features/providers/utils/__tests__/batchEdit.spec.ts`
- `frontend/src/features/providers/utils/antigravityQuota.ts`
- `frontend/src/features/routing/__tests__/routingPolicy.spec.ts`
- `frontend/src/features/routing/components/__tests__/RoutingPriorityPolicyEditor.spec.ts`
- `frontend/src/features/routing/components/RoutingPriorityPolicyEditor.vue`
- `frontend/src/features/routing/utils/routingPolicy.ts`
- `frontend/src/i18n/messages.ts`
- `frontend/src/utils/__tests__/oauth-icons.spec.ts`
- `frontend/src/utils/__tests__/providerKeyQuota.spec.ts`
- `frontend/src/utils/oauth-icons.ts`
- `frontend/src/views/admin/__tests__/RoutingProfiles.allowed-models.spec.ts`
- `frontend/src/views/admin/PoolManagement.vue`
- `frontend/src/views/admin/ProviderManagement.vue`
- `frontend/src/views/admin/RoutingProfiles.vue`

## Handoff and limits

- Frontend is ready for shared-root integration. Preserve MERGE_HEAD; no commit was made by this agent.
- No browser, screenshot, visual comparison, live upstream provider, existing database, service, or deployment validation was performed.
- Worktree-only ignored dependencies remain for integrator use and will disappear with the task worktree cleanup; all invoked verification processes exited or were explicitly interrupted.
- Existing Vue style warnings were not broadened into unrelated UI formatting work.
