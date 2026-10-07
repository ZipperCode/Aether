# 修复 Responses 本地计划失败诊断

## Goal

调查并修复 `no_local_stream_plans` 的候选诊断缺口，保留 Endpoint 级 URL、协议、认证、请求规则和专用操作边界。原始“绑定导致失败”假设未被同 trace 证据证实；生产具体失败分支仍须部署后取证，不通过删除绑定或虚构 fallback 掩盖错误。

## Confirmed facts

- 154 当前运行 `ghcr.io/zippercode/aether:latest`，版本 `0.7.42`，revision `dfe1de0ab62ad83e4eec876306fc424a18a3c32e`，容器 healthy。
- 2026-10-07 近 2 小时日志出现 6,692 次 `no_local_stream_plans`；主要请求为 `POST /v1/responses`、模型 `gpt-6-astra`、`plan_kind=openai_responses_stream`、`candidate_count=1`、`skipped_candidate_count=0`，返回 503。
- 154 的 `gpt-6-astra` Provider Model 当前绑定 `openai:responses`、`openai:responses:compact`、`openai:search` 三个 Endpoint；Codex Provider 有 7 个 active OAuth Key；系统 `enable_format_conversion=false`。
- 候选查询通过 `model_endpoint_bindings` 与 Endpoint 做 inner join，并按请求 API format 精确匹配；绑定不仅是 UI 关系数据，而是正式候选选择的硬约束。
- 绑定仍有必要：不同 Endpoint 可能使用不同 URL、custom path、认证、Header/Body rules、协议能力和操作类型；图片、Search、Compact、Live 不能安全地依赖“Provider 下所有 Endpoint”。
- 自动发现同步 `crates/aether-model-fetch/src/association_sync.rs:134-170` 使用上游模型的 `endpoint_ids` 写入 `source=discovered` 绑定；管理员显式选择使用 `source=manual`，持久层不会用自动同步覆盖 manual 绑定。

## Requirements

- R1. 保留 `model_endpoint_bindings`，不得改为按 Provider 或按 API format 全量推断 Endpoint。
- R2. 修复 Codex/Responses 候选构造失败的真实原因，确保绑定到 `openai:responses` 的可用模型能构造 `openai_responses_stream`；失败时必须保留可区分的候选诊断，而不是只落 `no_local_stream_plans`。
- R3. 自动发现只能写入上游明确声明的 Endpoint 关联；Search、Compact、Image 等专用 Endpoint 不得因为同名模型或模糊推断被误绑定到普通 Responses。
- R4. 管理端 manual 绑定优先级保持不变；自动刷新不得删除或覆盖 manual 绑定。
- R5. 对已有错误配置提供只读诊断和可回滚的修复路径；未经单独授权不直接修改生产数据库。
- R6. 不改变其他 Provider 的候选选择、格式转换、图片、Search、Compact、WebSocket/Live 语义。

## Acceptance Criteria

- [x] 同一模型的 Responses、Compact、Search 绑定保持隔离：真实二进制 HTTP smoke 证明普通 `/v1/responses` 仅命中 Responses；既有专用路由回归已执行。
- [x] `aether-data` 的 2 个 `automatic_reconcile` 内存仓储回归通过，覆盖保留 manual 绑定、删除失效 discovered 绑定及同步范围；未冒充真实 PostgreSQL 专项验证。
- [x] 真实二进制复现并修复缺认证值的 pool key 计划失败：候选记录包含 `transport_auth_unavailable` 与 `failure_diagnostic`，不再只有匿名 `no_local_stream_plans`。
- [x] 历史只读证据确认 154 模型、Endpoint、Key 和绑定状态；未执行生产写入或部署。
- [x] 相关定向回归、真实二进制 HTTP smoke、格式检查与构建通过；全量库测试的 4 个环境阻塞单独保留在 implement.md，不宣称全绿。

## Out of scope

- 删除 Endpoint 绑定表或改为 Provider 级默认 Endpoint。
- 全局打开 `enable_format_conversion`。
- 未经确认修改 154 的数据库、Provider、Endpoint、Key、容器或系统配置。
- 顺手重构所有模型发现、候选选择或前端 Endpoint 管理流程。

## Open questions

- 代码与本机 HTTP 修复已实现；生产历史失败的精确分支未证实。全量验证仍有本地 PostgreSQL 可执行文件与 Unix socket 目录权限阻塞；不自动提交或部署。
