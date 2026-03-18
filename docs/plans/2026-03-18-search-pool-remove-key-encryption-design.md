# 搜索池移除 Key 加密设计

## 目标
在当前开发阶段移除搜索池 SQLite 中对上游 API Key 的加密存储，改为直接以明文字段存储，并同步清理加密相关配置与代码。

## 背景
当前 `search_pool_gateway` 模块在 SQLite 中通过 `key_encrypted` 保存加密后的 key，并在代理转发和 Tavily 同步时解密使用。由于项目仍处于开发阶段，现阶段不要求兼容旧数据，也不要求保留这层加密。

## 设计决策

### 1. 存储语义改为明文
- 将模型字段从 `key_encrypted` 改为明文语义字段，例如 `raw_key`
- 创建 key 时直接保存明文
- 继续保留 `key_masked`，用于后台列表和日志脱敏展示

### 2. 移除搜索池模块内的加解密依赖
- `GatewayKeyService.create_key()` 不再调用加密逻辑
- `proxy_service` 直接读取明文字段转发上游请求
- `usage_service` 直接使用明文字段与 Tavily 返回的 `key` 做精确匹配
- 如果搜索池模块内不再引用 `GatewayCryptoService`，则删除该文件

### 3. 不兼容旧 SQLite 数据
- 本次不实现迁移或兼容逻辑
- 修改完成后，开发环境需删除当前搜索池 SQLite 文件并重新建库
- 若继续使用旧文件，出现 schema 不匹配错误属于预期

### 4. 清理环境变量
- 删除 `SEARCH_POOL_GATEWAY_CRYPTO_KEY`
- 更新 `.env.example`，只保留搜索池 SQLite 路径与 Tavily 同步相关变量说明

## 影响范围
- 后端模型、仓储、服务层
- 搜索池模块相关测试
- `.env.example`

## 不做的内容
- 不兼容旧数据
- 不实现自动迁移
- 不改变后台返回结构中的脱敏展示方式

## 验证方式
- 删除旧的搜索池 SQLite 文件
- 运行搜索池回归测试：

```bash
uv run pytest tests/modules/search_pool_gateway/test_usage_sync_and_stats.py tests/modules/search_pool_gateway/test_admin_routes_workspace.py tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py tests/modules/search_pool_gateway/test_proxy_routes.py -q
```
