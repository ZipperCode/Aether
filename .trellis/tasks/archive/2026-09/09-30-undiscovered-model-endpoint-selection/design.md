# 技术设计

## 状态与数据流

`getProviderEndpoints(providerId)` 与全局模型、已有模型、密钥并行加载。关联项仍按三条路径提交：手动配置项 → `createModel` 携带 `endpoint_ids`，发现模型项 → 现有精确创建，未指定项 → 现有批量推断接口。

新增状态仅属于当前弹窗会话：手动模式模型集合、真实上游名称、Endpoint ID 集合。打开/关闭/切换 Provider 时清空；发现结果刷新只更新发现选择，不覆盖手动状态。

## 交互

每个本轮新增模型显示上游模型选择。增加“手动指定模型与 Endpoint”选项；选择后显示真实上游名称输入和当前 Provider Endpoint 多选框，展示 API 格式与地址。Endpoint 不默认全选。

## 约束

使用现有 API 和组件样式，不改后端。保存前在前端阻止空名称、空 Endpoint；后端仍校验 Endpoint 所属关系。手动项不进入批量推断。
