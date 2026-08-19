# v0.7.24 发布路径调查

调查时间：2026-08-19（Asia/Shanghai）。所有远端查询均为只读；未记录 remote URL、token 或 credential。

## Git 现场

- `git rev-parse HEAD`：`05e15a5df4660dea3d00b4c9178648412675d8c4`。
- `git rev-parse origin/master` 与 `git ls-remote origin refs/heads/master`：`c73e865adcf2922160f13ae261a9bace89c29882`。
- `git status --short --branch`：`master...origin/master [ahead 4]`；唯一工作区项为未跟踪的本任务目录。
- 超前提交：`793ac8625`、`0a26f87d9`、`c1c6ee602`、`05e15a5df`。
- `git diff --name-status origin/master..HEAD` 显示候选提交包含 `apps/aether-gateway/**` 和 `crates/**`，会命中 Rust CI 的路径过滤。
- 本地 `git show-ref` 与远端 `git ls-remote --tags` 均未发现 `v0.7.24`。

## GitHub 与 CI

- `gh --version`：2.96.0；`gh repo view` 可读取 `ZipperCode/Aether`，默认分支为 `master`。
- 活跃 workflow：`Rust CI`、`Release Aether`、`Build aether-tunnel`。
- `.github/workflows/rust-ci.yml:3-20`：push 监听 `master`/`main`，路径包含 `Cargo.toml`、`Cargo.lock`、`crates/**`、`apps/**` 与 workflow 文件。
- `.github/workflows/rust-ci.yml:666-683`：最终 `check` job 依赖 `fmt`、`clippy`、`test`、`data_db_smoke`，并在任一依赖非 success 时失败。
- `gh api .../branches/master/protection` 返回 `404 Branch not protected`；repository ruleset 数量为 0。
- 当前远端 master 对应 `Rust CI` run `32213575607`，event=`push`、headBranch=`master`、headSha=`c73e865...`，22 个 jobs 全部 success，run URL 为 `https://github.com/ZipperCode/Aether/actions/runs/32213575607`。这只证明现有远端基线，不能替代候选 SHA 推送后的新 run。

## Tag 与发布历史

- 最新稳定 tag/release 为 `v0.7.23`，peeled commit 为 `13d0e3ba915763ca6633647f2d8b0b3f2f4913cb`。
- `git cat-file -p v0.7.23` 证明它是 annotated tag，消息为 `发布 v0.7.23`；`v0.7.22` 也是 annotated tag。`v0.7.20` 是历史上的 lightweight 例外。
- `.github/workflows/release.yml:3-14`：push tag `v*` 触发 `Release Aether`，并使用按 ref 分组且不取消在途 run 的 concurrency。
- release preflight 只接受 `vX.Y.Z`、`vX.Y.Z-beta.N` 或 `vX.Y.Z-rc.N`；稳定版设置 `make_latest=true`。
- 最近 `v0.7.23` release run `31585427886` 的 7 个 jobs 全部 success：preflight、frontend、linux-amd64、linux-arm64、Docker multi-arch、tarballs、GitHub Release assets。
- `gh release view v0.7.23` 显示 release 非 draft、非 prerelease，并有 4 个 uploaded assets：两个架构 tarball、`install.sh`、`SHA256SUMS`。
- `.github/workflows/build-tunnel.yml:3-6` 只监听 `tunnel-v*`，与 Aether 应用 `v0.7.24` 无关。

## 结论

- 最小安全发布机制是：锁定 SHA → 普通 push master → 等待该 SHA 的 Rust CI 全绿 → annotated tag 同一 SHA → 推送单一 tag → 验证 Release Aether。
- GitHub 没有服务器侧 master 保护，因此执行代理必须自行坚持精确 SHA 和 CI 成功门禁。
- 用户已确认完整发布门禁：tag push 后必须等待整个 `Release Aether` 成功，并核验 GitHub Release 非 draft、非 prerelease以及两个版本 tarball、`install.sh`、`SHA256SUMS` 4 个 uploaded assets；仅确认 workflow 触发不构成完成。

## 发布执行与 Phase 2.2 独立复核

复核时间：2026-08-19（Asia/Shanghai）。复核查询均为只读，未记录 remote URL、token 或 credential。

- 锁定 SHA、本地 `HEAD`、本地 tracking ref 与实时 `origin/master` 均为 `05e15a5df4660dea3d00b4c9178648412675d8c4`。执行前远端基线 `c73e865adcf2922160f13ae261a9bace89c29882` 是该 SHA 的祖先，`origin/master` reflog 记录为 `update by push`。
- [`Rust CI` run 32253938997](https://github.com/ZipperCode/Aether/actions/runs/32253938997) 为 `event=push`、`headBranch=master`、精确 SHA、`completed/success`；22/22 jobs 均成功，包含汇总 job `check`。run 于 `2026-08-19T12:58:59Z` 完成。
- `v0.7.24` 是 annotated tag，tag object 为 `bdcb45e94129a11fec00a1c66c86d6306236357c`，消息为 `发布 v0.7.24`；本地与远端 peeled SHA 均为锁定 SHA。tagger 时间为 `2026-08-19T21:01:07+08:00`，晚于 Rust CI 完成。
- [`Release Aether` run 32255755866](https://github.com/ZipperCode/Aether/actions/runs/32255755866) 为 `event=push`、`headBranch=v0.7.24`、精确 SHA、`completed/success`；7/7 jobs 均成功。
- [GitHub Release v0.7.24](https://github.com/ZipperCode/Aether/releases/tag/v0.7.24) 非 draft、非 prerelease，且恰有 4 个 assets：`aether-v0.7.24-linux-amd64.tar.gz`、`aether-v0.7.24-linux-arm64.tar.gz`、`install.sh`、`SHA256SUMS`，状态均为 `uploaded`。
- 本地和远端均不存在 `tunnel-v0.7.24`；产品代码、Cargo 文件与 `.github/workflows/**` 的 tracked/staged diff 均为空。
