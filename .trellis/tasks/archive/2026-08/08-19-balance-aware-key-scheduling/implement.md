# Implementation Plan

## Progress

- [x] 步骤 1-7：实现、接线、回归测试与质量复核完成。
- [x] 步骤 8：核心三包、Gateway 三组定向测试及格式/diff 检查全部通过；Gateway 两个测试目标完成链接。
- [x] 步骤 9：最终 diff、脏工作区归属和临时进程已检查；无任务临时源码或残留构建进程。

1. 在 provider-pool 实现并导出统一余额判断，补齐 fresh、阈值、多币种、invalid、unlimited 和 subscription 回归测试。
2. 将余额事实接入 `PoolMemberSignals` 和 pool-core 过滤，增加 skip reason、scan-budget/active-probe 语义及单元测试。
3. 将余额事实接入普通候选 runtime snapshot 与 scheduler-core selectability，补齐普通候选 skip reason 测试。
4. 让 sticky Pool Key 以单候选复用共享 Pool scheduler，验证 seen-key、跳过证据、索引和普通 page 回退。
5. 让 ObservationOnly 余额提供商自动加入 account self-check，保持 60 分钟默认周期及失败 fail-open；补齐未启用配置仍刷新和恢复测试。
6. 在余额持久化路径比较 eligibility 前后状态，复用候选缓存失效函数；覆盖 low→active、low→stale 和状态不变。
7. 复核所有新增字段的构造方、调用方和测试夹具，补充实质性中文说明，不做无关重构。
8. 仅格式化本任务修改的 Rust 文件，随后运行：
   - `cargo test -p aether-provider-pool`
   - `cargo test -p aether-pool-core`
   - `cargo test -p aether-scheduler-core`
   - `cargo test -p aether-gateway pool_key_cursor`
   - `cargo test -p aether-gateway official_balance`
   - `cargo test -p aether-gateway account_self_check`
   - `cargo fmt --all --check`
9. 检查最终 diff、既有脏工作区和任务临时产物；仅在定向验证暴露跨区风险时扩大验证。

## Risk and Rollback Points

- 余额事实跨三个内部类型传播，任何遗漏会导致普通/Pool 路径行为不一致；每层必须有定向测试。
- sticky 复用必须保留扫描预算、seen-key 和审计记录；若现有 helper 无法无损复用，停止并回到设计阶段，不复制过滤器。
- 自动自检会增加余额 API 调用，但保持现有 60 分钟默认周期；不得新增更激进的轮询。
- 首次 Rust 1.95 工具链/依赖准备曾失败，验证必须顺序运行并把环境失败与测试失败分开报告。
