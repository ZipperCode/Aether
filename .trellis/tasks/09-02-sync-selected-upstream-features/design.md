# 同步上游路由、Pool 与协议修复：技术设计

## 设计目标

在不合并整条 `upstream/main` 的前提下，将 PRD R1-R4 对应的上游最终行为移植到当前 fork，并保留 fork 已发布的模型能力检测、Responses 原生透传/首段错误、余额调度、永久额度阻断和 Endpoint 精确绑定合同。

## 集成边界

- 基线固定为 `50c96d060442fb1b612a27c587b91dec4f79a613`。
- 仅复用 27 个范围内上游补丁；重复补丁各取一个，格式化提交由最终 `cargo fmt` 替代。
- `dd2958a45` 只取四个 Provider Key 保存回显文件中的必要片段。
- `9631b229b` 只取 R2 所需的 Key 并发、缓存亲和、OAuth/额度刷新及管理载荷；排除 `control/auth/gate.rs` 和 `wallet_runtime/access.rs` 的独立钱包语义。
- 硬排除 VSCodex、通用 Usage API、套餐权益撤销、Nightly/发布以及上游 merge wrapper。
- 不新增数据库 migration，不删除既有 `max_retries` 存储/API 字段；只停止用它展开本地候选重试槽，并移除 Provider 表单输入。
- 上游补丁中的英文或缺失说明不能直接视为满足当前项目约束；对本次新增/修改的手写函数、类型和业务/配置字段补充实质性中文说明，但不因此建立额外抽象或改写未触及代码。

## 集成策略

使用一个串行实现者，按四批语义合并；Root 在每批停止写入后核验并提交。补丁优先使用上游原始 diff，冲突处按当前 fork 合同逐符号合并，禁止整文件采用 `ours`/`theirs`。

### 批次 A：协议基础

顺序：

`4da8c57fe` → `8cdfa338e` → `c4b4dfa99` → `dd2958a45` 四文件子集 → `5687dad17` → `5b6fce1a7` → `64e572533` → `f0b0064f3` → `83098f98b` → `5bcdcca78` → `1bc2287ba` → `9837ce119` → `36daba7a3` → `b35364d7f` → `56395945c`。

- `fa8e443f7` 与 `5b6fce1a7` patch-id 相同，只采用 first-parent 已合入版本 `5b6fce1a7`。
- `f0b0064f3` 和 `83098f98b` 必须连续，最终混合工具只允许 Gemini 3。
- `9837ce119` 和 `36daba7a3` 必须连续，Antigravity 最终字段为 `parameters`。
- Gemini ID-less 配对必须替换 fork 的局部算法，不能并存两套状态机。
- 原生 Responses 继续复制对象/字节并保留未知字段和事件；所有 carrier、Schema 和工具转换只作用于对应跨格式或 Gemini 私有边界。

### 批次 B：Provider Pool 与额度

顺序：`9631b229b` R2 子集 → `2fe260002` → `ee55f4696` → `57abb2077` → `6c71f8758`。

- `concurrent_limit` 已有存储/API；新增执行前的原子 Key semaphore，并让 HTTP sync、HTTP stream、Responses/Live/Realtime WebSocket 的每个 turn 共享同一 admission 合同。
- memory runtime 只保证单进程；Redis runtime 才能保证多实例共享上限。
- Cache affinity 复用现有 Pool 排序：sticky 命中仍过共享过滤，未命中按 `single_account`（默认）或 `lru` 分配。
- 模型额度只收窄模型窗口，余额和管理员恢复型永久阻断仍是 Key-wide；Pool catalog 继续 strong-read 且缺失时 fail closed。
- Antigravity 摘要由一个前端 utility 统一投影，原始模型条目仍用于测试模型选择。

### 批次 C：身份与运行时兼容

顺序：`a39048ecc` → `3e540ce58` → `d07dc8637` → `633363e19` → `88d2b002b` → `7ae984df4`。

- Codex fingerprint context 每个逻辑 turn 只解析一次，穿过 HTTP retry/replan、Responses WebSocket rebind/quota retry 和 Live plan/admission。
- `decision_input.rs` 同时保留 fork 的三个 Endpoint capability 字段和新 fingerprint context。
- Pool 饱和原因加入现有诊断集合；只有所有原因均为容量类时最终返回 429，任何余额、永久额度或基础设施原因都保持各自语义。
- Gemini precommit gate 与 fork 的同格式 Responses `FirstClassifiedBody` 并存；不得覆盖 Responses `EmbeddedError` 的既有 failover。
- Responses `ping` 只在转换状态机中忽略，原生同格式 SSE 仍字节透传。
- `e2154629c` 与 `7ae984df4` patch-id 相同，只采用 `7ae984df4`；DeepSeek 判断保留严格 host parser，并增加最终映射模型证据。

### 批次 D：路由策略

顺序：`415b2da81` → `7323d41fb`。

- 有效路由策略是排序唯一来源；无有效策略时先读系统默认路由组，再回退旧系统配置。
- 启动时尽力从旧配置创建系统默认组；存储不可用时不阻断启动。
- `key_priority_overrides_by_format` 优先于全局 Key override，再回退 catalog。
- 每个候选只物化一个 attempt；只有第一候选可按 `sticky_key_attempts` 延迟派生同 Key 重试，其他候选各一次。
- 固定与动态 attempt loop 均保留 fork 的 exhaustion/fallback 上下文、drain/mark-unused、admission 和 Endpoint failover。

## 关键数据流

```text
路由 JSON → 统一 policy resolver → SchedulerOrderingConfig
        → candidate ranking / Pool Key ranking → report context
        → lazy same-Key attempt → candidate failover

Provider Key concurrent_limit → catalog strong read → keyed semaphore
        → sync / stream / WS turn admission → RAII release
        → saturation diagnostic → candidate skip reasons → 429 或 503

Gemini/Responses request → exact-boundary normalization → canonical model
        → provider-specific request → stream/sync canonical events
        → target response；同格式 Responses 绕过 canonical allowlist
```

## 冲突保护合同

- `decision_input.rs`：保留模型/Endpoint capability 上下文，同时增加 Codex context 和路由字段。
- `stream/commit_policy.rs`、`stream/execution.rs`：Responses 首段错误与 Gemini semantic gate 同时存在。
- `pool_scheduler.rs`：保留余额、runtime quota、strong-read、精确 Endpoint 和缓存失效，再增加模型额度/亲和/格式级优先级。
- `canonical.rs`、`registry.rs`、Gemini request/stream：保留 fork 的 namespace/custom tool、previous-response expansion 和 fail-closed 分支。
- `model_test.rs`：保留 v0.7.26 固定参考、buffered body、评分和路由合同，仅补最终 DeepSeek 模型参数。
- 前端 Routing/Pool/Provider 页面：保留本地按模型过滤、永久额度恢复和既有配额展示。

## 范围审计

- 以 27 个选中 SHA 的文件并集加 `dd2958a45` 四个批准路径为允许集合；额外产品路径必须有编译错误和 R1-R4 映射证据。
- 对 `aether-vscodex/**`、VSCodex gateway/frontend 接线、`.github/workflows/**`、`README.md`、`install.sh`、Usage API、用户套餐/结算路径和 `dd2958a45` 的 Endpoint 布局路径做零差异断言。
- 扫描新增 diff 中的 `vscodex`、权益撤销路由及 `usage_api` 标识。
- 验证两个重复补丁均只应用一次，五个硬排除提交、merge wrapper 和格式化 SHA 均不是最终 ancestry。
- 对最终改动符号执行说明审计，确认新增/修改的函数、具名回调、类型及业务/配置字段均有中文用途和边界说明。

## 回滚与交付

- 所有工作在独立 worktree 中完成；每批单独提交，失败可按批回滚。
- 验证通过后只 fast-forward 本地 `master`，不推送 `origin`。
- 合并后清理任务 worktree 和临时分支；若主工作区在期间出现用户改动则停止合并并报告。
