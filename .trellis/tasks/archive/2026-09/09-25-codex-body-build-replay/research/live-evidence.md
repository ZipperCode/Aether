# 154 请求体构造失败只读回放

核查时间：2026-09-25 05:01 UTC 后。生产仅执行只读 Docker 状态、SELECT 和定时段日志检索；数据库会话强制 read-only。

- 当前镜像：`ghcr.io/zippercode/aether:0.7.39`，源码 `22124fe5984ea1fb52c8d6b3195b999d9a1c6fbb`，启动时间 `2026-09-25T04:44:53Z`，健康。
- 三次请求时间：北京时间 12:58:22–12:58:23。
- 请求 ID：`cc588981-e46c-459a-ba3f-2f76b1669545`、`43eb7125-2ee1-42ca-9ac2-08225e51749e`、`7201d89c-7740-4d91-9232-aeff2aa004cb`。
- 模型均为 `gpt-image-2.5-flare`，源/目标格式均为 `openai:image`。
- Codex 候选 Key `06770cbe…` 均 `skipped / provider_request_body_missing`，`started_at` 和上游 status/error 均空，说明未向 Codex 发起请求。
- failure_diagnostic 只保留 `path=$`、`stage=request`、通用提示、source_format/target_format 和 safe_to_show；没有具体字段或源码来源。
- 相同请求最终由 `sub.muxing.cfd` 返回 HTTP 200，target_model 同为 `gpt-image-2.5-flare`。这更支持 Codex 专用构造限制，不足以证明 JSON 无效；不能将公共 size/quality 校验缺口直接当作本次根因。
- 三次 `usage_http_audits` 的请求/上游请求/响应 capture state 均 disabled，mode=none；usage 旧正文列为空，也没有对应 body blob。Content-Type 是 application/json。
- 图片 Endpoint 没有 body_rules，模型没有额外映射或相关 config；该时段结构化容器日志未找到三次 request ID。

## 13:11–13:17 继续只读核查

- `sub.muxing.cfd` 已由用户于 13:08:53 禁用。13:09:22.737 的 `ce2a2d23-da93-4c11-9058-fed464682657` 和 13:09:35.823 的 `525b1b29-b692-41b6-8b13-6f188d5eee27` 均只有 Codex skipped/provider_request_body_missing 候选，started_at NULL，usage 和 usage_http_audits 均无对应记录。
- 容器日志是纯文本格式，之前只筛 JSON 日志不完整。实际日志确认两次均 POST `/v1/images/edits`，缓冲正文分别 1,290,549 / 1,290,464 bytes；访问日志记录 status_code=200、execution_path=execution_runtime_sync、request_id=-。这不是上游成功证据。
- 13:06:17 的 `5a618313-0cff-4e2f-be6c-450639994d48` 已有正文采集，request_body/provider_request_body gzip blob 均 487 bytes，JSON 参数为 model=gpt-image-2.5-flare、n=1、size=1536x1024、output_format=png、prompt 字符数137。提示词及图片内容未输出或保存。
- Codex 对这次相同请求仍 provider_request_body_missing，custom 200。共享 normalizer/common projector 接受上述参数；Codex 后置 allowlist 独立拒绝 output_format，已具备真实字段证据与无敏感信息的最小回归输入。
- 13:09 编辑请求无正文审计可恢复；不能声称其正文必然与13:06相同。补齐全跳过场景的失败记账后，同类诊断将保留既有捕获策略允许的请求正文。
- 新版 Codex Search 于13:03–13:04 已有三次 HTTP200 记录；先前11:47失败属于部署前旧版，不归入当前图片问题。

## 最小回放参数

```json
{"model":"gpt-image-2.5-flare","prompt":"a blue square","n":1,"size":"1536x1024","output_format":"png"}
```

原始提示词由占位描述替换，其他影响构造的字段保留。编辑场景使用本地测试图片引用验证共享构造器，不伪称为13:09原文。

## 独立确认的代码问题

`sanitize_request_candidate_extra_data_for_persistence` 的 failure_diagnostic 字段名单省略了内部固定 `kind` 和 `source`，导致构造端携带的 `codex_openai_images_request_contract` 等来源落库后消失。后续修复应保留可安全展示的结构化诊断，不依赖开启原始正文采集。
