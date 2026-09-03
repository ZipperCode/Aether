# 执行计划

## CI 修复

- [x] 推送初始同步结果并锁定失败 run `33692043056`。
- [x] 补齐图片流和直接流执行 Key 目录夹具。
- [x] 定位并修复 tunnel hub 被 data state 重建清空、非法 SSE 导致预提交互锁的问题；WSL 相关测试 7/7 通过。
- [x] 将本地 tunnel 测试顺序合同写入现有 Provider Pool spec。
- [x] 修复 deferred pending 测试，使其同时满足真实 Key 准入和等待 headers 的业务目标。
- [x] 补齐同步执行与 heartbeat 的强准入 Key 目录夹具，并为等待上游的测试增加有限超时。
- [x] 完成 `cargo fmt --all --check` 和 `git diff --check`；按用户要求停止本地编译，后续仅以 GitHub CI 认证。
- [x] GitHub run `33704528677` 定位 2 个视频跟进请求旧夹具；补齐其实时 Key 目录，tag 仍未创建。

## 发布

- [x] 提交并推送剩余修复；GitHub `Rust CI` run `33705601372` 在 `e8400c35ebb397c607614b45dcb644d73b7b7db2` 全绿。
- [x] 创建并推送 annotated tag `v0.7.27`，标签说明为 `发布 v0.7.27`。
- [x] 精确 tag/SHA 的 `Release Aether` run `33706688704` 成功。
- [x] 核验 GitHub Release URL、4 个非空资产，两个 tarball 的 `SHA256SUMS` 与 GitHub 服务端 digest 一致。
- [x] 清理 WSL `/tmp/aether-ci-gateway-tmV4TK` 和 Windows Release 校验临时目录。
- [x] 更新验收证据，归档任务并记录 journal；bookkeeping 提交不改变 tag 锁定的已认证 SHA。

## 发布证据

- Rust CI：`https://github.com/ZipperCode/Aether/actions/runs/33705601372`
- Release workflow：`https://github.com/ZipperCode/Aether/actions/runs/33706688704`
- GitHub Release：`https://github.com/ZipperCode/Aether/releases/tag/v0.7.27`
- `amd64`：44,690,333 bytes，SHA-256 `f4f11c344d10ae2469aac6ec3410a8813dffef0bda713a483ee6b5665c4f6ebb`
- `arm64`：40,880,896 bytes，SHA-256 `31d53345645332c7d2a4a63afdee3f619a8545972741f0edddea705182da8fad`
