# 验证首次完整上游与 CI 优化推送并修复真实失败

## Goal

用户要求先 push 触发 CI，再尽量避免频繁修 CI；已立即推送现有已验证提交 `948c1c16f2f9927b37a8570b86767128abd6b7c8`，远端 SHA 核对一致，Rust CI run `34210248162` 已启动。按此实际运行的完整失败清单修复，验证最终代码的精确 SHA 成功，不删门禁、不发布部署。

## Requirements

1. 不在第一次 push 前重复全量本地验证或新增批准门槛（已完成）。
2. 以已完成 job 的原始日志确认根因，区分 format/Clippy/compile/runtime/fixture，不按汇总失败重复计数。
3. 收集本轮全部真实失败，尽量一次修复后统一推送；保持测试、数据库、功能和最终 gate，不用跳过/放宽断言制造成功。
4. 每次修复只验证受影响的目标，复用仍有效的历史证据；CI 是最终 Linux/实际工作流证据。
5. 用户此次授权包含正常 commit/push 以及跟进修复；不 force push，不修改 secret/权限/付费 runner，不创建 tag/release 或操作现有部署和数据库。

## Acceptance Criteria

- [x] 首次已提交代码正常推送，捕获精确 SHA 对应的 CI run。
- [ ] 本轮所有实际失败有具体日志、根因、最小修复和对应验证。
- [ ] 修复提交推送后，最终目标 SHA 的 Rust CI 成功；如有真实外部阻塞，准确报告而非声称通过。
- [ ] 记录实际时间与失败类型，不将新增全量上游合并的运行与旧样本作无控制的提速承诺。
- [ ] 本地工作区收口、任务归档和任务自有临时资源清理；无发布/部署。

## Evidence / Current State

- 本地/远端 master 均为 `948c1c16f`；第一轮 CI https://github.com/ZipperCode/Aether/actions/runs/34210248162。
- 首个失败 job `102009266027`：Rust fmt PASS，CI dispatcher fixture PASS，build-watch fixture在 unchanged输出Fresh但字符串断言失败；CI全局 `CARGO_TERM_COLOR=always`，疑似ANSI控制码差异，需以原始输出确认。
- 其余 jobs仍运行，保持当前远端执行收集失败，不提前push触发取消。

首轮已终态：Gateway完整no-fail-fast暴露35项失败，其他6个独立断言/lint点已修复并经独立检查；全列表/时间/18job矩阵在 research/run-34210248162.md。按4个非重叠写集解决共享根因后统一验证推送，不将新增Gateway失败排除在本次交付之外。
