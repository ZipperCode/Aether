# 系统导入导出合并收据

## 写集与三方

- 隔离 worktree：`C:/Users/Zipper/AppData/Local/Temp/aether-full-upstream-20260908`。
- base `7892aa94853461c1e634f7a5babbb1280128720f`；ours `b0ad8ff7f7bdb1888abac8c06f47f61e76761431`；theirs `c7e403b410139d12a6189dda9c2bdf0c7c80782e`。
- 本轮只修改六个获分配产品路径及本文，未修改上一轮已归还的数据写集：
  - `apps/aether-gateway/src/handlers/admin/request/system/import.rs`
  - `apps/aether-gateway/src/handlers/admin/request/system/export.rs`
  - `apps/aether-gateway/src/handlers/admin/system/shared/export/providers.rs`
  - `apps/aether-gateway/src/handlers/admin/system/shared/settings.rs`
  - `apps/aether-gateway/src/handlers/admin/system/shared/update.rs`
  - `crates/aether-admin/src/system.rs`

## 保留与合并

- 配置文档继续使用 fork 的 **2.4**，保留 2.0–2.3 的既有兼容导入。用户交互导出和回滚检查点采用上游 **1.6**；RecoveryBackup 按上游既有 `users_export_version()` 使用 **1.5**，不混淆两种模式。
- Endpoint 的可选 `id`、模型的 `provider_model_mappings` 和可选 `endpoint_bindings` 全程保留；每项绑定的 `endpoint_id`、`source`、`is_active` 均往返保存，包含显式 false。
- Provider 导出采用上游 `SystemExportMode` 投影 URL、headers/body rules、proxy 和任意 JSON，并与本地绑定读取并存。交互导出凭据不出现、凭据依赖对象停用；RecoveryBackup 的凭据读取及失败策略保持上游代码。
- 用户导出的管理员范围与凭据模式分开处理：单独用户导出排除管理员；完整 data 导出包含管理员，两类都传递同一个 `SystemExportMode`。公共方法仍为上游 `build_admin_system_users_export_payload(mode)`；内部 helper 为 `(include_admin_users, mode)`，无新跨模块接口。
- 本地 Endpoint 绑定预校验移到上游统一 `prevalidate_admin_system_config_import`，删除第二段独立预检循环；任何实际写入前即拒绝跨 Provider 引用、重复/空绑定及旧格式歧义。
- Endpoint 创建后同时保留原 ID→目标 ID 映射和上游 mutation journal ID；模型 create/overwrite 使用本地原子 model+bindings 接口，调用上游模型构造器时提供 existing record 和 `credentials_not_exported`，不会遗漏现有敏感 JSON 的还原策略。
- 保留上游导入模式、独立预校验、rollback checkpoint/journal、用户/凭据锁定、Provider ops/OAuth/LDAP/SMTP 等目的地绑定和字段校验。不新增旧导入入口，不进行数据库迁移或实际导入。
- 本地 OAuth Pool score seed 的 quota projection lock + strong Key re-read 保持；设置默认值/更新校验保留 `agent_format_bridge_mode`，`cyber_continue_failover` 去除重复 match arm 后保留上游默认 false。
- revision 写入仍经过既有系统配置接口；本轮没有直接写数据库或绕过上一轮 PG/memory revision 契约。
- GitHub release 检查和新上游同版本资产路径校验均绑定 `ZipperCode/Aether`，保留上游 HTTPS、redirect host、同版本 tarball/checksum、安全更新流程。未执行下载、更新或部署。

## 检查与回归

- 六文件直接 `rustfmt --edition 2021 --config skip_children=true` 后 `--check`：exit 0；未运行全局格式化。
- 六文件 `git diff --check`：exit 0；冲突标记扫描 0 命中。
- 新增纯逻辑测试 `config_import_preserves_endpoint_bindings_with_secret_safe_model_projection`：序列化保留原 ID 和禁用绑定；模型映射/独立绑定同时重映射到 local ID；敏感占位符移除而普通 config 字段保留；未知 Endpoint 引用拒绝。
- 更新 release URL 目标仓库的已有测试，保留 release-assets 域和同版本完整性案例。
- 未编译或运行测试，遵守并发整合期间的禁止全局编译/测试约定。
- 整合后目标检查：`cargo check -p aether-admin -p aether-gateway --all-targets`；`cargo test -p aether-admin agent_bridge_mode_defaults_to_auto_and_accepts_only_auto_or_off`；`cargo test -p aether-admin parse_admin_system_config_import_request`；gateway `config_import_preserves_endpoint_bindings_with_secret_safe_model_projection`、`config_import_restores_existing_secret_safe_json_and_strips_new_placeholders`、导入 prevalidation/rollback 与完整数据导出相关已有测试。
- 更新资产测试 `update_binds_tarball_and_checksum_to_official_same_version_release` 按现有函数仅支持 Linux/macOS，Windows 无受支持资产时会拒绝；在支持的构建环境验证，不为测试添加产品兼容分支。

## 跨写集交接

- 已通知 integrator：`apps/aether-gateway/src/tests/control/admin/system.rs` 约 960 行冲突应同时保留本地 Endpoint binding 断言和上游 LDAP/OAuth 不导出凭据及禁用断言，两者不互斥。
- 未引入其他 helper/API 扩展请求；外部 DTO 构造点需由整合者结合全 crate 类型检查核对既有可选字段。
- 无生成 schema 修改、无任务进程、无临时调试文件待清理，无提交/push/部署。
