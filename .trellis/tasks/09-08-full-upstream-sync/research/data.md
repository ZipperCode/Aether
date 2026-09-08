# 数据域合并收据

## 范围与三方

- worktree: `C:/Users/Zipper/AppData/Local/Temp/aether-full-upstream-20260908`。
- ancestor: `7892aa94853461c1e634f7a5babbb1280128720f`；ours: `b0ad8ff7f7bdb1888abac8c06f47f61e76761431`；theirs: `c7e403b410139d12a6189dda9c2bdf0c7c80782e`。
- 实现者仅拥有 `crates/aether-data/**` 及本文；没有修改主 checkout、根 Cargo/lock、数据库、服务或提交。
- 初始冲突通过 `git diff --name-only --diff-filter=U -- crates/aether-data` 得到以下 10 项：
  - `adapters/postgres/src/provider_catalog.rs`
  - `contracts/src/repository/provider_catalog/mod.rs`
  - `contracts/src/repository/provider_catalog/types.rs`
  - `runtime/src/backend/maintenance.rs`
  - `runtime/src/backend/read.rs`
  - `runtime/src/backend/system/postgres.rs`
  - `runtime/src/lifecycle/export/tests.rs`
  - `runtime/src/lifecycle/migrate/tests.rs`
  - `runtime/src/repository/provider_catalog/memory.rs`
  - `runtime/src/repository/provider_catalog/mod.rs`

## 语义处理

- 公共导出取双方新增能力的并集：本地 `ProviderCatalogKeySchedulingStateCasUpdate`、`StoredProviderCatalogAuthMaintenanceCandidate`、`StoredProviderCatalogModelFetchCandidate`，与上游 Provider config/proxy/credentials CAS 共存。memory/PostgreSQL 实现及 trait 路由均保留，没有改调用签名。
- 认证及模型发现查询仍使用轻量列投影；非 active/auto-fetch Key 不取 upstream metadata。认证轻量时间戳映射复用上游 `optional_u64`，负值不再被吞成 None。
- 系统配置保留 revision-only 强读、单查询 revision/value 快照、递增墓碑及普通读取隐藏墓碑。上游字符串 CAS 额外原子递增 revision；integrator 已确认 gateway 内存实现同步使用 `next_memory_system_config_revision()`。
- 保留 scheduling CAS 对加密凭证、auth type、旧调度对象的比较，以及仅写 scheduling 子对象的语义。新增的 Debug 实现遵循上游 CAS 敏感数据策略；不改变已批准的原始 HTTP Header 持久化。
- PostgreSQL 生命周期测试保留本地模型 Endpoint 绑定版本 `20260811000000` 和 revision 版本 `20260906000000`，合入上游新增迁移序列、policy-null 和历史解耦检查。
- 本地 PostgreSQL 外部测试 URL 隔离数据库能力移入上游共享 `postgres_test_support.rs`；保留上游 pg_ctl 停止/失败保留所有权行为。两条进程专属回归调用 local-only 入口，不被外部 URL 替代。
- 移除已淘汰 MySQL/SQLite 的安装分支、强读路由、导出/迁移回归及 model_catalog 导出；另删除每个旧适配器各 1 个本地 model_catalog.rs 和 2 个本地新增迁移，共 6 个孤立文件。此前 integrator 的 26 项 upstream-deleted 删除保持不变。
- 更新数据 AGENTS 的数据库支持范围；逻辑 schema、bootstrap 和生成 SQL 中的本地 revision / Endpoint 绑定均已保留。本轮未手改任何生成 SQL，也未运行生成器。

## 新增/扩展最小回归

- `system_config_string_cas_advances_revision_atomically`：字符串条件替换和 revision 同条 SQL 更新。
- `upstream_provider_cas_coexists_with_scheduling_state_cas`：Provider config/proxy CAS 与调度 CAS 共存；旧状态/错凭证拒绝、确认后保持 sibling quota、清除只写 scheduling。
- `provider_catalog_cas_debug_output_redacts_credential_fences` 扩展本地调度 CAS canary。
- 保留原有 auth/model-fetch 轻量投影测试、revision-only/墓碑 SQL 测试、上游严格负时间戳测试。

## 已执行与待整合检查

- 12 个手工修改 Rust 文件直接 `rustfmt --edition 2021 --config skip_children=true --check`：exit 0；未运行全局 fmt。
- `git diff --check -- crates/aether-data`：exit 0。
- owned paths 已 stage；`git diff --name-only --diff-filter=U -- crates/aether-data` 为空，`git diff --cached --check`（仅 owned paths）exit 0，`MERGE_HEAD` 仍为锁定上游 SHA。
- data 范围冲突标记扫描：0 命中；旧驱动 cfg/import/type 扫描：0 命中。数据库名负面测试继续保留。
- 曾临时用 `git -c core.autocrlf=false diff --check`，该命令把 Windows CRLF 误报为行尾空白；恢复仓库正常 Git 配置后检查通过，未修改 Git 配置。
- 并发整合期没有编译、运行逻辑测试或启动数据库，因此以上回归尚未声明运行通过。
- 整合后建议：`cargo check -p aether-data --no-default-features`；`cargo check -p aether-data -p aether-data-postgres --all-targets`；targeted `cargo test -p aether-data upstream_provider_cas_coexists_with_scheduling_state_cas`、`cargo test -p aether-data revision_query_tests`、`cargo test -p aether-data-contracts provider_catalog_cas_debug_output_redacts_credential_fences`、`cargo test -p aether-data-postgres candidate_query`、`cargo test -p aether-data-postgres provider_catalog_negative_security_timestamps_fail_closed`。
- schema 待跑 `cargo run -p aether-data-schema --bin aether-schema -- check` 和 `split_baseline_sources_match_executable_migrations`。现有 PG live tests 只允许任务自有临时资源/显式任务测试 URL；不要直接使用应用 .env 或既有数据库 URL。必要时验证 tombstone→recreate→string CAS 的真实 PostgreSQL 往返。
- 无任务进程或调试临时文件待清理；删除的旧适配器文件可由 Git 历史恢复。
