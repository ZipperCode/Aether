# 技术设计

## 同步边界

不合并 `upstream/main`，也不机械覆盖本地文件。以 13 个上游非合并提交为审计集合，将 8 个功能提交的行为适配到当前代码，并带入 2 个直接测试支持提交：

- 编号 1：`1eb2d10de`。
- 编号 3：`fe8ff268d`、`c5ae9c2c7`；测试支持 `c8d1ae3e7`。
- 编号 4：`9282cce1d`、`206995645`、`57cdef4b8`；测试支持 `344b3031e`。
- 编号 5：`66d6c17d2`、`14744abd5`。
- 排除：`dabaeb8df`、`ba11a7221`、`86f7cc0d5`。

每条本地实现记录来源 SHA。因冲突适配会改变 patch-id，完成判据是提交映射、调用链和回归测试，而不是 `git cherry` 的正负号。

## 写集与执行波次

第一波可并行的唯一写集：

1. 路由前端：`frontend/src/features/routing/**`，复用当前 `globalModelId`、组件测试和更严格的 Provider 资格过滤。
2. Provider/Antigravity：OAuth、quota、model-fetch、PoolManagement 及对应控制面测试；目录同步必须构造本地完整导入 DTO，并绑定当前 Endpoint。
3. 格式与流式生命周期：统一负责 `execution_runtime`、candidate loop、usage runtime、AI formats 和 mock upstream，避免 `execution.rs`、格式终态及测试由多个写者冲突修改。

共享任务工件、规范索引、最终格式化、全范围检查和 Docker 重建由串行集成阶段处理。

## 关键兼容决策

- 路由查询使用 GlobalModel ID；ID 未就绪时为空，不短暂回退全量 Provider。保留客户端“Provider 启用且至少一个 Key 启用”的二次过滤和隐藏覆盖值往返行为。
- Antigravity userinfo 使用现有 OAuth 网络与 override 上下文；保留 Nous 特例。Codex reset-credit 只在现有凭据代次、重置代次和 CAS 门禁内合并或本地递减。
- quota 发现模型在成功持久化后同步目录；同步失败只告警。使用本地完整 `AdminImportProviderModelSource`，把模型精确绑定到触发刷新请求的 Endpoint。
- 复用现有 `StreamAttemptTerminalGuard` 作为唯一取消终态所有者；只融合无正文 usage seed、紧凑快照和 watchdog 所有权改进，不新增第二套 Guard。
- 首字节期限从请求起点计算并跨候选重试共享，不改变余额调度、同 Key 重试或准入许可语义。
- 非空 Gemini thought 是可见首输出；已提交的流遇到工具调用终态错误时发完整 `response.failed`，不得再切换 Candidate。
- Responses 回放只删除 Aether 自己编码的 Gemini signature carrier；保留真实 Provider 密文、合法 reasoning item、同格式未知字段和提交 2xx 前的裸错误分类。
- 跨格式同步收尾继续 fail closed，并保留本地嵌套错误识别与 Responses failover。

## 回滚与运行态

不含数据库迁移、配置变更或数据重写。按三个产品提交单元可分别回滚。Docker 验证只重建并重建容器，不删除 PostgreSQL/Redis 卷，不提交 `.env`；失败时保留当前已健康镜像与数据并报告。
