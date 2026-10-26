# 修复导入凭据自动发现上游模型

## Goal

让批量导入的 Provider API Key，以及 OAuth 单个/批量导入创建的 Provider Key，默认开启自动发现上游模型，使导入凭据可进入现有模型发现与定时刷新链路。

## Confirmed Facts

- `StoredProviderCatalogKey.auto_fetch_models` 的持久化默认值为 `false`。
- Provider API Key 批量导入前端 `ProviderKeyBatchImportDialog.vue:createDefaultSettings` 当前将 `auto_fetch_models` 设为 `false`，提交时会把该值发送到批量导入接口。
- OAuth 导入统一经过 `apps/aether-gateway/src/handlers/admin/provider/oauth/provisioning.rs:create_provider_oauth_catalog_key` 创建 Key；该构造路径当前未设置 `record.auto_fetch_models`，因此沿用 `StoredProviderCatalogKey::new` 的 `false`。
- 自动模型发现运行时只处理 `is_active && auto_fetch_models` 的 Key；创建 API Key 的显式入口已有“创建后立即抓取”逻辑，可作为行为参考。

## Requirements

- 批量导入 API Key 的新 Key 默认开启自动发现上游模型。
- OAuth 导入创建的新 Key 默认开启自动发现上游模型，覆盖单个 OAuth 导入和 OAuth 批量导入共用的创建路径。
- 保留现有导入表单的其他 Key 设置、模型包含/排除规则和显式配置能力；不改变已存在 Key 的设置。
- 复用现有模型发现运行时与持久化字段，不新增数据库字段、兼容分支或硬编码模型列表。

## Acceptance Criteria

- [x] 批量 API Key 导入请求默认携带 `auto_fetch_models: true`，后端创建的每个 Key 持久化为 `true`。
- [x] OAuth 单个导入和 OAuth 批量导入创建的每个 Key 持久化为 `auto_fetch_models = true`。
- [x] 新导入 Key 会进入现有后台自动发现筛选条件，并可由现有模型查询/刷新路径读取发现结果。
- [x] 已有的显式关闭自动发现能力（批量 Key 导入表单）仍按用户选择生效；OAuth 重复替换保留既有 Key 设置。
- [x] 相关前端与后端行为测试覆盖默认开启及导入路径，不引入敏感凭据日志。

## Key Decision

- 批量 Key 导入界面保留“自动获取模型”开关，但默认值改为开启；用户仍可显式关闭。OAuth 单个/批量导入没有该表单开关，统一默认开启。这样满足导入即发现，同时保留批量 Key 导入的请求成本控制。

## Out of Scope

- 不修改手动创建 Key、已有 Key 编辑、非导入的模型发现默认值。
- 不立即为历史已导入且 `auto_fetch_models = false` 的 Key 批量回填开启。
- 不更改 OAuth 凭据交换、模型过滤或模型目录关联规则。

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
