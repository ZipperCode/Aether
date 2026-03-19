# 搜索池 Tavily Cookie 同步下线设计

## 背景
当前 `search_pool_gateway` 的 Tavily 额度同步通过环境变量提供单个全局 Cookie，请求 Tavily 控制台接口并回写本地 key 使用量。

这套方案已经通过联调验证可用，但它有两个结构性问题：

- 它不是按账号管理登录态，而是单 Cookie 全局同步
- 后续如果要支持“账号密码登录 + 邮箱验证码 + 持久化会话”，现有环境变量方案不具备可演进性

因此本次不继续扩展 Cookie 同步，而是把 Tavily 额度同步能力回退为占位状态。

## 目标
- 放弃当前基于环境变量的 Tavily Cookie 同步能力
- 去掉本地环境变量配置入口与相关实现
- 保留管理端“同步额度”功能入口，但状态改为暂未启用
- 不清理已有 `usage_*` 字段，为后续账号级同步方案保留数据结构

## 非目标
- 本次不实现账号密码登录
- 本次不实现邮箱验证码自动拉取
- 本次不实现按账号持久化 Cookie
- 本次不删除 Tavily 额度展示字段
- 本次不删除“同步额度”按钮和管理端接口

## 方案

### 1. 后端行为
对于 `POST /api/admin/search-pool/usage/sync`：

- 当 `service=tavily` 时，不再读取：
  - `SEARCH_POOL_TAVILY_SYNC_COOKIE`
  - `SEARCH_POOL_TAVILY_SYNC_USER_AGENT`
  - `SEARCH_POOL_TAVILY_SYNC_REFERER`
- 不再请求 Tavily 控制台 `/api/keys`
- 不再尝试匹配本地 key 并回写使用量
- 直接返回固定占位结果

推荐返回结构：

```json
{
  "result": {
    "service": "tavily",
    "synced_keys": 0,
    "errors": 0,
    "message": "Tavily 额度同步能力暂未启用",
    "synced_at": "2026-03-19T00:00:00+00:00"
  }
}
```

### 2. 数据策略
- 不修改现有 `GatewayApiKey` 表结构
- 不清空已有 `usage_key_*` / `usage_account_*` 字段
- 不为 Tavily key 写入新的 `usage_sync_error`
- 已有历史同步数据继续展示，后续如无新同步则保持原值

### 3. 前端行为
- 工作台继续保留“同步额度”按钮
- 点击后调用同一接口
- 成功提示文案从“同步完成，共处理 X 个 Key”改为“同步能力暂未启用”
- Key 表格先维持现有展示逻辑，不强制新增额外占位文案

### 4. 配置与文档
- 从 `.env.example` 删除 Tavily Cookie 同步相关环境变量说明
- 废弃当前 Tavily Cookie 同步设计文档中的环境变量方案
- 新增本设计文档说明本次回退决策，避免后续误以为现有 Cookie 同步仍是正式方案

## 影响范围
- `src/modules/search_pool_gateway/services/usage_service.py`
- `frontend/src/views/admin/SearchPoolServiceWorkspace.vue`
- `.env.example`
- `tests/modules/search_pool_gateway/test_usage_sync_and_stats.py`

## 测试策略

### 后端
- Tavily 同步接口在无任何环境变量时可调用成功
- 返回固定占位消息
- `synced_keys == 0`
- `errors == 0`
- 不改写已有 usage 字段

### 前端
- 点击同步后显示“同步能力暂未启用”类提示
- 页面现有挂载与 API 调用测试继续通过

## 后续演进
后续如果要恢复 Tavily 额度同步，正确方向应为：

1. 新增 Tavily 账号实体
2. 保存账号级登录态而不是全局环境变量
3. 接入邮箱验证码获取链路
4. 按账号拉取和同步 key 使用量

在该方案落地前，当前占位状态应保持稳定，不再继续扩展环境变量 Cookie 路线。
