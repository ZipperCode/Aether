# 修复长流生命周期与断流诊断

## Goal

Prevent live long-running streams from being finalized by stale cleanup, preserve consistent terminal request/candidate facts, and diagnose stalled or unusually large upstream responses without recording their contents. The user approved the proposed fix and explicitly excluded a total-duration cap.

## Background

Request 8cf62c57 on deployed revision ef7ff420 returned its first byte in 1,528 ms, received 20,815,233 bytes, and failed reading the upstream stream at 900,195 ms. Cleanup had prematurely marked the still-streaming request and candidate successful at roughly 10m50s. The final usage became failed/502 while the candidate remained successful. Audit bodies were disabled; underlying body errors were reduced to ProtocolError.

## Requirements

- R1: A still-running stream remains streaming beyond the stale-request threshold. Neither streaming status alone nor HTTP 200 proves successful completion. Preserve cleanup of genuinely abandoned requests where reliable evidence exists.
- R2: Real success, failure, and cancellation finalize request and serving candidate consistently; delayed nonterminal writes cannot overwrite terminal facts. A false legacy cleanup success without completion evidence must not suppress a real stream failure.
- R3: Emit lightweight, bounded stream diagnostics: request/candidate correlation, byte and SSE-event counters, last content progress timing, and observed terminal completion. Do not log prompt/output text, credentials, arbitrary upstream fields, or raw transport error strings.
- R4: Preserve transport failure categories (timeout, connection reset/closed, decode/protocol) through stream handling while retaining sanitized client errors and existing protocol compatibility.
- R5: Native stream bytes and existing first-byte/idle-timeout semantics remain unchanged. No total stream-duration cap, provider/model hardcoding, schema migration, new dependency, production deployment, or configuration change.

## Acceptance Criteria

- [x] AC1: A live stream older than cleanup threshold remains active; completed evidence can recover stale bookkeeping; abandoned requests are not converted to success.
- [x] AC2: Normal completion, midstream failure and cancellation yield consistent request/candidate outcomes, including a legacy premature-success candidate; stale streaming updates do not regress terminal state.
- [x] AC3: Fragmented SSE and keepalive traffic produce bounded diagnostics without body data; normal byte forwarding is unchanged; missing completion is observable.
- [x] AC4: Actual loopback transport failures retain safe classified error facts rather than all becoming ProtocolError.
- [x] AC5: Focused runtime and PostgreSQL regressions pass, affected architecture/format checks pass; no new total timeout is introduced.

## Authorization

Local implementation and safe focused verification only. Commit and push isolated verified changes to the current remote branch under the repository Git policy. No production access or mutation is needed for this implementation.
