# Antigravity rejected thought-signature recovery

## 1. Scope / Trigger

Public Gemini history can contain opaque signatures emitted by another provider. The observed Antigravity request received HTTP 400 with `INVALID_ARGUMENT` and `Corrupted thought signature.` across 16 keys; five signatures were traced unchanged to earlier apiyi responses. This proves rejected cross-provider replay, not account binding or cryptographic corruption caused by Aether.

The user approved error-triggered compatibility degradation: retain visible conversation/tools but do not reuse the rejected historical reasoning state. Normal successful requests preserve signatures. Precedents: [sub2api #946](https://github.com/Wei-Shaw/sub2api/pull/946) and [CLIProxyAPI #3470](https://github.com/router-for-me/CLIProxyAPI/pull/3470).

The 2026-09-17 incident adds a distinct confirmed rejection: Google reports `TYPE_BYTES` / `Base64 decoding failed` for `request.contents[151].parts[0].thought_signature`. The incoming opaque value was 16,766 characters; its origin remains unproven because the generating response was not retained. Recognize the rejection rather than guessing validity or reconstructing lost bytes.

## 2. Signatures

- Provider transport: `classify_antigravity_thought_signature_rejection(u16, &Value) -> Option<AntigravityThoughtSignatureRejection>` and `repair_antigravity_thought_signature_rejection(&Value, AntigravityThoughtSignatureRejection) -> Option<Value>`; the legacy corrupted-signature helpers preserve their existing behavior.
- Gateway: candidate-local `AntigravitySignatureRecovery`; independent from OAuth refresh state.
- Runtime consumers: precommit stream execution, regular sync candidate loop, fixed-target admin model-test execution.
- Regular sync candidates must enter the embedded local tunnel through `execute_sync_plan_via_local_tunnel_with_response_started` before falling back to ordinary direct transport. Reuse the response-start callback so recovery retains the same observation and OAuth-success ownership as direct sync execution.
- No public API, database schema, dependency, configuration flag or signature cache is introduced.

## 3. Contracts

- Require explicit `envelope_name=antigravity:v1internal` and Gemini provider format. Do not infer identity from URL or model names.
- First send is unchanged. Require actual HTTP 400 and JSON `error.status=INVALID_ARGUMENT`. Accept either the legacy `Corrupted thought signature` (ignoring surrounding whitespace/trailing periods) or a Google `TYPE_BYTES` / `Base64 decoding failed` rejection naming the exact private `request.contents[n].parts[m].thought_signature` field. When structured `fieldViolations` exist, do not override them with a looser message match.
- The legacy unlocated rejection rewrites existing part-root `thoughtSignature` / `thought_signature` in historical `role=model` contents. A field-specific Base64 rejection rewrites only the reported model part, leaving every other signature intact. Both use the existing Antigravity compatibility sentinel `skip_thought_signature_validator` and support public `contents` or private `request.contents` request representations. Do not recursively edit arbitrary JSON, synthesize fields, discard parts, modify tool arguments/results/order or pad/re-encode opaque signatures.
- Antigravity HTTP 400 recovery and final stream failure classification may inspect at most 64 KiB before parsing echoed error JSON. Recovery retains the existing prefetch deadline; ordinary error inspection remains 16 KiB. Incomplete/larger unsupported JSON does not trigger a guessed repair; retained upstream chunks remain available to the existing downstream/error path. Both failure stages must use the shared classifier so repeated rejection skips the provider rather than rotating its sibling keys.
- A changed body receives one compatibility retry using the same provider/endpoint/key/auth/model/URL. Never recursively restart the candidate and reset its recovery allowance.
- No change or repeated rejection skips the provider only for this request when existing policy allows failover. `StopLocalFailover` remains terminal; unrelated errors retain existing behavior.
- Keep a single candidate lifecycle and remaining deadline. Intermediate 400 is not a terminal failure, credential invalidation, health penalty, charge or lease release.
- Record fixed diagnostic fields and HTTP request-order IDs through existing candidate tracing; never record raw signatures as diagnostic metadata. Final upstream capture is the actual compatible body; original client capture is unchanged.
- The only safe diagnostic error labels are `Corrupted thought signature` and `Base64 decoding failed`; both survive candidate persistence and read projection.
- Both report-driven status writes and lightweight async status snapshots must carry the compact recovery field. An earlier active-state diagnostic upsert alone is insufficient: later terminal projection can replace its extra_data. The gateway owns this projection; no database schema change is needed.
- Candidate data-contract persistence/read projection must explicitly preserve the new diagnostic: bounded numeric statuses, fixed error/outcome strings and UUID HTTP send-order IDs only. Reuse the existing projection rules; do not disable filtering or retain arbitrary new payload fields.
- Routine passthrough must not gain full-body/context copies merely to check whether recovery is needed.

## 4. Validation & Error Matrix

| Condition | Behavior |
| --- | --- |
| First request succeeds | One send, original signatures |
| Exact rejection plus modifiable history | Same-account compatible send once |
| Base64 rejection names an existing model part signature | Replace that part's signature only, same-account send once |
| Wrong byte field, user/tool part, out-of-range location or no signature | No field-specific repair |
| Second identical rejection / no modifiable signature | Provider-scope fallback if allowed; otherwise original stop behavior |
| Other INVALID_ARGUMENT / HTTP 200 embedded error / non-Antigravity | No signature recovery |
| Deadline exhausted before recovery send | No extra HTTP request |
| Recovery transport fails | Preserve original diagnostic and existing transport-error disposition |

## 5. Good / Base / Bad Cases

- Good: preserve first signed tool call; after explicit rejection, replace only its signature and resend the same candidate.
- Base: unsigned history or already-compatible successful calls are untouched.
- Bad: clearing all signatures before the first request, guessing validity from Base64 prefixes, or interpreting final fallback HTTP 200 as Antigravity success.

## 6. Tests Required

Use existing Rust inline/loopback fixtures and authenticated local tunnel frame fixtures. Assert exact trigger exclusions, input immutability, protected nested tool data, same authentication on both sends, one recovery budget, provider/stop behavior, actual final capture, diagnostic retention and unique terminal effects. Cover a synthetic 16,766-character signature and Google error echo exceeding 16 KiB in stream, sync and fixed-target tests; unrelated historical signatures must remain byte-identical.

Docker-only focused commands: provider `antigravity::request` / `same_format_gemini`, gateway `antigravity_signature`, and existing Gemini signature round-trip filters. Check nonzero selected test counts, scoped formatting and affected package type/lint checks. Local mock success is not production validation; deployment/replay requires separate authorization and must show Antigravity itself succeeded.

## 7. Wrong vs Correct

Wrong: inspect only the final usage provider, assume no Antigravity call occurred, and then remove every signature speculatively.

Correct: correlate failed `request_candidates` with the trace and real upstream error, preserve successful first requests, recover once after the exact verdict, and retain both failure and recovery evidence. The final `provider_request_body` after failover belongs to the successful fallback provider, not the earlier failed wire.
