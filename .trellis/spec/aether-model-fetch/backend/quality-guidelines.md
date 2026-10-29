# Quality Guidelines

> Code quality standards for backend development.

---

## Overview

<!--
Document your project's quality standards here.

Questions to answer:
- What patterns are forbidden?
- What linting and testing rules do you enforce?
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
- 强制刷新返回已经完成的管理投影，不根据“存在模型抓取 Endpoint”将兼容 `data[]` 目录重新认定为原生 Codex cards。预设元数据先形成再补全。
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

## Codex account-scoped text discovery eligibility

### 1. Scope / Trigger

Required when changing Codex OAuth discovery, account model permissions, pool membership, ordinary runtime selection, Responses WebSocket turns or pinned admin model execution.

### 2. Signatures

- Shared namespace: `aether_provider_transport::CODEX_MODEL_DISCOVERY_NAMESPACE = "codex_model_discovery"`.
- Shared gate: `codex_oauth_discovered_model_skip_reason(provider_type: &str, auth_type: &str, api_format: &str, selected_provider_model_name: &str, upstream_metadata: Option<&Value>) -> Option<&'static str>`.
- Persisted value: `{"codex_model_discovery":{"model_ids":["exact-remote-id"]}}`; reuse existing namespace persistence and routing-cache invalidation, without schema changes.
- Explicit per-account refresh: `POST /api/admin/provider-query/models` with `provider_id`, `api_key_id`, `force_refresh=true` and real administrator authentication.

### 3. Contracts

- Only successful remote `fetch_standard_models` creates evidence, collecting slug-first/id-second exact identities from raw merged cards. Serialize borrowed IDs before moving them into the fetch outcome. Hidden cards remain evidence; bundled execution cards, presets, locked/include/exclude results and local image enrichment never become evidence.
- Aggregate successful endpoints for the same account; conflicting identities fail without replacing evidence. Native `models:[]` retains its existing parse error; compatible successful empty `data`/array replaces discovery with `model_ids:[]`.
- Background success and direct admin refresh persist all namespaces. Successful discovery replaces, never unions, prior IDs; errors/presets/cache-only reads preserve last success. `auto_fetch_models=false` permits explicit direct refresh without changing the switch.
- The gate applies only to trimmed case-insensitive `codex` + `oauth` with alias-equivalent Responses, Compact or Search. Compare `selected_provider_model_name.trim()` by exact case-sensitive identity. No wildcard, client alias, family name, global mapping or tier label authorizes a model.
- Reject missing/non-object namespace, non-array/empty IDs, empty requested model, and any non-string, blank or edge-whitespace entry anywhere in the array, including after a matching entry. Scan borrowed values without sets, auth parsing or request-path I/O.
- Configuration permissions remain additional constraints: `allowed_models=None` is unrestricted configuration; `[]` denies all. Neither permissions nor locked models bypass discovery. Do not disable the account, invalidate OAuth or mark quota exhaustion when a model is absent.
- Common transport checks actual members after existing capability/permission checks. Ordinary runtime checks strong-read real Keys, never a PoolGroup representative. Strong-read pool-page checks run before hot-pool fallback/window truncation, preserving existing quota/auth/cooldown priorities. Sticky and later pages use actual account evidence. Pinned admin text tests gate final request models before auth refresh/send; images and Live bypass text discovery.
- Public versioned `/v1/models` remains a bootstrap/display path writing only scoped KV catalogs, never database authorization. No evidence TTL is added. Upgrade traffic requires each account to complete background or explicit administrator refresh first.

### 4. Validation & Error Matrix

| Condition | Result |
| --- | --- |
| Legacy cards/permissions but no discovery | `key_model_disabled`, zero text upstream sends |
| Alias/locked advanced model absent remotely | Skip that account; retain its basic-model service |
| Case-distinct IDs | Exact identity only |
| Failed refresh after success | Last successful model remains eligible |
| Successful removal/re-addition | Immediate denial/recovery through existing cache invalidation |
| Compatible empty / native empty | Explicit empty evidence / error preserving LKG |
| Image, Live, non-OAuth or non-Codex | Existing independent behavior |
| Pool representative lacks target | Expand and inspect real members, not whole-group rejection |

### 5. Good / Base / Bad Cases

Good: free serves only its fetched basic model while paid handles the advanced model. Base: a failed refresh preserves the successful directory. Bad: effective bundled cards or provider-wide unions authorize an unfetched account.

### 6. Tests Required

- Names include `codex_model_discovery`: strict helper malformed-array/case/bypass matrix and alias-whitelist final-target denial.
- Raw hidden/compatible/multi-endpoint/empty provenance; background locked/image/preset separation, LKG and successful replacement; real independently authenticated two-account model fetch.
- Dedicated loopback gateway fixture: free representative, locked/alias, basic/free + sticky, first-fetch rejection/recovery, remove/re-add invalidation, ordinary sync/SSE, real two-turn WebSocket, pinned admin and versioned bootstrap. Pool cursor paging/frozen-window/hot fallback preserves account-level isolation and existing reason priorities.
- Independent curl streaming smoke observes advanced `free=0, paid=1`, then successful basic free traffic; all credentials synthetic and loopback only. Remove temporary launchers after proof. Production account behavior requires separately authorized real-account validation.
- Existing OAuth-refresh, expiry, quota and scheduling fixtures must seed the selected upstream model in `codex_model_discovery.model_ids` whenever discovery is not their failure under test. For `keeps_refresh_failed_oauth_candidate_selectable_before_local_auth_resolution`, seed `gpt-4.1-responses` and retain both candidate-order and empty-skip assertions. Include these non-`codex`-named tests in the full Gateway gate; a name-filtered discovery run does not cover them.

### 7. Wrong vs Correct

Wrong: treat `codex_models.cards` or a permissive alias whitelist as account entitlement. Correct: intersect independent raw remote evidence with configured permissions using the final sending account/model on every execution candidate.

## Antigravity account evidence and model discovery

### 1. Scope / Trigger

Required when changing Antigravity account hydration, quota refresh, model discovery or metadata persistence. Existing companion projects do not establish that paid-tier observations exist.

### 2. Signatures

- Pure owner: `aether_provider_transport::antigravity::parse_antigravity_code_assist_response(&Value) -> Option<AntigravityCodeAssistInfo>`.
- Shared policies: `antigravity_account_observation_update`, `antigravity_account_observation_due`, `antigravity_entitlement_candidates`, `is_antigravity_entitlement_rejection`.
- Model builder: `build_antigravity_fetch_available_models_plan(runtime, transport, base_url, project_id, tier: Option<&str>)`.
- Quota builder: `build_antigravity_pool_quota_request(key_id, base_url, authorization, project_id, tier: Option<&str>, identity_headers)`.

### 3. Contracts

- `subscription_tier` is display-only canonical `free|pro|ultra|null`; it never populates scheduling `plan_type`. Raw `paid_tier_id` and `current_tier_id` are protocol evidence, never inferred from display names or upgrade menus.
- Successful observation writes nullable tier fields, `account_info_status=success`, checked and updated Unix-second timestamps. Failure writes only `account_info_status=error` and checked timestamp, preserving last successful evidence.
- Success (including unknown tier) has a 900-second refresh TTL; failures have a 60-second cooldown. Future timestamps are stale. Explicit quota refresh always attempts discovery.
- Antigravity namespace updates shallow-merge old fields absent from incoming updates. Explicit null clears old evidence. Replacing `quota_by_model` still drops removed models and preserves a returned model's missing reset time.
- Both discovery paths try observed paid ID, current ID, then no entitlement; exclude free-tier, legacy-tier and duplicates. A 400 `INVALID_ARGUMENT` from a request carrying an entitlement is a bounded compatibility rejection and advances the candidate; 400 project errors without an entitlement reference remain terminal. A 412 `FAILED_PRECONDITION` still requires explicit tier evidence. Independent model host failover is not entitlement fallback.
- Admin provider-wide model queries iterate every selected account and union only the account-local discovered rows. Provider-wide output is a response projection; it must never be copied into any account's `allowed_models` or used to authorize another account.
- Upstream `disabled: true` entries are excluded before both model and quota projections. Unknown enabled IDs remain discoverable. No static catalog, entitlement label, quota bucket or alias may add an Antigravity model.
- Antigravity discovery and generation share the account-local CLI identity `antigravity/cli/<version> <os>/<arch>`; a `client_version` override applies only to that account and must not be silently discarded.

### 4. Validation & Error Matrix

| Input / failure | Required result |
| --- | --- |
| Existing project, missing/due observation | Query account evidence without re-import |
| loadCodeAssist failure, usable cached project | Continue with cached project/tier; persist error observation |
| Successful unknown tier | Clear previous canonical tier and raw IDs explicitly |
| 401/403/429 or transport failure | Do not switch entitlement candidate |
| Invalid entitlement parameter | Advance bounded candidate list |
| Model metadata refresh | Preserve account evidence and other namespaces |
| First account succeeds, later account has exclusive model | Continue and include the later account's model in provider response |
| One account fails, another succeeds | Preserve successful rows and surface the failed account as warning |
| Upstream `disabled:true` row | Exclude the row and its quota metadata |

### 5. Good / Base / Bad Cases

Good: an old project-only account acquires Pro, paid entitlement rejection falls back to current entitlement. Base: free account sends project without entitlement. Bad: a failure patch replaces the entire Antigravity namespace, deleting the project and last successful tier. Good: mixed-tier accounts expose the union only at provider-query projection while each account remains isolated. Bad: stop after the first successful account or synthesize Gemini 4 from a quota bucket.

### 6. Tests Required

- Shared parsing precedence, raw-ID versus display-name distinction, candidate deduplication/exclusion and TTL boundaries.
- Real model-fetch execution sequence, authentication/throttle/transport non-fallback and cached evidence on discovery failure.
- Namespace merge failure preservation and explicit-null clearing; local HTTP smoke must inspect actual request bodies and returned metadata.
- Provider-query loopback coverage for later-account-only models, empty first success, mixed tiers, partial failures, duplicate endpoint evidence, selected-account isolation, stale provider-cache bypass and disabled/quota projection.

### 7. Wrong vs Correct

Wrong: return early solely because project_id exists, or infer `g1-pro-tier` from a UI label. Wrong: treat one successful account as the provider's complete model list.

Correct: use the shared observation freshness and raw-entitlement policy, merge field patches without erasing successful evidence, iterate every requested account, and keep the provider union separate from account-local authorization.
