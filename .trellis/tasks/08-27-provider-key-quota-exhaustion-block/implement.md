# Implementation Plan

1. 扩展 provider transport/request report context，捕获内部凭据指纹；在 data contracts 与四类 repository 中实现 scheduling 状态 CAS。
2. 在 orchestration policy/classifier 中实现强/弱额度证据、人工正则、暂态排除与 Credential retry scope，并把证据加入 error-flow 诊断。
3. 在共享 execution effects 中实现疑似/阻断/清除状态机，调整同步、流式和 HTTP 200 错误包路径，隔离健康、熔断、自适应和 OAuth 副作用。
4. 在 provider-pool、pool-core、scheduler-core 和 gateway scheduler 中统一读取阻断事实、skip reason、active-probe、Pool score 和缓存失效。
5. 增加人工恢复路由、handler、状态清除和 Pool score 重建。
6. 扩展 Failover Rules 配置 UI、状态类型、Provider Key 列表与 Pool 管理页的状态/恢复操作。
7. 对照 PRD 做全链路静态审查，修复跨层字段、优先级、并发 fencing 和缓存遗漏。

## Validation

- `cargo fmt --all --check`
- `cargo check -p aether-data-contracts -p aether-data -p aether-data-sqlite -p aether-data-mysql -p aether-data-postgres -p aether-provider-transport -p aether-provider-pool -p aether-pool-core -p aether-scheduler-core -p aether-admin`
- `cd frontend && npm run type-check`
- `git diff --check`
- 使用静态夹具/现有调用链逐项验证强信号、弱信号、暂态 429、人工规则、普通/Pool/sticky 调度、凭据轮换与人工恢复场景；不新增单元测试，不运行 `aether-gateway` 大模块编译。

## Review Gates

- 不得把额度阻断写入 `is_active`、OAuth invalid、health 或 circuit breaker。
- 不得让成功/刷新/重建自动清除已确认状态。
- 不得在状态、日志或前端响应中暴露凭据、原始响应体或敏感头。
- 恢复动作只清除额度阻断；其他阻断和人工开关必须保留。

## Completion Record

- 已完成强/弱额度证据、重置期限排除、Credential 重试范围和错误流诊断。
- 已完成 memory/SQLite/MySQL/PostgreSQL 的凭据隔离 scheduling CAS，以及普通候选、Pool、sticky、active-probe、Pool score 和缓存消费。
- 已完成幂等人工恢复接口、故障转移规则配置、Provider Key 与 Pool 两个管理界面的状态和恢复操作。
- 已通过相关小 crate `cargo check` 与 Clippy `-D warnings`、`cargo fmt --all --check`、前端 `npm run type-check` / `npm run test:run` / `npm run build` 及 `git diff --check`。
- 审查修复已把 HTTP 402/精确额度码置于 reset 元数据之前，并用 provider/Key 运行时锁串行 scheduling CAS、Pool 分数/active-probe 投影、调度兴趣回写、后台探测、自检、周期重建、OAuth/导入分数初始化和人工恢复；普通成功仅在缓存命中 `quota_suspected` 时执行强读清除。
- 普通候选、Pool fallback 与 sticky 准入改为强读 Key，避免另一实例写入阻断后仍命中本机目录缓存；投影锁不可用时仍执行凭据隔离 scheduling CAS，并以候选强读和后续重建兜底派生 Pool 状态。
- Pool 新增的 `pool_key_quota_exhausted` 只表示运行时人工恢复阻断，既有可重置 Provider 硬额度继续使用原语义；已有 `AuthInvalid` 等更具体硬阻断仍保留，运行时 scheduling 事实独立保证额度过滤。
- 非重试 execution error 与禁用普通 success failover 的计划仍保留请求终止优先级，同时不再丢失同一响应中的额度证据；Endpoint 能力不匹配会清除疑似状态但不会产生健康惩罚。
- OAuth 自动刷新后会用强读的当前 Key 更新请求凭据指纹，避免新凭据的额度响应被误判为旧响应；恢复入口与 Pool 重建同样使用强读。
- 最终审查移除了成功/非额度终态的本机缓存负向门槛，改为强读后仅对疑似状态执行无 Pool 锁的凭据 CAS，保证多实例下夹在两次弱信号之间的普通响应会真实清零，同时避免每次成功都争用分布式投影锁。
- 传输错误现在覆盖超时、断链、TLS/代理/协议及远端执行器包装的 transport error，均不推进也不清除疑似状态；HTTP 200 额度错误包不再执行健康、自适应、Pool 成功训练或成功专用 finalize，并按候选失败记录。
- 凭据指纹对齐了真实 Endpoint 的 auth-config 吸收语义，修复安全子集因 Endpoint 冲突未能吸收时两侧哈希不一致；内存 scheduling CAS 也与三种 SQL 后端统一将非对象根归一化为空对象。
- Pool 目录强读失败仍 fail-closed，但改用独立 `pool_key_state_unavailable`，不再把基础设施故障误报成需人工恢复的额度耗尽。
- 结构化 Responses WebSocket 错误事件会进入共享 attempt lifecycle 并复用额度状态机；纯透传且不可解析的 Live/WebSocket 帧只能消费调度前已存在的阻断，当前无法从 opaque 帧本身新建额度证据，需作为协议边界保留说明。
- `aether-pool-core` 25 项、`aether-provider-pool` 83 项、`aether-scheduler-core` 95 项及前端 186 文件/1083 项现有测试均通过。
- `aether-data` 206 项单元测试与 3 项 public-entrypoint 测试、`aether-admin` 231 项、`aether-provider-transport` 444 项现有测试均通过；前端生产构建通过。
- 按项目约束未新增单元测试、未编译 `aether-gateway`、未提交代码；因此 Trellis 任务保留为 `in_progress`，不执行会自动提交的 archive/journal 步骤。
