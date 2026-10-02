# 实施计划

## 顺序

1. 检查基线、读取指南、保存合并树冲突清单。
2. 评估所有冲突域，执行 upstream/main merge（先不自动提交）。
3. 按互不重叠的领域分工解决冲突：provider pool/scheduler、gateway/stream、formats、frontend、CI/manifests。各实现 worker 不运行构建、测试、formatter，不 stage/commit；父级统一验证并提交。
4. 运行格式检查、Rust 受影响包测试、frontend type-check/受影响 Vitest，再运行实际 gateway HTTP smoke 和浏览器 UI smoke。
5. 文档记录兼容性处理和验证结果；提交 upstream 合并。未提交前不应用 PR。
6. 重新获取重点 PR 状态/head，按 872 → 857/874 → 875/876 → 869/873 的兼容性结果选择纳入顺序；840 与 858 重复则跳过。
7. PR 合并后运行相关回归与真实路径 smoke；保留逐项提交与未纳入理由。

## 验证约束

- Rust 版本、Cargo 选项遵循仓库配置；本机环境与现有缓存以实际检查为准。
- `cargo fmt --all --check`；不要运行会修改无关文件的 frontend lint。
- gateway smoke 使用临时配置和本地测试上游，不调用真实计费服务，不泄露凭据。
- Postgres 测试需要真实本地测试数据库；没有数据库证据不得宣称通过。
- 所有构建/测试由集成负责人在 worker 完成后执行，避免共享编译争用。
- 验证失败不得自动提交；修复本任务引入的问题后再次运行失败范围。

## 执行结果（当前）

- 步骤 1–5 完成：上游合并 `5f37fa2af`，`42c750155` 修复合并后测试契约；基线验证为 Docker 重跑后 gateway 5438 通过/3 忽略、前端 1885 通过。
- 步骤 6 完成：#840 与上游 #858 重复，跳过；实际纳入顺序 #872（`d564f00b9`）→ #857（`0683f229c`）→ #874（`3d0bd34eb`）→ #869（`c32fd01fb`）→ #873（`0c0003737`）→ #876（`87411aaff`）→ #875（`1999c9771`）。
- 配套修复：`4e1f37852` 提供商视图 xl 断点空白；`d93a0198d` 自定义排序分页完整加载；集成中另修复 Claude metadata 回填耗尽状态未被 stored-only 谓词清除、Darwin `os_info` 架构按官方格式回退 `unknown` 而非 Rust triple。
- 步骤 7 针对性验证已记录：#876 formats 1083、transport 559、原生记忆 1、搜索 3；#875 CLI 13；提供商页面 23。#872 的 PII 脱敏盲区保持原样，不在本次范围。
- 最终整仓集成回归通过：gateway lib 5456/3 忽略（Docker wrapper 精确管理 PG 路径）、architecture_guard + admin_unsigned_identity_headers 210、workspace all-target 检查、cargo fmt --check、前端 238 文件 type-check、1889 项测试、前端构建。步骤 4/7 的现场冒烟：首轮（HTTP 聊天同步 200、SSE 200 收到 [DONE]、提供商 UI 1365px 表格可见无浏览器错误、Docker 迁移 70 项含 wallet）来自此前 smoke 二进制；最终重建二进制的复验冒烟已通过——cargo clippy gateway lib/bins `-D warnings` 与重建通过，全新启动日志 Codex 0.159.3、Claude 2.1.284，POST 聊天同步 200（`finish_reason=stop`）、SSE 200 [DONE]，admin/tasks 报告 `maintenance.codex.client.profile` 与 `maintenance.claude_code.client.profile` 运行。冒烟仅使用本地伪造上游，未验证真实上游 CLI/OAuth，无真实提供商密钥；未 push。
