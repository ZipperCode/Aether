# AntiGravity auth 内存放大修复

## Goal

在不破坏跨节点吊销一致性和路由全局排序的前提下，消除 AntiGravity 大量 auth 导致的并发整份配置副本、完整 transport 批量驻留，以及模型抓取对 Provider 全量宽 Key 的重复读取，使网关内存随当前活跃工作量而非 auth 或 Key 总量放大。

## Requirements

- 系统配置强读返回原子递增 revision 与 value；同一 revision 的 Antigravity bearer allowlist 只允许一次完整解析，并复用不可变解析快照。
- allowlist 查找使用紧凑集合；认证上下文缓存 key 不得长期保存原始 bearer。
- 路由策略下跨页候选仍保持全局排序，但跨页只保存轻量排序快照；候选真正尝试时才读取和解析 transport。
- routed aggregate 的 resolved-page 缓存不得持有完整 transport、解密 auth 或原始 JSON。
- model-fetch 目标收集和批末白名单核对分别读取轻量 Key 投影；每个成功 Provider 每批只执行一次全量可用性核对。
- Pool score rebuild 只从维护摘要选择 ID，并在当前 Key 上执行单项强读；启动阶段只执行一次 rebuild。
- 保留跨节点吊销、候选失效后的后备链、格式及 provider 特例语义。

## Acceptance Criteria

- [x] 10,000 条 allowlist、128 并发请求在同一 revision 下至多一次完整配置解析；同秒连续更新能识别不同 revision。
- [x] 现有跨节点吊销测试通过，节点更新后下一次 refresh 拒绝已吊销 bearer。
- [x] 2,048 条 AntiGravity 候选带 routing policy 时仍按全局排序，且完整 transport 仅在当前 attempt 物化并及时释放。
- [x] resolved-page 缓存值不含 `GatewayProviderTransportSnapshot`；高优先级失效候选仍能选择正确后备候选。
- [x] model-fetch 不再执行 Provider-wide 完整 Key 查询；内存与三种 SQL 适配器均使用条件化元数据的轻量投影，每个成功 Provider 每批只核对一次。
- [x] Pool score 启动仅 rebuild 一次，不预载多 Key 完整目录，并保留当前 Key 的单项强读校验。
- [x] 8,356-Key Docker 对比中，50-target RX 从 5.87 GiB 降至 167 MB，峰值 RSS 从 3.125 GiB 降至 568.59 MiB。
- [x] 相关 allowlist/auth-channel/provider 特例测试、格式检查、Docker 定向测试与 `cargo check --all-targets` 通过。

## Out of Scope

- 不改为每页独立排序，不做未经确认的 Top-K 截断。
- 不使用普通本地配置缓存替代强一致来源。
- 不限制 allowlist 的业务条目上限；仅消除并发副本和完整 transport 的批量驻留。
- 不改变破坏性 whole-config purge/backup 语义；普通系统配置删除继续使用递增 revision 的 null 墓碑。
