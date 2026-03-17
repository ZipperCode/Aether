# 搜索池网关设计（替换 Tavily 号池）

**日期：** 2026-03-17  
**状态：** 已评审（用户确认）

## 1. 目标与范围

基于 Aether 现有工程体系，实现一个新的“搜索池网关”模块，替换当前 `tavily_pool`：

- 支持双服务：`Tavily + Firecrawl`
- 对外接口兼容上游风格：
  - `POST /api/search`
  - `POST /api/extract`
  - `ANY /firecrawl/{path:path}`
- 使用独立网关 token 做调用鉴权
- 管理端 UI 交互参考 `tavily-key-generator`，但视觉风格保持 Aether 当前项目一致
- 彻底替换旧 Tavily 号池模块
- 不做旧数据迁移，空池启动

## 2. 方案选择

### 2.1 备选方案

1. 在 `tavily_pool` 上扩展为多服务网关（改动小，历史包袱重）  
2. 新建 `search_pool_gateway` 模块并替换旧模块（边界清晰，维护性最佳）  
3. 直接 vendor 上游 `proxy/` 代码（一致性高，但与 Aether 架构冲突）

### 2.2 最终决策

采用 **方案 2**：新建 `search_pool_gateway` 并替换 `tavily_pool`。

## 3. 后端架构

### 3.1 模块划分

新增模块：`src/modules/search_pool_gateway/`

建议结构：

```text
src/modules/search_pool_gateway/
├── __init__.py
├── routes_admin.py
├── routes_proxy.py
├── models.py
├── schemas.py
├── sqlite.py
├── repositories/
│   ├── key_repo.py
│   └── token_repo.py
└── services/
    ├── key_service.py
    ├── token_service.py
    ├── proxy_service.py
    ├── usage_service.py
    └── key_pool.py
```

### 3.2 路由与鉴权

- 管理 API 前缀：`/api/admin/search-pool`
  - 鉴权：`require_admin`
- 对外兼容 API：
  - `POST /api/search`
  - `POST /api/extract`
  - `ANY /firecrawl/{path:path}`
  - 鉴权：独立网关 token（`Authorization: Bearer <gateway_token>`）

### 3.3 存储

- 独立 SQLite：`/sqlites/search_pool_gateway.db`
- 启动时自动 schema 初始化
- 与主业务库隔离，避免耦合

## 4. 数据模型

### 4.1 `gateway_api_keys`

- `id`
- `service` (`tavily`/`firecrawl`)
- `key_encrypted`, `key_masked`
- `email`, `active`
- `total_used`, `total_failed`, `consecutive_fails`, `last_used_at`
- `usage_key_used`, `usage_key_limit`, `usage_key_remaining`
- `usage_account_plan`, `usage_account_used`, `usage_account_limit`, `usage_account_remaining`
- `usage_synced_at`, `usage_sync_error`
- `created_at`, `updated_at`

### 4.2 `gateway_tokens`

- `id`
- `service` (`tavily`/`firecrawl`)
- `token`, `name`
- `hourly_limit`, `daily_limit`, `monthly_limit`
- `created_at`, `updated_at`

### 4.3 `gateway_usage_logs`

- `id`
- `service`
- `token_id`, `api_key_id`
- `endpoint`, `success`, `latency_ms`, `error_message`
- `created_at`

### 4.4 `gateway_settings`

- `key`, `value`
- 可选用于扩展独立管理口令等配置

## 5. API 设计

### 5.1 管理 API（`/api/admin/search-pool`）

- `GET /stats?service=...`
- `GET /keys?service=...`
- `POST /keys`（单条/批量导入）
- `DELETE /keys/{id}`
- `PUT /keys/{id}/toggle`
- `POST /usage/sync`
- `GET /tokens?service=...`
- `POST /tokens`
- `DELETE /tokens/{id}`

### 5.2 对外兼容 API

- `POST /api/search`（转发 Tavily）
- `POST /api/extract`（转发 Tavily）
- `ANY /firecrawl/{path:path}`（转发 Firecrawl）

## 6. 运行策略

### 6.1 Key 池与轮询

- 按 `service` 分池轮询
- 每次请求从对应服务池取下一个可用 key
- 失败累计到阈值（默认 3）自动禁用并从池中摘除

### 6.2 调用与转发

- Tavily：请求体注入真实 key
- Firecrawl：优先 `Authorization` 头透传改写，必要时改写 JSON 体 `api_key`
- 尽量透传上游响应结构（JSON/非 JSON）

### 6.3 配额与统计

- token 维度校验小时/日/月配额
- 记录 usage log
- 支持额度同步接口刷新 key 真实额度字段

## 7. 前端设计

新增搜索池管理页面（保持 Aether 风格）：

- `SearchPoolDashboard.vue`
- `SearchPoolKeys.vue`
- `SearchPoolTokens.vue`

交互特征：

- 服务切换 Tabs（Tavily / Firecrawl）
- Key 列表：新增、批量导入、启停、删除
- Token 列表：新增、删除、配额管理
- 统计卡片：调用量、成功率、可用 key、失败 key

说明：UI 信息架构参考上游，但组件与样式体系完全沿用现有项目。

## 8. 替换策略

一次性替换：

1. 删除 `tavily_pool` 模块注册与后端实现
2. 删除 Tavily 相关前端页面、路由、feature
3. 接入 `search_pool_gateway` 新模块与管理页面
4. 零迁移，空库启动

## 9. 错误处理

- 上游失败：返回上游状态码 + 精简错误摘要
- 代理主流程错误与日志写入故障隔离
- 参数校验失败返回明确 4xx 错误

## 10. 测试与验收

### 10.1 测试

- 后端：
  - key 轮询与失败摘除
  - token 鉴权与服务匹配
  - 代理路由转发行为
  - 管理 API 增删改查/导入/同步
- 前端：
  - 服务切换与列表渲染
  - key 导入与操作
  - token 创建删除与错误提示

### 10.2 验收标准

- 管理端可维护 Tavily / Firecrawl key 与网关 token
- 对外可按兼容接口直接调用
- UI 风格与现有项目一致
- 新增与受影响测试通过
