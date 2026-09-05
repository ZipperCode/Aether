# 技术设计

## Boundaries

本次修改覆盖 Provider Catalog 的只读投影、gateway 认证维护调度以及普通文本候选的惰性构造约束。数据库存储结构、管理 API、前端和 Provider 协议均保持不变。

## Data Flow

### OAuth 自动刷新

```text
Provider/Endpoint 小型快照
  -> 轻量认证维护候选（仅资格字段）
  -> 根据现有规则筛选候选 ID
  -> 等待共享认证维护 permit
  -> 按单个 ID 强读取完整 Key
  -> 构造单个 transport 并刷新/持久化
  -> 丢弃完整 Key/transport，释放 permit
```

轻量投影包含 `id`、`provider_id`、`is_active`、`auth_type`、是否存在认证配置、过期时间和 OAuth invalid 状态。它不包含任何密文、大 JSON 快照或请求体。缓存包装层直接委托底层仓储，不把该维护扫描放进会被逐 Key 写入反复清空的目录缓存。

### 账号自检

```text
轻量认证维护候选
  -> 现有 stale/never checked 选择（每 Provider 最多 200）
  -> 现有 provider concurrency
  -> 等待同一个共享认证维护 permit
  -> 单 Key 强读取、quota/OAuth 操作、状态持久化
  -> 释放 permit
```

现有 Provider 周期和业务判定不变；共享 gate 只限制跨 worker 的合计在途量。

### 普通候选执行

```text
数据库候选页（固定 256）
  -> dynamic attempt source
  -> next_attempt
  -> 为当前 Key 构造 provider body/report context
  -> 执行或失败切换
  -> 停止时直接丢弃未物化页/候选
```

移除仅被 re-export、未被生产调用的 eager `Vec<AiSyncAttempt>` / `Vec<AiStreamAttempt>` 构造入口，保留特殊图像/文件桥接确实需要的静态候选语义。测试用构造计数验证大 body 不随候选总数复制。

## Concurrency Contract

- 单一环境变量：`AETHER_AUTH_MAINTENANCE_CONCURRENCY`。
- 默认值 4，最小值 1，最大值 64。
- gate 为进程级共享实例；OAuth token refresh 和 account self-check 都必须在完整 Key 加载之前取得 owned permit。
- 每个 worker 自身的较小并发仍然有效；实际并发是 worker 上限与共享上限的较小者。
- permit 依赖 RAII 释放，任务取消、提前返回和错误路径不得单独手写 release。

## Compatibility

- 无数据库迁移；四个仓储后端只新增 SELECT 投影。
- 默认账号自检周期、每 Provider 上限和 Provider 配置语义不变。
- OAuth 原有“扫描所有符合资格 Key”的结果语义不变，只改变内存生命周期和最大并发。
- 新环境变量缺失时采用默认值；错误值不会导致 worker 关闭或无限并发。

## Risks And Mitigations

- 并发化可能改变完成顺序：汇总只统计计数，不依赖顺序；测试验证总数和错误隔离。
- Key 在轻量筛选后可能被修改：取得 permit 后按 ID 强读取，并再次执行完整资格判断，避免使用陈旧凭证。
- 账号自检和 OAuth 刷新可能命中同一 Key：现有 OAuth coordinator/singleflight 与凭证 CAS 继续负责同 Key 一致性，共享 gate 只负责资源上限。
- 删除 eager 入口可能暴露隐藏调用：先用全仓引用检查确认仅 re-export/死代码，再由编译和架构测试验证。

## Rollback

修改不含 schema 或数据迁移，可按提交整体回滚。远端部署仍需单独授权，并保留原 Compose 镜像 tag 作为运行回滚点。
