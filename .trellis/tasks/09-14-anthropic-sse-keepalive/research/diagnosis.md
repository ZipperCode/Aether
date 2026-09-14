# Read-only diagnosis (2026-09-14)

## Evidence
- Main checkout: master at 0d3ceb46508406052916936ec19ecb08188cf4c5, 22 dirty status entries from prior Antigravity work. Byte hashes of the 33 individual WIP files are in main-wip-baseline.json.
- Existing production instance: 192.168.2.212:8084, image 0.7.33, revision c2c30e60d3543fbde19ebc6e1d9f083033649ee0. No writes, restarts or new inference were performed.
- GLM BigModel CN uses native claude:messages -> claude:messages, https://open.bigmodel.cn/api/anthropic/v1, no format conversion or body rules. Recent three-day snapshot: 771 completed HTTP 200 requests, zero failed usage rows.
- Claude 2.1.236 session e8fbdc1c-2669-439e-a8f9-78243761d254 recorded streaming fallback SyntaxError and JSON Parse error: Property name must be a string literal. Captured requests had 80 loaded tools and no deferred definitions; this is distinct from the historical OpenRouter deferred-tool error.
- Request 01e50ccc-4caa-407d-aa88-a833172afecb: created 22:18:46+08, first byte 4207 ms, client error at 22:19:35.128+08, about 44921 ms after first byte. Request 492bf960-ffaa-4f42-a217-d1278b8906c4: created 22:19:51+08, first byte 6964 ms, error at 22:20:13.295+08, about 15331 ms after first byte. Created timestamps are second-resolution, so these are approximate correlations, not packet captures.
- In a 100-response snapshot, all 84 streamed upstream/client JSON captures were equal; concatenated tool JSON was valid. A later offline replay of 100 stream captures / 137 tools through the current official Anthropic SDK partial parser found zero errors.
- Both inspected runtime revisions insert a timer comment unconditionally in build_sse_body_stream. Raw fragments pass through its input channel. Client audit capture happens before this wrapper, so it misses the injected bytes.

## Minimal reproduction
Original fragments `event: content_block_delta\ndata: {` and the remainder of a valid event parse normally when joined. Insert `: aether-keepalive\n\n` between them and JSON parsing fails at object property position. Place the same comment after the complete record and decoding is unchanged. This establishes a gateway framing defect; production raw-wire A/B remains unverified.

## Reference boundaries
- Official native endpoint: https://docs.bigmodel.cn/cn/guide/develop/claude/introduction.md
- Reasoning/tool replay requirements: https://docs.bigmodel.cn/cn/guide/capabilities/thinking-mode.md
- new-api native Zhipu adaptor: https://github.com/QuantumNous/new-api/blob/9fe0457ee1f4b9de407a254500d54f5a8f41ee29/relay/channel/zhipu_4v/adaptor.go#L30
- Anthropic streaming: https://platform.claude.com/docs/en/build-with-claude/streaming

Do not apply cross-format tool rewrites or remove thinking/defer_loading to repair this shared byte-framing issue. Validate the final HTTP response, not only stored audit events.
