# 执行计划

## CI 修复

- [x] 推送初始同步结果并锁定失败 run `33692043056`。
- [x] 补齐图片流和直接流执行 Key 目录夹具。
- [x] 定位并修复 tunnel hub 被 data state 重建清空、非法 SSE 导致预提交互锁的问题；WSL 相关测试 7/7 通过。
- [x] 将本地 tunnel 测试顺序合同写入现有 Provider Pool spec。
- [x] 修复 deferred pending 测试，使其同时满足真实 Key 准入和等待 headers 的业务目标。
- [x] 补齐同步执行与 heartbeat 的强准入 Key 目录夹具，并为等待上游的测试增加有限超时。
- [x] 完成 `cargo fmt --all --check` 和 `git diff --check`；按用户要求停止本地编译，后续仅以 GitHub CI 认证。

## 发布

- [ ] 提交并推送剩余修复，等待精确 SHA 的 `Rust CI` 全绿。
- [ ] 创建并推送 annotated tag `v0.7.27`。
- [ ] 等待精确 tag/SHA 的 `Release Aether` 成功。
- [ ] 核验 GitHub Release URL、资产名称、非零大小和 `SHA256SUMS`。
- [ ] 清理 WSL `/tmp/aether-ci-gateway-tmV4TK`。
- [ ] 更新验收证据，归档任务并记录 journal；若 bookkeeping 提交在 tag 后产生，再推送 `master` 并说明 tag 仍锁定已认证发布 SHA。
