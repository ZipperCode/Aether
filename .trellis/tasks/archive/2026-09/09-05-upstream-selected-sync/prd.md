# 同步上游 1、3、4、5 功能更新

## Goal

把 `fawney19/Aether` 的选定上游功能安全同步到当前 `master`，保留本仓库已有能力与刚完成的 Provider Model Endpoint 证据修复，不做整分支合并。

## Background

- 当前本地与 `origin/master` 的同步基线为 `c51adc205`；本地另有 5 个已收口但未推送的 Endpoint 修复、归档与日志提交。
- `upstream/main` 已从上次记录的 `ddcbeb3ae` 前进到 `27b0381a9`，新增 20 个提交，其中 13 个非合并补丁均未被当前 fork 等价吸收。
- 当前 fork 与上游长期分叉，直接合并会带入大量未选择功能；本次必须按提交依赖同步。

## Requirements

1. 同步编号 1：路由配置按已选模型过滤 Provider，包含必要前端实现与目标测试。
2. 同步编号 3：Antigravity/OAuth 身份、Codex 重置额度、上游发现模型同步到目录，以及所需契约和测试适配。
3. 同步编号 4：流式请求在首字节前被取消时的终态结算、重试共享首字节期限、h2c 截断顺序稳定性和跨格式同步收尾。
4. 同步编号 5：Antigravity reasoning 流与终止错误、Gemini/Codex Responses 重放修复。
5. 保留本仓库现有 Endpoint 推断、余额调度、Responses 错误协议以及 Provider Model Endpoint 精确绑定行为。
6. 上游提交只在功能或编译依赖成立时纳入；允许为冲突适配修改提交内容，但不得以整分支合并代替依赖分析。
7. 保留当前本地 `.env`、PostgreSQL 数据卷和真实 Compose 数据，不重置或替换本地数据。

## Acceptance Criteria

- [x] 编号 1、3、4、5 的上游行为均存在于最终代码，并有对应目标测试或已有上游回归覆盖。
- [x] 上游 13 个非合并补丁中，纳入、语义适配、测试依赖和排除项均有明确映射；冲突适配以行为与测试为准，不以相同 patch-id 代替验证。
- [x] `c8d1ae3e`、`344b3031e` 作为直接测试支持纳入；`dabaeb8df`、`ba11a7221`、`86f7cc0d5` 不进入最终产品提交。
- [x] 当前 Provider Model Endpoint 修复及其 7 项目标测试继续通过。
- [x] 相关 Rust 目标测试、前端目标测试、前端类型检查、格式检查和 Trellis 检查通过。
- [x] 使用标准 `docker-compose.yml + docker-compose.local.yml` 从最终源码重新构建并启动；Aether、PostgreSQL、Redis 健康，真实登录和前端资源验证通过。
- [x] 不提交 `.env`，不修改或删除真实 Compose 数据，不 push。

## Out of Scope

- 整体合并 `upstream/main`。
- Nightly CI、VSCodex、计划权限撤销、通用 Usage API 模板和其他未选择的管理/构建功能。
- 与选定提交无依赖关系的格式化、重构或依赖升级。
- 发布、打 tag 或推送远程。
