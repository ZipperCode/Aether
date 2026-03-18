# 搜索池网关服务端日志设计

## 目标
为搜索池网关代理请求补齐服务端结构化日志，便于直接通过 `logs/app.log` 排查认证失败、选 key、转发异常和上游错误，不新增管理端页面，也不新增请求明细入库。

## 现状
当前搜索池代理链路位于 `src/modules/search_pool_gateway/services/proxy_service.py`。
现有观测能力只有 `GatewayUsageLog` 的聚合记录，字段不足以支撑请求级排障。项目日志底座已经由 `src/core/logger.py` 提供，默认写入 `logs/app.log` 与 `logs/error.log`。

## 范围
本次仅实现服务端日志：
- 在 Tavily / Firecrawl 代理链路中增加结构化日志
- 保留现有 `GatewayUsageLog` 聚合统计逻辑
- 不新增 SQLite 调试表
- 不新增后台页面
- 不记录完整请求体或响应体

## 日志原则
- 只记录排障必要字段
- 全部敏感值脱敏后输出
- 日志前缀统一为 `[search-pool]`
- 采用 `key=value` 风格，便于 grep/rg 检索

## 记录字段
- `service`
- `request_id`
- `stage`
- `gateway_endpoint`
- `upstream_url`
- `method`
- `gateway_token_masked`
- `api_key_masked`
- `status_code`
- `latency_ms`
- `success`
- `error_summary`

## 明确不记录
- 明文网关 token
- 明文上游 API key
- 完整请求体
- 完整响应体
- 搜索词、抓取正文等原始用户数据

## 打点阶段
- `auth_failed`: 缺失 token、token 无效、服务不匹配
- `key_unavailable`: 没有可用上游 key
- `key_selected`: 成功选中上游 key
- `upstream_request_started`: 准备向上游发起请求
- `upstream_response_received`: 收到上游响应
- `upstream_request_failed`: httpx 异常、网络错误等

## 实现方式
在 `src/modules/search_pool_gateway/services/proxy_service.py` 内添加极小的日志辅助函数：
- 脱敏 token/key
- 组装日志字段
- 统一输出 `logger.info` / `logger.warning` / `logger.error`

不额外引入跨模块抽象，避免把当前工作扩散到整个项目。

## 测试策略
在 `tests/modules/search_pool_gateway/test_proxy_routes.py` 增加回归测试，覆盖：
- 鉴权失败时写 `auth_failed`
- 无可用 key 时写 `key_unavailable`
- 成功转发时写开始/结束日志，且脱敏输出
- 上游异常时写 `upstream_request_failed`

测试只断言日志调用和敏感值未泄漏，不依赖日志文件本身。
