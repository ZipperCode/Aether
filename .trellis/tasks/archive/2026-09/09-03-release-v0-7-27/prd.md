# 发布 v0.7.27 并修复同步后 CI

## 目标

修复上游功能同步后由 GitHub Rust CI 暴露的过时测试夹具，在精确 `master` 提交通过全部 CI 后发布下一版本 `v0.7.27`，并验证 Release workflow 与产物。

## 需求

- 保留 Provider Key 强一致准入、流式预提交门禁和本地隧道生产语义，只修复不再符合合同的测试及必要架构扫描。
- CI 失败必须定位实体 job；汇总 job 的连带失败不重复修复。
- 本地使用 Linux/WSL 按 CI 条件验证相关测试，临时构建产物放在 `/tmp` 并在完成后清理。
- 推送 `master` 后仅以该精确 SHA 的 `Rust CI` 成功为发布门禁。
- 最新稳定 tag 为 `v0.7.26`；仅在门禁成功后创建带注释 tag `v0.7.27`，说明为 `发布 v0.7.27`，并只推送该 tag。
- tag 推送后监控 `Release Aether` 到成功，核验 GitHub Release、非空资产、校验文件以及 tag/SHA 一致。
- 不 force push，不改生产配置，不引入范围外功能。

## 验收标准

- [x] AC1：所有 CI 证实的过时测试夹具已修复，生产路径语义未放宽。
- [x] AC2：最终 GitHub `Rust CI` 在精确发布 SHA 上通过。
- [x] AC3：创建 tag 时，远端 `master`、本地 HEAD 与发布 SHA 一致，且工作区干净。
- [x] AC4：注释 tag `v0.7.27` 精确指向通过 CI 的发布 SHA，并已推送到 `origin`。
- [x] AC5：`Release Aether` 对 `v0.7.27` 成功，Release 产物存在且均非空，校验清单与服务端 digest 一致。
- [x] AC6：任务归档、journal 记录和本次任务临时目录清理完成。
