# 合并与发布设计

## Git、生命周期与所有权

- 隔离worktree `C:/Users/Zipper/AppData/Local/Temp/aether-upstream-release-20260909`，分支 `codex/upstream-release-20260909`，以4f865ff53起步。
- 执行真实 `git merge --no-commit --no-ff 361952ada9e3d01ac180846cee497cd1f55ad1c3`，基于共同祖先逐块语义合并，不批量ours/theirs。
- 一个Trellis implement integrator拥有隔离worktree产品写集与research；若冲突大且边界确实独立，Root明确交接后才并行。check在所有写代理终态后独占，Root持有任务/规范/Git和发布。
- 正常merge commit→确认主worktree未变→ff-only合回master→push→获取exact SHA Rust CI。CI如失败按实际日志修复，不能绕过门禁。
- CI绿色后在该SHA建annotated新应用tag，push单tagref，跟进Release并验证。CI后书面记录提交不作为偷换tag目标的理由。

## 合并语义

- complete-SSE分类与首字节观测分离：不能退回Headers即成功、不能预读时漏记firstbyte、不能在已可见输出后切provider。
- 上游延迟开场事件交付及策略全局规则需与本地上述边界整合；图片成功不重放，provider预算耗尽仅跳该provider，global预算检查不强停已执行请求。
- 新`cancel_on_client_disconnect=false`整体生命周期需保留准入直到完成；true取消时按上游取消费用合同对齐钱包/预留，不保留重复取消写入owner。
- 本地Antigravity工具schema修复严格在private wrapper节点保留，不删除任意同名业务属性/默认值，其他provider不受影响。
- 已修复的错误类别/余额projection、schema baseline历史不变量和测试夹具（Basic/Full、bound credential、null墓碑、Kiro profile）保留或按新版真实合同适配，不能丢失未冲突的fork能力。

## 验证策略

- 先U/marker/diff/fmt/config和最相关小crate回归；记录手改文件与测试映射。
- 本地跨平台检查只覆盖真实改动与编译接缝，不搭建新服务/数据库，不重复4千+Gateway本地suite。此前main target有缓存但不直接让并发write/build争用；可在全部source冻结后按明确缓存路径运行有界check，使用相同既有工具链/profile。
- 未跑UI视觉检查。前端类型/逻辑只在影响契约且必要时验证，不全量失败重跑。
- GitHub对最终SHA完整CI才是发布门禁；本地未运行项必须明确。每轮失败全部收齐后再一次push，不修第一个就取消其余job。
- Release至少确认两个tar包、install.sh/SHA256SUMS/provenance/VSIX等当前workflow规定资产，artifact digest与签名subject一致，GHCR linux/amd64+arm64；不执行部署。
