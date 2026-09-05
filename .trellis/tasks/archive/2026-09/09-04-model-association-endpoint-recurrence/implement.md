# 实施计划

1. 在 `BatchAssignModelsDialog.vue` 基础数据加载完成后自动加载聚合上游模型，并复用现有会话守卫与同名同步逻辑。
2. 上游初始查询期间禁用保存，避免竞态回落到旧批量推断。
3. 在 `ProviderModelsQueryResponse` 补齐 `endpoint_ids` 类型字段。
4. 扩展现有目标测试，覆盖无需点击 Key 的 `gemini-3.7-flash` 同名关联，并保持不同名、无上游和陈旧响应用例。
5. 运行目标 Vitest、`npm run type-check` 与 `git diff --check`。
6. 按复发分析更新跨层指南，随后执行 Trellis 检查与提交收口。

## 完成证据

- 目标 Vitest：1 个文件、7 个测试通过。
- `npm run type-check`：通过。
- `python ./.trellis/scripts/task.py validate .trellis/tasks/09-04-model-association-endpoint-recurrence`：通过。
- `git diff --check`：通过，仅显示 Windows 工作区既有 LF/CRLF 转换提示。
- 独立 Trellis 检查发现并修复基础数据加载窗口的提前保存竞态，随后重跑全部最小验证。
- 当前工作区通过 `Dockerfile.app.local` 构建镜像 `aether-endpoint-recurrence:test`，镜像 ID 为 `sha256:a6058f537f7e048c94482914aab17bd0ceffac08875e6510fccbe1c8e040e6a2`。
- 隔离 SQLite 实例完成 31 个迁移并进入 `gateway_ready`；health、登录、`/api/users/me`、首页和实际生产 JS chunk 均返回 HTTP 200，启动日志中 PANIC/ERROR/FATAL 均为 0。
- 多 Endpoint 真实链路先复现旧 `assign-global-models` 的“无法推断 Endpoint”，且四次延迟读取均确认失败未创建模型；聚合查询随后只返回 `gemini-3.7-flash`、`gemini:generate_content` 和唯一 Gemini Endpoint。
- 按前端 `createModel` 请求创建后，以 DB/WAL/SHM 只读核验：唯一 Provider Model 与 Global Model 均为 `gemini-3.7-flash`，全表仅 1 条 active binding，指向 Gemini Endpoint；active `openai:chat` binding 为 0。
- 标准 `docker-compose.yml + docker-compose.local.yml` 已直接执行源码构建，生成 `aether-app:latest`，镜像 ID 为 `sha256:df93da566495f65d7bd088259d5c9c2972e1fe34fa42cc911068135796c7f150b`。
- 标准 Compose 的 PostgreSQL、Redis、Aether 均已健康运行；`/health`、真实登录、`/api/users/me`、首页和实际生产 JS chunk 均返回 HTTP 200，应用日志 PANIC/ERROR/FATAL 为 0。
- 标准 Compose 实例保留运行供用户验收，不创建预览容器或模拟数据。
- Docker app/mock 容器、专用网络、辅助 CA 镜像以及 DB/证书/静态文件临时目录已清理；保留目标验证镜像供后续复核。
- 按项目规则未运行会自动写文件的 `npm run lint`，未做 UI 视觉验证或全量前端测试。
- 浏览器工具返回 `No browser is available`，因此 UI 可视点击未验证；真实生产静态资源 HTTP、管理 API 和 SQLite 数据链已覆盖本次逻辑边界。
