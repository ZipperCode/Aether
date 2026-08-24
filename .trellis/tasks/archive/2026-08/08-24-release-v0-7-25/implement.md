# Aether v0.7.25 执行计划

## 1. 修复前端 CI 覆盖

- 修改 `.github/workflows/rust-ci.yml` 的 push/pull_request paths。
- 新增 Frontend job，复用 Node 22、npm cache 和现有 package scripts。
- 将 Frontend job 纳入最终 `check`。
- 本地运行 YAML 解析、`git diff --check`，并按需执行与 CI 相同的前端命令。

## 2. 检查、提交并推送

- 由 `trellis-check` 只审查 CI workflow 改动和门禁完整性。
- 中文提交 CI 修复；任务工件不进入 release SHA。
- 普通推送 `master`，确认 `origin/master` 精确等于新 SHA。

## 3. 等待精确 SHA 的 Rust CI

- 有界查询 `ZipperCode/Aether`、`rust-ci.yml`、push/master/精确 SHA。
- 使用 `gh run watch --exit-status` 等待完成。
- 核对全部 jobs，特别是 `Frontend` 与 `check`。
- 失败时只修首个真实根因；新提交后重新锁定 SHA，不复用失败 SHA 的结论。

## 4. Tag 前二次检查

- 工作区除任务工件外无产品/配置改动。
- 本地 HEAD 与远端 master 等于 release SHA。
- 本地与远端 `v0.7.25` 均不存在。

## 5. 创建并推送 Tag

```powershell
git tag -a v0.7.25 $releaseSha -m "发布 v0.7.25"
git push origin refs/tags/v0.7.25:refs/tags/v0.7.25
```

- 验证本地 tag 类型为 `tag`。
- 验证本地与远端 peeled commit 均等于 release SHA。

## 6. 验证 Release Aether

- 查找 `ZipperCode/Aether` 中 `headBranch=v0.7.25`、`headSha=release SHA` 的 release run。
- 等待 7 个 jobs 全部 success。
- 核验 GitHub Release 非 draft、非 prerelease，以及四个必需资产的名称、状态和非零大小。

## 7. Trellis 收尾

- 更新任务验收证据。
- 判断无需新增代码规范后，提交任务工件、归档、记录日志并推送这些发布后 bookkeeping commits。

## 停止条件

- 远端 master 或 Tag 与预期 SHA 不一致。
- CI/Release 失败、取消、超时或 SHA 不匹配。
- 公开 Tag 已推送后出现失败。
- 权限、网络或 GitHub API 无法给出实时证据。
