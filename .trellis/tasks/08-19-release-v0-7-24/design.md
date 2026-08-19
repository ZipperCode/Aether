# Aether v0.7.24 发布设计

## 边界与最小机制

本任务是单一、严格串行的发布操作，不拆分父子任务，也不新增脚本或抽象。直接复用 Git、GitHub CLI 和现有两个 workflow：

1. 锁定候选 commit。
2. 普通推送 `master`。
3. 等待该精确 commit 的 `Rust CI` 全绿。
4. 在同一 commit 创建并推送 annotated tag。
5. 验证 tag 对应的 `Release Aether`。

任何一步失败都停止后续步骤；尤其不能以“旧 run 成功”或“分支当前看起来正常”替代精确 SHA 门禁。

## Commit 锁定契约

- 执行开始时以 `git rev-parse HEAD` 得到不可变的 `release SHA`。
- push 前要求远端 `master` 仍为调查时的预期基线或其经现场重新确认的安全基线，且该远端提交是 `release SHA` 的祖先。
- push 后通过 `git ls-remote origin refs/heads/master` 直接确认远端等于 `release SHA`。
- 等待 CI 和创建 tag 时都显式传入 `release SHA`；若本地 `HEAD` 或远端 `master` 在等待期间变化，停止并重新评估，不对旧判断继续发布。

## 发布前 CI 判定

`.github/workflows/rust-ci.yml` 在 `master` push 且变更命中 `Cargo.toml`、`Cargo.lock`、`crates/**`、`apps/**` 或 workflow 本身时运行。本次候选提交包含 `apps/**` 与 `crates/**`，因此应产生一个对应的 push run。

GitHub 没有为 `master` 配置保护或 ruleset，所以本任务采用更明确的应用层门禁：

- workflow：`Rust CI`
- event：`push`
- branch：`master`
- head SHA：精确等于 `release SHA`
- run：`completed` 且 `conclusion=success`
- jobs：汇总 `check=success`，并且 run 中全部实际 jobs 的 conclusion 都为 `success`

`check` 依赖 `Format`、`Clippy`、`Test`、`Data DB Smoke` 四个汇总分支；后三个汇总 job 又依赖各自矩阵和数据库 smoke jobs。任一依赖失败都会使 `check` 失败。找不到 run、仍排队、取消、超时或任何非 success 都是停止条件。

## Tag 契约

- 名称：`v0.7.24`，符合 release preflight 的稳定版格式 `vX.Y.Z`。
- 类型：annotated tag，复用最新 `v0.7.22`/`v0.7.23` 的主流发布方式。
- 目标：显式的 `release SHA`，不依赖当时的 `HEAD`。
- 消息：`发布 v0.7.24`，复用 `v0.7.23` 约定。
- 推送：只推送 `refs/tags/v0.7.24`，不使用 `--tags`。

创建前后都验证 tag 冲突和 peeled commit。tag 一旦推送即视为不可移动的公开发布标识。

## Release 触发与产物

`.github/workflows/release.yml` 监听 `v*` push，稳定 tag 会执行：

1. `Release preflight`
2. `Build frontend`
3. `Build linux-amd64`
4. `Build linux-arm64`
5. `Docker multi-arch`
6. `Release tarballs`
7. `GitHub Release assets`

最近的 `v0.7.23` run 七个 jobs 全部成功，GitHub Release 包含：

- `aether-v0.7.23-linux-amd64.tar.gz`
- `aether-v0.7.23-linux-arm64.tar.gz`
- `install.sh`
- `SHA256SUMS`

对 `v0.7.24` 的对应期望是版本号替换后的两个 tarball，以及同名脚本和校验文件。release workflow success 同时作为 Docker multi-arch 发布成功的证据；GitHub Release assets 另行通过 `gh release view` 核验。

## 失败与回滚边界

- **push 失败或远端漂移**：停止；不 force push，不改写本地历史。
- **Rust CI 失败/取消/超时**：停止且不创建 tag；保存 run URL 和失败 job，产品修复必须另行评估。
- **本地 tag 已创建但 tag push 失败**：保留该本地 tag 供诊断，不移动、不删除、不改名；先报告失败。
- **tag 已推送后 release 失败**：不移动或删除公开 tag，不自动重跑 workflow，不创建替代 tag；报告失败 run 和已产生的产物，等待用户决定恢复路线。
- **信息安全**：只报告 remote 名 `origin`、owner/repo、SHA 和 GitHub 页面 URL，不展示 remote fetch/push URL 或认证材料。

## 固定完成边界

用户已确认完整发布门禁：必须等待 `Release Aether` 的 7 个 jobs 全部 success，并核验 GitHub Release 非 draft、非 prerelease，且两个 `v0.7.24` tarball、`install.sh`、`SHA256SUMS` 均为 `uploaded`。只看到 workflow 被触发或仍在运行不构成任务完成。
