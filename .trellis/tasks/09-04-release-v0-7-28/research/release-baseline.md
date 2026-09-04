# v0.7.28 发布基线

## 现场状态（2026-09-04）

- Repository：`ZipperCode/Aether`
- Branch：`master`
- Candidate HEAD：`83c3058b76fc785d91526b4474a1d040bda48de1`
- Remote master：`0e78031b4c880f107e27fbadc471dcd61c35aaf4`
- Latest application tag/release：`v0.7.27`
- Candidate tag：`v0.7.28`（检查时不存在）
- GitHub CLI：已登录 `ZipperCode`，Git 使用 SSH。

## 当前工作流

- `.github/workflows/rust-ci.yml`：workflow 名 `Rust CI`；push 到 `master`/`main` 且命中 `frontend/**` 等路径时触发。
- `.github/workflows/release.yml`：workflow 名 `Release Aether`；应用 Tag `v*` 触发。
- `.github/workflows/build-tunnel.yml`：独立 tunnel 发布，本任务不触发。

## 既有发布合同

上一版 `v0.7.27` 使用：精确 SHA push → `Rust CI` success → annotated app Tag → `Release Aether` success → 4 个资产与 checksum/digest 验证。当前代码、远端和 GitHub API 已重新核对，版本号与 SHA 不直接复用。

## 发布结果（2026-09-04）

- Review：PASS，无阻塞发现。
- Candidate/Tag peeled SHA：`83c3058b76fc785d91526b4474a1d040bda48de1`。
- Rust CI run `33825487633`：23/23 成功。
- Release Aether run `33826435698`：7/7 成功。
- GitHub Release：`v0.7.28`，非草稿、非预发布，4 个非空资产。
- 两个 tarball 的本地 SHA-256、GitHub digest 与 `SHA256SUMS` 三方一致；压缩包结构检查通过。
