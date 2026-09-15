# Antigravity rejected thought-signature recovery

## 1. Scope / Trigger

Public Gemini history can contain opaque signatures emitted by another provider. The observed Antigravity request received HTTP 400 with `INVALID_ARGUMENT` and `Corrupted thought signature.` across 16 keys; five signatures were traced unchanged to earlier apiyi responses. This proves rejected cross-provider replay, not account binding or cryptographic corruption caused by Aether.

The user approved error-triggered compatibility degradation: retain visible conversation/tools but do not reuse the rejected historical reasoning state. Normal successful requests preserve signatures. Precedents: [sub2api #946](https://github.com/Wei-Shaw/sub2api/pull/946) and [CLIProxyAPI #3470](https://github.com/router-for-me/CLIProxyAPI/pull/3470).

## 2. Signatures

- Provider transport: `is_antigravity_corrupted_thought_signature(u16, &Value) -> bool` and `repair_antigravity_thought_signatures(&Value) -> Option<Value>`.
- Gateway: candidate-local `AntigravitySignatureRecovery`; independent from OAuth refresh state.
- Runtime consumers: precommit stream execution, regular sync candidate loop, fixed-target admin model-test execution.
- Regular sync candidates must enter the embedded local tunnel through `execute_sync_plan_via_local_tunnel_with_response_started` before falling back to ordinary direct transport. Reuse the response-start callback so recovery retains the same observation and OAuth-success ownership as direct sync execution.
- No public API, database schema, dependency, configuration flag or signature cache is introduced.

## 3. Contracts

- Require explicit `envelope_name=antigravity:v1internal` and Gemini provider format. Do not infer identity from URL or model names.
- First send is unchanged. Recognize only actual HTTP 400, JSON `error.status=INVALID_ARGUMENT`, and `Corrupted thought signature` ignoring surrounding whitespace/trailing periods.
- Rewrite only existing part-root `thoughtSignature` / `thought_signature` in historical `role=model` contents (public `contents` or private `request.contents`) to `skip_thought_signature_validator`. Do not recursively edit arbitrary JSON, synthesize missing fields, discard parts, or modify tool arguments/results/order.
- A changed body receives one compatibility retry using the same provider/endpoint/key/auth/model/URL. Never recursively restart the candidate and reset its recovery allowance.
- No change or repeated rejection skips the provider only for this request when existing policy allows failover. `StopLocalFailover` remains terminal; unrelated errors retain existing behavior.
- Keep a single candidate lifecycle and remaining deadline. Intermediate 400 is not a terminal failure, credential invalidation, health penalty, charge or lease release.
- Record fixed diagnostic fields and HTTP request-order IDs through existing candidate tracing; never record raw signatures as diagnostic metadata. Final upstream capture is the actual compatible body; original client capture is unchanged.
- Both report-driven status writes and lightweight async status snapshots must carry the compact recovery field. An earlier active-state diagnostic upsert alone is insufficient: later terminal projection can replace its extra_data. The gateway owns this projection; no database schema change is needed.
- Candidate data-contract persistence/read projection must explicitly preserve the new diagnostic: bounded numeric statuses, fixed error/outcome strings and UUID HTTP send-order IDs only. Reuse the existing projection rules; do not disable filtering or retain arbitrary new payload fields.
- Routine passthrough must not gain full-body/context copies merely to check whether recovery is needed.

## 4. Validation & Error Matrix

| Condition | Behavior |
| --- | --- |
| First request succeeds | One send, original signatures |
| Exact rejection plus modifiable history | Same-account compatible send once |
| Second identical rejection / no modifiable signature | Provider-scope fallback if allowed; otherwise original stop behavior |
| Other INVALID_ARGUMENT / HTTP 200 embedded error / non-Antigravity | No signature recovery |
| Deadline exhausted before recovery send | No extra HTTP request |
| Recovery transport fails | Preserve original diagnostic and existing transport-error disposition |

## 5. Good / Base / Bad Cases

- Good: preserve first signed tool call; after explicit rejection, replace only its signature and resend the same candidate.
- Base: unsigned history or already-compatible successful calls are untouched.
- Bad: clearing all signatures before the first request, guessing validity from Base64 prefixes, or interpreting final fallback HTTP 200 as Antigravity success.

## 6. Tests Required

Use existing Rust inline/loopback fixtures and authenticated local tunnel frame fixtures. Assert exact trigger exclusions, input immutability, protected nested tool data, same authentication on both sends, one recovery budget, provider/stop behavior, actual final capture, diagnostic retention and unique terminal effects.

Docker-only focused commands: provider `antigravity::request` / `same_format_gemini`, gateway `antigravity_signature`, and existing Gemini signature round-trip filters. Check nonzero selected test counts, scoped formatting and affected package type/lint checks. Local mock success is not production validation; deployment/replay requires separate authorization and must show Antigravity itself succeeded.

## 7. Wrong vs Correct

Wrong: inspect only the final usage provider, assume no Antigravity call occurred, and then remove every signature speculatively.

Correct: correlate failed `request_candidates` with the trace and real upstream error, preserve successful first requests, recover once after the exact verdict, and retain both failure and recovery evidence. The final `provider_request_body` after failover belongs to the successful fallback provider, not the earlier failed wire.
