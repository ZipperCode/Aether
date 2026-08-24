# v0.7.25 发布现场证据

- 2026-08-24 最新本地/远端稳定 App Tag：`v0.7.24`。
- `v0.7.25` 在本地和 `origin` 均未占用。
- 所有既有内容已提交并推送；基线候选 SHA 为 `38b1516cb898dbc500568c871d3f361e271aaa5e`。
- `origin/master` 已现场核对等于该 SHA。
- `gh` 已认证到 `ZipperCode`；自动仓库识别曾指向 `fawney19/Aether`，因此所有后续 GitHub 命令必须显式使用 `--repo ZipperCode/Aether`。
- 对基线 SHA 查询 `Rust CI` 精确 run 返回空数组。
- 根因：`.github/workflows/rust-ci.yml` 的 paths 未包含 `frontend/**`；最近产品变更是 frontend-only，随后提交是 `.trellis/workflow.md`，两者都不会触发现有 Rust CI。
- `.github/workflows/release.yml` 监听 `v*`，稳定 Tag 预期运行 7 个 jobs 并发布两个 tarball、`install.sh`、`SHA256SUMS`。
- 上一稳定发布 `v0.7.24` 使用 annotated tag 和消息 `发布 v0.7.24`，本次沿用为 `发布 v0.7.25`。

## 最终发布证据

- 最终 release SHA：`92def9b5012f0e5c2db148aa45310de675d51bf5`；Tag 前本地 `HEAD` 与 `origin/master` 均精确等于该 SHA。
- 最终 Rust CI：run `32708090808`，`event=push`、`headBranch=master`、`headSha=92def9b5012f0e5c2db148aa45310de675d51bf5`，completed/success，23/23 jobs success。
- CI 失败处理保留了每轮新 SHA 门禁，修复了 Gateway Clippy 多余借用、取消终态测试绕过上层保护器、公共响应体上限被 scoped resolver 忽略，以及候选队列二分隔离后的陈旧调用次数/指标断言。
- annotated tag：`v0.7.25`，消息 `发布 v0.7.25`，tag object `fca78269261ab0c7a6b746935891275d0fda17fb`；本地与远端 peeled SHA 均为 release SHA。
- Release Aether：run `32709769836`，`headBranch=v0.7.25`、`headSha=92def9b5012f0e5c2db148aa45310de675d51bf5`，completed/success，7/7 jobs success。
- GitHub Release：`https://github.com/ZipperCode/Aether/releases/tag/v0.7.25`，`draft=false`、`prerelease=false`，发布时间 `2026-08-24T09:25:57Z`。
- 发布资产均为 `uploaded` 且非零：
  - `aether-v0.7.25-linux-amd64.tar.gz`：44,299,083 B。
  - `aether-v0.7.25-linux-arm64.tar.gz`：40,396,185 B。
  - `install.sh`：66,689 B。
  - `SHA256SUMS`：200 B。
- 未创建 `tunnel-v0.7.25`，未执行 force push、历史改写、Tag 移动或删除。
