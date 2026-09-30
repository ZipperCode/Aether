# 实施与验证

1. 修改 BatchAssignModelsDialog 的新增关联编辑区与 Endpoint 加载，保留现有自动/发现路径。
2. 在现有测试文件覆盖手动模式边界、发现刷新优先级和会话隔离，避免实现细节断言。
3. 启动现有 Vite，用浏览器挂载真实关联组件，使用本地确定性 API fixtures 验证 UI 和请求（不接触真实凭证/数据）。
4. 运行相关 Vitest、只读 ESLint 和 frontend type-check；检查后更新现有契约文档并提交本次改动。

回退：恢复本次提交即可；无持久化格式或 API 改动。
