# Implementation Plan

1. 在 `RoutingProfiles.vue` 解析当前按模型策略对应的全局模型 ID，并传给排序编辑器。
2. 在 `RoutingPriorityPolicyEditor.vue` 为按模型 Provider 列表应用三项静态资格筛选，并更新空状态文案。
3. 调整可见 Provider 的拖拽/移动更新，使其合并并保留隐藏覆盖值。
4. 添加一个聚焦组件测试，覆盖正确模型、错误模型、停用 Provider、无启用 Key、缺失模型 ID、统一模式以及隐藏覆盖保留。
5. 同步 Demo Provider 摘要的活跃全局模型关联，并用 Mock 契约测试锁定响应。
6. 运行聚焦 Vitest 和 `npm run type-check`；不得运行会写文件的 `npm run lint`。

## Rollback

改动仅限两个 Vue 文件、Demo Mock handler 和两个测试文件；回退这些文件即可恢复原行为，无数据迁移或运行时状态需要处理。

## Verification Evidence

- 聚焦 Vitest：`2 files passed / 6 tests passed`。
- `npm run type-check`：通过。
- 五个目标文件执行不带 `--fix` 的 `npx eslint`：通过。
- `git diff --check`：通过。
- 增量独立 `trellis-check` 已补强 Demo 活跃关联/去重契约测试并校正任务工件范围；未运行全量测试、构建或浏览器手工验证。
- 本次未产生新的 API、数据库或跨层代码契约，无需更新 `.trellis/spec/`。
