# Oversized terminal body retention

## 1. Scope / Trigger

Terminal usage events may contain four FULL HTTP bodies whose combined serialized size exceeds the queue payload ceiling. Queue encoding can succeed by omitting diagnostic data; enqueue success therefore does not prove complete body retention. This contract preserves local oversized full-body retention (ported from upstream PR #830, commit `5842c7232ecd6cdfa95994da9afaac14f5a8deb0`) alongside explicit audit body compaction and size capping (upstream PR #755).

## 2. Signatures

- System configuration keys:
  - `request_record_compact_base64_images`: boolean, default `false`.
  - `request_record_max_body_size_kb`: u64 (0 to 1,048,576), default `0` (where `0` denotes unlimited / `max_body_bytes: None`).
- `UsageBodyCapturePolicy`:
  ```rust
  pub struct UsageBodyCapturePolicy {
      pub record_level: UsageRequestRecordLevel,
      pub compact_base64_images: bool,
      pub max_body_bytes: Option<usize>,
  }
  ```
- `limit_usage_body_capture_value(value: Value, max_bytes: Option<usize>) -> LimitedUsageBodyCapture`
- `UsageQueue::encode_event(&UsageEvent) -> Result<EncodedUsageEvent, DataLayerError>` reports `diagnostics_omitted` and produces bounded fields.
- `UsageQueue::enqueue_encoded(EncodedUsageEvent)` sends those fields without serializing again.
- `UsageRuntime::enqueue_terminal_event_or_fallback` owns the decision; `try_write_terminal_direct_fallback` remains the bounded database path.
- PostgreSQL `FIND_BY_ID_SQL` and `FIND_BY_REQUEST_ID_SQL` project four `http_*_body_state` aliases.

## 3. Contracts

1. **Body Capture Configuration Defaults**:
   - System settings `request_record_compact_base64_images` defaults to `false`.
   - System settings `request_record_max_body_size_kb` defaults to `0`.
   - Gateway mapping maps `0` or null/absent values to `max_body_bytes: None`. When `record_level` is set to `Full`, captured body size remains completely unlimited by default; `record_level` (e.g. `Basic`) continues to gate whether HTTP bodies are stored at all.
   - Only when an administrator explicitly sets `request_record_max_body_size_kb > 0` are bodies truncated up front before queuing.
   - Only when `request_record_compact_base64_images == true` are embedded Base64 image data URLs compacted.

2. **Queue Omission & Bounded Direct Persistence**:
   When encoding would omit diagnostics, try existing bounded persistence with the original event before sending its stripped queue representation. Successful persistence returns `PersistedDirectly` and enqueues no duplicate. Otherwise reuse the already encoded bounded fields.

3. **Guarantees**:
   Preserve capture policy, queue byte limit, database-pressure checks, shared worker write gate, terminal fallback gate, settlement ownership, first-byte ordering and existing enqueue failure handling. The oversized direct persistence mechanism itself requires no additional database migration or retry mechanism. Encode-attempt metrics remain attempts, not a count of irretrievably lost bodies.
## 4. Validation & Error Matrix

| Condition | Result |
| --- | --- |
| `request_record_max_body_size_kb` = 0 or unset | `max_body_bytes: None`, full bodies retained without upfront truncation |
| `request_record_max_body_size_kb` = N (> 0) | Upfront truncation at $N \times 1024$ bytes; `reason: body_capture_limit_exceeded` |
| `request_record_compact_base64_images` = false | Base64 image URLs preserved verbatim |
| `request_record_compact_base64_images` = true | Base64 image URLs replaced with `<base64-image-data-omitted:...>` placeholder |
| Event fits queue | Existing queue path |
| Diagnostic omission needed and bounded write succeeds | Original complete bodies persisted directly, no queue duplicate |
| Writer unavailable, pressured, saturated or write fails | Bounded queue fallback; dropped bodies truthfully marked `Truncated` |
| Core cannot be encoded / queue itself fails | Existing classified failure and bounded fallback rules |
| Historical body already discarded | Remains unavailable; no reconstruction claim |

## 5. Good / Base / Bad Cases

- Good: default configuration leaves `max_body_bytes: None` and `compact_base64_images: false`; four captured bodies jointly exceed 1 MiB; the direct record contains all four exact bodies and the stream remains empty.
- Base: ordinary terminal events use unchanged queue encoding and processing.
- Bad: change default max size to non-zero, raise global queue limits, clone unbounded serialized payloads, or report a known truncation as `legacy_unknown`.
## 6. Tests Required

- `request_record_body_capture_defaults_are_backward_compatible`: confirms `request_record_compact_base64_images` defaults to `false` and `request_record_max_body_size_kb` defaults to `0`.
- `request_record_body_capture_updates_validate_types_and_bounds`: validates boolean and $0 \le \text{size} \le 1{,}048{,}576$ limits.
- `oversized_full_terminal_capture_is_persisted_without_queue_truncation`: original bodies, one enrichment/write, zero enqueued entries.
- `oversized_full_terminal_capture_keeps_bounded_queue_fallback`: no writer/write error/worker gate/fallback gate; one bounded entry, accounting facts retained and all omitted body states `Truncated`.
- `usage_sql_reads_http_audits_for_single_record_fetches`: both detail selectors project every capture state.
- Frontend `body-document-engine.spec.ts`: Worker gzip load/copy preserves complete content beyond preview page limits.

## 7. Wrong vs Correct

Wrong: change default body cap from 0 to a non-zero limit that silently truncates body data for existing deployments. Treat successful bounded encoding as permission to discard FULL diagnostic bodies immediately.

Correct: use canonical system config keys `request_record_compact_base64_images` and `request_record_max_body_size_kb` with defaults `false` and `0` (`None` / unlimited). Attempt original-event persistence under existing limits when queue limits are reached, then reuse bounded fallback if necessary. Report the resulting capture state honestly.
