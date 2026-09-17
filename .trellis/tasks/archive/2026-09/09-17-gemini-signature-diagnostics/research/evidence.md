# Verified incident evidence (2026-09-17, Asia/Shanghai)
- Local starting HEAD/origin/master: 0c873852d4360916390030bf9cc795f25abc9759; clean.
- Remote 192.168.2.212 aether-app: v0.7.35, revision 52df0e6243683f0d85eb84a32b03048f480c4bec; healthy.
- Client user agent: opencode/1.18.29 ai-sdk/provider-utils/4.0.27 runtime/bun/1.3.14.
- First affected request: 6450e576-7d69-4246-b226-1019c83d482c at 09:33:28; 39 requests subsequently succeeded through apiyi failover. Six final failures occurred 10:16–10:32.
- Representative failed request: e46201d9-10cf-46ad-a327-52a058e2fb89.
- Native Gemini input contains contents[151].parts[0].thoughtSignature, a 16,766-character string with only Base64 alphabet characters, length mod 4 = 2 and non-canonical trailing bits. Do NOT reconstruct by padding or re-encoding.
- Google HTTP 400 error begins: Invalid value at 'request.contents[151].parts[0].thought_signature' (TYPE_BYTES), Base64 decoding failed for ... . Ordinary Gemini error omits request. prefix. INVALID_ARGUMENT; error echoes the long signature in message/details. Candidate stored response can be a truncated STRING rather than parsed JSON. Verify actual stream/sync recognition before diagnostic truncation with a synthetic >16 KiB rejection.
- Across six failures Antigravity attempted 198 times/36 keys, all Base64 400; ordinary Google 210 equivalent errors. This is not evidence that rotating keys repairs malformed history.
- Original generating request b728d843-d204-40ec-ad53-4893e5d6ecd0 at 09:33:12 has all body states truncated, no body refs/blobs. Origin of signature damage remains unproven.
- Read-only follow-up found 87 persisted Antigravity recoveries (`Corrupted thought signature` -> `recovered` -> HTTP 200) on v0.7.35 between 2026-09-16 21:18:20 and 22:06:45. This supports reusing the existing private compatibility sentinel, but does not constitute production acceptance of the new Base64 error branch.
## Sources
- https://github.com/fawney19/Aether/pull/830 — upstream bounded FULL-body retention; merged as 364692da55eb22adc42ca2649c18deed84671b0f; work commit 5842c7232ecd6cdfa95994da9afaac14f5a8deb0.
- https://github.com/openclaw/openclaw/pull/82995 — merged malformed history signature handling; do not copy speculative prefix heuristics.
- https://github.com/vertesia/llumiverse/pull/470 — merged streamed-fragment repair for that library; not proof of OpenCode cause here.
- Upstream #818 concerns false 429, #831 Google Search, #832 Claude schema/thought replay. They are out of scope.
## Validation constraints
Use Docker Rust 1.95.0; root owns builder/cache scheduling. No live model requests. Existing shared cache volumes: aether-trellis-target, aether-trellis-cargo-registry, aether-trellis-cargo-git. The old aether-gateway-builder:trellis-check image is absent; recreate a task-scoped builder from available pinned Rust image as needed. Do not touch unrelated containers.
