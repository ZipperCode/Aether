# 额度感知 Key 调度与调度审查

## Goal

让所有产生标准余额快照的 Key 根据最新有效余额自动退出或恢复调度，修复 sticky、后台刷新和候选缓存路径中的一致性缺陷，并给出全局调度策略的证据化审查结论。

## Background

- 变更前 `provider_pool_quota_snapshot_exhausted_decision` 对 `kind="balance"` 固定返回非耗尽，余额不会影响 Key eligibility（`crates/aether-provider/pool/src/quota.rs:237`）。
- 普通候选已有统一运行态过滤，Pool 候选另有账户/额度过滤；sticky Pool Key 绕过后者直接入队（`crates/aether-scheduler-core/src/candidate/selectability.rs:42`、`crates/aether-pool-core/src/scheduler.rs:195`、`apps/aether-gateway/src/dispatch/pool_scheduler.rs:965`）。
- 余额刷新失败会保留旧快照并标记为 `stale`；余额写入默认只失效 catalog cache，无法保证 eligibility 立即变化（`apps/aether-gateway/src/handlers/admin/provider/oauth/quota/official_balance/persistence.rs:69`）。
- `ObservationOnly` 余额提供商当前仍依赖手工开启账户自检，默认单 Key 检查周期为 60 分钟（`apps/aether-gateway/src/maintenance/runtime/account_self_check.rs:696`）。

## Requirements

1. 所有返回 `status_snapshot.quota.kind="balance"` 的 Key 默认自动参与余额判断，不依赖 `skip_exhausted_accounts`。
2. 仅当快照明确为 `fresh`、非 unlimited、余额数组非空、每条单位非空、每条金额均为有限可解析数值且全部 `<= 1.0` 时，Key 才退出调度。
3. 多币种不做汇率换算；任一余额 `> 1.0`，或快照 stale/unknown/空、字段缺失、解析失败、刷新失败时均 fail-open。
4. 普通候选、Pool 候选和 sticky Pool 候选必须使用同一余额事实；代表 PoolGroup 的 Key 不得因自身余额屏蔽整个池。
5. `ObservationOnly` 余额提供商必须自动进入现有账户自检，继续刷新被跳过的 Key；沿用现有配置，缺省为 60 分钟和 4 并发。
6. 余额事实从 low/active/stale 之间发生 eligibility 变化时，必须失效 candidate page 和 resolved candidate cache；状态不变时保持现有 catalog-only 失效。
7. 订阅型 quota 的 exhaustion 与 `skip_exhausted_accounts` 语义保持不变。
8. 新增或修改的手写函数、业务字段和关键逻辑必须带实质性中文说明。
9. 交付时提供当前调度链路、已修问题和未纳入本轮的优化/风险清单。

## Acceptance Criteria

- [x] fresh 单余额为 `0` 或 `1` 时普通 Key 与 Pool Key 均不进入执行候选；`1.0001` 时可调度。
- [x] 多币种仅在所有有效余额都 `<= 1` 时跳过；任一有效余额 `> 1` 时保留。
- [x] stale、空、缺字段、无单位、非法/非有限数值、unlimited 和刷新失败均不因余额被阻断。
- [x] 订阅型 quota 仍只在 `skip_exhausted_accounts=true` 时跳过。
- [x] sticky 指向低余额或其他不可调度 Key 时记录跳过证据并继续选择后续 Key，且不会重复扫描同一 Key 或耗尽扫描预算。
- [x] 未手工开启自检的 `ObservationOnly` 余额提供商仍被后台周期性刷新；刷新成功后自动恢复，失败不写 cooldown/hard-state。
- [x] fresh low→fresh active 及 fresh low→stale 都会使候选缓存失效，下一次选择看到新 eligibility。
- [x] 不新增数据库迁移、HTTP/前端字段、用户配置、汇率换算或新依赖。
- [x] 相关 Rust 定向测试和格式检查有实际结果；无法通过时报告精确阻塞，不以旧结果代替。

## Verification Evidence

- `cargo test -p aether-provider-pool`：82 passed。
- `cargo test -p aether-pool-core`：24 passed。
- `cargo test -p aether-scheduler-core`：93 passed。
- 质量修复后 `cargo test -p aether-provider-pool balance_scheduling`：2 passed。
- `cargo fmt --all --check` 与 `git diff --check`：通过。
- 已安装 NASM 3.02（`C:\Users\Zipper\AppData\Local\bin\NASM\nasm.exe`），并仅在测试子进程中前置其目录；没有持久化 PATH、CMake 或 Cargo target 配置。
- `cargo clean -p boring-sys2` 仅清理该包 1371 个可重建文件（121.0 MiB）；取消显式 CMake generator 后，BoringSSL Debug RuntimeLibrary 为 `MultiThreadedDLL`，对象依赖 `MSVCRT`，CRT 冲突解除。
- `cargo test -p aether-gateway pool_key_cursor --no-run`：退出码 0，Gateway lib/main 两个测试目标成功链接。
- `cargo test -p aether-gateway pool_key_cursor`：12 passed，0 failed。
- `cargo test -p aether-gateway official_balance`：31 passed，0 failed，2 ignored（既有手工测试）。
- `cargo test -p aether-gateway account_self_check`：6 passed，0 failed。
- 最新 `cargo fmt --all --check` 与 `git diff --check` 均通过，且无残留 `cargo`/`rustc`/`cmake`/`MSBuild`/`link` 进程。

## Out of Scope

- 高并发计数只读取最近 128 条记录及非原子准入竞态，登记在子任务 `08-19-atomic-admission-accounting`。
- 秒级 load-balance seed、默认 failover 无上限、provider quota block map 串行读取仅列为审查发现，不在本轮修改。
- 不改提供商告警阈值、生产配置、数据库、前端或公开 HTTP 合同。
