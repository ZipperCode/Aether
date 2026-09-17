# Oversized terminal body retention

## 1. Scope / Trigger

Terminal usage events may contain four FULL HTTP bodies whose combined serialized size exceeds the queue payload ceiling. Queue encoding can succeed by omitting diagnostic data; enqueue success therefore does not prove complete body retention. This contract ports upstream PR #830, commit `5842c7232ecd6cdfa95994da9afaac14f5a8deb0`.

## 2. Signatures

- `UsageQueue::encode_event(&UsageEvent) -> Result<EncodedUsageEvent, DataLayerError>` reports `diagnostics_omitted` and produces bounded fields.
- `UsageQueue::enqueue_encoded(EncodedUsageEvent)` sends those fields without serializing again.
- `UsageRuntime::enqueue_terminal_event_or_fallback` owns the decision; `try_write_terminal_direct_fallback` remains the bounded database path.
- PostgreSQL `FIND_BY_ID_SQL` and `FIND_BY_REQUEST_ID_SQL` project four `http_*_body_state` aliases.

## 3. Contracts

When encoding would omit diagnostics, try existing bounded persistence with the original event before sending its stripped queue representation. Successful persistence returns `PersistedDirectly` and enqueues no duplicate. Otherwise reuse the already encoded bounded fields.

Preserve capture policy, queue byte limit, database-pressure checks, shared worker write gate, terminal fallback gate, settlement ownership, first-byte ordering and existing enqueue failure handling. No migration, new configuration or retry mechanism is needed. Encode-attempt metrics remain attempts, not a count of irretrievably lost bodies.

## 4. Validation & Error Matrix

| Condition | Result |
| --- | --- |
| Event fits queue | Existing queue path |
| Diagnostic omission needed and bounded write succeeds | Original complete bodies persisted, no queue duplicate |
| Writer unavailable, pressured, saturated or write fails | Bounded queue fallback; dropped bodies truthfully marked `Truncated` |
| Core cannot be encoded / queue itself fails | Existing classified failure and bounded fallback rules |
| Historical body already discarded | Remains unavailable; no reconstruction claim |

## 5. Good / Base / Bad Cases

- Good: four captured bodies jointly exceed 1 MiB; the direct record contains all four exact bodies and the stream remains empty.
- Base: ordinary terminal events use unchanged queue encoding and processing.
- Bad: raise global queue limits, clone unbounded serialized payloads, or report a known truncation as `legacy_unknown`.

## 6. Tests Required

- `oversized_full_terminal_capture_is_persisted_without_queue_truncation`: original bodies, one enrichment/write, zero enqueued entries.
- `oversized_full_terminal_capture_keeps_bounded_queue_fallback`: no writer/write error/worker gate/fallback gate; one bounded entry, accounting facts retained and all omitted body states `Truncated`.
- `usage_sql_reads_http_audits_for_single_record_fetches`: both detail selectors project every capture state.
- Frontend `body-document-engine.spec.ts`: Worker gzip load/copy preserves complete content beyond preview page limits.

## 7. Wrong vs Correct

Wrong: treat successful bounded encoding as permission to discard FULL diagnostic bodies immediately.

Correct: attempt original-event persistence under existing limits, then reuse the bounded fallback if necessary. Report the resulting capture state honestly.
