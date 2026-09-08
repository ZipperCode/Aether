# 执行进度

- [x] 核对master/clean/remote祖先并正常push，远端SHA为948c1c16f。
- [x] 捕获Rust CI34210248162，读取首个完成失败job原始日志。
- [x] 修复build-watch fixture对CI颜色环境的真实差异，验证相同CI env下仍通过。
- [x] 等当前run完成并收集其他真实失败；全部35个Gateway失败已列出，按下方明确文件分组继续。
- [ ] 独占check审查本次修复；Root统一commit/push并跟进精确新SHA CI直到成功或确有外部阻塞。
- [ ] 归档和清理；不tag/release/deploy。

## 初始所有权

- fixture writer：`tests/gateway_build_watch_test.py`、必要现有CI fixture回归（如需第二文件先请求）、`research/build-watch-failure.md`；不得改build.rs或工作流来掩盖失败。
- CI evidence reader：当前run/已完成job只读，`research/run-34210248162.md`；不写产品，不push/cancel/rerun。
- Root：PRD/design/implement/jsonl/task状态、Git提交/push及最终核验；派发期间不重复产品调查。

## 验证策略

使用相同CI输入重现已失败脚本，运行最小回归，不编译整个网关。后续真实Clippy/运行期失败依所属路径限定验证；既有38Rust/423frontend和24fixture历史通过不是新CI整体成功证据。每批实际修复和已收集失败必须写入research；用户不需要替我们追失败链。

## 首轮终态与已完成批次

- Run34210248162：18个展开jobs全部完成，8成功/10失败（其中3为汇总）；Gateway编译7m29s，5421pass/35fail/3skip，完整测试列表见 research/run-34210248162.md。
- 已完成且独立check通过：颜色fixture、前端2个旧断言、cookie iterator Clippy、CAS Debug测试误用、历史baseline源1行，以及Data/Rest no-fail-fast收集；这些8文件保留，不覆盖。
- 本轮Data/Rest因fail-fast合计2222条未运行；下一轮会收集全部结果。当前修复不能假称这些未运行用例通过。

## Gateway第二波独占写集

- A planner/pool/architecture（5失败）：`apps/aether-gateway/src/ai_serving/planner/candidate_materialization.rs`、`apps/aether-gateway/src/dispatch/pool_scheduler.rs`、`apps/aether-gateway/src/tests/architecture/ai_serving.rs`；其余ranking/alias源只读，确需修改先申请。
- B execution/capture/finalize（15失败）：`apps/aether-gateway/src/execution_runtime/stream/execution.rs`、`apps/aether-gateway/src/execution_runtime/sync/execution.rs`、`apps/aether-gateway/src/executor/candidate_loop.rs`、`apps/aether-gateway/src/executor/orchestration.rs`、`apps/aether-gateway/src/executor/outcome.rs`、`apps/aether-gateway/src/tests/usage/local.rs`、`apps/aether-gateway/src/tests/ai_execute/finalize_local_cli/cross_format.rs`。
- C admin/catalog/frontdoor（11失败）：`apps/aether-gateway/src/handlers/shared/catalog.rs`、`apps/aether-gateway/src/tests/control/admin/endpoints/quota.rs`、`apps/aether-gateway/src/tests/control/admin/models/provider.rs`、`apps/aether-gateway/src/tests/control/admin/providers.rs`、`apps/aether-gateway/src/tests/control/admin/proxy_nodes.rs`、`apps/aether-gateway/src/tests/control/admin/system.rs`、`apps/aether-gateway/src/tests/control/admin/system_import.rs`、`apps/aether-gateway/src/tests/frontdoor/ai.rs`。
- D OAuth Kiro（4失败）：`apps/aether-gateway/src/tests/control/admin/oauth.rs`；真实OAuth implementation只读，如需修改先申请精确路径。
- 各组仅追加自身 `research/gateway-{group}-fixes.md`；Root持有PRD/任务/规范/Git。禁止对并发正在变化的网关执行整包构建或全局formatter，先按实际根因处理并记录要跑的目标，组完成后唯一integrator统一验证。新生产文件须申请，不跨组覆盖。

## 第二波完成与统一验证

- A/B/D组和C组均已终态、返还所有权。35项CI失败逐条有修复映射；唯一新增业务修复为C组获准恢复 `crates/aether-admin/src/provider/redaction.rs` 的余额字段投影，其余Gateway改动在测试/测试helper。
- 27个实际产品/测试/配置文件的当前diff全部属于本任务，包括第一批独立检查通过的8文件，不回滚或重复覆盖。
- 下一步唯一check/integrator：先admin余额projection两回归，再只构建一次Gateway lib测试产物、运行当前35个精确原失败/重命名过滤器，集中收集其后续断言问题；不运行完整Gateway几千项本地suite。最终完整LinuxCI由下一次正常push认证。
- Root复核后统一commit/push，不再等待已结束的首轮CI；不tag/release/部署。

## 第二波最终本地证据与待推送

- admin projection 2/2通过，当前Gateway测试产物构建18m05s并运行35个精确目标：33通过/2失败（64.135s）。两项剩余共同原因是共享候选类别表遗漏 `endpoint_capability_mismatch`。
- check已获准在 `crates/aether-data/contracts/src/repository/candidates/types.rs` 增加此单一类别+现有同层回归；对应contracts测试1/1、admin/contracts范围Clippy均通过，PostgreSQL沿用同一常量。
- 为避免仅一项类别登记再本地重建整个Gateway，最终2项不标本地PASS，交下一次精确SHA Linux CI确认；其余有效结果复用。28个产品修改均归本任务，所有写代理/编译已终态。
- Root已完成共享规范与break-loop事实复盘，立即统一commit/push，不追加全量前置门槛。

## 第二轮 CI 与末批修复

- 已正常推送9026380d1957e7a57a746a409335f15eb1675515，run34217818968终态15成功/3失败；Gateway5456/5456、Frontend1635/1635及Build、Data372/372、全部Clippy均通过。原41处观测失败已修复。
- Rest完整no-fail-fast跑到先前未执行后半段，新5失败为model-fetch1、Kiro1、Tunnel2、usage顺序1；修复仅4文件，完整根因见second-ci四收据。
- 独立check：model-fetch3/3、OAuth2/2、usage确定性交错+原失败+两个共享fixture共4/4通过；Tunnel仅补精确wire静态类别，原测试未改，本地未重跑2项。
- Tunnel production-only严格Clippy仅因3处既有Windows dead_code失败（未改文件）；不加allow、不改无关平台函数，不伪称本地通过，下轮LinuxCI认证。没有Gateway重新构建或全量本地套件。
- 全部writer/check已归还；Root正常commit/push最后4文件并继续精确SHA CI，不结束在“已push但CI未知”。
