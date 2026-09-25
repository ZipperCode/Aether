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

## Codex 图片能力补全契约

### 1. Scope / Trigger

修改 Codex 模型发现、Key 白名单同步或管理端模型关联时适用；文本目录不等于图片能力目录。

### 2. Signatures

`supplement_codex_image_models(models: &mut Vec<Value>, image_endpoint_ids: &[String]) -> bool` 仅用于管理 legacy/association 投影。模型 ID 复用 `aether_ai_formats::api::CODEX_OPENAI_IMAGE_DEFAULT_MODEL`。

Gateway 调用该常量必须经既有 `crate::ai_serving::CODEX_OPENAI_IMAGE_DEFAULT_MODEL` 入口，不得从业务模块直接引用 `aether_ai_formats::api`；以 `ai_serving_crate_api_is_confined_to_root_seams` 架构检查验证。

### 3. Contracts

- 调用方确认 Provider 为 Codex、图片 Endpoint active、Key 显式非空格式列表允许 `openai:image`；空格式列表沿用模型发现的继承语义。管理查询与后台刷新必须一致。
- 新条目携带 `supports_image_generation=true`、`api_formats=["openai:image"]` 和准确 `endpoint_ids`，不得绑定 Responses 端点代替图片端点。
- 原生 `cached_models`、版本化目录及 `codex_models` 元数据不经过补全；preset 元数据必须在补全前形成。
- 自动刷新对补全 ID 继续执行 `apply_model_filters`，再持久化白名单并沿用每 Provider 一次的可用性协调。管理查询不改手工白名单，不在 scheduler/transport 增加绕过。

### 4. Validation & Error Matrix

| 条件 | 行为 |
| --- | --- |
| 文本目录不含图片模型，存在可用图片端点 | 管理投影补全；自动刷新恢复白名单及模型可用性 |
| 无 active 图片端点或显式 API 格式不允许图片 | 不补全 |
| exclude 匹配 `gpt-image-*` | 补全仍经排除规则，不恢复该 Key 的图片权限 |
| 旧版本化文本缓存 | 管理投影补全，缓存原文不改；旧自动白名单强刷后恢复 |
| 手工白名单不含图片模型 | 不改写，正常拒绝调度 |

### 5. Good / Base / Bad Cases

Good：文本模型仍来自远端，图片能力仅绑定真实图片端点。Base：非 Codex 维持原有流程。Bad：将图片卡写进 Codex CLI 原生目录，或全局放开所有 `gpt-image-*` 模型。

### 6. Tests Required

- `codex_image_model_fetch_repairs_legacy_whitelist_and_restores_image_model_availability`：强刷后白名单、可用性、图片绑定及原生元数据隔离。
- `codex_image_model_fetch_without_image_permission_or_endpoint_does_not_expand_whitelist`：格式、Endpoint、exclude 三种拒绝边界。
- 管理端 `gateway_supplements_codex_gpt_image_2_for_admin_provider_query_models`：新鲜与缓存、指定 Key 与默认聚合，保留准确图片能力。
- 真实本机 HTTP 同步/流式图片请求可执行，显式白名单排除时上游零调用。

### 7. Wrong vs Correct

Wrong：上游文本列表缺失图片模型，因此判定 Key 不支持图片；或忽略显式模型限制强行调度。

Correct：在管理发现投影补全已实现的图片能力，继续使用原有过滤、持久化、关联和执行链。
