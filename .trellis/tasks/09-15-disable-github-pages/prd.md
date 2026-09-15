# 恢复停用 GitHub Pages

## 目标
按用户 2026-09-15 明确选择恢复停用 Pages：移除上游合并重新带回的发布工作流，撤下已发布的站点。保留 v0.7.34 发布及普通 CI、Release、Nightly 流程。

## 已核实事实
- 起始 master / origin/master：72cdcf6d86a43d9a69b70f04b46472c739a752e0；主工作树有 22 条状态、33 个 WIP 文件，均属于 Antigravity 并已留存 SHA256 快照。
- 历史 1dd4df56b 移除 deploy-pages.yml，89523fb88 移除 GITHUB_PAGES 前端配置；上游合并又带回工作流。
- v0.7.34 的 Rust CI 34867894365 为 18/18 通过；Release 34869813892 为 8/8 通过。
- 手动 Pages 34910631274 为 3/3 通过，但首页引用 /assets/ 返回 404，实际资源位于 /Aether/assets/。
- 用户选择“恢复停用（推荐）”，明确涵盖移除流程并撤下网站。

## 验收
- .github/workflows/deploy-pages.yml 已移除；剩余工作流没有 Pages 配置、上传或部署 action。
- GitHub Pages 站点已删除，Pages API 返回 404，公开站点撤下。
- 清理提交合回开始时的 master 并推送；核对对应 GitHub CI 结果。
- v0.7.34 的 annotated tag 和发布资产不变。
- 现有 33 个 WIP 文件逐字节保持，任务 worktree 完成后清理。

## 边界
仅删除 Pages 工作流，记录停用契约。保持 frontend/vite.config.ts 的 base 为 /，不修改生产服务、权限保护规则、Release/Nightly/普通 CI 定义，不新增测试框架或配置，不创建第二个版本 tag。
