# 技术设计

## 边界

只复用现有前端查询与创建接口：`BatchAssignModelsDialog` 自动调用 `fetchModels(providerId)`，不新增后端契约、缓存层或推断规则。

## 数据流

```text
打开关联弹窗
  → 加载 Global Models、已有 Provider Models、Keys
  → 无 Key 参数读取聚合上游模型
  → 精确同名匹配
  → createModel(global_model_id, upstream.id, upstream.endpoint_ids)
```

完整初始加载链路使用现有会话代次守卫。基础数据和聚合查询期间都复用 `fetchingAutoMatchedModels` 并禁用保存，避免 Global Model 先返回时在模型证据到达前误走批量兜底。

## 兼容与回滚

- 查询为空或失败时不阻断弹窗，保存继续走已有 `assign-global-models` 推断。
- Key 菜单仍可强制刷新单个 Key 的上游列表。
- 回滚组件、响应类型和目标测试即可，无数据迁移。
