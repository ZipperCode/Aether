# 发布 Aether v0.7.24

## Goal

将当前 `master` 的候选提交安全推送到 GitHub；只有该精确提交的发布前 CI 全部成功后，才创建并推送 Aether 应用 tag `v0.7.24`；等待 `Release Aether` 全部成功并核验完整发布产物后，任务才算完成。

## Background

- 2026-08-19 调查时，本地 `master` 的 `HEAD` 为 `05e15a5df4660dea3d00b4c9178648412675d8c4`，远端 `master` 为 `c73e865adcf2922160f13ae261a9bace89c29882`，本地超前 4 个提交。
- 这 4 个提交包含 `apps/**` 和 `crates/**` 变更，会命中 `.github/workflows/rust-ci.yml` 的 `master` push 与路径过滤条件。
- GitHub 上 `master` 没有 branch protection，也没有 repository ruleset；因此发布前门禁由本任务显式规定，而不是由 GitHub 强制。
- 当前最新稳定应用 tag/release 是 `v0.7.23`。`v0.7.22`、`v0.7.23` 都是 annotated tag；`v0.7.23` 的 tag 消息为 `发布 v0.7.23`。
- `.github/workflows/release.yml` 监听 `v*` 并发布 Aether；`.github/workflows/build-tunnel.yml` 只监听 `tunnel-v*`，不属于本任务。
- 执行前远端和本地均不存在 `v0.7.24`。GitHub CLI 2.96.0 已认证并可读取 `ZipperCode/Aether`。
- 用户随后批准最终规划并授权执行普通 push、annotated tag 与完整发布核验。
- 用户确认的完整发布门禁已经满足：`Release Aether` 的 7 个 jobs 全部成功，GitHub Release 及 4 个必需 assets 均已核验。

详细调查证据见 `research/release-evidence.md`。

## Requirements

- **R1 候选提交锁定**：执行开始时记录唯一 `release SHA`；后续 CI 查询、tag 创建和远端核验都必须使用该 SHA，不使用会漂移的分支名替代。
- **R2 推送前检查**：重新核对当前分支、工作区、`HEAD`、远端 `master`、提交祖先关系以及本地/远端 `v0.7.24` 冲突。除本任务工件外出现未预期改动，或远端已变化时停止。
- **R3 普通 push**：只执行 `git push origin master`；禁止 force push、历史重写、修改既有 tag 或夹带新的产品改动。
- **R4 发布前 CI 门禁**：只接受 `event=push`、`headBranch=master`、`headSha=release SHA` 的 `Rust CI` run。run 必须 completed/success，汇总 job `check` 和全部实际 jobs 必须 success；失败、取消、超时、找不到对应 run 或 SHA 不匹配时均不得创建 tag。
- **R5 Tag 契约**：CI 成功后再次确认远端 `master` 仍等于 `release SHA`，且本地/远端仍不存在 `v0.7.24`；随后在该 SHA 创建 annotated tag，消息为 `发布 v0.7.24`，并只推送该精确 tag ref。
- **R6 完整发布门禁**：tag push 后验证远端 tag 的 peeled commit 等于 `release SHA`，且 `Release Aether` 已为 `v0.7.24` 和该 SHA 创建 push run；必须等待 7 个 jobs 全部 success，并确认 GitHub Release 非 draft、非 prerelease，且 amd64/arm64 tarball、`install.sh`、`SHA256SUMS` 4 个 assets 均为 `uploaded`，之后任务才算完成。
- **R7 敏感信息**：命令和最终报告不得输出 credential、token、Cookie 或 remote URL 中的 userinfo。

## Acceptance Criteria

- [x] **AC1 / R1-R3**：`origin/master` 通过普通 push 精确指向锁定的 `release SHA`，没有 force push、历史重写或未预期产品改动。
- [x] **AC2 / R4**：该 SHA 的 `Rust CI` push run 为 success；汇总 `check` 和全部 jobs 均为 success。若不满足，远端和本地均没有新建 `v0.7.24`。
- [x] **AC3 / R5**：`v0.7.24` 只在 AC2 满足后创建；本地 annotated tag 与远端 peeled tag 均精确指向同一 `release SHA`。
- [x] **AC4 / R6**：`Release Aether` 对 `v0.7.24` 正确触发，run 对应 `release SHA`，7 个 jobs 全部 success；最终报告包含 run URL、结论与 SHA 证据。
- [x] **AC5 / R6**：GitHub Release 非 draft、非 prerelease，且 `aether-v0.7.24-linux-amd64.tar.gz`、`aether-v0.7.24-linux-arm64.tar.gz`、`install.sh`、`SHA256SUMS` 4 个 assets 均为 `uploaded`。
- [x] **AC6 / R7**：未创建 `tunnel-v*`，未修改产品代码或 workflow，未泄露凭证。

## Out of Scope

- `aether-tunnel` 的独立发布。
- 修改产品代码、GitHub workflow、全局配置、凭证或历史提交。
- 修改、移动或删除任何既有 tag；force push 或用管理权限绕过失败门禁。
- CI 或 release 失败后未经重新评估直接修复产品、重跑 workflow、移动/删除已推送 tag。
