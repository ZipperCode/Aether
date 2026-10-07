# Same-trace production evidence (read-only, 2026-10-07)

- Trace `fcc46626-c074-4346-a28d-e3927ec71831`: `/v1/responses`, `gpt-6-astra`, 1,046,175-byte body, 503 `no_local_stream_plans`, zero `request_candidates` rows for this trace, no execution-runtime log.
- Adjacent requests with same user/API key/model/session and same 1,046,175-byte body: one succeeded at 22:49:08, two failed at 22:49:02/05. Successful requests selected Codex key `06770cbe…` and do have candidate rows; failures selected no provider/key and never reached execution.
- At the failure timestamps the selected key had overlapping active requests; the pool key limit is 6. Provider pool config enables `single_account`, `pro_first`, and `priority_first`; the candidate query collapses the provider's keys into one PoolGroup candidate.
- For the failing traces, `usage_http_audits` body capture is disabled, so the exact upstream request JSON is unavailable. This is a per-trace fact only: failed traces have zero candidate rows; it does not mean candidate recording is globally disabled (successful adjacent traces recorded candidates). No production write performed.

## Corrected root-cause boundary

The same trace only establishes a **pre-execution failure**: one PoolGroup candidate, no provider/key selected, zero candidate rows, no execution-runtime log. It cannot distinguish which silent drop point fired and is not proof of the exact plan-builder `Ok(None)` branch or of an Endpoint binding fault.

Silent-drop classes supported by code walk-down (all in repair; none proven as the exact historical branch on 154):

1. Responses stream plan-builder `Ok(None)` previously returned without any candidate marking (fixed by builder owner: direct skipped row + safe failure diagnostic; the direct mark path bypasses the pool skipped-persistence filter, so the per-key row persists). Landed categorical skip reasons: `transport_auth_unavailable` (api_key with no/empty sealed value → incomplete auth pair), `provider_request_headers_missing`, `provider_request_body_missing`, `execution_plan_identity_missing`, `upstream_url_missing`; rows carry `extra_data.failure_diagnostic.source = "openai_responses_stream_plan_builder"`. All-keys-plan-build-failure terminal miss reason is `all_candidates_skipped` with per-key categorical skip counts (confirmed by builder owner, final landed state).
2. A pool-yielded key attempt silently dropped by runtime quarantine (`attempt_is_runtime_quarantined` branches in `candidate_materialization.rs`); this class can also produce `candidate_count=1, skipped=0, rows=0` after the pool yielded a key (diagnostics-only repair owned by QuarantineDiagnostic, no policy change).
3. Real attempted-candidate exhaustion being overwritten by the planner's `no_local_stream_plans` terminal reason (fixed by runtime owner: `execution_runtime_candidates_exhausted` is set only when a candidate was actually attempted; `NoPath` is never overwritten).

普通文本 Responses resolver 的 `None` 站点已经标记 skip，不重复增加 fallback；图像桥接的独立返回不在本次范围。

Production availability is not repaired by this code change and no production deployment is authorized; the read-only recheck of 154 after deployment remains a separate prerequisite.
