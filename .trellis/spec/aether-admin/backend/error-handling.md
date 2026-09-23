# Error Handling

> How errors are handled in this project.

---

## Overview

<!--
Document your project's error handling conventions here.

Questions to answer:
- What error types do you define?
- How are errors propagated?
- How are errors logged?
- How are errors returned to clients?
-->

(To be filled by the team)

---

## Error Types

<!-- Custom error classes/types -->

(To be filled by the team)

---

## Error Handling Patterns

<!-- Try-catch patterns, error propagation -->

(To be filled by the team)

---

## API Error Responses

<!-- Standard error response format -->

(To be filled by the team)

---

## Common Mistakes

<!-- Error handling mistakes your team has made -->

(To be filled by the team)

## 批量导入 Key 与模型同步

### 适用范围

提供商批量导入的逐项错误处理、自动获取模型设置及前端成功关闭行为。

### 接口

`POST /api/admin/pool/{provider_id}/keys/batch-import` 接收统一 `settings` 和 `keys[].settings` 覆盖；响应保留 `imported`、`skipped`、`errors: [{index, reason}]`，另返回 `model_sync`（未请求同步时为 `null`）。

### 字段契约

设置支持 `auto_fetch_models: boolean`、`model_include_patterns: string[] | null`、`model_exclude_patterns: string[] | null`，与单 Key 字段同义；空数组及 `null` 表示无规则。先持久化，再通过既有 `perform_model_fetch_for_keys` 同步已创建且开启自动获取的 Key。

### 错误矩阵

- 开关不是布尔值或规则不是字符串数组：拒绝设置，不静默忽略。
- 某项导入失败：`errors[].index` 指向本次请求索引；前端保留该项及原因，重试不重复提交已成功项。
- 导入成功而模型抓取失败：不回滚 Key；`model_sync` 报告失败，后续按既有自动抓取机制处理。

### 正常与异常示例

正常：统一开启自动获取，包含 `gpt-*`，单项可覆盖配置。基础：未开启时不执行本次模型同步。错误：返回部分成功后让用户重交整个原列表，导致重复入库或重复失败。

### 回归验证

`validates_and_applies_auto_fetch_models_settings` 覆盖校验及清空规则，`batch_import_key_record_persists_auto_fetch_models_settings` 覆盖 Key 构建；前端 `provider-key-batch-import-behavior.spec.ts` 验证真实组件的成功关闭、部分失败与仅重试失败项。

### 错误与正确实现

错误：把模型抓取失败当作导入回滚，或用成功提示掩盖逐项错误。正确：持久化结果与模型同步结果分开表达；全部成功关闭弹窗，部分失败保留可修正条目。
