# Responses / PoolGroup repair — implementation and evidence

## Implemented

- Responses stream builder returns typed failures for missing headers/body, identity, auth pair and URL. Candidate source persists the corresponding safe per-key diagnostic through the existing direct skip seam; ordinary pool materialization persistence policy is unchanged.
- Payload construction borrows the candidate attempt; the success path does not clone the full candidate for hypothetical errors. Control-plan and WebSocket callers migrated without changing their fallback contract.
- Runtime-quarantined candidates record `endpoint_capability_quarantined` once at the dropping gate. Cache scope/TTL and pool yielded-key guards remain unchanged.
- Actual Responses execution exhaustion sets `execution_runtime_candidates_exhausted` while preserving previous skip counts and routing metadata. `NoPath` keeps planner diagnostics.
- Shared stream transport failures retain bounded sanitized detail using the existing transport helper; secrets and upstream URL values are not added to diagnostics.
- Backend diagnostic category registration and frontend labels updated. No binding SQL/schema, discovery synchronization, provider selection or production configuration changes.

## Verification observed (2026-10-08)

### Real binary HTTP before/after

Disposable Docker PostgreSQL plus a loopback HTTP upstream; provisioned through admin APIs. No production connection or data copied.

- Baseline: 28 assertions, 25 passed / 3 failed. Pool API-key authentication with no key value reproduced HTTP 503 `no_local_stream_plans`, zero upstream calls and no concrete skipped-key diagnostic.
- Fixed binary: **28/28 passed**. Same missing-auth case returns HTTP 503 `all_candidates_skipped`; trace contains `transport_auth_unavailable` and `failure_diagnostic` with `kind=transport_auth`, `source=openai_responses_stream_plan_builder`, `path=$.auth`.
- Both builds passed healthy Responses traffic, exact Responses endpoint isolation with Compact/Search bindings present, upstream 503 retry/failover to the second pool key, and invalid Header-rule pre-send rejection.
- Fixed `cargo build -p aether-gateway --bin aether-gateway --locked` passed. Gateway and disposable DB were stopped after smoke; throwaway script removed after evidence capture.

### Automated checks

- Data-contracts + model-fetch library suites: **338 passed** (242 + 96).
- Frontend `npm run type-check`: passed; targeted skipReason suite: **6 passed**.
- Gateway full library attempt: **5462 passed, 6 failed, 3 ignored**. Two new fixture failures were corrected (explicit pool retry count and withholding response headers until semantic SSE content); four unrelated environment failures remained below.
- `cargo test -p aether-data --lib automatic_reconcile --locked`: **2 passed**, preserving manual bindings and scoped discovered-binding replacement in the in-memory repository; not a claim of live PostgreSQL binding validation.
- After excluding only those four known environment blockers: **5463 passed, 1 failed, 3 ignored, 4 filtered**. Remaining failure was the new capacity fixture expecting 503; existing pure-capacity contract is 429, and the fixture was corrected without changing runtime policy.
- Final pool regression run: **3/3 passed**, covering real upstream 401 failover, all-key execution exhaustion, and concurrent pool-key saturation returning 429 without attempted candidates.
- `cargo fmt --all --check` and `git diff --check`: passed.
- Final read-only cross-review: no production correctness findings. Constructor/constant-echo tests flagged during review were removed; builder rejection and real-path tests retained.

### Full-suite environment blockers (not rerun to confirm)

1. `control::auth::resolution::tests::due_antigravity_bearer_refresh_observes_cross_node_allowlist_revocation`: local PostgreSQL executable missing.
2. `control::auth::resolution::tests::strong_system_config_read_bypasses_app_and_data_caches`: same prerequisite.
3. `tests::video::xai::xai_video_native_and_compatibility_http_lifecycle_postgres`: same prerequisite.
4. `execution_runtime::server::tests::execution_runtime_unix_socket_rebinds_stale_current_user_socket`: parent-directory permission denied.

No unrelated test/runtime code was changed to mask these failures. No automatic commit or task archival while full verification remains blocked.

## Root-cause boundary and retrospective

Cross-layer failure propagation was incomplete: a logical PoolGroup count does not imply a concrete key reached execution; an unclassified `None` or quarantine drop could lose the rejection. The first draft incorrectly inferred a request-body failure from every plan miss and assumed bulk pool suppression also covered direct failure writes. Typed errors, actual persistence-path tracing, and a real binary before/after scenario corrected both assumptions.

154's historical trace establishes a pre-execution failure but does not identify its exact branch. Local missing-auth reproduction is proof of the fixed bug class, not proof that production OAuth credentials were missing. No production deployment or write occurred; production availability is not claimed restored. Binding functionality remains necessary and unchanged.
