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

## Codex OAuth 套餐、模型与 Endpoint

Codex 文本 `/models` 目录不是完整图片目录。Aether 从该提供商已启用的图片模型配置、真实图片 Endpoint 绑定及图片能力补全管理发现结果，不修改原生 Codex 模型卡或版本化目录。默认 `gpt-image-2` 只处理请求省略型号的情况，不再代表完整支持列表。

- 启用 `openai:image` Endpoint，默认 base URL 为 `https://chatgpt.com/backend-api/codex`；配置真实图片型号及其生图能力和图片绑定。后续同协议的新型号只需配置并刷新，保留真实上游名称，不映射为文本宿主。
- Key 的显式 API 格式权限、include/exclude/locked 规则继续生效。旧迁移将文本型号关联图片 Endpoint 的记录，不会因此自动变成图片模型。
- 已知 Free Codex OAuth 不允许生图，返回独立原因 `codex_plan_image_generation_unsupported`。Plus、Pro、ProLite、ProMax 和未知套餐不被固定套餐名单额外排除；每个型号的实际权限和额度仍由原有规则与上游决定。官方 [图片能力判定](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/core/src/tools/spec_plan.rs#L727) 明确区分 Free，不能将套餐展示名称当作原始身份。
- 启用自动模型获取的旧 Key，需要强制刷新一次或等待自动刷新，才能修复持久化白名单与 Endpoint 关联；只查看发现列表不会改写手工白名单。手工白名单仍需显式允许目标型号，显式空列表表示禁止。

| Endpoint | 发现与调用边界 |
| --- | --- |
| Responses | 保留原生 HTTP/SSE/WebSocket、实际模型及认证契约 |
| Compact | 真实发现的文本型号关联已启用且 Key 允许的 Compact，调用原生 `/responses/compact` |
| OpenAI Search | 真实发现的文本型号关联已启用且 Key 允许的 Search；使用同步 JSON `/alpha/search`，不改成 Responses 文本搜索 |
| Images | 配置驱动的图片型号，调用原生 `/images/generations` 或 `/images/edits`，保留模型限制和套餐检查 |
| Live | 保留显式模型绑定和 OAuth WebRTC 调用；没有专属能力证据时不从文本目录自动生成 Live 绑定，也不将普通模型测试冒充 Live 验证 |

Responses 的既有格式权限覆盖 Search/Compact，人工停用的绑定保持停用。原生卡片 `supports_search_tool` 表示工具发现，不能用它推断网页搜索权限；官方 [独立网页搜索选择](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/core/src/tools/spec_plan.rs#L1038) 使用 Provider 网页搜索能力和客户端功能开关。

图片与 Search 仍执行认证、Key 模型限制、额度和健康检查。可路由不等于上游必定授权，429 额度耗尽也不等于型号不支持。真实上游权限变化、型号协议或参数变化须依据返回证据适配；不隐式更换协议或伪造成功。

Codex Images 生成和编辑保留通过共享图片校验的 `output_format`（`png`、`jpeg`、`webp`），不因客户端 DTO 缺少该字段而跳过账号。图片同步 JSON 心跳只用于存在可执行候选的请求；全候选跳过时返回正常错误并记录失败 usage，保留已配置的正文采集策略。
