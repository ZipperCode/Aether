# 审查并发布 v0.7.28

## Goal

审查 `origin/master..HEAD` 中的上游模型自由关联修复与 Trellis 收口提交；确认没有阻塞问题后，将精确候选 SHA 推送到 `origin/master`，等待该 SHA 的 GitHub `Rust CI` 全部成功，再创建并推送 annotated tag `v0.7.28`，最终验证 `Release Aether` 与发布资产。

## Background

- 当前本地 `master`/候选 SHA 为 `83c3058b76fc785d91526b4474a1d040bda48de1`，工作区在创建本发布任务前干净。
- 当前远端 `origin/master` 为 `0e78031b4c880f107e27fbadc471dcd61c35aaf4`，候选包含 4 个已提交变更。
- 最新本地和远端应用 Tag、GitHub Release 均为 `v0.7.27`；`v0.7.28` 当前不存在。
- `Rust CI` 对 `master` 的 `frontend/**` push 触发；`Release Aether` 对 `v*` Tag push 触发。
- 上一版本已确立“分支精确 SHA CI 成功后才创建 Tag，并验证资产”的发布门禁。

## Requirements

1. 独立审查 `origin/master..83c3058b7` 的完整差异，重点检查不同名上游模型关联、Endpoint 传递、旧推断兼容、异步会话隔离和测试有效性。
2. 审查发现问题时先修复、提交并重新锁定候选 SHA；没有阻塞问题才允许推送。
3. 推送 `master` 后确认远端分支精确等于候选 SHA，并只接受同 SHA、push 事件、`master` 分支的 `Rust CI` 成功作为 Tag 门禁。
4. CI 失败时定位实际失败 job；不得把汇总 job 的连带失败当作独立根因，不得在失败状态创建 Tag。
5. CI 成功后再次确认 `v0.7.28` 本地、远端及 GitHub Release 均不存在，创建 annotated tag `v0.7.28`，说明为 `发布 v0.7.28`，并只推送该 Tag 引用。
6. Tag 必须精确 peel 到通过 CI 的候选 SHA；不得移动、覆盖或 force push Tag。
7. 等待精确 Tag/SHA 的 `Release Aether` 成功，验证 GitHub Release、要求的 4 个非空资产，以及 tarball digest 与 `SHA256SUMS` 一致。
8. 发布完成后更新证据、归档任务、记录 journal，并推送新增 bookkeeping 提交；Tag 保持指向认证候选 SHA。
9. 不发布 `tunnel-v*`，不修改生产配置，不执行 force push。

## Acceptance Criteria

- [x] AC1：独立审查无未解决阻塞发现，候选 SHA 已明确记录。
- [x] AC2：`origin/master` 等于候选 SHA，精确 SHA 的 `Rust CI` 全部成功。
- [x] AC3：annotated tag `v0.7.28` 的本地与远端 peeled SHA 均等于候选 SHA。
- [x] AC4：精确 Tag/SHA 的 `Release Aether` 成功。
- [x] AC5：GitHub Release 存在；`aether-v0.7.28-linux-amd64.tar.gz`、`aether-v0.7.28-linux-arm64.tar.gz`、`install.sh`、`SHA256SUMS` 均存在且非空。
- [x] AC6：两个 tarball 的 GitHub digest 与 `SHA256SUMS` 声明一致。
- [ ] AC7：发布任务归档、journal 完成，最终 `master` 与 `origin/master` 一致且工作区干净。

## Out of Scope

- 发布 aether-tunnel 或创建 `tunnel-v*`。
- 未被审查发现所要求的新功能、重构或版本命名变更。
- 本地全量 Rust 编译；最终认证由 GitHub 精确 SHA CI 提供。
