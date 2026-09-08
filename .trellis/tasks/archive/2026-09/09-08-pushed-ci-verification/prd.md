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
- [x] 本轮所有实际失败有具体日志、根因、最小修复和对应验证。
- [x] 修复提交推送后，最终目标 SHA 的 Rust CI 成功；如有真实外部阻塞，准确报告而非声称通过。
- [x] 记录实际时间与失败类型，不将新增全量上游合并的运行与旧样本作无控制的提速承诺。
- [x] 本地产品工作区收口，任务自有临时资源清理；仅剩本任务归档记录，无发布/部署。

## Evidence / Current State

- 第一轮 `948c1c16f` / run34210248162：完整Gateway收集35失败，其他6个断言/lint点，按共同根因集中修复；不是逐个失败反复push。
- 第二轮 `9026380d1` / run34217818968：Gateway5456、Frontend1635及Build、Data372和LinuxClippy全部通过；Rest完整跑到前轮未执行部分后新增5失败，按4文件修复。
- 最终代码 `74abb40cfd8f4c9df93ce657c8bbcbf9996acda6` / https://github.com/ZipperCode/Aether/actions/runs/34220941447：Root直接核验 headSha匹配、completed/success、18/18 jobs成功、unsuccessful为空。
- 最终Gateway5456、Rest3545、Frontend1635、Data372全部通过；既有skip单独记录。整轮18m31s，Gateway编译5m31s/测试738.786s，无受控百分比提速承诺。
- 最终报告见 research/run-34220941447.md；没有遗留任务进程/临时文件，正常Cargo缓存保留用于复用，不删除用户缓存。
