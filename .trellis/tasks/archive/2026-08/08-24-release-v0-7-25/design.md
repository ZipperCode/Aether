# Aether v0.7.25 发布设计

## 最小 CI 修复

继续使用 `.github/workflows/rust-ci.yml` 作为发布前唯一汇总门禁，不创建平行 workflow：

1. 在 push 和 pull_request 的 paths 中加入 `frontend/**`。
2. 新增单一 `Frontend` job，使用 Node 22 与 npm lockfile cache。
3. 依次执行 `npm ci`、`npm run type-check`、`npm run test:run`、`npm run build`。
4. 将 Frontend job 加入汇总 `check.needs`，并在 Verify required jobs 中要求 success。

这样既能让本次 workflow 修改自身触发精确 SHA CI，也能让未来 frontend-only 变更进入同一发布门禁。项目的 `npm run lint` 带 `--fix`，不得放入只读 CI。

## 发布串行契约

1. 提交并普通推送 CI 修复。
2. 以推送后的不可变 commit 锁定 release SHA。
3. 等待 `ZipperCode/Aether` 中该 SHA 的 `Rust CI` 全部成功。
4. 二次确认本地 HEAD、远端 master、工作区与 Tag 冲突均未漂移。
5. 在 release SHA 创建 annotated `v0.7.25`，仅推送该 tag ref。
6. 等待该 Tag/SHA 的 `Release Aether` 全部成功并核验 GitHub Release 资产。

任何一步都不得用分支当前值、旧 run 或其他 SHA 替代精确 SHA 判断。

## CI 失败边界

- Tag 前失败：查看失败 job/step/log，修复根因，提交并推送新 SHA，然后重新等待完整 CI。
- Tag 推送后失败：公开 Tag 不可移动或删除；保留 run 与资产证据并停止，后续恢复路线另行决定。
- GitHub 查询始终显式传入 `--repo ZipperCode/Aether`，避免 fork/upstream 自动识别错误。

## 发布资产

稳定 Tag 期望 `Release Aether` 产生 7 个成功 jobs，并发布：

- `aether-v0.7.25-linux-amd64.tar.gz`
- `aether-v0.7.25-linux-arm64.tar.gz`
- `install.sh`
- `SHA256SUMS`
