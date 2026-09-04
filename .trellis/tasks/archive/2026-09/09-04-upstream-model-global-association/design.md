# 技术设计

## 边界

本次只调整 `BatchAssignModelsDialog.vue` 的新关联路径并补充对应前端测试。复用现有 `createModel` 与 `batchAssignModelsToProvider`，不修改 Rust、数据库或公开 API。

## 数据流

```text
选择 Key → 获取 UpstreamModel{id, endpoint_ids}
                       ↓ 用户选择
GlobalModel{id} + UpstreamModel{id, endpoint_ids}
                       ↓ createModel
ProviderModel{global_model_id, provider_model_name, endpoint binding}
```

弹窗保留当前 Global Model 多选结构。上游模型加载后，仅对“本次新增”的 Global Model 显示真实上游模型选择器：

- 选择器允许从完整上游列表中任选一项；
- 大小写不敏感的同名项自动填入；
- 不同名项由用户显式选择；
- 已有关联不在该弹窗内改写主模型名。

## 保存策略

新增项分为两组：

1. 有显式上游选择：逐项调用现有 `createModel`，传递真实 `provider_model_name` 与去重后的 `endpoint_ids`。
2. 无显式上游选择：统一调用现有 `batchAssignModelsToProvider`，维持自动推断兼容路径。

两组都以单项错误汇总到现有“部分操作失败”提示。移除路径不变。

## 状态与清理

- 弹窗关闭或 Provider 改变时清空上游列表及本轮选择关系。
- 取消勾选某个新 Global Model 时移除其临时上游选择，避免陈旧状态再次提交。
- 上游刷新后只保留仍存在的选择，并重新填充唯一同名项。

## 兼容与风险

- 现有无上游列表、单 Endpoint、模型元数据推断路径保持不变。
- 不做模糊匹配，避免 `flash-high`、`flash-low` 等变体被错误绑定。
- 若选中的上游记录没有 Endpoint ID，仍提交真实模型名并让后端使用现有推断；错误按原部分失败机制返回。

## 回滚

回滚弹窗与测试改动即可；没有数据迁移或后端契约变化。
