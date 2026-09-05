# Bug Analysis: 关联模型仍缺少 Endpoint 证据

## 1. Root Cause Category

- **Category**: C/D - 变更传播遗漏与测试覆盖缺口
- **Specific Cause**: 上次修复只覆盖了用户额外点击 Key 后的显式关联路径；默认“勾选模型 → 保存”仍可在未加载上游模型时进入旧批量推断。

## 2. Why Fixes Failed

1. `9a05f2b02`：补充 Global Model 元数据推断，只解决有明确模型族元数据的模型，无法提供真实上游模型与 Endpoint 对应关系。
2. `ce4a40057`：支持用户选择真实上游模型，但把加载上游模型留在可选 Key 操作中，默认路径仍未获得证据。
3. 原测试只验证不同名模型在点击 Key 并手选后的请求，没有复现用户实际的直接保存步骤。

## 3. Prevention Mechanisms

| Priority | Mechanism | Specific Action | Status |
|---|---|---|---|
| P0 | Architecture | 默认打开流程复用聚合查询并在完成前阻止保存 | DONE |
| P0 | Test Coverage | 覆盖无需点击 Key 的 `gemini-3.7-flash` 同名关联 | DONE |
| P1 | Cross-layer type | 在查询响应类型声明 `endpoint_ids` | DONE |
| P1 | Documentation | 固化默认路径、兜底和异步会话契约 | DONE |

## 4. Systematic Expansion

- **Similar Issues**: 任何“可选刷新按钮提供后续写操作所需 ID”的界面都可能让默认路径绕过必要证据。
- **Design Improvement**: 必需证据归默认流程；辅助按钮只负责刷新或消歧。
- **Process Improvement**: 回归测试按用户原始操作顺序编写，并另测 pending 与旧会话响应。

## 5. Knowledge Capture

- [x] 新增 `.trellis/spec/aether-gateway/backend/model-association-endpoint-contract.md`。
- [x] 更新 `.trellis/spec/guides/cross-layer-thinking-guide.md` 默认路径检查项。
- [x] 项目不存在 `src/templates/markdown/spec/` 等规范模板目录，无副本可同步。
