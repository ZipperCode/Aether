# Independent source check receipt

Reviewed both child task manifests, PRDs, designs and implementation plans, the signature implementation receipt, the upstream retention receipt, and the current source paths. This is a source/format review; the root session owns all Cargo/Docker execution.

## Findings fixed

1. **Stream terminal failure still used the legacy-only detector.** `execute_stream_from_frame_stream_with_retry_scope` classified only `Corrupted thought signature`, and its collector retained only 16 KiB. A repeated Base64 rejection could consequently retry another key instead of skipping the provider for this request. The collector now accepts its caller's explicit bound: 64 KiB only for private Antigravity HTTP 400, otherwise the original 16 KiB. The final retry-scope branch uses the shared rejection classifier. Existing stop policy remains outside and above that branch. The existing terminal/stop regression now exercises both legacy and structured Base64 errors, including recovered, rejected and stopped outcomes.
2. **The long-error fixture did not match the observed Google shape.** It contained one signature echo in the top-level message only. The shared fixture now supplies `code=400`, the quoted 16,766-character signature in `message`, and `google.rpc.BadRequest.fieldViolations` with the same long description. The stream regression asserts that the actual encoded JSON is larger than 32 KiB and smaller than the 64 KiB bound, first-send equality and unchanged authentication.
3. **Integer parsing accepted signed indices.** The exact proto field parser now requires nonempty ASCII decimal digits before parsing each index. Added signed/whitespace/empty/overflow/nested-path negatives to the existing table and tool/out-of-range-part negatives to the existing immutable targeted-repair test.

Exact product paths changed by this reviewer:

- `apps/aether-gateway/src/execution_runtime/antigravity_signature.rs`
- `apps/aether-gateway/src/execution_runtime/stream/execution.rs`
- `apps/aether-gateway/src/execution_runtime/stream/error.rs`
- `crates/aether-provider/transport/src/antigravity/request.rs`

## Full-scope review

- Signature classification remains actual HTTP 400 + `INVALID_ARGUMENT`, confirmed private field path and `TYPE_BYTES` / Base64 evidence. Existing legacy handling remains. Structured violations take precedence over loose message interpretation. Ordinary public Gemini and unrelated formats remain outside the enabled recovery path.
- Recovery modifies existing model part-root signature aliases only, preserves original client input and unrelated/tool data, reuses the candidate-local single budget and remaining deadline, and keeps intermediate failures out of terminal billing/health ownership. Fixed diagnostic strings survive candidate projection; raw error/signature text is not introduced into compact recovery metadata.
- The three runtime consumers reuse shared recovery: precommit stream, regular sync (including existing local-tunnel route), and fixed-target admin execution. No new retry abstraction, encoding reconstruction, dependency or configuration was added.
- Reviewed all six upstream #830 paths. `enqueue_encoded` reuses the bounded encoding; successful direct persistence returns before enqueue. Existing writer/database-pressure, worker/fallback gates, settlement and first-byte ordering remain authoritative. If direct persistence cannot complete, the bounded queue event retains accounting facts and truthful truncation states.
- Both PostgreSQL detail selectors now expose the four capture-state aliases consumed by the existing row decoder. The frontend change is regression coverage for complete Worker decoding/copying and does not alter preview or download production behavior.
- Root's updated signature and retention contracts match the reviewed code. The signature contract should explicitly include the same 64 KiB bound for final stream failure classification in addition to recovery prefetch.

## Verification

- PASS: scoped `rustfmt --edition 2021 --check` across all 11 changed Rust files, including the new stream error collector touch point.
- PASS: `git diff --check`.
- Root-reported and receipt-backed: frontend Worker spec 13 passed, scoped ESLint passed, frontend type-check passed; no frontend source changed during this review, so these were not rerun.
- At reviewer completion, Rust compilation, Clippy and targeted tests were pending under root ownership. Root subsequently passed all 49 unique scoped backend tests, including `execution_runtime::stream::error::tests`, after fixing two test-only constant references with `super::`. Root's validation receipt is authoritative for execution results; final affected-package Clippy passed with `-D warnings`. See the validation receipt for the three behavior-preserving lint cleanups.
- No unresolved source finding within the approved scope. Real new Base64 acceptance on Antigravity remains unverified until separately authorized deployment/replay; this reviewer made no production reads/writes, model calls, commits or pushes.
