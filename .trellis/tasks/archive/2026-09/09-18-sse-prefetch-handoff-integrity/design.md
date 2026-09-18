# Prefetch handoff integrity

## Verified flow

`execute_stream_from_frame_stream_with_retry_scope` consumes complete Data fragments. At baseline `execution.rs:7242`, provider replay bytes are capped at `MAX_STREAM_PREFETCH_BYTES` (16,384), while `StreamCommitGate::observe_provider_bytes` counts the full fragment. Prefetch feeds the full fragment to the normalizer/rewriter, then drops those borrowed-context instances. The spawned forwarding task rebuilds them and replays the capped prefix at `execution.rs:7949-8037`, losing an incomplete record's already-consumed suffix.

## Chosen change

Keep the existing `Vec<u8>` for provider replay and append the complete consumed chunk with `extend_from_slice`; remove its truncation flag. Keep the separate inspection buffer budgeted and leave commit policy unchanged. Prefetch still ends at the current byte/frame/time/semantic boundary, so replay retains at most the precommit prefix plus the last admitted fragment, not the response lifetime. Existing frame/parser limits remain in force.

The same complete prefix restores private normalization, local rewriting, usage observation and error inspection. Restoration output is discarded; previously produced client chunks are emitted once. Audit capture retains its existing independent budget. Drop semantic replay buffers after restoration. Preserve native terminal truncation, sync-to-stream bridging and response-history deduplication.

Direct state transfer is not selected: both existing normalizer and rewriter borrow report contexts, so moving them across the spawned task would require broader ownership changes. No new abstraction is needed.

## Boundaries

No new public types, wire shape, configuration, dependencies or persistence changes. Existing malformed-JSON passthrough and the independent >1 MiB control-filter defect are excluded. Local verification only; no live requests, release or deployment. Rollback is a normal revert of the scoped work commit.
