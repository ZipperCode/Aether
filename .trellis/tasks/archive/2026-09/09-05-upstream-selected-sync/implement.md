# 实施计划

1. 配置 Trellis implement/check 上下文，启动任务。
2. 第一波并行实现：
   - 路由：补服务端 `model_id` 查询、变化重载和旧响应丢弃，扩展现有组件回归。
   - Provider/Antigravity：按 `fe8 → c8 → c5` 语义适配 OAuth 身份、reset-credit 和发现模型精确 Endpoint 目录同步。
   - 格式与流式：按 `66d6 → 14744 → 9282 融合 → 206995 → 344 → 57c` 适配终态、回放、取消结算、共享期限和同步收尾。
3. 等全部写者停止后，由唯一集成阶段检查写集、来源 SHA 映射和本地 Endpoint/Responses/余额契约；只在共享文件上串行修复。
4. 执行目标验证：
   - 路由组件 Vitest、Provider reset-credit Vitest、当前 Endpoint 7 项 Vitest、`npm run type-check`。
   - 各研究文件列出的精确 Rust 单测；确认过滤器确实匹配非零测试。
   - `cargo fmt --all --check`、`cargo check --workspace --all-targets`；仅在最小测试暴露公共契约风险时扩展测试。
5. 派发 `trellis-check` 做最终契约、测试和来源映射检查；修复后重跑失败项。
6. 按最终源码执行标准 `docker compose build app` 和 `up -d --no-build`，保留现有卷；验证三容器健康、登录、`/api/users/me`、首页、生产 JS 和日志。
7. 更新必要 code-spec、任务证据和提交映射；按 Trellis 提交计划收口，不 push、不发布。

## 实施进度与证据

- 路由 `1eb2d10de` 已按本地 `globalModelId` 契约适配：服务端查询携带 `model_id`，ID 未就绪不请求，模式/模型变化重载，陈旧成功与失败响应均丢弃；保留本地 Provider/Key 资格过滤和隐藏覆盖值。组件 Vitest 4/4、前端类型检查通过。
- Provider `fe8ff268d → c8d1ae3e7 → c5ae9c2c7` 已适配：Antigravity userinfo 邮箱身份链、Codex reset-credit 本地递减与失败保留、Pool 共享合并、quota 成功后发现模型精确绑定当前 Endpoint。OAuth 1/1、Admin 1/1、前端 reset-credit 14/14、三个相关 crate check、定向 rustfmt 与 ESLint 均通过。
- 格式/流式 `66d6c17d2 → 14744abd5 → 9282cce1d → 206995645 → 344b3031e → 57cdef4b8` 已适配：Gemini thought/终态、Responses carrier 回放、单一 Guard 无正文快照、请求级首字节期限、h2c 截断和跨格式收尾。定向 rustfmt、19 项非零目标测试及 `aether-ai-formats/aether-usage-runtime/aether-gateway/aether-integration-tests --all-targets` 检查通过。
- `dabaeb8df`、`ba11a7221`、`86f7cc0d5` 保持排除；语义适配与来源 SHA 映射记录在本任务研究文件，不声称保留上游 patch-id。
- Windows 本地验证需使用 VS18 `vcvars64.bat`、`CMAKE_GENERATOR=NMake Makefiles` 及 dev/test `opt-level=1`，否则 CMake 3.24 会生成 Debug CRT 的 BoringSSL 并在最终链接报 `_CrtDbgReport`；该问题只影响本地验证命令，未修改项目配置。
- 最终 `trellis-check` 修复两项：同步器原先误读上游 `quota_by_model` 而非本地 `models`；Global Model 测试仓储缺 Endpoint→Provider 归属。修复后 Provider Gateway 4/4、跨类别 spot-check 8/8、前端 25/25、全 workspace all-targets check、全局 rustfmt、TypeScript、只读 ESLint、任务校验和 diff check 均通过。
- 已直接执行 `docker compose -f docker-compose.yml -f docker-compose.local.yml build --build-arg RUST_BASE_IMAGE=aether-rust-1.95-compose-local:temp app`：前端生产构建通过，Rust `release` 编译在 14 分 08 秒完成，新镜像为 `sha256:d68bcd27e0cbb099387f030b365771bc04154cf647d09f788285a5ad82249046`。
- 已执行真实 Compose `up -d --no-build`，仅重建 `aether-app`，保留 PostgreSQL/Redis 容器与数据卷。`app/postgres/redis` 均为 `healthy`；新应用运行上述镜像。
- 运行态端到端验证通过：`GET /health`、`POST /api/auth/login`、`GET /api/users/me`、`GET /` 及新生产资源 `/assets/index-CAh8-cz8.js` 均为 HTTP 200；登录返回令牌，当前用户邮箱/用户名匹配本地配置且角色为 `admin`。新容器启动后 77 行日志中 panic 为 0、error/fatal 为 0，并出现 `gateway_ready`。
- 已更新 Antigravity 模型元数据/精确 Endpoint、Codex reset-credit、Responses/Gemini 回放和流式尝试生命周期 code-spec；新增规范已加入 gateway-execution 索引，规范链接检查、`git diff --check` 与任务上下文校验通过。
- 已删除任务专用临时基础镜像 `aether-rust-1.95-compose-local:temp`。执行策略拒绝删除 `C:\Users\Zipper\AppData\Local\Temp\aether-compose-build-ca`，因此仅残留 279 字节 Dockerfile 与 1301 字节证书副本；未绕过限制，残留不影响运行环境或 Git。

## 风险与回滚点

- `execution.rs` 和流转换文件是类别 4/5 的共享核心写集，只允许一个实现者修改。
- 本地已有取消终态 Guard，若出现第二套终态所有者立即回退该实现批次。
- Provider 目录同步若缺 Endpoint 证据或尝试旧自动推断，视为验收失败。
- `cargo fmt --all` 只在所有写者停止后执行，避免跨写集污染。
- Docker 重建不得执行 `down -v`、删除卷或覆盖 `.env`。
