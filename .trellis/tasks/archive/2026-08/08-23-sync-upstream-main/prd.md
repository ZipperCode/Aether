# 同步上游 main 分支

## Goal

将 fork 远端 `origin/master` 与源仓库 `upstream/main` 的变更完整集成到本地 `master`，保留本地分叉能力和用户现有未提交修改，为后续人工决定是否推送提供可验证的本地结果。

## Background

- 任务基线为本地 `master` 的 `15f07f9ec24797086b0432be4af5c001db2abe27`。
- `origin/master` 为 `e95da1545968d8962295bfd7186e260f94c4002c`，相对基线有 3 个远端独有提交，本地有 6 个独有提交。
- `upstream/main` 为 `3f2b67f191342bae39c7f1fa3c08876ca8d10537`；它与 `origin/master` 的共同祖先为 `3a759fae8922de11b83cc98df4a79eb15a79a237`，两侧分别有 53 与 81 个独有提交。
- 主工作区已有用户修改 `.trellis/workflow.md`，该修改不属于本任务，必须原样保留。

## Requirements

- 采用 merge 保留三条历史，不改写或丢弃本地独有提交。
- 先集成 `origin/master`，再集成 `upstream/main`。
- 冲突解决必须同时保留本地架构约束与上游新行为；不得用整侧覆盖掩盖语义冲突。
- 仅修改完成同步与冲突消解所必需的文件，不做无关重构、发布或部署。
- 不推送 `origin`，不创建远端 PR，不修改生产或共享设施。
- 在隔离 worktree 中完成合并与验证，验证后再回并本地 `master` 并清理任务 worktree/临时分支。

## Acceptance Criteria

- [x] 原始本地基线、`origin/master` 与 `upstream/main` 均为最终本地 `master` 的祖先。
- [x] 最终仓库不存在未解决冲突，`git diff --check` 通过。
- [x] 实际冲突涉及的 Rust/TypeScript 逻辑至少通过工作区 Rust 编译检查与前端 TypeScript 检查；环境阻塞必须有明确证据。
- [x] 主工作区原有 `.trellis/workflow.md` 修改内容与状态保持不变，除它之外没有任务遗留的未提交文件。
- [x] `upstream` 远端指向 `https://github.com/fawney19/Aether.git`，但本任务没有执行 push。
- [x] 隔离 worktree 与临时同步分支在回并后清理完成。

## Out of Scope

- 发布新版本、部署服务、迁移生产数据或推送远端。
- 逐项重做上游功能设计或清理双方历史技术债。
- 与合并冲突无关的格式化、重命名或兼容层新增。

## Verification Evidence

- 合并提交：`e4e223cb80369c825589a517155b13576919eea6`，父提交为本地同步结果 `821de21b3` 与 `upstream/main` 的 `3f2b67f19`。
- 祖先检查：任务基线、`origin/master`、`upstream/main` 均通过 `git merge-base --is-ancestor`。
- 通过：`cargo fmt --all --check`、`cargo check --workspace`、`cargo check -p aether-data-postgres`、`cargo check -p aether-gateway --lib`、`npm run type-check`、PostgreSQL sanitizer 定向测试。
- queue 定向测试未进入测试代码：其 dev 依赖 `boring-sys2` 在 Visual Studio/CMake 探测阶段失败；对应 Gateway lib 编译和零调用静态审计已通过。
- 独立审查清理了 PostgreSQL 重复 sanitizer 路径和 request queue 七个零调用旧 helper；最终冲突、冲突标记、工具输出污染与实际 NUL 字节均为 0。
