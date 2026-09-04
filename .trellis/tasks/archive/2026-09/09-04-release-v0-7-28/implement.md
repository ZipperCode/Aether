# 执行计划

1. 独立审查 `origin/master..HEAD`，核对任务需求、代码、测试及无秘密/调试残留。
2. 复用已完成的目标 Vitest、前端类型检查和 diff 检查证据；审查若改代码则重跑相应最小验证。
3. 锁定候选 SHA，推送 `master`，校验远端 SHA。
4. 查询并等待精确 SHA 的 `Rust CI` 完成；检查所有 job 结论。
5. 确认 `v0.7.28` 不存在，创建并推送 annotated tag。
6. 查询并等待精确 Tag/SHA 的 `Release Aether` 完成；检查所有 job 结论。
7. 验证 GitHub Release、4 个资产、文件大小和两份 tarball checksum/digest；清理临时目录。
8. 更新本任务验收与证据，提交发布记录，归档任务并记录 journal。
9. 推送最终 bookkeeping，确认本地/远端 master、Tag 与工作区状态。

## 已知验证证据

- `npm run test:run -- src/features/providers/components/__tests__/BatchAssignModelsDialog.loading.spec.ts`：4/4 通过。
- `npm run type-check`：通过。
- `git diff --check`：通过。

## 审查结果

- 独立审查结论：PASS，无阻塞发现；未发现调试残留、类型绕过或敏感信息。
- 认证候选 SHA：`83c3058b76fc785d91526b4474a1d040bda48de1`。
- `origin/master` 推送后精确等于候选 SHA。

## 发布证据

- Rust CI：`https://github.com/ZipperCode/Aether/actions/runs/33825487633`，精确候选 SHA，23/23 Job 成功。
- Annotated tag：`v0.7.28`，远端 peeled SHA 为 `83c3058b76fc785d91526b4474a1d040bda48de1`。
- Release workflow：`https://github.com/ZipperCode/Aether/actions/runs/33826435698`，精确 Tag/SHA，7/7 Job 成功。
- GitHub Release：`https://github.com/ZipperCode/Aether/releases/tag/v0.7.28`，非草稿、非预发布。
- `aether-v0.7.28-linux-amd64.tar.gz`：44,690,692 bytes，SHA-256 `5a6c710a40cf9212344e6e474532f2ea462afefc3d6e91036879927f27a9da5f`。
- `aether-v0.7.28-linux-arm64.tar.gz`：40,881,944 bytes，SHA-256 `c4b837e0a8087bc33f3ea9bcfdb72a28228763b2d96ebc313bc5256d0f6a04c2`。
- `install.sh`：66,689 bytes，SHA-256 `9f15fba334be455bea249ddfc5bc73474b69cdbe954c97b3f8738a7dcee8e69d`。
- `SHA256SUMS`：200 bytes，SHA-256 `6815da853f7927b0a9a70118d99997f1d9c241d5f0d664fc738719c28ca1b4e1`。
- 4 个下载文件的本地 SHA-256 均与 GitHub digest 一致；`SHA256SUMS` 含 2 个 tarball 条目且均匹配。
- 两个 tarball 均可读取，各含 247 个条目，网关二进制、前端、安装/更新脚本、Compose、环境示例、README 和 LICENSE 均存在。
- 未创建或发布任何 `tunnel-v*` Tag。

## 待收口

- 归档任务、记录 journal，并推送 bookkeeping 提交。
- 本次资产验收目录位于系统临时目录；递归和逐文件清理均被当前执行策略拒绝，未执行删除。
