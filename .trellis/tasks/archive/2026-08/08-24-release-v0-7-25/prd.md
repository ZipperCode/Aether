# 发布 Aether v0.7.25

## Goal

补齐前端变更缺失的 GitHub CI 门禁，并只从精确通过完整 CI 的 `master` SHA 创建 annotated tag `v0.7.25`；等待 `Release Aether` 全部成功并核验发布资产后才完成任务。

## Background

- 当前最新稳定 App Tag 为 `v0.7.24`，本地和远端均不存在 `v0.7.25`。
- 用户要求提交所有现有内容、检查并处理 GitHub CI、随后发布新 Tag。
- 当前全部既有内容已提交并推送，`origin/master` 曾精确指向 `38b1516cb898dbc500568c871d3f361e271aaa5e`。
- 该 SHA 没有对应的 `Rust CI` run，因为 `.github/workflows/rust-ci.yml` 的 push/pull_request 路径只覆盖 Rust、`apps/**` 和 workflow 自身，未覆盖本次 `frontend/**` 逻辑改动。
- `Release Aether` 监听 `v*`，稳定 Tag 会构建前端、amd64/arm64 Gateway、multi-arch Docker、tarball 和 GitHub Release 资产。
- GitHub CLI 已认证到 `ZipperCode`；所有 GitHub 查询必须显式指定 `--repo ZipperCode/Aether`，不得依赖自动识别的上游仓库。

## Requirements

- **R1 CI 覆盖**：现有 `Rust CI` 必须监听 `frontend/**`，并在同一 workflow 内运行 `npm ci`、前端类型检查、前端测试和生产构建；汇总 `check` 必须依赖该 Frontend job。
- **R2 最小修改**：只修改现有 `.github/workflows/rust-ci.yml`，复用 `frontend/package.json` 已有脚本，不新增 workflow、依赖、产品代码或兼容路径。
- **R3 精确 SHA 门禁**：CI 修复提交推送后锁定新的唯一 release SHA；只接受 `repo=ZipperCode/Aether`、`event=push`、`headBranch=master`、`headSha=release SHA` 的 `Rust CI` run。
- **R4 CI 失败处理**：Tag 创建前若任一 job 失败，定位首个真实失败并在已授权范围内做最小修复；每次修复产生新 SHA 并重新走完整 CI。不得用旧 run 或其他 SHA 代替。
- **R5 Tag 契约**：只有精确 SHA 的全部 CI jobs（含 `Frontend` 与汇总 `check`）成功后，才创建 annotated tag `v0.7.25`，消息为 `发布 v0.7.25`，并只推送该 tag ref。
- **R6 完整发布门禁**：远端 peeled tag 必须等于 release SHA；`Release Aether` 对该 Tag/SHA 的 7 个 jobs 必须全部成功；GitHub Release 必须非 draft、非 prerelease，并包含两个 tarball、`install.sh`、`SHA256SUMS` 四个 uploaded assets。
- **R7 安全边界**：禁止 force push、历史改写、移动/删除既有或已公开 Tag、输出凭证，或把 `tunnel-v*` 纳入本次发布。

## Acceptance Criteria

- [x] `rust-ci.yml` 对 `frontend/**` 变更触发，并有 Frontend job 执行安装、类型检查、测试和构建。
- [x] 最终 release SHA 已普通推送且精确等于 `origin/master`。
- [x] 最终 SHA 的 `Rust CI` completed/success，所有 jobs（含 `Frontend`、`check`）均 success。
- [x] annotated `v0.7.25` 只在 CI 全绿后创建并推送，本地与远端 peeled SHA 均等于 release SHA。
- [x] `Release Aether` 对 `v0.7.25` completed/success，7 个 jobs 全部成功。
- [x] GitHub Release 非 draft/非 prerelease，四个必需资产均为 uploaded。
- [x] 未创建 `tunnel-v*`，未强推、改写历史、移动或删除公开 Tag，未泄露凭证。

## Out of Scope

- Aether Tunnel 发布。
- 修改产品功能、数据库、运行时配置或发布产物结构。
- 在公开 Tag 已推送后自动移动/删除 Tag；该阶段失败只报告证据并停止。
