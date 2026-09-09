# 合并上游更新并在 CI 通过后发布 v0.7.33

## Goal

完整合并上游 `361952ada9e3d01ac180846cee497cd1f55ad1c3`，保留本地修复，推送最终提交；完整 GitHub Rust CI 对精确 SHA 成功后，发布下一个应用 tag `v0.7.33` 并验证发布产物。

## Confirmed Baseline and Authorization

- 用户明确要求“合并代码，然后推送，github CI编译通过之后，发布新的tag”，这包含本批完整合并、正常提交/push、真实CI失败修复及成功后的新tag/Release。
- 起点本地 `4f865ff53518885acf756250fb083ce6a86a50e5`，干净工作区，比 origin/master `4b5d4d64a9a6d86f379b972932b8537b81b97c09` 领先3提交；这些提交必须保留。
- 上游共同祖先 `c7e403b410139d12a6189dda9c2bdf0c7c80782e`，目标 `361952ada`，新增8提交/117文件。再次fetch已确认目标未变化。
- 最新稳定应用Release为v0.7.32，故下一个应用tag拟定v0.7.33；创建前再次确认不存在，不移动任何已有tag。

## Requirements

1. 合入全部8项上游更新，包括策略断连/计费、策略故障切换、跨格式错误/诊断导出、DNS/SMTP/Tunnel、模型映射Key选择、钱包SQL复用与CI收尾，不以只挑易合文件代替完整合并。
2. 保留本地c1e994159完整SSE首段错误预提交判定、7eaea44b0预读取首字节记账、cbf1346fe Schema节点级删除`$schema`（保留数据/属性名及公共Gemini行为），及既有内存、Endpoint、余额/手动额度恢复、原始HTTP捕获和CI修复。
3. 上游新断连策略默认继续完成、开启后取消及按次计费，必须整体接入生命周期/billing/usage，不只改UI；新规则仅可在可见业务输出前重试，已输出及图片成功不重放。
4. 保留fork发布owner/master/GHCR、现有CI共享入口/no-fail-fast/缓存修复；不取消或弱化任何测试/门禁，不修改已有migration校验和。
5. 在隔离worktree整合并必要检查后合回本地master、正常push。遇CI失败收齐同轮实际错误并集中修复，以精确SHA CI绿色后才允许tag。
6. 验证Release所有job、非空资产、校验和/来源签名和GHCR双架构manifest；发布完成而不是仅tag推送即算完成。

## Acceptance Criteria

- [x] 双方历史均保留，冲突/标记清零，本地新增修复有明确保留证据。
- [x] 相关最小本地检查通过，统一push后的完整GitHub Rust CI对精确最终SHA成功。
- [x] 新annotated v0.7.33只指向已认证SHA，Release工作流及产物核验通过。
- [x] 隔离worktree/临时分支安全合回并清理，完成任务证据，按finish-work归档并同步远端。

## 完成证据

合并c2c30e60d保留两方历史；Rust CI34313596660首轮18/18成功；v0.7.33→c2c30e60d，Release34315225804首轮8/8成功。6资产/小文件hash/tar摘要/来源签名/GHCR双架构验证全部通过。详见research/ci-release.md；产品未部署，正常缓存保留。

## Out of Scope

- 部署、修改现有.env/数据库/账号/远端secret/权限/付费runner；force push、移动旧tag、业务功能裁剪或新兼容框架。
- 重复上一轮全量本地测试或UI截图/浏览器检查；本地验证以冲突和新增合同为边界，完整环境认证由已授权GitHub CI完成。
