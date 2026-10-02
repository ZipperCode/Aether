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
