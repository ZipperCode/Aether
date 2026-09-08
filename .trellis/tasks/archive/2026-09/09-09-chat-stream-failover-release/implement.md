# 实施与交付账本

## 所有权

- Root：任务 artifacts、spec、整合、GitHub/发布和最终审计。
- trellis-implement：apps/aether-gateway/src/execution_runtime/stream/ 必要产品修改及相邻测试；apps/aether-gateway/src/tests/ 模拟回归。其他路径先申请扩展；不写任务/spec、不提交、不做生产写入。
- 发布 explorer：只读 CI/tag/release 门禁，无写集。
- 后续 trellis-check：实现停止后独占上述产品写集，review/修正并验证。

## 顺序

- [x] 核对 clean master 50dbcea37、origin 一致、gh 可用。
- [x] PRD/设计/计划收敛，用户已授权，无待确认产品选择。
- [x] 配置上下文、启动任务并派发。
- [x] 修改前复现 → 最小根因修复 → 相关检查。
- [x] 独立 review/修正 → 模拟和质量验证通过。
- [x] 更新合同及证据，中文提交并推送 origin/master。
- [x] 精确 SHA GitHub 必需 CI 全部成功；失败修复后正常提交再验证。
- [x] 推送下一未占用应用 patch tag，跟进 release/产物。
- [x] 完整审计与清理；归档/journal由后续Trellis自动提交记录证明，最终目标由update_goal审计收口。

## 验证策略与进度

优先现有最小 cargo/nextest filter，复用同一配置。遵循 tools/ci.py 的 incremental/dev-debug/test-debug=0、Gateway 16 MiB 测试栈。构建保留有界进程句柄，观察超时不重启。远端必需矩阵不能由本地窄测试代替。

历史日志/候选/响应已读取，原始线头不完整；下一步由模拟失败证据收敛 A1-A4。无真实收费调用和线上变更。

## 2026-09-09 实施交接与 Review

- 修改前红测已失败：HTTP 200 + server_error；provider 调用实际 [2,1,0]，第三家未执行。原测试 session 73276 exit 1，构建 12m02s。
- 首次修复后主三家回归通过；fixture 并发不稳定，已修正为唯一身份和权威 provider/fixed_order/sticky=2 路由组。不得将旧全局 provider_priority_mode 当作有效排序配置。
- 最新 session 9928：40 项默认并行，39 PASS / 1 FAIL，构建 8m27s；失败为 visible 场景实际 [0,1,0]、预期 [2,1,0]。第三家未访问且正常内容后错误仍同流终止，但首家为何未执行未闭环。不得声称全绿，不删断言掩盖。
- 产品变更：stream/commit_policy.rs、error.rs、execution.rs；测试变更：tests/ai_execute/stream/chat_failover.rs、mod.rs、tests/ai_execute/lifecycle.rs。实现 owner 已停，无活动构建/测试进程。
- lifecycle 首段错误旧 HTTP200 预期已调整为既有 exhausted HTTP503，并保留每候选真实429错误断言；review 必须核对合同而非只接受变绿。
- fmt --all --check 与 diff --check 通过；Clippy 未跑。临时失败证据 C:/Users/Zipper/AppData/Local/Temp/aether-chat-failover-red-20260909.txt，收口时转存必要摘要并清理。
- 下一步：独立 trellis-check 接管上述完整产品写集，review、诊断可见内容测试候选顺序、完成全部模拟和相关检查。Root 仍独占 task/spec/Git/发布。
- 发布只读核验：gh 必须指定 --repo ZipperCode/Aether；预计下一 v0.7.32。Rust CI 当前18作业，release8作业/6资产；推tag前重新核对远端占用和精确SHA所有门禁。

## Review 最终收口

- 独立 reviewer 修正 SSE 控制记录提前提交及多行 data 漏检；保留原始透传字节和既有边界解析。
- fixture 不稳定根因：Chat 默认 target-select window=2，在 fixed_order 排名后仍按目标压力选择；非排序 bug/传输失败/健康缓存污染。只在诊断子进程设 window=1 时旧诊断 binary 4/4 PASS。最终 fixture 保持默认窗口，独立目标准入压力0/4/8，严格调用断言不删。
- 2026-09-09 session32749：47 passed/0failed/0ignored，5321filtered，编译9m08s、执行2.94s；4真实HTTP场景全通过，未设置诊断环境覆盖。
- 额外授权仅 main.rs 非Unix导出函数移除 needless_return 并中文说明，行为不变。它在测试后修改，由最终Clippy覆盖，不重复不相关lib测试。
- 最终 python tools/ci.py clippy-gateway PASS（lib/bins/examples、-D warnings，5.33s）；fmt/diff session61896 PASS。无未解决产品finding，无活动任务进程。
- 当前工作交接：产品7文件、spec合同更新及任务证据由Root收口。下一步提交并推送，精确SHA等待完整CI。

## GitHub CI

- 修复提交 c1e9941595e8cfc8ffe57b6e77a0762b68f7d3c2 已正常推送 origin/master；不是强推，无tag提前发布。
- 下一步查询该精确SHA的Rust CI全部作业，成功前禁止创建应用tag。

- Rust CI34259830393失败：Gateway 5462执行、5461通过、1失败、3跳过。唯一失败 execute_execution_runtime_stream_records_first_stream_event_before_visible_text（315.782s）。其他叶子作业均成功，未推tag。下一步由原review owner有界复现/修复此流首事件回归后再提交和运行新SHA CI。

- CI唯一失败已由原独立reviewer修复：预提交首Data即非终态记账，handoff去重，fixture并发释放消除互锁。49/49相关测试、Clippy、fmt/diff全部通过，具体证据见verification.md。仅execution.rs与合同/证据追加，下一步正常追加提交推送，不改写历史、不移动tag。

- 追加修复7eaea44b082d2cdd1e4b0133ee04c83fb7b703ca已正常推送并核对远端一致。新Rust CI34263635757正在运行：https://github.com/ZipperCode/Aether/actions/runs/34263635757。只有该精确SHA全门禁成功才放行tag。

- 34263635757已核验completed/success，headSha严格等于7eaea44b082d2cdd1e4b0133ee04c83fb7b703ca，18/18作业success；watch句柄64639正常exit0。
- CI通过后重新核对远端latest v0.7.31且v0.7.32未占用，创建annotated应用tag v0.7.32并仅推该ref，目标为上述精确SHA。下一步跟进release终态与产物，不部署。

- Release34265118756 8/8成功，v0.7.32已latest稳定发布，6资产、checksum/provenance与双架构镜像核验通过。详细数据见release-verification.md；所有任务临时文件/进程已清理，未部署。下一步仅提交证据、归档及journal并推送记录。
