# 搜索池 Tavily 额度同步设计

## 目标
把搜索池工作台里的“同步额度”从占位逻辑改为真实同步。第一版通过环境变量提供 Tavily 控制台 Cookie，请求 `https://app.tavily.com/api/keys` 拉取 key 使用情况，回写到搜索池 SQLite，并输出可排障日志。

## 现状
当前 `GatewayUsageService.sync()` 只会更新 `usage_synced_at` 和清空 `usage_sync_error`，不会请求任何上游接口，也没有日志。因此前端提示“同步成功”并不代表真的同步了额度。

## 输入来源
第一版新增环境变量：
- `SEARCH_POOL_TAVILY_SYNC_COOKIE`: 完整 Cookie 请求头原文
- 可选：`SEARCH_POOL_TAVILY_SYNC_USER_AGENT`
- 可选：`SEARCH_POOL_TAVILY_SYNC_REFERER`

## Tavily 接口
使用控制台登录态请求：
- `GET https://app.tavily.com/api/keys`

示例响应：
```json
[
  {
    "key": "tvly-dev-xxx",
    "limit": 2147483647,
    "usage": 18,
    "key_type": "development",
    "search_egress_policy": "allow_external",
    "name": "default"
  }
]
```

## 字段映射
对每条 Tavily key：
- `usage_key_limit` <- `limit`
- `usage_key_used` <- `usage`
- `usage_key_remaining` <- `max(limit - usage, 0)`
- `usage_account_plan` <- `key_type`
- `usage_synced_at` <- 当前时间
- `usage_sync_error` <- 空字符串

本地匹配方式：
- 解密本地 `GatewayApiKey.key_encrypted`
- 与 Tavily 返回的 `key` 做精确匹配

## 日志策略
新增 `[search-pool-sync]` 日志，记录：
- 同步开始
- Cookie 是否存在（不输出明文）
- 请求 Tavily `/api/keys` 成功/失败
- 返回 key 数量
- 本地 key 匹配成功/失败数量
- 每个 key 的同步结果
- 总结统计

## 错误处理
- 缺失环境变量：返回结果中标记失败，并写日志
- Tavily 接口非 200：记录状态码和摘要
- JSON 解析失败：记录失败原因
- 单个 key 匹配不到：不中断整批，同步结果里累计 unmatched
- 单个 key 数据缺字段：尽量降级处理，无法计算时记录 `usage_sync_error`

## 范围
本次只做：
- Tavily 真实同步
- 环境变量版 Cookie 配置
- 服务端日志
- `.env.example` 补充说明

暂不做：
- 后台持久化 Cookie
- Firecrawl 额度同步
- 自动定时同步

## 后续演进
等环境变量版验证成功后，再把 Tavily Cookie 从环境变量迁移到后台配置存储，复用相同的同步实现。
