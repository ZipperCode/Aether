# 修复调度时间与排序种子混用

## Goal

修复候选预选把请求分布种子当作 Unix 秒使用的问题，使普通 Key 的熔断、RPM、并发窗口、OAuth 时效和自适应限流重新按真实时间判断，并消除最终候选排序的同毫秒热点。

## Background

- 当前 `candidate_source.rs` 将 `request_distribution_seed()` 传入名为 `now_unix_secs` 的调度入口；该种子由毫秒时间旋转并混入计数器，数量级远大于 Unix 秒。
- 线上 `192.168.2.212:8084` 最近 24 小时内，7 个 `glm-5.2` Key 在 circuit 已打开且无成功恢复的情况下持续产生 403，累计制造 1,881 条失败候选。
- 现行路由合同已确认 `sticky_key_attempts` 是同 Key 重试的唯一权威；Provider/Endpoint `max_retries` 只保留配置兼容。

## Requirements

- 以具名内部上下文分离 `now_unix_secs` 与 `load_balance_seed`，禁止两个裸 `u64` 在候选入口继续共用同一参数。
- 普通预选、分页预选和直连候选入口均使用真实 Unix 秒判断运行态；分页等待只刷新时间，不改变该游标的排序种子。
- 预选排序和最终解析后排序均使用现有 `request_distribution_seed()`，同一排序批次只生成一次种子。
- 保持 `sticky_key_attempts`、Pool、额度阻断、余额过滤、缓存亲和、Endpoint 精确绑定和故障转移语义不变。
- 为本次新增或修改的手写类型、函数和业务字段补充实质性中文说明。
- 本地验证后推送精确提交，由 GitHub CI 构建并发布正式补丁 tag；线上只拉取该 tag 对应的 GHCR 镜像并重建 app，不在 `192.168.2.212` 编译源码。
- 不得修改线上 Key、PostgreSQL、Redis 或 `/home/zipper/Aether` 现有源码与未跟踪文件；只按现有 Compose 镜像配置持久化新 tag。

## Acceptance Criteria

- [x] 当 `now_unix_secs=100`、`load_balance_seed` 为极大值且 circuit 探测时间为 200 时，Key 仍以 `key_circuit_open` 被跳过。
- [x] RPM/活动请求窗口消费真实时间，不受排序种子大小影响。
- [x] 分页游标跨页保持排序种子稳定，等待重试只更新真实时间。
- [x] 最终候选排序不再直接使用毫秒时钟，同毫秒请求可获得不同分布种子。
- [x] 相关 scheduler-core、gateway 候选测试、gateway check、格式和 diff 检查通过。
- [x] GitHub `Rust CI` 与 tag 对应的 `Release Aether` 均在精确提交成功，GHCR 正式 tag 可拉取。
- [x] 线上新容器健康且运行该正式 tag 镜像；active-open 且探测时间未到的 Key 不再生成执行候选。
- [x] 失败可切回部署前镜像，数据库和运行配置未改变。

## Out of Scope

- 不恢复 `max_retries` 的执行权，不建立第二套重试预算。
- 不实施 `08-19-atomic-admission-accounting` 的跨后端原子 reservation。
- 不移植缺少本次线上证据的上游大范围调度、额度或路由重构。
- 不停用、删除或编辑那 7 个异常 Key；除完成本任务的正常分支推送和正式补丁 tag 外不操作远端引用。
