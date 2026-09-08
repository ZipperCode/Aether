# Research: AntiGravity auth 内存根因

- Query: 追踪大量 AntiGravity auth 导致 aether-app 内存增长的入口、候选解析、transport 与缓存持有链
- Scope: internal
- Date: 2026-09-06

## 后续运行时归因修正（最终）

- 初始静态调查确认候选 transport 批量物化是独立内存风险，但后续同库 Docker
  采样证明，观测到的启动峰值主因是 `model_fetch`：50 个成功 Key 分别触发一次
  Provider 全量白名单核对，重复读取 8,356 条完整宽 Key，共 417,800 行，产生
  5.87 GiB RX 与 3.125 GiB RSS 峰值。
- 最终实现同时修复两条路径：候选全局排序改为轻量快照并在当前 attempt 才读取
  transport；模型抓取改为轻量投影，并在每批次对每个成功 Provider 只核对一次。

## 初始静态 Findings（修复前代码）

1. 候选路径最直接的静态线性放大风险是 resolved-page cache。当时 `CandidateResolvedPageCache` 的 value 是 `Arc<CandidateResolvedPageSnapshot>`，snapshot 内含 `Vec<EligibleLocalExecutionCandidate>` 与 `Vec<SkippedLocalExecutionCandidate>`；两类 candidate 都可持有 `Arc<GatewayProviderTransportSnapshot>`。这意味着一次缓存加载会把该页所有 provider transport（包括解密配置和 auth 相关数据）整体驻留。

2. 缓存 key 包含 auth identity（standalone api_key_id 或 user_id+api_key_id），因此大量不同 auth 会形成大量独立 cache entry（`apps/aether-gateway/src/cache/candidate_page.rs:87-112`）。每个 entry 的 stale TTL 默认 300 秒（`candidate_page.rs:258-268`），而 `ValueCache` 只在插入时按 `AUTH_RUNTIME_CACHE_MAX_ENTRIES` 限制数量，没有按 value 大小限制（`apps/aether-gateway/src/cache/auth_runtime.rs:571-611`）。若上限较大，内存峰值约为 auth 数 × 每页 transport 数 × transport/config 大小。

3. 初始调查时 routed page 已绕过 resolved-page cache，但这只消除了“跨请求缓存驻留”，没有消除单请求全量 materialization：`GatewayLocalCandidateResolutionPort::read_candidate_transport` 对每个 candidate 调用 `read_candidate_transport_snapshot_arc`，之后才进行 common skip、eligible 构造与 ranking。因此 2,048 候选的单次请求仍可能同时持有大量完整 transport；并发 auth 请求会把这个峰值相乘。

4. 解析结果类型本身把完整 transport 放进 eligible/skipped 结果，后续 `next_attempt()` 只是消费已经物化的结果；所以“只禁用 routed cache”不是核心修复。根因修复应将跨页/排序对象改为轻量 candidate facts，并把 `read_candidate_transport` 移入当前 attempt 的 materialization 边界；否则即使 cache 清空，批量候选仍会造成瞬时内存暴涨。

5. Bearer allowlist 的 revision snapshot 改动只影响配置 JSON 并发解析，不能解释 provider auth 数量线性增长；它解决的是同 revision 的重复反序列化，不解决候选 transport 的批量驻留。

## 当时建议与最终落地

- 候选路径最终采用核心修复：preselection/ranking 只接收
  `SchedulerMinimalCandidateSelectionCandidate` 与紧凑排序/auth-channel facts；
  `next_attempt()` 单 candidate 读取并解析 transport，失败后按已排序轻量列表继续尝试。
- 运行时主因采用模型抓取轻量投影；目标收集与批末核对各自重读当前轻量数据，
  Provider 全量可用性核对对每个成功 Provider 每批只执行一次。
- 验证（Docker）：在容器内执行 10k allowlist/2,048 candidates 并发压测，采集容器 RSS/heap 与 cache entry 数；比较改动前后请求期间峰值及 drain 后基线。禁止宿主机 Cargo/CMake 构建。

## Related specs

- `.trellis/spec/aether-gateway/backend/auth-maintenance-memory-contract.md`
- `.trellis/tasks/09-06-antigravity-auth-memory/implement.md`

## Caveats / Not Found

- 本节记录的是初始研究阶段，当时未运行构建或修改产品代码；最终 Docker
  验证与运行时对比见 `implement.md`。
