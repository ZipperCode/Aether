# Quality Guidelines

> Code quality standards for backend development.

---

## Overview

<!--
Document your project's quality standards here.

Questions to answer:
- What patterns are forbidden?
- What linting rules do you enforce?
- What are your testing requirements?
- What code review standards apply?
-->

(To be filled by the team)

---

## Forbidden Patterns

<!-- Patterns that should never be used and why -->

(To be filled by the team)

---

## Required Patterns

<!-- Patterns that must always be used -->

(To be filled by the team)

---

## Testing Requirements

<!-- What level of testing is expected -->

(To be filled by the team)

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)

## Codex OAuth 模型与 Endpoint 能力契约

### 1. Scope / Trigger

修改 Codex 模型发现、Key 白名单同步、套餐准入或管理端模型关联时适用。原生文本目录、管理员模型限制、Endpoint 协议和实时额度是独立事实。

### 2. Signatures

发现入口 `fetch_and_persist_key_models` 与管理模型查询共享 Codex management projection。配置模型由 `ModelFetchAssociationStore::list_admin_provider_models` 与 `list_model_endpoint_bindings` 提供。

`aether_provider_transport::codex_oauth_capability_skip_reason(provider_type, auth_type, api_format, upstream_metadata: Option<&Value>, auth_config: Option<&Value>) -> Option<&'static str>` 为发现与执行提供一致的已知套餐限制。Gateway 通过既有 `crate::provider_transport` seam 使用传输能力，不新增跨域转发层。

### 3. Contracts

- 图片型号来自该 Provider 已启用的真实模型配置、有效图片 Endpoint 绑定及图片家族/明确能力，不能固定注入默认型号，也不能把旧 migration 的纯文本模型图片绑定当成生图型号。读取配置启用状态而非动态 `is_available`，使旧白名单禁用状态可以恢复。
- 图片条目携带 `supports_image_generation=true`、`openai:image` 和准确 Endpoint IDs；保留实际上游映射名称，不映射为文本宿主型号。
- 真正发现的 Codex 文本模型关联 active 且 Key 允许的 Responses、Compact、Alpha Search。使用既有 `api_format_permission_covers` 保留 Responses 权限覆盖 Search/Compact 的语义；不据此自动关联 Live。
- `supports_search_tool` 在官方 Codex 中表示 deferred tool discovery，缺失或 false 不等于 Alpha Search 不可用，禁止将它用于网页搜索拒绝。
- 套餐保留原始身份：Free 图片请求返回 `codex_plan_image_generation_unsupported`；Plus、Pro、ProLite、ProMax 和未知套餐不添加未经证实的静态型号限制。此规则只作用于 Codex OAuth，认证、白名单和额度仍独立检查。
- 管理查询、新鲜抓取、旧缓存与预设投影保持一致；不修改原生 `cached_models`、版本化目录和 `codex_models`。预设元数据先形成再补全。
- 自动刷新继续执行 `apply_model_filters`；手工白名单与停用绑定不被覆盖，`None` 模型白名单保持无限制语义，显式 `[]` 在管理测试和正式调用中都禁止。
- 保持每 Provider 一次可用性协调和轻量 Key 投影；只增加所需认证类型标量，不扫描完整 Key、认证材料或状态快照。
- 图片配置映射的 `api_formats: []` 与 `endpoint_ids: []` 沿用正式候选的拒绝语义；`operations: []` 经既有持久层规范化表示继承，不能把两者混同。
- 强制刷新返回已经完成的管理投影，不根据“存在模型抓取 Endpoint”将兼容 `data[]` 目录重新认定为原生 Codex cards。
- 无文本抓取 Endpoint 的预设路径中，如果 Codex OAuth 已配置可用图片模型且 metadata 没有套餐事实，只在现有维护许可内读取一个选中图片 transport 的认证声明；不发出上游请求，不扩大批量轻量读取，并继续隔离损坏的旧凭据。

### 4. Validation & Error Matrix

| 条件 | 行为 |
| --- | --- |
| 已配置未来图片型号且有效图片绑定 | 无需代码枚举即可补全，原型号进入图片上游 |
| 无有效图片配置/绑定或 Key 格式不允许 | 不补全，不从旧文本绑定制造生图能力 |
| 已知 Free OAuth，图片调用 | 发现不新增图片能力；正式与管理执行以独立套餐原因拒绝 |
| Plus/Pro/ProLite/未知套餐 | 不凭套餐名假定每个型号可用；保留原有模型、额度和上游校验 |
| 新文本型号缺少 Search/Compact 绑定 | 按已启用原生 Endpoint 和 Key 格式权限补齐，保留人工停用 |
| `supports_search_tool=false` 或缺失 | 不阻断 Alpha Search，不虚构型号权限 |
| include/exclude/locked 规则 | 沿用既有优先级，补全不会绕过过滤 |
| 旧版本化文本缓存 | 只补管理投影，强刷后修复自动白名单与绑定 |
| 手工白名单缺少模型或显式空列表 | 保持限制，管理测试与正式请求一致 |
| 无 Live 专属证据 | 不从文本列表自动生成 Live 绑定，保留手工配置和 OAuth WebRTC 边界 |

### 5. Good / Base / Bad Cases

Good：配置新图片型号后刷新即可路由，新发现文本型号恢复原生 Search/Compact。Base：非 Codex 与未改动 Responses 行为保持不变。Bad：将图片卡写入 Codex CLI 原生目录，或用 `gpt-image-*` 前缀绕过手工权限和套餐。

### 6. Tests Required

- 动态配置的未来图片型号、旧缓存、禁用 Endpoint/模型、旧文本迁移绑定及映射名称，断言白名单、绑定、原生卡隔离。
- Free / Plus / Pro / ProLite / 未知身份矩阵，正式同步/流式与管理测试的 Free 拒绝不得发出上游请求；保留原有额度阻断。
- Search/Compact 新型号投影、Responses 格式权限继承和人工停用绑定；工具发现标志 false/missing 不阻断 Alpha Search。
- 真实本机 HTTP 图片与 Alpha Search 精确 URL/请求/响应契约；空白名单和显式排除上游零调用。
- 相关 Compact、Responses 和 Live 协议回归及架构依赖检查，不能用 mock runtime 代替真实 HTTP 证明。

### 7. Wrong vs Correct

Wrong：默认型号就是全部支持范围，或者把模型卡上含有 search 的任意字段当作 Alpha Search 权限。

Correct：配置/真实发现生成可路由模型和 Endpoint，已核实的 Free 限制独立执行，其余实际权限由既有策略与上游确定。官方依据见任务研究记录及 [Codex image generation guard](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/core/src/tools/spec_plan.rs#L727)、[standalone search selection](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/core/src/tools/spec_plan.rs#L1038)。
