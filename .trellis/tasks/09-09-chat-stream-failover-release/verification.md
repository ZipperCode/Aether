# 修复与独立 Review 证据

## 红绿验证

- 修改前 session73276 exit1：HTTP200 + 首段server_error，三提供商调用实际[2,1,0]、期待[2,1,1]；构建12m02s，测试1.87s。
- 中间9928为39/40，诊断85315为1/4，均未当作通过。根因是fixed_order排名后默认target-select窗口2仍按目标压力择一，未选候选仍留队列，不是传输失败/健康缓存/候选丢失。
- 仅诊断子进程window=1时旧binary4/4通过；最终保留默认窗口，独立目标准入压力0/4/8、唯一Provider/Key/Endpoint/Model及实际ID封装，权威路由组provider/fixed_order/sticky=2。
- 最终 session32749：47 passed/0failed/0ignored，5321filtered，构建9m08s，测试2.94s，exit0。四个真实本地HTTP/SSE模拟全通过，无外部收费调用。

## 覆盖和 Review

- 首家400同Key两次、第二家HTTP200首段错误、第三家成功，真实调用[2,1,1]。
- 控制记录、无空格data、多行JSON及分块错误不误提交；完整正常正文包括未知字段逐字节保留。
- 第二家已输出内容再错误时[2,1,0]，保持本流终止；显式stop同样不访问第三家并返回400。
- usage归属最终provider，成功tokens=(3,2,5)，全部已执行candidate终态可查。
- 相关Responses/Cyber、首字节预算、取消/429单次结算、传输错误SSE终止均通过。
- 独立review修正初版按单行分类的问题：使用既有record边界，忽略控制记录，合并data再分类，覆盖LF/CRLF/CR，不改原始透传字节。
- lifecycle从HTTP200改为既有候选耗尽503合同，同时保留实际Failed429/rate_limit_error/slow down；不是削弱错误断言。
- 额外经Root批准修正main.rs非Unix分支needless_return并补中文说明，仍Unsupported且无写文件行为。

## 实际命令

```powershell
$env:CARGO_INCREMENTAL='0'
$env:CARGO_PROFILE_DEV_DEBUG='0'
$env:CARGO_PROFILE_TEST_DEBUG='0'
$env:RUST_MIN_STACK='16777216'
cargo test -p aether-gateway --lib --locked -- chat_stream_failover 'execution_runtime::stream::commit_policy::tests' 'execution_runtime::stream::error::tests' 'same_format_responses_prefetch_retries_bare_error_before_committing_success' 'gateway_retries_next_local_openai_chat_stream_candidate_after_retryable_429_execution_runtime_status' 'stream_attempt_guard_records_admission_timeout_once_as_429' 'stream_candidate_watchdog' 'gateway_settles_stream_attempt_when_client_disconnects_before_first_byte' 'execute_execution_runtime_stream_emits_terminal_sse_error_event_after_body_started' 'gateway_returns_error_body_when_prefetch_detects_embedded_stream_error' 'execution_runtime::attempt_cancellation::tests' 'prefetched_codex_cyber_policy_violation' --nocapture
python tools/ci.py clippy-gateway
cargo fmt --all --check
git diff --check
```

本机nextest缺失，未安装工具。统一Clippy实际为cargo clippy -p aether-gateway --lib --bins --examples -- -D warnings，最终exit0/5.33s；最终fmt/diff61896 exit0。

## 时序与边界

所有测试语义输入在32749开始前完成；之后仅01:31:34两处fixture纯空白格式改动，产物01:39:56。随后main.rs等价修正由最终Clippy覆盖，不重跑不相关lib测试。无未解决产品finding，所有代理进程已结束。远端Linux/数据库/完整矩阵尚待新SHA的GitHub CI，不用本地窄测试替代。

## Bug Analysis

1. B/D/E：响应交付与重试边界缺口，首段错误缺回归，固定排名等于执行次序的假设错误。
2. 初次fixture修正仅隔离ID/固定排名，未覆盖真实target选择；最终以candidate阶段和真实调用链证据纠正。
3. 防复发采用现有FirstClassifiedBody、完整record分类及严格三家HTTP回归，已更新执行生命周期spec。
4. 共用首段分类的Responses回归通过，不扩展通用调度或新配置。
5. 证据进入本任务和既有spec，仓库无对应模板源码，不复制其他项目模板路径。
