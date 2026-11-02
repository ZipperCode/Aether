# Design

## Lifecycle ownership

No existing cross-instance registry identifies live request IDs: UsageRuntime and request_activity only count producers, while distributed admission leases use anonymous tokens. Therefore add a small periodic heartbeat bound to the existing cloneable AdmissionPermit/request-body/background-execution lifetime. Write a dedicated internal numeric `runtime_heartbeat_unix_secs` into existing usage.request_metadata via atomic JSON patch; never touch updated_at/response-time clock or add a schema. Stop its task when the last permit drops. Cleanup respects fresh heartbeat using its existing abandonment grace; live requests have no elapsed-time cap, crashed owners stop refreshing and can be reclaimed.

Only candidate success with stream_completed=true proves completion. Remove cleanup mutation of streaming candidates into success. Reconcile a legacy premature-success serving candidate only from an accepted authoritative failed/cancelled stream usage event, matching request and candidate identity and requiring absent completion evidence. Keep genuine completed/nonstream candidate facts and delayed-write guards unchanged.

## Stream diagnostics

Keep diagnostics in gateway execution runtime, not provider payload or public DTO. Preserve typed body-read categories before converting errors to strings. Use existing ExecutionErrorKind values; log fixed internal categories for reset/closed/decode, never raw error chains. Migrate direct prefetch, frame pump and passthrough consumers consistently. Reuse bounded SSE observation for known event counts plus other bucket, byte counts, last content progress and terminal evidence. Unknown names/text are not logged; forwarding and timeout semantics remain unchanged.

## Safety and verification

No schema migration, dependency, frontend contract, total deadline, automatic request retry, production change or body capture change. Verify heartbeat lifecycle and orphan expiry, actual PostgreSQL cleanup and legacy repair, normal/failure/cancel terminal outcomes, bounded fragmented SSE diagnostics and loopback transport failures. Parent owns final focused verification and integration.
