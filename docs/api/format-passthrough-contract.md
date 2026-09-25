# Format Passthrough Contract

Last audited: 2026-09-23

This document defines the boundary between runtime passthrough, canonical roundtrip tests, and cross-format conversion.

## Runtime Same-Format Path

Runtime same-format provider paths must not call canonical conversion.

Current implementation:

- `crates/aether-provider/transport/src/same_format_provider/mod.rs` checks `api_format_alias_matches(client_api_format, provider_api_format)`.
- When formats match, the provider body is built by copying the parsed JSON object field-for-field.
- When formats differ, the provider body is built through `aether_ai_formats::convert_request_pure`.
- Model override, body rules, model directives, Claude Code sanitization, Gemini function-response id stripping, and stream policy are applied only after the passthrough/conversion branch in provider transport.

Important limitation:

- The current transport helper receives `body_json: &serde_json::Value`, not raw request bytes. It therefore guarantees no canonical conversion and JSON value preservation at this layer, but it cannot preserve original whitespace or object key order by itself.
- Eligible native HTTP requests now have a higher-level exact-body path. The frontdoor retains the decoded entity bytes in `OriginalRequestPayload`; after every candidate-specific model, body-rule, redaction, compatibility, and encoding decision, the plan reuses those bytes only when the final JSON value is unchanged and no re-encoding is required. Other requests continue through JSON serialization.
- Exact-body reuse is carried through the existing serialized `RequestBody.body_bytes_b64` contract. It preserves normalized JSON whitespace and key order without changing tunnel/remote execution DTOs, but it does not preserve the client's original gzip/zstd octets after frontdoor decoding.

Provider schema drift does not change this rule. If OpenAI, Claude, or Gemini add a new field, same-format runtime routing must still forward it as part of the original provider body. The schema inventory and field coverage matrix are audit aids, not the runtime allowlist for same-format traffic.

## Canonical Same-Format Roundtrip

Canonical same-format roundtrip is only a test/audit mode:

```text
source format -> Canonical -> same source format
```

Required behavior:

- JSON-normalized equality, ignoring object field order and whitespace.
- Field values, array order, unknown fields, extension namespaces, and unknown enum strings must be preserved.
- This path may parse and emit; it is not the runtime path.
- Unknown provider fields are carried in provider extension namespaces and replayed when emitting the same provider format.

## Cross-Format Conversion

Cross-format conversion is strict:

```text
source format -> Canonical -> target format
```

Required behavior:

- Emit only fields valid for the target provider format.
- Map provider-specific enum values through explicit provider enum types.
- Preserve source fields only when the target has an equivalent field or documented extension passthrough.
- Fail closed with `FormatError::UnauditedField`, `FormatError::LossyConversionBlocked`, `FormatError::UnsupportedField`, `FormatError::InvalidEnumValue`, or `FormatError::InvalidTargetField` when no lossless mapping exists.
- Do not use `None` or silent omission to represent conversion failure.
- Newly added provider fields follow the same rule as other unknown fields: preserve same-format, fail closed cross-format with `UnauditedField`. A code change is required only when Aether intentionally supports a new cross-format semantic mapping.

### 运行时转换与纯转换的一致性

`convert_request` 和 `convert_request_pure_with_context` 共用 `validate_cross_format_request_contract`。运行时先执行已有历史展开，再审计规范化后的请求；模型映射、传输流策略与明确的内部投影标记不应被误判为未知业务字段。embedding/rerank 保持各自校验分支，同格式不进入跨格式字段审计。

- Responses 的 `moderation`、`context_management` 等字段没有目标映射时，运行时同样必须返回明确的转换错误，不能成功后删字段。
- Anthropic 原生服务端工具必须有明确的跨格式实现；不能仅因为带有 `name` 就降级为普通函数。`web_fetch_20250910` 即使没有 `max_uses`，也必须拒绝无损转换；普通自定义函数和已实现的工具映射不受影响。
- Provider 的签名、加密推理状态不可仅凭同为字符串就互换；不以放宽安全门的方式让旧有损转换测试通过。

### Gemini 流式函数调用

跨格式回写 Gemini 时，canonical parser 保留实际函数名，不补造 `unknown`。函数名或参数分片尚未到齐可缓冲；终态仍缺失/空白函数名，或非空参数不是完整 JSON 对象时，返回 `AiSurfaceFinalizeError`。错误信息仅包含索引和原因，不包含原始工具参数。

明确结束的合法无参数调用可输出 `args: {}`；截断参数不能伪造成 `{}`。发生转换错误后不得继续补发调用或正常 `STOP`。原生同格式流仍执行原有透传合同。

## Pure Conversion Interface

Pure conversion lives in `crates/aether-ai/formats` and is limited to:

- parse
- emit
- provider-specific field/enum mapping
- `ConversionReport`

Pure conversion must not:

- override `model`
- add, remove, or force `stream`
- apply body rules
- apply model directives
- read the original request body to patch missing target fields
- perform provider transport policy edits

Current pure entrypoints:

- `parse_request_pure`
- `emit_request_pure`
- `convert_request_pure`
- `convert_request_pure_with_context`
- `parse_response_pure`
- `emit_response_pure`
- `convert_response_pure`

`convert_request` and `convert_response` remain legacy wrappers for existing callers that still need mapped model/report-context behavior during migration.

## Codex OAuth 图片模型与调度

Codex 文本 `/models` 目录不返回 `gpt-image-2`，不代表账户没有生图能力。Aether 在管理用发现结果中补全本地已支持的图片模型，不修改原生 Codex 模型卡或版本化目录。

- 提供商类型为 `codex`，启用 `openai:image` Endpoint；默认 base URL 为 `https://chatgpt.com/backend-api/codex`，不要填入完整的 `/images/generations` 地址。
- 全局模型与提供商模型使用 `gpt-image-2`，启用生图能力，绑定 `openai:image` Endpoint，不要把图片模型映射为文本模型。
- Key 的 API 格式显式列表若非空，必须包含 `openai:image`；空列表沿用发现层的继承语义。停用的图片端点不产生补全能力。
- 启用自动模型获取的旧 Key，执行一次强制刷新模型或等待自动刷新：补全 ID 先经过原有 include/exclude/locked 规则，再写入 `allowed_models` 并重新协调提供商模型可用性。显式排除 `gpt-image-*` 仍会阻止调度。
- 关闭自动获取、手工维护白名单的 Key，需自行允许 `gpt-image-2`；仅查看发现列表不会改写该白名单。

请求 `POST /v1/images/generations` 或 `/v1/images/edits` 仍走正常调度和原生 `/backend-api/codex/images/*` 上游，不跳过鉴权、Key 模型限制、额度或健康检查。发现能力不保证真实账户权限，上游拒绝仍按原有错误处理返回。

实现对照：[sub2api 的 Codex Images 直调](https://github.com/Wei-Shaw/sub2api/blob/main/backend/internal/service/openai_images_direct.go) 对 `gpt-image-2` 使用相同路径；[codex-proxy 的 Responses 工具桥](https://github.com/icebear0828/codex-proxy/blob/dev/src/routes/shared/image-generation.ts) 则将文本宿主模型与图片工具分开。本次保留原生图片语义，不引入隐式 Responses 降级。
