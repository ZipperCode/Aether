# Research: Antigravity thought-signature wire representation

- Query: Identify the actual Aether signature path and the smallest evidence-backed, lossless correction for repeated Antigravity `Corrupted thought signature` failures.
- Scope: mixed; local source/specs plus current primary Google documentation and upstream implementation source. No product changes, git operations, host compilation, remote SSH, or live inference performed by this researcher.
- Date: 2026-09-09
- Baseline: parent reports master `0c901cc90`, no product diff, and `cbf1346fe` is an ancestor of `v0.7.33`. These git facts were supplied by the parent, not independently checked by this research-only role.

## Findings

### Conclusion and confidence

**No lossless production code repair is yet proven.** The evidence establishes cross-provider signed-history incompatibility much more strongly than corruption introduced by Aether. It does not establish account binding, a generally invalid signature, or that adding/removing a protobuf wrapper will make private cloudcode accept the same encrypted state.

| Finding | Confidence / limit |
| --- | --- |
| The incident uses same-format Gemini JSON, not the canonical projection that synthesizes skip sentinels. | High; real same-format caller and cloning branch identified below. |
| The Antigravity builder preserves signature strings today. | High; both public-body and existing-private-envelope branches clone the input and only normalize tool names/parameter schemas. |
| At least five failed-history signatures originated in earlier successful non-Antigravity provider responses and survived response/client/request replay unchanged. | High for those five; live peer supplied equality results, not raw data. |
| Antigravity rejects a request containing raw opaque `0x01...` signature bytes that another provider emitted/accepted. | High for supplied incident evidence; the complete final successful fallback does not prove each field was validated by that provider. |
| The private endpoint may expect a protobuf field-2 → field-1 container around the opaque bytes. | Plausible candidate, not established: the independent implementation recognizes this container, but does not implement raw-byte wrapping. |
| Wrapping the raw bytes will cure the incident without losing reasoning. | Unverified. Matching an outer wire shape cannot demonstrate compatibility of the encryption keys, model state, or signed part history. |

Do not implement blanket deletion, global sentinel substitution, or automatic wrapping based only on the first byte. All three can change otherwise valid opaque history; the first two explicitly discard reasoning continuity. A complete diagnosis is useful progress, but is not evidence that the user-visible bug is fixed.

### Files found and local call chain

CodeGraph was queried before locating source, then targeted reads/searches filled gaps. Current package paths are nested (`crates/aether-provider/transport`, `crates/aether-ai/formats`); older flattened overview paths are stale.

| File | Role / evidence |
| --- | --- |
| `apps/aether-gateway/src/ai_serving/planner/passthrough/provider/family/request.rs` | Real same-format request candidate preparation and Antigravity wrapping. |
| `crates/aether-provider/transport/src/same_format_provider/mod.rs` | Same-format JSON clone, model/body policy, no Gemini signature codec. |
| `crates/aether-provider/transport/src/antigravity/request.rs` | Shared private-envelope boundary; owns the smallest potential provider-local representation repair. |
| `apps/aether-gateway/src/ai_serving/planner/antigravity.rs` | Cross-format callers converge on the same private-envelope builder. |
| `apps/aether-gateway/src/handlers/admin/provider/query/models/model_test.rs` | Admin endpoint-test caller converges on the same builder. |
| `crates/aether-ai/formats/src/formats/shared/stream_rewrite.rs` | Chooses private-envelope unwrap versus canonical stream conversion. |
| `crates/aether-ai/formats/src/provider_compat/private_envelope.rs` | Unwraps Antigravity JSON response/SSE; adds IDs but has no signature wrapper codec. |
| `crates/aether-ai/formats/src/formats/gemini/generate_content/request.rs` | Canonical tool-signature preservation and synthetic-history sentinel; not the incident input path. |
| `crates/aether-ai/formats/src/formats/registry.rs` | Existing synthetic-history and opaque signature round-trip regression cases. |
| `crates/aether-ai/formats/src/formats/gemini/generate_content/stream.rs` | Canonical Gemini stream state preserves signature metadata; relevant to JSON-array/cross-format output branches. |

The bounded request path is:

1. `resolve_local_same_format_provider_candidate_payload_parts` receives `body_json` and calls the same-format body builder at `.../family/request.rs:124` and `:203`. Existing PII policy, operator body rules, and model directives run through their current owners; they are not a new signature-normalization feature.
2. `build_same_format_provider_request_body_inner` clones every JSON key/value when formats match at `same_format_provider/mod.rs:459`; canonical conversion is only the `else` at `:470`. Gemini model removal is at `:523`, configured body rules at `:573`, and function-response ID removal at `:616`. None is a signature codec. Live peer equality supports that the incident's signatures survived these paths.
3. The candidate calls `build_antigravity_safe_v1internal_request` at `.../family/request.rs:419`, with `Agent` request type. The existing-envelope input clones `source.request` at `antigravity/request.rs:77`; public input clones `source` at `:100`. Both remove model/safety fields, normalize built-in tool names and parameter schemas, then return a newly owned envelope. No history traversal or signature rewriting exists there.
4. All production direct callers of this shared builder found by source search: same-format candidate above; `build_antigravity_v1internal_payload` at `planner/antigravity.rs:152`; admin model test at `model_test.rs:2763` using `EndpointTest`. Imports/re-exports and inline tests are not additional execution owners. Cross-format payload builders call `build_antigravity_v1internal_provider_request`, which calls the shared payload function at `planner/antigravity.rs:54`.

This means a *proven* Antigravity-only codec belongs in this shared request builder, not in generic canonical Gemini emission and not independently in each caller. Both input forms and input immutability must be covered.

### Response / replay path

- Public same-format SSE with a private envelope selects `EnvelopeUnwrap` at `stream_rewrite.rs:212`. A native client consuming the same private envelope instead remains unwrapped by Aether at `:107` and `:220`.
- `transform_provider_private_stream_line` at `private_envelope.rs:167` unwraps `body.response` at `:235`, preserves candidate/part JSON, and optionally injects missing tool IDs at `:244`. The actual ID helper at `:824` only inserts `functionCall.id`; it does not modify `thoughtSignature`.
- Sync private-response handling clones the inner response at `private_envelope.rs:120`, or aggregates chunks at `:135`. It does not decode/re-encode signature bytes. Sync response postprocessing at `:923` handles response IDs/tool IDs, not thought signatures.
- Public JSON-array mode is distinct: `stream_rewrite.rs:84` selects canonical `Standard` when a private envelope exists. Gemini stream state/aggregation has explicit signature storage and re-emission (`generate_content/stream.rs:238`, `:385`, `:717`; `shared/sync_products.rs:3683`, `:3910`). Do not claim every output mode is byte-transparent merely because the ordinary SSE path unwraps raw JSON.
- The canonical request emitter preserves `extensions.gemini.thoughtSignature`/`thought_signature` at `generate_content/request.rs:547`, only using `skip_thought_signature_validator` for an unsigned first synthesized tool call at `:557`. Changing that fallback would not fix the supplied same-format input.
- The stream ID injection is an existing replay-part mutation worth remembering if a future *Antigravity-origin* signed-history repro appears. It is not the established cause here: the five traced signatures came from another provider, and the current investigation did not prove signatures bind IDs.

### Runtime evidence supplied by the independent live researcher

No secret values were requested or copied into this artifact.

- Parent supplied trace `9d378f2b-4026-4325-9f36-881a22662817`, 2026-09-09 14:56 +08: public `/v1beta/models/gemini-3.8-flash:streamGenerateContent` to Antigravity private daily-cloudcode-pa; 17 HTTP 400 attempts across 16 keys, then non-Antigravity success.
- Older trace `2405260a-3f89-46df-98d3-30e774c98f59` contained 16 `functionCall` signatures. Live peer subsequently reported both investigated traces have the same signatures at input and final fallback body.
- All 16 are canonical standard Base64; decode exactly once to binary, not another Base64 string. First decoded byte is `0x01`; decoded lengths are 142–3617 bytes. Full protobuf parsing from byte zero fails with field zero, and tested offsets did not yield complete protobuf either.
- Five historical signatures at `contents[25]/[27]/[29]/[31]/[33].parts[0]` equal earlier successful provider response signatures and client-visible response signatures exactly. Those responses were from the configured provider named `apiyi claude 满`, with `modelVersion=gemini-3.8-flash`, not Antigravity. The first 11 signature origins were not traced by this researcher.
- A provider-emitted value is not necessarily a direct Google-issued value: an intermediary can transform it. These facts establish provenance at the Aether provider boundary, not Google's internal issuer/key family.

### Primary external references and versions

1. [Google Gemini v1beta discovery document](https://generativelanguage.googleapis.com/$discovery/rest?version=v1beta), fetched 2026-09-09: `schemas.Part.properties.thoughtSignature` is `type: string`, `format: byte`, described as an opaque signature reusable in subsequent requests. It documents neither protobuf contents nor a universal public↔cloudcode codec. Treating it as arbitrary opaque bytes is the published API contract.
2. [Google thought-signatures page](https://ai.google.dev/gemini-api/docs/thought-signatures), fetched 2026-09-09: the page now says it moved to the [Thinking guide](https://ai.google.dev/gemini-api/docs/thinking); page date is 2026-08-18. The current guide focuses on Interactions API, explains that generateContent signature placement differs, and says stateless thought blocks must be replayed unchanged. Its Interactions model-switch statement must **not** be generalized to private cloudcode generateContent.
3. [Google Gemini CLI converter](https://github.com/google-gemini/gemini-cli/blob/ed2ac40df67a319bf348bd7e3d10494696b31b38/packages/core/src/code_assist/converter.ts#L129), pinned inspected tree `ed2ac40df67a319bf348bd7e3d10494696b31b38`: Code Assist request conversion delegates contents through part conversion, and response conversion assigns the upstream candidates directly (`:155`). `toPart` (`:241`) preserves part fields except a separate thought/count-token adaptation. There is no thought-signature byte codec in this converter. This is Google's implementation but Code Assist is not a formal contract for Antigravity's private route.
4. [CLIProxyAPI Gemini→Antigravity request translator](https://github.com/router-for-me/CLIProxyAPI/blob/7fac6b15bcfe5ea55c18c9eaec8e5b7e6457d974/internal/translator/antigravity/gemini/antigravity_gemini_request.go#L152), pinned inspected commit `7fac6b15bcfe5ea55c18c9eaec8e5b7e6457d974`: Gemini-target requests call `SanitizeGeminiRequestThoughtSignatures`, not an unwrap/wrap routine. Claude-target behavior is separate.
5. [CLIProxyAPI Gemini signature validator](https://github.com/router-for-me/CLIProxyAPI/blob/7fac6b15bcfe5ea55c18c9eaec8e5b7e6457d974/internal/signature/gemini_validation.go#L425): decodes Base64; recognized replay form is an exact protobuf field 2 bytes containing an exact field 1 bytes (`:484`); opaque payload normally begins `0x01` (`:515`) or is a wrapped UUID. Its comments describe provider observations, not a decrypting verifier or Google's normative schema. Raw `0x01...` is therefore **unrecognized by this implementation**, not proven invalid everywhere.
6. [CLIProxyAPI compatibility decision](https://github.com/router-for-me/CLIProxyAPI/blob/7fac6b15bcfe5ea55c18c9eaec8e5b7e6457d974/internal/signature/provider_compatibility.go#L429): recognized Gemini signatures are returned intact (`:441`); it does not remove the protobuf wrapper or add one around raw state. Incompatible Gemini-target signatures select a skip sentinel (`:271`). [Its sanitizer](https://github.com/router-for-me/CLIProxyAPI/blob/7fac6b15bcfe5ea55c18c9eaec8e5b7e6457d974/internal/signature/gemini_sanitize.go#L26) applies that to the first function call and removes incompatible later/model signatures. This is a lossy compatibility policy, not the requested lossless fix.
7. [CLIProxyAPI Antigravity executor tests](https://github.com/router-for-me/CLIProxyAPI/blob/7fac6b15bcfe5ea55c18c9eaec8e5b7e6457d974/internal/runtime/executor/antigravity_executor_signature_test.go#L808) construct the wrapped protobuf signature and assert the exact full value is transmitted upstream (`:846`). This is mock-test evidence for its preservation contract, not proof that Google's live servers accept an arbitrary synthesized payload. [Response translator](https://github.com/router-for-me/CLIProxyAPI/blob/7fac6b15bcfe5ea55c18c9eaec8e5b7e6457d974/internal/translator/antigravity/gemini/antigravity_gemini_response.go#L164) extracts `response` and adjusts unrelated metadata/function names; no signature codec was found.

### Smallest candidate repair and evidence gate

If an approved controlled replay proves raw provider bytes become acceptable merely by adding the known wrapper, the potential byte-preserving transformation is:

`B = base64_decode(signature)` → `inner = tag(1, bytes) || varint(len(B)) || B` → `outer = tag(2, bytes) || varint(len(inner)) || inner` → `base64_encode(outer)`.

This is an **experiment specification**, not an implementation recommendation yet. The encrypted byte sequence B is preserved, but adding a wrapper is only semantically lossless if the target expects that exact wrapper around the same state. A `0x01` prefix alone is insufficient to establish that expectation or discriminate a valid future raw private signature. Unwrapping already-containerized signatures is contradicted by the inspected independent implementation.

Minimum evidence needed before choosing that code change:

- A fresh Antigravity-generated function-call signature and native replay success to establish the target's real current shape.
- Controlled comparison of the same minimal tool history: original raw signature versus wrapped signature, on the same authorized target/model/key, without dumping raw data. If native control fails, the experiment cannot identify representation as the only defect.
- Prefer a fresh tiny history over resubmitting the large production conversation. Any live inference/probe can incur usage and send conversation data; main session must confirm the authorized scope before it.
- If original fails and wrapped succeeds, add only the confirmed Antigravity boundary translation and a real criterion distinguishing that source representation. Preserve native opaque signatures, both envelope inputs, tool order/args/names and all other JSON. Existing `base64` dependency is already declared at `crates/aether-provider/transport/Cargo.toml:19`; a general protobuf framework is not justified for two proven length-delimited fields.

If the only verified remedy is bypassing signature validation or skipping Antigravity after its first deterministic rejection, those are different decisions: bypass discards reasoning state; request-scoped provider exclusion preserves the body but changes candidate/failure policy and does not make Antigravity accept the history. Obtain the product choice instead of silently folding either into a wire-normalization patch. Account stickiness is not established by these observations.

### Proposed focused validation after the evidence gate

- Existing `antigravity/request.rs` inline tests are the natural home: 16-call/multi-turn synthetic incident shape, proven transformed representation if applicable, exact native-signature preservation, unknown opaque unchanged, unsigned/sentinel unchanged, public body and native envelope, input immutability, and existing schema cleanup.
- Existing `same_format_provider/mod.rs` Gemini tests should assert public same-format opaque signature preservation; do not make the global builder apply Antigravity translation.
- Keep `registry.rs::gemini_tool_signature_roundtrips_through_openai_responses_history`, `post_call_gemini_signature_carrier_replays_to_previous_function_call`, and the current Gemini stream signature regression (`generate_content/stream.rs:1411`) green. Only expand response tests if the confirmed fix changes response adaptation.
- Docker/builder only: scoped `cargo test -p aether-provider-transport --lib antigravity::request`, relevant `same_format_gemini` tests and `aether-ai-formats` signature filters, plus scoped formatting. Verify nonzero selected test counts. This research ran no compiler/tests and claims no validation PASS.

### Related specs

- `.trellis/workflow.md`: research artifacts persisted; implementation waits for this task's explicit evidence gate, not a new generic approval ritual.
- `.trellis/spec/aether-provider-transport/backend/antigravity-tool-schema-contract.md`: adapt only the private owner, preserve unknown/literal tool data, both input forms and immutability; retain the already-fixed `$schema` behavior.
- `crates/aether-ai/formats/AGENTS.md`: runtime same-format traffic must not go through canonical conversion; opaque/unknown fields must survive.
- `.trellis/spec/guides/cross-layer-thinking-guide.md`: keep request, response and replay representation contracts explicit.
- `.trellis/spec/aether-provider-transport/backend/index.md`, `.trellis/spec/aether-ai-formats/backend/index.md`, `.trellis/spec/aether-gateway-execution/backend/index.md`, `.trellis/spec/aether-gateway/backend/index.md`: reviewed; generic guideline entries are largely placeholders, so concrete checked-in contracts and source are stronger evidence.

## Caveats / Not Found

- No public Google specification for Antigravity thought-signature protobuf framing was located; Google discovery deliberately calls the field opaque.
- No lossless raw↔wrapped Gemini signature implementation was found in the inspected primary implementations. Their signature rejection/sentinel policy must not be mistaken for evidence that arbitrary raw state can be repaired.
- No account/project/session binding proof, no direct decryption validation, and no current Antigravity-native successful signature/control sample were available to this researcher.
- The exact currently supplied 400 error establishes incompatibility but not which of representation, issuer keys, model generation or signed-history association is responsible.
- `web__run` failed with upstream HTTP 503; primary documents/source were retrieved with direct read-only HTTPS requests. URLs and pinned upstream SHAs are recorded above. The thought-signatures URL has moved, so older page-specific claims were not reused as current text.
- Initial memory lookup was used only as a pointer to the same-format preservation/verification convention (`MEMORY.md:4876`, `:4885-4886`); actual code was checked anew. No memory was edited.
- Research-only role forbids git operations, so history/ancestor facts remain explicitly parent-supplied. Only this research file was written; no product file, task status, spec, process or remote environment was changed.
