# Research: Gateway streaming lifecycle

- Query: category 4 依赖、融合、顺序与验证
- Scope: internal
- Date: 2026-09-05

## Findings

- `9282cce1d` 功能：首字节前 Future 被丢弃时，将 candidate/usage 结算为 cancelled/499；bodyless seed 避免复制正文。父提交 `14744abd5` 仅有上下文重叠。
- `206995645` 功能、直接子提交：固定/动态候选共享请求起点，以绝对首字节期限阻止重试重置预算。
- `344b3031e` 仅测试：mock 截断错误前等 10ms，稳定 h2c 帧顺序；无产品依赖。
- `57cdef4b8` 功能：同步错误识别 `error!=null/status=failed/type=error`；跨格式完整 JSON capture 直接转换，流事件仍聚合。
- `86f7cc0d5` 仅无关 routing-cache lint，排除；Nightly `dabaeb8df` 也排除。

顺序：category 5 `66d6c17d2/14744abd5` → `9282cce1d` → `206995645` → `344b3031e` → `57cdef4b8`。

本地已有更强的 `StreamAttemptTerminalGuard`（`apps/aether-gateway/src/execution_runtime/stream/execution.rs:2741,4039`）。不要新增 guard；把 bodyless seed 融入它，仅持标识、snapshot、诊断、时刻和 boxed seed；保留错误终态、watchdog 抢占、强 Key 准入、Pool 许可、Endpoint 隔离。usage 点位：`crates/aether-usage/runtime/src/write.rs:68,1690,1999`。

candidate 终态不可回退（`aether-data/contracts/src/repository/candidates/types.rs:593`）；cancelled usage 为 billing=`void` 且不结算（`aether-usage/runtime/src/record.rs:210`）。206995 扩展 `candidate_loop.rs:968` 的 port 和 watchdog 8 个调用，保留余额/同 Key 重试。57c 保留本地 `sync_products.rs:1012` 的嵌套错误识别及 Responses bare-error failover（`execution.rs:11599`）。新增/修改项补中文说明。

最小验证，串行且确认非 0 tests：

- usage：`describing_request_bodies`
- gateway：`frame_stream_records_deferred_pending`、`stream_candidate_retry_does_not_reset`、`same_format_responses_prefetch_retries_bare_error`
- formats：`complete_provider_bodies_are_recovered`、`unframed_stream_events`
- integration：`cargo test -p aether-integration-tests --bin mock_openai_upstream truncated_sse_is_a_partial_body_error_over_h2c`
- `cargo fmt --all --check`；再跑 `cargo check --workspace --all-targets`。

## Caveats / Not Found

- 五补丁当前均为 `git cherry +`。语义融合若 patch-id 不同，PRD“不再缺失”不会自动满足，主会话须选等价映射或保 patch-id 策略。
- 未发现流取消专用 settlement spec；证据来自当前代码、测试及 Responses/Key/余额规范。未运行测试或改产品代码。
