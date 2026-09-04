# 发布设计

## 发布锁定

发布候选初始锁定为 `83c3058b76fc785d91526b4474a1d040bda48de1`。若审查产生修复提交，锁定值更新为修复后的 HEAD；后续所有分支、CI、Tag 和 Release 校验都使用同一精确 SHA。

## 门禁顺序

```text
审查 origin/master..候选 SHA
  → push origin master
  → 验证 origin/master == 候选 SHA
  → 等待 Rust CI(push/master/候选 SHA) success
  → 创建 annotated v0.7.28
  → 只推送 refs/tags/v0.7.28
  → 等待 Release Aether(v0.7.28/候选 SHA) success
  → 验证 Release 与资产 digest
  → Trellis 归档、journal、推送 bookkeeping
```

任何一步失败都停留在当前门禁处理；CI 失败前不创建 Tag。Tag 推送后的 Release 失败不移动或删除 Tag，除非用户另行明确授权。

## 资产验证

通过 GitHub API 读取 Release 资产的名称、大小、下载 URL 和 digest，要求资产名集合精确包含：

- `aether-v0.7.28-linux-amd64.tar.gz`
- `aether-v0.7.28-linux-arm64.tar.gz`
- `install.sh`
- `SHA256SUMS`

下载 `SHA256SUMS` 与两个 tarball 到系统临时目录，计算本地 SHA-256，并同时对照 GitHub asset digest；验证后清理临时目录。

## 兼容与回滚

- Tag 前可通过普通修复提交更新候选 SHA 并重新走 CI。
- Tag 后不覆盖发布历史；若 Release 失败，保留 Tag 并报告具体 job/资产状态。
- 发布版本来自 Git Tag，不修改 Cargo 包内部 `0.1.0` 版本。
