# Research: GitHub Actions 失败与耗时证据

- Query: 为什么本地改动后 GitHub CI 反复修复、编译很久，是否因功能过多？
- Scope: mixed；GitHub Actions/API 历史运行与不可变提交为主，只读调查；当前工作流设计由另一个研究主题负责。
- Date: 2026-09-08

## Findings

### 证据边界与样本

使用已配置 `gh` 的只读 `run view --json`、指定 job 日志和 GitHub REST API。调查 8 个 Rust CI（3 成功、5 失败）及 1 个 Release 成功样本；失败样本是有目的取样，不能把 5/8 当全仓失败率。无 build/test、dispatch/rerun/cancel/push、真实数据库/provider 调用。

历史最近远端提交为 `876a4fb5a95a95d9ccd6493b818176ae53e87eeb`；任务本地基线 `798ba9d5c8166febab1ab3c56b29f93f17d0e6ee` 的上游合并尚未推送，以下耗时不代表当前本地代码。历史还包含 MySQL/SQLite job，不能据此声称当前 PostgreSQL-only 代码仍运行这些 job。

### 成功运行的等待时间与 runner 时间

run elapsed = `updatedAt - createdAt`，含调度/汇总开销；job = `completedAt - startedAt`。runner 合计是所有 job 秒数之和，不是用户等待时间，也不是按分钟取整后的账单。

| Rust CI run | SHA | run elapsed | 23 个 job 合计 | Gateway job | lib step | bins step |
|---|---|---:|---:|---:|---:|---:|
| [33972120561](https://github.com/ZipperCode/Aether/actions/runs/33972120561) | `876a4fb5` | 17m38s | 60m32s | 17m21s | 13m37s | 3m16s |
| [33959401471](https://github.com/ZipperCode/Aether/actions/runs/33959401471) | `0d3250c6` | 18m20s | 61m46s | 18m05s | 14m10s | 3m22s |
| [33851770345](https://github.com/ZipperCode/Aether/actions/runs/33851770345) | `3475f21b` | 16m42s | 53m50s | 16m29s | 12m47s | 3m13s |

最近一次 run：2026-09-05 14:32:35Z 至 14:50:13Z。三次均由 Gateway 作业主导，最后只差 13–17 秒汇总等开销。

最近 run 其他作业：Integration Scenarios 6m51s；Workspace Rest tests 6m44s；Clippy Workspace Rest 5m05s；Clippy Gateway 5m01s；Frontend 2m08s；Postgres smoke 1m53s。它们主要并行运行；缩短这些非关键作业不能等量缩短整次等待。

### Gateway 不是全在“编译”，也不是全在“跑测试”

从 `Finished test profile` 和 nextest `Summary` 分离编译/链接与测试执行：

| run / Gateway job | lib 编译+链接 | lib 执行 | lib 测试数 | bins 编译+链接 | bins 执行 |
|---|---:|---:|---:|---:|---:|
| [33972120561 / 101322362405](https://github.com/ZipperCode/Aether/actions/runs/33972120561/job/101322362405) | 6m54s | 399.159s | 4435 pass，3 skip | 3m14s | 1.026s，61 pass |
| [33959401471 / 101288506980](https://github.com/ZipperCode/Aether/actions/runs/33959401471/job/101288506980) | 7m26s | 400.169s | 4425 pass，3 skip | 3m20s | 1.062s，61 pass |
| [33851770345 / 100955965153](https://github.com/ZipperCode/Aether/actions/runs/33851770345/job/100955965153) | 6m07s | 397.403s | 4420 pass，3 skip | 3m10s | 1.035s，61 pass |

最近 run 的 Gateway 约 608 秒编译/链接、400 秒执行测试，余下约 33 秒为 setup/启动/汇总。bins 的 3 分多钟几乎全在构建，不是那 61 个测试执行慢。三次 lib 测试执行稳定约 6m40s；编译部分波动更明显。只凭这些日志不能进一步分离 rustc 编译与链接各自耗时，也不能证明合并两条命令必然消除全部额外构建。

最近 Gateway lib 的最慢 10 个测试（同一个已取回 job 日志，全部 PASS；只列模块内函数名）：

| 测试名 | 秒 |
|---|---:|
| `pool_key_cursor_does_not_spend_effective_scan_budget_on_blocked_accounts` | 37.590 |
| `pool_key_cursor_does_not_spend_effective_scan_budget_on_exhausted_accounts` | 17.719 |
| `pool_key_cursor_simulates_large_lru_pool_with_lazy_pages_and_dynamic_skips` | 13.283 |
| `gateway_serves_codex_model_cards_for_versioned_models_requests` | 4.047 |
| `pii_redaction_forwards_text_above_previous_scan_cap_only_after_masking` | 3.809 |
| `gateway_handles_admin_provider_ops_sub2api_balance_with_refresh_token_rotation` | 3.802 |
| `gateway_starts_admin_provider_oauth_kiro_batch_import_task_locally_with_trusted_admin_principal` | 3.621 |
| `gateway_starts_admin_provider_oauth_batch_import_task_locally_with_trusted_admin_principal` | 3.594 |
| `gateway_handles_admin_provider_ops_sub2api_balance_against_site_root_when_base_url_has_path` | 3.463 |
| `gateway_handles_admin_provider_ops_sub2api_balance_with_session_login` | 3.345 |

前三项属于 `dispatch::pool_scheduler::tests`。4435 个 lib PASS 记录的测试时长加和 1595.354 秒，中位数 0.026 秒，670 项至少 1 秒，只有 3 项超过 5 秒；最慢 10 项加和 94.273 秒，仅占测试时长总和约 5.9%。因测试并行，单测试耗时求和不能当关键路径；日志表明不是一个几百秒的异常慢用例独占执行，剩余大量测试也贡献累计成本。历史失败耗时例见下表（0.18 秒夹具失败、1.196 秒等待失败、2.574 秒并发断言失败），并非全由慢单测导致。

### 缓存的实际效果

以上三个 Gateway job 都命中同一 Rust target cache：`v0-rust-rust-ci-Linux-Linux-x64-a972f308-4ac4d1f2`，恢复 `264115963 B`（日志约 252 MB）。最新 job 在 14:32:59Z 显示 hit、14:33:02Z restored successfully；结束时 14:49:56Z 显示 `Cache up-to-date.`，并非每次重新上传整个缓存。

三次 sccache 统计一致：4 compile requests、1 executed、0 hits、1 miss、3 non-cacheable calls，原因 `crate-type`。最新样本无 cache read/write errors、timeouts。sccache 命中率的分母只涵盖可缓存调用，不能用 0% 得出“依赖缓存全部失效”；target cache 已有明确成功恢复。相反，它证明当前关键 Gateway 构建不能靠再开一个 sccache 开关获得确定收益。

历史源码定位：[rust-ci.yml @ 876a4](https://github.com/ZipperCode/Aether/blob/876a4fb5a95a95d9ccd6493b818176ae53e87eeb/.github/workflows/rust-ci.yml)：

- `:29-31`：`CARGO_INCREMENTAL=0`，dev/test debug 为 0。
- `:212-214`：Gateway 使用 `Swatinem/rust-cache@v2`、`shared-key: rust-ci-${{ runner.os }}`。
- `:231-232`、`:239-240`：两条独立 nextest `--lib` / `--bins`，各自配置 mold RUSTFLAGS。

这里只记录真实运行对应的旧配置；当前工作流是否有重复构建、错误失效或可合并的目标，应由当前源码研究验证。

### 失败分类：不是五次独立“编译失败”

五次底层失败均为 `Test (Gateway) / Test lib`，精确命令均是 `cargo nextest run -p aether-gateway --lib`。Clippy、Format、Frontend、Data smoke 等实体作业通过；`Test` 和 `check` 是连带汇总失败，不应重复计数。

| run / SHA / Gateway job | 精确失败证据 | 分类与置信度 |
|---|---|---|
| [33692043056](https://github.com/ZipperCode/Aether/actions/runs/33692043056/job/100452689524) / `83d263a0` | `execute_execution_runtime_stream_bridges_openai_image_sync_json_from_remote_runtime_to_image_sse`；`execution.rs:15574:10`；`Internal("provider key catalog state unavailable: key-1")` | 强准入所需 Key 目录夹具缺失，高；不是编译/DB 服务故障 |
| [33693091589](https://github.com/ZipperCode/Aether/actions/runs/33693091589/job/100455997528) / `d0ef009c` | `execute_execution_runtime_stream_bridges_sync_json_body_from_remote_runtime_to_sse`；`execution.rs:15251:10`；同一个 `provider key catalog state unavailable: key-1` | 相邻调用路径同类夹具遗漏，高 |
| [33699968580](https://github.com/ZipperCode/Aether/actions/runs/33699968580/job/100476946750) / `d2469545` | `frame_stream_records_deferred_pending_before_waiting_for_headers`；`execution.rs:11023:10`；`deferred pending usage should be recorded before headers: Elapsed(())` | 异步等待/夹具或流程条件，高置信是测试运行期超时；仅此日志不能确证随机 flaky 或确定性根因 |
| [33704528677](https://github.com/ZipperCode/Aether/actions/runs/33704528677/job/100490720185) / `75bd1edb` | Gemini cancel 跟进 `gemini_sync_task.rs:269:5` 与 OpenAI remix 跟进 `openai_sync_create.rs:762:5`；各 `left: 500 / right: 200` | 事后修复补齐两处 live Key catalog，夹具漂移，高 |
| [33849608433](https://github.com/ZipperCode/Aether/actions/runs/33849608433/job/100952592318) / `20faefe1` | `gateway_executes_openai_responses_sync_after_api_key_concurrency_wait_budget_elapses`；`sync/cli.rs:1306:5`；`left: 503 / right: 200` | 并发等待预算耗尽的旧断言与新合同不一致，高；不是 runner 性能随机失败 |

后续不可变修复进一步佐证：

- [94ddca675](https://github.com/ZipperCode/Aether/commit/94ddca675)：只改两个视频测试，分别 seed `StoredProviderCatalogKey` 并 attach provider catalog repository；保持强准入，不修改业务安全语义。补丁见 `gemini_sync_task.rs:244` 与 `openai_sync_create.rs:740` 附近。
- [3475f21b](https://github.com/ZipperCode/Aether/commit/3475f21b982692f52cf24a077f3d6778b4a19f5c)：将并发等待测试重命名为 `gateway_returns_concurrency_limited_after_wait_budget_expires_for_openai_responses_sync`，预期由 OK 改 SERVICE_UNAVAILABLE，同时验证诊断、Skipped 候选和仅一次执行；随后样本 33851770345 全绿。

失败轮次会逐次暴露更后面的测试：336920/336930 约第 1188/1189 项；336999 只执行 1218/4418（1217 pass/1 fail）；337045 才到约第 4327/4328 项。证据支持“只修第一个报错，未同时追踪新合同影响的其他夹具”导致多轮反馈，而不支持“业务功能数量直接造成失败”。

失败 run elapsed 分别 9m03、8m18、8m08、12m26、23m41；最后一个 Gateway job 实际仅 9m13。失败 run 的 run elapsed 不能直接视为编译时间，可能含排队/作业启动延迟等；本次未追溯该异常延迟。

### Release 单独计算

[Release 33973061278](https://github.com/ZipperCode/Aether/actions/runs/33973061278) 同 SHA `876a4fb5`，7/7 jobs success；run elapsed 18m47，runner 合计 36m25。

- Build linux-amd64 job 17m22、Build step 16m30。
- Build linux-arm64 job 16m45、Build step 15m54。
- Frontend 42s；Docker multi-arch 54s；tarballs 23s；GitHub assets 17s；preflight 2s。

两架构并行构建是 Release 主路径；不能将两架构之和说成用户等待，也不能用 Release 的 release-profile 构建耗时替代 Rust CI 的 test-profile 耗时。保留 exact-SHA CI 成功后才打 tag 的发布合同；不建议删门禁或启用付费 runner。

### 追加复核：当前 rust-cache pin 不是失效 commit

当前本地 `.github/workflows/rust-ci.yml:168` 等 10 处使用 `49a0bdc70d2e1b713ca9e2869b211fcce03d3c1c`；旧 `876a4` 是 12 处 `@v2`。

独立 GitHub API 结果：

- `GET repos/Swatinem/rust-cache/commits/49a0...`：422 `No commit found for SHA`，只说明该对象不是 commit。
- [git/tags/49a0...](https://api.github.com/repos/Swatinem/rust-cache/git/tags/49a0bdc70d2e1b713ca9e2869b211fcce03d3c1c)：对象确实存在，是 annotated tag `v2`，指向 commit `6323deb102c322ba6fcbdcafc7e3dddab59af2b6`。
- [git/ref/tags/v2](https://api.github.com/repos/Swatinem/rust-cache/git/ref/tags/v2)：当前 v2 ref 指向 tag 对象 `49a0...`。
- [contents/action.yml?ref=49a0...](https://api.github.com/repos/Swatinem/rust-cache/contents/action.yml?ref=49a0bdc70d2e1b713ca9e2869b211fcce03d3c1c)：成功，blob `1481945d779f128e201b6aa5498c53fe4ae9632c`。

因此不能把 raw URL 404 或 commits API 422直接定性为“不存在/CI 必定失败”。GitHub Contents 能解析此 tag-object SHA；本次未执行 Actions，尚无执行器对该 SHA 的接受/拒绝实测。若选择真正 commit pin，已确认其 peeled commit，但是否修改由主研究整合决定。

### 最小优化方向（证据支持，不是已实测收益）

1. 可靠性先对齐合同：强准入/并发/流式预提交改变时追踪所有真实调用方及测试夹具，优先修共享 fixture 或统一构造入口；不要每轮只补首个失败测试，不要放宽产品合同。相关本地验证应覆盖同族测试，测试收集可考虑在有限范围一次收集所有失败。
2. 耗时先看 Gateway：编译/链接约 9m17–10m46，测试执行约 6m40，均是真实开销。检查当前 `--lib`/`--bins` 构建目标、feature/profile/cache key 一致性及可消除的串行重复；不声称简单合并命令就能省掉完整 bins 构建。
3. 已有 target cache 明确有效，sccache 的关键非缓存调用为 crate-type；在没有新增命中证据前，不增加新缓存基础设施。任何 key/profile 调整必须保留完整覆盖并经后续授权的远端同类运行验证。
4. 功能体量会影响 Gateway 构建与 4400+ 测试总量，但本样本反复失败的直接原因是合同/夹具漂移，不是“功能太多”。无证据支持删功能、删测试或放宽发布门禁。

### Files Found / Related Specs / References

- `.trellis/workflow.md`：研究只落任务 research，规划/实施授权边界。
- `.trellis/tasks/09-08-ci-reliability-speed/prd.md`：历史 SHA 与未推送基线、必要测试和发布门禁、禁止外部变更。
- `.trellis/spec/guides/index.md`：跨层修改要追踪受影响调用与数据边界。
- `.trellis/spec/aether-gateway/backend/quality-guidelines.md:117-128`：真实 Provider/Key ID 的测试夹具和共享合同要求；文件部分通用章节仍是占位，本次不修改。
- 历史 `.github/workflows/rust-ci.yml`：Gateway 命令、profile/cache 配置；以上链接固定 SHA。
- 历史 `apps/aether-gateway/src/execution_runtime/stream/execution.rs`、`tests/video/*.rs`、`tests/ai_execute/sync/cli.rs`：日志行号和不可变修复证据；不是当前本地行号。
- 外部事实均来自 GitHub run/job/REST/commit primary sources；版本为历史 Rust 1.95.0 合同、Swatinem v2，当前 pin 对象类型单独复核。

## Caveats / Not Found

- 没有当前本地基线的 GitHub 执行结果，不能宣称当前优化已经提速。
- 3 个成功样本无冷缓存对照、固定硬件对照或统计显著性；没有测编译/链接细分、各 crate 成本、当前单测全量分布。
- 没有将超时一次就定性为 flaky；未定位该次等待超时的完整修复提交。历史记忆帮助找到视频夹具修复 commit，关键结果已通过 GitHub 不可变补丁复核。
- 不将 `Test`/`check` 汇总失败当独立根因；不将 job 合计 runner 时间当用户等待或真实账单。
- 官方文档浏览工具在追加 pin 调查时返回 503；未据此推断 Actions 执行器是否支持 annotated-tag-object SHA。
- 无手动临时文件/构建产物需要清理；`gh` 自身常规日志缓存未删除。唯一研究产出为本文件。
