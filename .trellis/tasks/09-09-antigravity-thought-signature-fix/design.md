# Approved reactive signature recovery

## Evidence and precedent
Use sub2api PR946 (https://github.com/Wei-Shaw/sub2api/pull/946, merged 826090e0): original first send, signature HTTP400, same-account compatible send once. Do not copy its all-JSON recursive replacement. CLIProxyAPI PR3470 (https://github.com/router-for-me/CLIProxyAPI/pull/3470#issuecomment-4476982750) reports original400 -> sentinel200 SSE; this is upstream evidence, not Aether live verification.
Research established five history signatures came from apiyi and persisted unchanged; no account-binding/decryption claim. cbf1346fe IS in v0.7.33. User has approved reactive semantic degradation, so older research gates do not block implementation.

## Pure helper
Provider transport Antigravity owns exact error recognition and cloned body rewrite. Require explicit Antigravity adaptation identity plus Gemini format, not hostname/model alone. Match actual HTTP400 and JSON error.status INVALID_ARGUMENT with Corrupted thought signature ignoring surrounding whitespace/trailing periods, not arbitrary signature text or 200 SSE events.
Traverse only actual contents or private request.contents, role=model, direct part fields thoughtSignature/thought_signature; replace existing values with skip_thought_signature_validator. Preserve missing fields, all unrelated structures/order, input immutability and existing schema normalization. Unchanged/already-sentinel body has no retry.

## Execution wiring
- stream/execution.rs::execute_in_process_stream_with_oauth_retry (~1239) shares direct/tunnel (~1213) and error prefetch (~1543). Recover here before client commit/terminal effects. Do not re-enter candidate inner after error consumption (~6109).
- sync/execution.rs result/OAuth loop (~2775) precedes failure analysis (~2913). First regular sync send uses execute_direct_sync_runtime_candidate (~1553), not transport helper.
- transport.rs::execute_sync_plan_with_report_context (~1379) covers admin model_test.rs (~2861); include its tunnel early return (~1401). Admin knows provider and can pass narrow internal identity; no public serialized fields/config.
One repair budget belongs to the logical candidate, independent of OAuth retry state. Same IDs/auth/model/URL on repair; no recursive high-level execution resetting budget. Later same signature error uses existing Provider retry scope when RetryNextCandidate allowed; StopLocalFailover remains stop. Fixed admin target returns final error.

## Lifecycle, diagnostics and timing
Intermediate400 is diagnostic, not a terminal attempt: no duplicate failure/health/OAuth-invalid/PoolError/lease/billing effects. Final response retains unique owner. Keep logical request/candidate IDs, each HTTP send has distinct response_observation.request_order_id.
Outer watchdog covers whole recovery Future; remaining first-byte/non-stream total deadline is reduced rather than renewed, and exhausted budget sends nothing.
Update local final plan.body/usage seed, not just send bytes, so capture shows actual compatible upstream request; original client capture immutable. Admin separately-held provider_request_body must receive final body through internal result plumbing, public response shape unchanged.
Use existing trace/candidate diagnostics for original error and outcome; no secret dumps or new logging subsystem.

## Limits
No public DTO/database/frontend/dependency changes, signature caches/codecs/heuristics or production actions. Docker-only verification. Preserve ordinary Gemini and all successful first-send signatures.
