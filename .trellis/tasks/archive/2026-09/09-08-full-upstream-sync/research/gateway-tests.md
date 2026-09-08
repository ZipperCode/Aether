# Gateway tests-only merge receipt

## Frozen ownership

- Assigned after the provider group returned to integrator. Only the 16 conflict paths captured at handoff and this file were edited. Production and former provider/OAuth/HTTP ownership were not touched.
- Merge stages: ours `b0ad8ff7f7bdb1888abac8c06f47f61e76761431`, theirs `c7e403b410139d12a6189dda9c2bdf0c7c80782e`, base `7892aa94853461c1e634f7a5babbb1280128720f`.

```text
apps/aether-gateway/src/scheduler/candidate/tests/required_capability.rs
apps/aether-gateway/src/scheduler/candidate/tests/selection.rs
apps/aether-gateway/src/tests/ai_execute/finalize_local_cli/cross_format.rs
apps/aether-gateway/src/tests/ai_execute/stream/decision.rs
apps/aether-gateway/src/tests/ai_execute/sync/chat/failover.rs
apps/aether-gateway/src/tests/ai_execute/sync/search.rs
apps/aether-gateway/src/tests/control/admin/endpoints/keys.rs
apps/aether-gateway/src/tests/control/admin/endpoints/quota.rs
apps/aether-gateway/src/tests/control/admin/models/global.rs
apps/aether-gateway/src/tests/control/admin/provider_query.rs
apps/aether-gateway/src/tests/control/admin/providers.rs
apps/aether-gateway/src/tests/control/admin/system.rs
apps/aether-gateway/src/tests/control/admin/system_import.rs
apps/aether-gateway/src/tests/usage/local.rs
apps/aether-gateway/src/tests/video/gemini_sync_task.rs
apps/aether-gateway/src/tests/video/openai_sync_create.rs
```

## Semantic resolutions

- Scheduler keeps named real time and load-balance seed contexts. Affected direct calls pass mandatory SchedulerOrderingConfig; helpers read the actual default routing policy. Separate-time/seed regression remains. The skip-reason helper's final None is request_operation, not obsolete optional ordering.
- Cross-format Responses expects the shared stable message ID. Error summaries follow upstream safe summary semantics while full captured upstream status, content type and body message remain exact, confirmed with integrator.
- Same-Key sticky retry counts and candidate traces remain in stream/chat/search/usage. Recording fixtures explicitly enable full capture where required; parameterized basic/full stream helper remains.
- Keys keeps credential-update/Pool recovery and score repository. Normal Agent Identity/read-only capability credentials use existing Provider/Key-bound helpers; legacy fixtures with writers remain. Removed unused legacy decryption imports only.
- Antigravity quota discovery keeps exact Endpoint ownership in model storage and also seeds upstream's pre-existing global model. Grouped quota requests and catalog assertions remain.
- Model routing installs the new default routing-group fixture and keeps capability quarantine. Provider-query retains auto-fetch and pinned capability tests, with bound credentials for its read-only transport repository.
- Provider payload expectations use canonical proxy URL and retained advanced configuration. System export keeps Endpoint IDs/bindings plus upstream omitted/deactivated LDAP/OAuth secrets and credential state. Config export remains 2.4; public user export 1.6; RecoveryBackup 1.5.
- System import combines all eight fork Endpoint-binding regressions and no-mutation assertions with upstream LDAP injection prevalidation. No Endpoint regression was deleted.
- Video cancellation/remix keeps full Provider/Endpoint/bound-Key catalog plus auth reader. Removed the second same-name Key-only repository that shadowed the full fixture; live Key and Provider IDs still match persisted task transport.

## Actual checks

- PASS: scoped rustfmt --edition 2021 --config skip_children=true --check over exactly these 16 files.
- PASS: conflict marker scan = 0; duplicate top-level function scan = 0.
- Three-stage top-level test-name inventory verified retained union. Expected upstream replacements only: empty legacy Codex cache -> stale cache; removed provider-priority setting -> rejection; overwriting exported user-key totals -> legacy import cannot mutate live keys by exported hash; base-record implementation -> parameterized basic/full implementation. Fork capability test remains despite its absence upstream.
- No MySQL/SQLite cases, retired accept_invalid_certs fields or Header-redaction assertions remained in the fixed set.
- Cargo compile/tests NOT RUN while shared production contracts/manifests change, per assignment. Unified integration must compile/run relevant existing tests; no runtime success claimed.
- No outstanding production integration request. Integrator confirmed safe error-summary versus exact upstream-response distinction.
- No process, service, database, commit, push, deployment, task-state or main-checkout mutation.
