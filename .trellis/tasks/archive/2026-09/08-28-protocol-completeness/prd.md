# 完善 Gemini 与 Anthropic 协议转换

## Goal

在不改变 Aether 现有同格式透传、候选路由和 fail-closed 总体设计的前提下，补齐 Gemini 与 Anthropic Messages 的关键协议语义，优先消除错误拒绝和跨格式响应静默丢字段，使协议转换行为与当前官方契约及 AxonHub 中可借鉴的成熟实现一致。

## Background

- 审查基线：Aether `81c841be4`，AxonHub `4483c2e4`。
- Aether 的 Gemini HTTP surface 已覆盖 generate/stream、models list/detail、countTokens、embedding、Files、video 与 Interactions，端点范围不弱于 AxonHub。
- 主要缺口集中在跨格式语义和 wire behavior：Gemini penalties/logprobs 被错误判定为不支持；Gemini/Claude provider response extensions 没有统一 fail-closed；Gemini 无 ID function call/result 可能失配；Gemini stream 未区分 SSE 与非 SSE JSON array。
- Aether 文档明确不承诺覆盖厂商全部管理类 API，因此 CachedContents、Gemini Live、Anthropic Files/Message Batches 不应自动进入本任务。

## Requirements

- R1：保持同格式运行时路径的原始字段保真、模型映射、认证隔离和现有 provider compatibility 行为。
- R2：Gemini `GenerationConfig` 显式解析并生成 `presencePenalty`、`frequencyPenalty`、`responseLogprobs`、`logprobs`，并修正跨格式 target capability 校验。
- R3：Gemini 与 Claude 的跨格式响应必须对 provider-specific extensions 做完整审计；已映射字段显式消费，未映射字段返回结构化 fail-closed 错误，不得静默丢弃。
- R4：修复 Gemini `FunctionCall.id` / `FunctionResponse.id` 缺失时的跨轮关联，保证 tool call/result 使用一致的 canonical ID。
- R5：保留现有 thinking、thought signature、redacted thinking、tool use/result、cache usage 与多模态转换行为。
- R6：同步更新格式覆盖矩阵生成规则或相关协议审计文档，使声明与实现一致。
- R7：实现保持局部、简单；不引入新的公共 API format、数据库变更或跨模块重构。
- R8：Gemini `streamGenerateContent` 按公开请求的 `alt=sse` 选择 wire format：显式 SSE 时返回 SSE，未指定 SSE 时返回流式 JSON array；内部 `/v1internal:streamGenerateContent` 保持现有行为。
- R9：Gemini models list/detail 不再发布无数据依据的固定 token/sampling 数值；`supportedGenerationMethods` 必须反映 Aether 对该目录模型实际可提供的 generate、stream 与 countTokens 能力。

## Constraints

- 不提交 Git commit。
- 不运行大模块编译。
- 不新增单元测试；只使用现有测试、格式检查、静态检索和目标 crate 的轻量验证。
- 不实现 CachedContents CRUD、Gemini Live/bidi、Anthropic Files、Message Batches、Skills 等新产品 surface。
- 不照搬 AxonHub 的可选 raw-pass-through 或未知字段静默丢弃策略。

## Acceptance Criteria

- [x] AC1：OpenAI/Claude → Gemini 的 penalties/logprobs 在语义可表达时不再被错误拒绝，输出 Gemini 官方字段名与类型。
- [x] AC2：Gemini → OpenAI/Claude 的 grounding、citation、safety、logprob、usage detail 等未映射响应字段不会静默丢失；映射或 fail-closed 结果可从现有转换入口观察。
- [x] AC3：Claude → OpenAI/Gemini 的 citations、provider response metadata 和 usage 扩展同样满足映射或 fail-closed。
- [x] AC4：无显式 ID 的 Gemini function call/result 在多轮 contents 中生成并复用一致 ID；显式 ID 保持不变。
- [x] AC5：Gemini/Claude 同格式请求和响应的未知字段仍可按现有 native/runtime 路径保留。
- [x] AC6：`cargo fmt --all --check` 或等价局部格式检查通过；目标文件 `git diff --check` 通过；不执行大模块构建。
- [x] AC7：协议覆盖文档不再把 Gemini 已支持的 penalty/logprob 字段描述为目标不支持。
- [x] AC8：公开 Gemini `streamGenerateContent?alt=sse` 返回 `text/event-stream`；未指定 `alt=sse` 时返回 `application/json` 且 body 是逐块组成的合法 JSON array；两种模式都保持稳定 response ID、tool arguments、finish reason 与 usage。
- [x] AC9：Gemini models list/detail 不再返回统一伪造的 `128000/8192/temperature/topP/topK`；目录只发布有事实来源的字段，并正确声明 `streamGenerateContent`。`countTokens` 仅在该模型存在可执行的 native Gemini count-token candidate 时声明。
