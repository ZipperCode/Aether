# 实施计划

1. 在 `BatchAssignModelsDialog.vue` 中保留获取到的 `UpstreamModel`，维护“Global Model ID → 上游模型 ID”的临时选择。
2. 在新勾选的 Global Model 行内增加上游模型选择器；同名项自动选择，允许自由改选不同名项。
3. 保存时复用 `createModel` 提交显式关系，未指定项继续走 `batchAssignModelsToProvider`，保留删除和部分失败语义。
4. 扩展现有 `BatchAssignModelsDialog.loading.spec.ts`，验证 `gemini-3.8-flash-high → gemini-3.8` 请求携带真实模型名与 Endpoint ID。
5. 运行目标 Vitest、前端类型检查和 `git diff --check`；按项目规则不运行 UI 视觉验证和全量测试。

## 风险文件与收口检查

- `frontend/src/features/providers/components/BatchAssignModelsDialog.vue`：避免行点击与选择器事件互相触发。
- `frontend/src/features/providers/components/__tests__/BatchAssignModelsDialog.loading.spec.ts`：只验证关联逻辑，不复制组件内部状态实现。
- 提交前确认后端、数据库和其他模型管理入口没有无关改动。

## 完成证据

- `npm run test:run -- src/features/providers/components/__tests__/BatchAssignModelsDialog.loading.spec.ts`：1 个文件、4 个测试通过。
- `npm run type-check`：通过。
- `git diff --check`：通过；仅有 Windows 工作区 LF/CRLF 提示。
- 独立检查补齐弹窗会话代次守卫，旧请求不会覆盖同 Provider 重开后的新状态。
- 未运行会自动写文件的 `npm run lint`，未做项目规则排除的 UI 视觉验证和全量测试。
- Spec 评估：未改变 API、数据库或通用编码约定；任务 PRD 已记录产品合同，因此无需更新 `.trellis/spec/`。
