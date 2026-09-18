# Implementation and verification

1. Reuse existing frame-stream test helpers and real Responses compatibility rewrite selection. Add focused regressions before changing production logic; run the main regression against the old code and retain the actual failure.
2. Make the minimal complete-prefix append change, delete the unused truncation flag, and add a Chinese comment explaining the semantic/inspection distinction. Preserve all other precommit and restoration behavior.
3. Exercise valid approximately 18 KB, 20 KB and 75 KB SSE fixtures with 5,000/6,000/16,384-byte fragments and 16 KiB-adjacent offsets, complete+partial records, base64 fragments for split UTF-8, normal terminal, private normalization and raw passthrough. Assert parsed payload content and event order plus exact unchanged bytes where appropriate; mere JSON parse success is insufficient.
4. Run new tests and focused existing precommit failover, first-byte accounting and stream-capture budget tests. Use the existing Rust test stack convention (16 MiB), avoid unrelated suites and real services. Run `cargo fmt --all --check` and a scoped gateway compiler/Clippy check as appropriate. Report unexecuted checks honestly.
5. Parent updates the existing Responses SSE contract, reviews final diff with a Trellis check agent, records commands/results and commits the task scope with a Chinese message; archive task and journal without pushing.

## Ownership

- Implement agent: gateway stream source and regression tests only; no task/spec edits or commits.
- Parent: task artifacts, spec update, integration review, commits and finish.
- Check agent: review product diff and verification evidence; fix only concrete scoped defects and coordinate any overlapping edits.
