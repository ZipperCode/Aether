# 限制 OAuth 与候选内存放大

## Goal

在拥有数千个 Provider Key、同时接收大请求体时，使 Aether 的认证维护和候选执行内存由固定并发与固定页大小约束，而不是随 Key 总数或“请求体大小 x Key 数量”线性增长。

## Background

- 远端 `154.217.246.184` 运行 `ghcr.io/zippercode/aether:0.7.29`，镜像 revision 为 `3475f21b982692f52cf24a077f3d6778b4a19f5c`，主机只有约 2 GiB RAM。
- 数据库共有 6,124 个启用 Key，其中 6,021 个属于 Antigravity；容器匿名 RSS 多次在约 0.5-1.5 GiB 间振荡，历史高水位约 1.72 GiB，并有旧容器 OOM 记录。
- `apps/aether-gateway/src/maintenance/runtime/oauth_token_refresh.rs:29` 会一次读取并持有全部 Provider、Endpoint 和完整 Key；`apps/aether-gateway/src/maintenance/runtime/workers.rs:526` 在启动时立即执行该扫描。
- `apps/aether-gateway/src/maintenance/runtime/account_self_check.rs:797` 以批次和并发刷新账号；Key 更新会触发 `apps/aether-gateway/src/data/state/core.rs:685` 的全目录缓存失效。两个任务重叠时，现场已观察到全量目录读取失效重试、数据库重复发送数十 GiB 数据以及批次内存峰值。
- 生产 Responses 候选链当前已经通过 `apps/aether-gateway/src/ai_serving/planner/candidate_materialization.rs:937` 分页并按需拉取，候选对象不携带请求体。旧版仍会在一个活跃尝试内保留多份请求体；当前 `master` 已包含取消 Guard 的 bodyless 修复。因此 `727 KiB x 6,021` 只能视为相关性，不能声称已证明 6,021 份请求体同时驻留。

## Requirements

- R1. OAuth 自动刷新不得调用一次返回全部完整 Key 的目录接口；先读取不含密文、`upstream_metadata`、`status_snapshot` 等大字段的轻量认证维护候选，再在取得执行许可后按 Key 加载完整凭证。
- R2. OAuth 自动刷新与账号自检必须共享进程级全局认证维护并发上限。默认上限为 4，可通过单一环境变量调整，并对无效值和过大值做有界归一化。
- R3. 每个执行许可最多对应一个完整 Key/transport 快照和一次上游认证或额度请求；完成、失败或取消后立即释放许可和本次完整对象。
- R4. OAuth 扫描保留现有候选资格、错误隔离、凭证 CAS、自动删除和汇总语义；账号自检保留现有每 Provider 周期、选择上限、并发和 Pool 状态语义。
- R5. Responses/Chat 等普通候选执行必须继续使用分页动态 attempt source；不得保留可把全部候选构造成带请求体 `Vec` 的生产入口。
- R6. 不新增数据库列或迁移，不改变管理 API、Provider 配置格式、Key 调度语义或前端行为。
- R7. 本轮只修改和验证本地代码/容器；未经再次明确授权，不更新远端生产配置或容器。

## Acceptance Criteria

- [x] AC1. 用至少 6,000 个轻量认证候选的测试证明：完整 Key 加载和认证执行的最大同时在途数不超过配置上限，且 OAuth 路径不调用 `list_keys_by_provider_ids` 全量完整读取。
- [x] AC2. OAuth 与账号自检同时运行的测试证明：二者合计最大在途认证维护数不超过同一个全局上限；取消或错误不会泄漏 permit。
- [x] AC3. PostgreSQL、MySQL、SQLite 和内存仓储都实现同一轻量投影；查询不读取密文、`upstream_metadata`、`status_snapshot` 或其他不参与资格判断的大字段。
- [x] AC4. 用数千候选和至少 500 KiB 请求体的回归测试证明：读取首个候选只构造一个带请求体 attempt，停止或 drain 不会构造剩余候选。
- [x] AC5. 源码检查不存在普通文本请求生产路径调用 eager `build_*_plan_and_reports`；必要的特殊图像/文件静态候选不受影响。
- [x] AC6. 相关 Rust 格式检查、定向单测、受影响包 `cargo check` 全部通过。
- [x] AC7. `docker compose build` 使用当前工作区源码成功，并在本地真实 Compose 环境完成健康检查和有界并发/大候选基数验证。（Compose 运行态已用当前镜像健康验证；6,000 候选与 512 KiB 请求体基数由定向 Rust 回归覆盖，未改动本地持久数据。）

## Out Of Scope

- 不以 Docker memory limit、增加 swap 或扩大服务器内存代替根因修复。
- 不修改远端生产数据、Provider 开关、Key 内容或容器版本。
- 不把本次任务扩展为通用缓存重写、数据库 schema 调整或所有后台 worker 的统一调度框架。
