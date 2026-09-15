# Research: Live Antigravity signature representation and provenance

- Query: Do the two observed Antigravity failures contain modified, double-encoded, wrapped, or cross-provider thought signatures? What is directly proven by existing capture data?
- Scope: internal; read-only production evidence on `192.168.2.212` and a narrow local transport-owner inspection.
- Date: 2026-09-09
- Authorization: existing SSH access, PostgreSQL `SELECT` only, in-memory gzip/JSON/Base64 analysis. No inference requests, credential extraction, production configuration changes, restarts, or remote/local payload artifacts.

## Findings

### 1. Confirmed trace outcome and capture ownership

| Trace | Created at (+08:00) | Antigravity attempts | Distinct Antigravity keys | Final outcome |
| --- | --- | ---: | ---: | --- |
| `2405260a-3f89-46df-98d3-30e774c98f59` | 2026-09-09 13:56:25 | 17 | 16 | HTTP 200 via `apiyi claude 满` |
| `9d378f2b-4026-4325-9f36-881a22662817` | 2026-09-09 14:56:18 | 17 | 16 | HTTP 200 via `apiyi claude 满` |

- Every Antigravity attempt is `status=failed`, `status_code=400`, `error_type=retryable_upstream_status`, with `extra_data.upstream_response.body.error.message = Corrupted thought signature.`
- Antigravity provider: `4b872ec3-8fe8-4ea4-a348-c7ef5ef1b3b9`; endpoint: `66830571-7738-49c8-a43a-fc6146b751d6`.
- Successful fallback provider: `e6308193-0f45-4f98-bfef-0ca8a4339e9e`.
- Both usage rows record `model=target_model=gemini-3.8-flash`, `api_format=endpoint_api_format=gemini:generate_content`.
- Each trace has exactly four stored body fields: `request_body`, `provider_request_body`, `response_body`, `client_response_body`.
- Both request captures have public Gemini root keys `contents`, `generationConfig`, `systemInstruction`, `tools`; neither has a private `request` envelope.
- **The stored `provider_request_body` is the final successful fallback body, not a capture of any failed Antigravity request.** Equality against it does not prove byte equality of the failed private wire.
- Failed candidate `extra_data` keys are `client_api_format`, `error_flow`, `gateway_execution_runtime`, `phase`, `pool_key_index`, `priority_mode`, `priority_slot`, `provider_api_format`, `ranking_index`, `ranking_mode`, `upstream_response`, `upstream_url`. A bounded structural walk through four nesting levels found only `upstream_response.body` among request/body/model/signature keys. No failed request-body capture was found there.

### 2. Exact request-history signature preservation

- The older request has 38 contents and the newer has 39; both have 16 `functionCall` parts, all under role `model`, all with a part-root camelCase `thoughtSignature`, plus 16 `functionResponse` parts.
- For each trace, the complete ordered list `(JSON path, exact signature string, functionCall marker)` from `request_body` equals that from final `provider_request_body`: **16/16 exact matches**.
- The two trace input lists also equal one another exactly: **the same 16 signatures were replayed again roughly one hour later**.
- No signature in this inspected list was missing, snake_case, attached to a non-functionCall part, or replaced with a sentinel.

All following paths are `contents[N].parts[0].thoughtSignature`. SHA-256 is computed over the original UTF-8 Base64 string, not over decoded binary.

| N | Base64 characters | Decoded bytes | SHA-256 of original string |
| ---: | ---: | ---: | --- |
| 1 | 2704 | 2027 | `579bd05b060595490a29abb40c8a3f2431ceac6831680a51625f0000a0c4912f` |
| 3 | 892 | 667 | `9d0d2e2068e5173da6586c4e1c15fbec6114b0ce95bcc10c483fa0d42c274707` |
| 5 | 200 | 149 | `a2552712317db0268417ff73cd6a414e0182865a633b9f97ba1275183d6b8b98` |
| 7 | 920 | 689 | `6373ea8644cb6b2a8903b185b1c328fc553ea94bed8b4ac294658de53c44d09c` |
| 9 | 312 | 234 | `adff488da73fd2daa66829e109560d94de7313d614df21af67d8588e7ab62ac3` |
| 11 | 244 | 183 | `013baad003d063a33b486db8b29d94ab76260f974d0a4abb7151a0acbc1de3da` |
| 13 | 236 | 175 | `8f1bdc30a2550b0794e8e5137b92e919f260902dfb4140e3bf2b48ed0234cfec` |
| 15 | 756 | 567 | `39faf35882afdaa4114ba5951688cb259996f63e7f0bc984860cfd0865468eb9` |
| 17 | 4824 | 3617 | `692ebdf2544d6bc65d7a9ba13b630854c01447d1e21c28f43d7dcd122783e364` |
| 21 | 2016 | 1510 | `75aef6cd778bb009f10543644274267f446b1261f555f4d81680e2199b4b659f` |
| 23 | 248 | 185 | `07810a83b42c59c3aceeb5b4a9d7effc7ab30c2f82428b09ad2ed4608aad9249` |
| 25 | 220 | 165 | `df07f6974ce3b20727525b2dbc2034de8bff82f78a81b1721366ce68c514c460` |
| 27 | 220 | 165 | `637f32ddcbd4b58cf9bb894b3de4ca5c312eed76c70a5c2a77625834292f8abc` |
| 29 | 384 | 288 | `6ac70aed58547285a8dc1c6886fa22d2c4748ace308b9e021c3302eaa07387b8` |
| 31 | 192 | 142 | `0335daca943fb1586adbfb113b4803301bc130ab1c76bdc3f942d57a2d2ee363` |
| 33 | 216 | 162 | `78711635828cc008142f495c08bf7955a566a436b4c1d81945cd146fae554e8f` |

### 3. Representation: opaque binary, not demonstrated wrapper corruption

For all 16 unique signatures:

- Strict standard Base64 decode succeeds.
- Re-encoding the decoded bytes yields the exact input: canonical standard Base64, with no whitespace or alternate spelling.
- Decoded bytes do not match the Base64-text alphabet; **no ordinary double-Base64 encoding is present**.
- Decoded first byte is `0x01` for 16/16; decoded length is at least five bytes for 16/16; there are no leading zero bytes.
- Interpreting byte zero as a protobuf tag would yield field number zero, which is not a valid protobuf field. There is therefore **no top-level protobuf field-2/field-1 wrapper recognizable by this parser**.
- As a bounded prefix check, starting a protobuf wire parser at offsets `1, 2, 3, 4, 5, 8, 16, 32` also produces no complete parse for any signature. Only field numbers, wire types, and lengths were examined; no encrypted/private thought content was interpreted.

**Do not infer cryptographic invalidity from these parse results.** An opaque binary signature need not itself be a protobuf message. These results neither establish that an outer wrapper is missing nor justify stripping, adding, or replacing bytes. The `0x01` prefix is structural evidence only, not proof of a specific cryptographic format or key binding.

### 4. Direct signature provenance: five earlier fallback responses

Selection was bounded: for each target trace, the previous five completed requests with the **same `api_key_id` and same model**, within the preceding 24 hours. The union contained five earlier requests and the older target trace. The internal client identifier was used only as a SQL equality condition, not extracted or published.

All five earlier requests have `provider_name=apiyi claude 满`, HTTP 200, `model=target_model=gemini-3.8-flash`, `api_format=gemini:generate_content`; captured response `modelVersion` is also `gemini-3.8-flash`.

Each earlier response contains two stored JSON chunks and exactly one thought signature. Each signature is at `chunks[0].candidates[0].content.parts[0].thoughtSignature` and is byte-for-byte equal to the input-history signature below. For each earlier trace, the response and client-response signature lists also match exactly.

| Earlier response trace | Created at (+08:00) | Exact target-history match | Characters |
| --- | --- | --- | ---: |
| `d804380f-ac35-414a-b851-0f9f0064241b` | 2026-09-09 10:32:40 | `contents[25].parts[0].thoughtSignature` | 220 |
| `bfffc654-3bdc-4415-84a9-7fd02e6c30b1` | 2026-09-09 10:32:53 | `contents[27].parts[0].thoughtSignature` | 220 |
| `44b9fa33-bdef-4fe9-8103-64fff6c4098a` | 2026-09-09 10:33:01 | `contents[29].parts[0].thoughtSignature` | 384 |
| `4ef1b52b-083d-4d1a-93c9-0fe1582d9285` | 2026-09-09 10:33:26 | `contents[31].parts[0].thoughtSignature` | 192 |
| `2b94f29c-568c-4eca-a034-cd4bb709be76` | 2026-09-09 10:33:35 | `contents[33].parts[0].thoughtSignature` | 216 |

This directly proves **cross-provider replay provenance for five of the sixteen historical signatures**. These five were not returned by an Antigravity success recorded on those requests. It does not prove which upstream product or exact underlying model the third-party fallback provider uses, nor whether the same signature can ever be accepted by Antigravity.

The two target traces' successful fallback responses contain 113 and 102 JSON chunks respectively, with one 104-character signature each. Neither newly returned signature is in the sixteen historical input signatures; each response/client-response signature pair is exact. Those success responses therefore do not establish Antigravity success or repair of the historical values.

### 5. Files found and code patterns

| File | Purpose |
| --- | --- |
| `.trellis/tasks/09-09-antigravity-thought-signature-fix/prd.md` | Preserve valid signature replay and prohibit remote changes. |
| `.trellis/tasks/09-09-antigravity-thought-signature-fix/design.md` | Evidence gate: withdraw blanket deletion and do not assume corruption/account binding. |
| `.trellis/tasks/09-09-antigravity-thought-signature-fix/implement.md` | Live structural/provenance research before implementation. |
| `crates/aether-provider/transport/src/antigravity/request.rs` | Private request envelope owner, inspected through CodeGraph. |
| `.trellis/spec/aether-provider-transport/backend/antigravity-tool-schema-contract.md` | Existing scoped schema metadata normalization contract. |

- `request.rs:50`: `build_antigravity_safe_v1internal_request` owns conversion into the private envelope.
- `request.rs:77`: native-envelope branch clones the existing inner request, removes model/safety-settings keys, and applies tool normalization.
- `request.rs:100`: public-input branch clones the public request and performs the same narrow removals/tool normalization.
- `request.rs:142`: schema alias normalization operates on tool declarations; it does not explain a thought-signature wrapper mutation.
- Local source inspection supports identifying the owner; **it is not itself a capture of the deployed failed request**.

### 6. Reproduction method and evidence hygiene

- SSH command shape: `ssh -o BatchMode=yes -o ConnectTimeout=8 zipper@192.168.2.212 'python3 -'`.
- The in-memory Python process invoked `docker exec -i aether-postgres psql -X -U postgres -d aether -At -v ON_ERROR_STOP=1`, with an explicit 20-second subprocess timeout and literal scoped SELECT statements.
- Blob data was read with `encode(payload_gzip,'base64')`, decoded in process memory, gunzipped, and parsed as JSON. It was never echoed, saved, or copied to this file.
- Request projection: iterate `body.contents[*].parts[*]`, taking only `thoughtSignature`/`thought_signature` string fields from the part itself. No buggy reference-based recursive PowerShell walker was used.
- Response projection: iterate stored `chunks[*].candidates[*].content.parts[*]`; request-style `contents` traversal would incorrectly report zero response signatures.
- Compared actual strings and paths, not just lengths or hash prefixes. Full SHA-256 values above are supporting evidence, not substitutes for the in-memory equality comparisons.
- No temporary files or persistent processes were created; the diagnostic Python/SSH processes exited.

### 7. Related specs and external references

- Related specs: `.trellis/spec/aether-provider-transport/backend/index.md`, `.trellis/spec/aether-provider-transport/backend/antigravity-tool-schema-contract.md`, `.trellis/spec/aether-ai-formats/backend/index.md`, `.trellis/spec/aether-usage-runtime/backend/index.md`.
- External references: none independently consulted by this live-evidence subtask; the peer source/docs investigation owns primary-source compatibility interpretation. No external claim is inferred solely from the binary prefix.
- Deployment context supplied by root: `aether-app` image `ghcr.io/zippercode/aether:0.7.33`; schema fix `cbf1346fe` is included. This researcher did not re-run image/commit inspection and does not claim the fix is absent.

## Caveats / Not Found

- Failed Antigravity request-wire payload was not found in these existing body captures or bounded candidate metadata. Final fallback payload is not an acceptable substitute.
- Origin of the first eleven signatures was not researched; five direct earlier-response matches already establish cross-provider provenance without a broad historical scan.
- Identical public model labels and response `modelVersion` strings do not prove identical underlying models or signing domains across providers.
- Neither canonical Base64 nor a failed protobuf parse proves a signature cryptographically valid/invalid for Antigravity.
- No live request tested whether adding a wrapper, retaining a wrapper, omitting signatures, or using a sentinel succeeds. No such remote inference request was authorized or performed.
- No evidence here proves that signatures are account-bound, that Aether corrupted them, or that changing accounts can fix them.

## Minimal Next Action

Use the confirmed cross-provider provenance and `0x01` opaque-binary form to evaluate the peer's primary-source representation contract. A lossless boundary fix needs an authoritative format mapping or a targeted synthetic contract check; do not implement guessed unwrapping or blanket signature deletion from this data. If only discarding historical reasoning/signature state or an extra request retry remains, raise that semantic change for the user decision required by the task design. Any production verification/deployment remains outside this subtask's authority.
