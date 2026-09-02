# 执行计划

## 0. 基线与所有权

- [x] 再次确认任务 worktree 位于 `50c96d060`、无产品改动，`upstream/main` 仍含目标 SHA。
- [x] 唯一 `trellis-implement` 代理串行拥有产品文件；Root 只维护任务工件、核验和提交。
- [x] 禁止写入 VSCodex、Nightly/发布、Usage API、套餐权益/结算及其他范围外路径。
- [x] 每批同步时同时补齐本次新增/修改手写符号的实质性中文说明；只解释用途、业务语义和必要边界，不添加占位注释。

## 1. 协议基础批次

- [x] 按 `design.md` 批次 A 顺序逐补丁应用并解决冲突。
- [x] 只提取 `dd2958a45` 的四个 Key 保存回显路径，并在批次说明中保留来源 SHA。
- [x] 保留原生 Responses 未知字段/事件、exact-body 和首段错误合同。
- [x] 跑 `cargo test -p aether-ai-formats`。
- [x] 跑 Provider Transport、Antigravity、Gateway mixed/additional-tools/compaction 的针对性测试。
- [x] 跑 Responses fork-preservation 针对性测试及 `python docs/api/generate_format_field_coverage.py --check`。
- [x] Root 检查范围与冲突说明后提交中文批次提交。

批次 A 证据：提交 `f485ae016`；`aether-ai-formats` 903/903，通过 Provider Transport、Antigravity、Model Fetch、9 个 Gateway 目标、格式覆盖生成器和格式检查；Provider Key 前端目标测试 7/7；26 个产品路径均在允许集合，硬排除路径零差异，新增具名函数中文说明审计 65/65。

## 2. Provider Pool 批次

- [x] 按 `design.md` 批次 B 集成；从 `9631b229b` 排除钱包与独立 auth gate 语义。
- [x] 贯通 Key semaphore 到 sync、stream 和三类 WebSocket turn admission，确保所有释放路径由同一 guard 管理。
- [x] 合并 cache affinity 二级模式、模型额度隔离、Antigravity reset/summary 最终态。
- [x] 保留余额 fail-open、永久额度 strong-read/manual recovery、精确 Endpoint 与缓存失效合同。
- [x] 跑 runtime-state、provider-pool、gateway Pool/Key admission、OAuth/refresh 和前端目标测试。
- [x] 跑前端 `npm run type-check`。
- [x] Root 检查范围与数据链后提交中文批次提交。

批次 B 证据：提交 `09c4aab9e`；Runtime/Pool/OAuth/Transport/Admin 目标通过；受影响 Rust crate `cargo check` 通过；Gateway 7 组共 10 项通过；前端 7 文件 48 项和类型检查通过；38/38 产品路径匹配允许集，`gate.rs` 与 `wallet_runtime/access.rs` 零差异，中文说明审计 113/113。

## 3. 身份与网关兼容批次

- [x] 按 `design.md` 批次 C 集成，确保 `9631b229b` 已落地。
- [x] 更新所有 Codex HTTP/WS/Live retry、replan、rebind 和 quota retry 调用点。
- [x] 合并 Pool capacity 原因、Gemini precommit gate、Responses ping 和 DeepSeek custom relay。
- [x] 保留模型能力检测、Responses `FirstClassifiedBody`/`EmbeddedError`、余额/runtime quota 语义。
- [x] 跑 Codex fingerprint/context、Gemini malformed call、Pool capacity、Responses ping、DeepSeek 和 fork-preservation 目标测试。
- [x] Root 检查范围与调用链后提交中文批次提交。

批次 C 证据：提交 `02ac8aac0`；四个受影响 Rust 包联合 `cargo check` 通过；Codex Transport 14 项、OAuth 1 项、Gateway 22 项定向回归通过；`aether-ai-formats` 905/905 通过；前端 Provider 表单 7/7 和类型检查通过。实际 36 个产品路径中，35 个来自所选上游路径，另 1 个为本地严格 Gemini 审计的必要集成修复；上游 `standard/family/payload.rs` 仅有的测试构造替换在本地已由统一构造器等价覆盖。新增定义中文说明审计 98/98、受影响定义 167/167。

## 4. 路由策略批次

- [x] 按 `design.md` 批次 D 集成系统默认 policy、格式级 Key 优先级和 lazy sticky retry。
- [x] 保留固定/动态 loop 的 fork exhaustion、drain、admission、quota skip 和 Endpoint failover。
- [x] 跑 `aether-routing-core`、`aether-ai-serving`、Gateway scheduler/attempt/Pool/failover 目标测试。
- [x] 跑 Routing 前端目标测试与 `npm run type-check`。
- [x] Root 检查范围与调用链后提交中文批次提交。

批次 D 证据：提交 `72994512b`；三包联合 `cargo check` 通过；`aether-routing-core` 22/22、AI Serving attempt loop 4/4、候选持久化 5/5 通过；Gateway 系统默认策略 4 项、attempt 身份 8 项、动态循环 3 项、Pool 53 项以及同步/流式故障转移、lazy retry、Endpoint 隔离和跨批兼容目标均通过；前端 Routing 3 文件 22 项与类型检查通过。57 个上游目标路径全部落地，额外 1 个公开模型查询路径为编译器定位的必要调用点；硬排除路径和标识均为零。新增 Rust 定义说明审计 50/50、受影响 Rust 定义 163/163，前端实际受影响类型/函数/具名回调均已补齐中文说明。

## 5. 最终检查

- [x] 运行 `cargo fmt --all`，随后 `cargo fmt --all --check`；格式器新增的范围外路径视为失败。
- [x] 运行 `cargo check --workspace`；本次跨核心契约，workspace check 属于必要收口。
- [x] 重跑失败过的目标测试；不得用全量重跑掩盖单项失败。
- [x] 运行最终前端类型检查和合并后的目标测试。
- [x] 运行格式覆盖生成器 check、`git diff --check` 和允许/拒绝路径审计。
- [x] 审计本次新增/修改函数、具名回调、类型、模型、接口及业务/配置字段的中文说明覆盖。
- [x] 派发唯一 `trellis-check` 代理复核规格、调用链、测试证据和范围。
- [x] 修复 checker 证实的问题并重跑最小相关检查。

最终复核证据：唯一 `trellis-check` 复核未发现产品缺陷；`cargo fmt --all --check`、`git diff --check`、Trellis 上下文校验和 `cargo check --workspace` 通过。协议格式 905/905、Provider Pool 86/86、Routing Core 22/22、AI Serving 9/9、前端目标 76/76，以及 Gateway 的并发、Codex、Gemini、Responses、DeepSeek、compaction 和 lazy retry 目标均通过；格式覆盖生成器最终检查通过。Runtime 全包 46/47，唯一失败为基线提交 `ab0a90de97` 已存在的 Redis `ZMSCORE` 测试，本机 Redis 不支持该命令，与本次产品差异无关；新 semaphore 目标 1/1 通过。最终产品差异共 147 个路径，范围外标识与硬排除路径均为零。

## 6. 收口

- [x] 更新 PRD 验收项和任务证据；仅在出现可复用的新合同且需要时更新 `.trellis/spec/`。
- [ ] 提交产品与任务工件，按 Trellis 流程归档并记录 journal。
- [ ] 确认主工作区仍处于原始干净 `master`，fast-forward 合并任务提交。
- [ ] 验证本地 `master` 包含任务提交、`origin/master` 未变化且工作区干净。
- [ ] 删除临时 worktree，删除临时分支并 prune。
