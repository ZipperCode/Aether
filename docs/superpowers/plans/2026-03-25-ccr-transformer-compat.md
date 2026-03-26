# CCR Transformer Compatibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Aether 增加可插拔 transformer pipeline，并持续补齐 `claude-code-router` 风格的协议兼容能力；当前已完成第一阶段全链路落地，并进入 provider 级 preset 收口。

**Architecture:** 保留现有 `normalizer -> internal -> normalizer` canonical conversion，在 internal request/response/stream/error 上新增 transformer pipeline。`body_rules` 继续处理静态字段覆写，`envelope` 继续处理 provider 私有包裹，兼容策略统一迁移到 transformer。

**Tech Stack:** Python, FastAPI, Pydantic, pytest

## Current Status

- [x] `TransformerPipeline` 基础设施已完成，并接入 request / response / stream / error 四条主链路
- [x] endpoint `transformers` 与 provider `default_transformers` 配置模型已完成
- [x] 第一批 builtin transformer 已完成：`tooluse`、`enhancetool`、`reasoning`、`sampling`、`maxtoken`、`cleancache`
- [x] family 级 builtin fallback 已完成：`claude:*`、`gemini:*`、`openai:*`
- [x] 端到端回归已覆盖 `openai -> claude`、`openai -> gemini`、`cli -> chat`、Claude same-format `cleancache`
- [x] provider 级 preset 已接入插件注册链：`codex`、`claude_code`、`kiro`、`gemini_cli`
- [ ] 下一步可继续补 provider 私有 diagnostics / degradation 标记，以及更细粒度 provider+endpoint 语义补丁

---

## File Map

### New Files

- `src/core/api_format/transformers/__init__.py`
- `src/core/api_format/transformers/base.py`
- `src/core/api_format/transformers/config.py`
- `src/core/api_format/transformers/pipeline.py`
- `src/core/api_format/transformers/registry.py`
- `src/core/api_format/transformers/builtin/__init__.py`
- `src/core/api_format/transformers/builtin/tooluse.py`
- `src/core/api_format/transformers/builtin/reasoning.py`
- `tests/core/api_format/transformers/test_pipeline.py`
- `tests/core/api_format/transformers/test_tooluse_transformer.py`
- `tests/core/api_format/transformers/test_reasoning_transformer.py`

### Modified Files

- `src/models/endpoint_models.py`
- `src/core/api_format/capabilities.py`
- `src/core/api_format/__init__.py`
- `src/api/handlers/base/chat_handler_base.py`
- `src/api/handlers/base/cli_sync_mixin.py`
- `src/api/handlers/base/stream_processor.py`
- `src/api/handlers/base/chat_error_utils.py` 或等价错误转换接点

---

### Task 1: 定义 Transformer 基础抽象与注册表

**Files:**
- Create: `src/core/api_format/transformers/base.py`
- Create: `src/core/api_format/transformers/config.py`
- Create: `src/core/api_format/transformers/registry.py`
- Create: `src/core/api_format/transformers/__init__.py`
- Test: `tests/core/api_format/transformers/test_pipeline.py`

- [ ] **Step 1: 写失败测试，覆盖 transformer 注册与配置合并的最小行为**

- [ ] **Step 2: 运行测试，确认因模块不存在或接口缺失而失败**

Run: `pytest tests/core/api_format/transformers/test_pipeline.py -v`

- [ ] **Step 3: 实现最小基础类型**

实现：
- `TransformerSpec`
- `TransformContext`
- `FormatTransformer`
- `TransformerRegistry`

- [ ] **Step 4: 再次运行测试，确认基础测试通过**

Run: `pytest tests/core/api_format/transformers/test_pipeline.py -v`

- [ ] **Step 5: 提交当前最小骨架**

---

### Task 2: 实现 Transformer Pipeline

**Files:**
- Create: `src/core/api_format/transformers/pipeline.py`
- Modify: `src/core/api_format/transformers/__init__.py`
- Test: `tests/core/api_format/transformers/test_pipeline.py`

- [ ] **Step 1: 写失败测试，覆盖 request/response/stream/error 四类阶段调用顺序**

- [ ] **Step 2: 运行测试，确认失败原因符合预期**

Run: `pytest tests/core/api_format/transformers/test_pipeline.py -v`

- [ ] **Step 3: 实现最小 pipeline**

实现：
- 按顺序执行 transformer
- 支持禁用
- stream 阶段支持 event 扩展或过滤

- [ ] **Step 4: 运行测试，确认 pipeline 行为通过**

Run: `pytest tests/core/api_format/transformers/test_pipeline.py -v`

- [ ] **Step 5: 提交 pipeline**

---

### Task 3: 扩展配置模型，支持 endpoint/provider transformer 配置

**Files:**
- Modify: `src/models/endpoint_models.py`
- Modify: `src/core/api_format/capabilities.py`
- Modify: `src/core/api_format/__init__.py`
- Test: `tests/core/api_format/transformers/test_pipeline.py`

- [ ] **Step 1: 写失败测试，覆盖 endpoint transformer 配置校验与 provider 默认配置读取**

- [ ] **Step 2: 运行失败测试**

Run: `pytest tests/core/api_format/transformers/test_pipeline.py -v`

- [ ] **Step 3: 最小实现配置模型与读取接口**

实现：
- endpoint `transformers`
- provider `default_transformers`
- 合并策略辅助函数

- [ ] **Step 4: 运行测试，确认配置层通过**

Run: `pytest tests/core/api_format/transformers/test_pipeline.py -v`

- [ ] **Step 5: 提交配置支持**

---

### Task 4: 将 Pipeline 接入 Chat 主链路

**Files:**
- Modify: `src/api/handlers/base/chat_handler_base.py`
- Test: `tests/core/api_format/transformers/test_pipeline.py`

- [ ] **Step 1: 写失败测试，覆盖 request/response 方向接入点**

- [ ] **Step 2: 运行失败测试**

Run: `pytest tests/core/api_format/transformers/test_pipeline.py -v`

- [ ] **Step 3: 最小接入 request/response pipeline**

要求：
- 不启用 transformer 时行为不变
- request 在 source normalizer 后执行
- response 在 provider normalizer 后执行

- [ ] **Step 4: 运行测试，确认通过**

Run: `pytest tests/core/api_format/transformers/test_pipeline.py -v`

- [ ] **Step 5: 提交主链路接入**

---

### Task 5: 将 Pipeline 接入 CLI / Stream / Error 链路

**Files:**
- Modify: `src/api/handlers/base/cli_sync_mixin.py`
- Modify: `src/api/handlers/base/stream_processor.py`
- Modify: `src/api/handlers/base/chat_error_utils.py`
- Test: `tests/core/api_format/transformers/test_pipeline.py`

- [ ] **Step 1: 写失败测试，覆盖 stream event 与 error transform**

- [ ] **Step 2: 运行失败测试**

Run: `pytest tests/core/api_format/transformers/test_pipeline.py -v`

- [ ] **Step 3: 最小实现 stream/error 接入**

- [ ] **Step 4: 运行测试，确认 pipeline 已覆盖四类阶段**

Run: `pytest tests/core/api_format/transformers/test_pipeline.py -v`

- [ ] **Step 5: 提交全链路接入**

---

### Task 6: 实现 `tooluse` Transformer

**Files:**
- Create: `src/core/api_format/transformers/builtin/tooluse.py`
- Create: `tests/core/api_format/transformers/test_tooluse_transformer.py`
- Modify: `src/core/api_format/transformers/builtin/__init__.py`
- Modify: `src/core/api_format/transformers/registry.py`

- [ ] **Step 1: 写失败测试，覆盖以下场景**

场景：
- OpenAI Chat `tool_calls` 到 Claude
- OpenAI Responses `function_call` 到 Claude
- Claude `tool_use/tool_result` 到 OpenAI Chat
- 缺失 tool id 的自动修复

- [ ] **Step 2: 运行失败测试**

Run: `pytest tests/core/api_format/transformers/test_tooluse_transformer.py -v`

- [ ] **Step 3: 实现最小 `tooluse` transformer**

要求：
- 修复 tool call id
- 统一 tool choice
- 保留合法参数，不额外扩展 schema

- [ ] **Step 4: 运行测试，确认通过**

Run: `pytest tests/core/api_format/transformers/test_tooluse_transformer.py -v`

- [ ] **Step 5: 提交 `tooluse`**

---

### Task 7: 实现 `reasoning` Transformer

**Files:**
- Create: `src/core/api_format/transformers/builtin/reasoning.py`
- Create: `tests/core/api_format/transformers/test_reasoning_transformer.py`
- Modify: `src/core/api_format/transformers/builtin/__init__.py`
- Modify: `src/core/api_format/transformers/registry.py`

- [ ] **Step 1: 写失败测试，覆盖以下场景**

场景：
- OpenAI `reasoning_effort` 到 Claude `thinking`
- Claude `thinking` 到 OpenAI `reasoning_effort`
- Gemini `thinkingConfig` 映射
- 不支持 reasoning 时的清理或降级

- [ ] **Step 2: 运行失败测试**

Run: `pytest tests/core/api_format/transformers/test_reasoning_transformer.py -v`

- [ ] **Step 3: 实现最小 `reasoning` transformer**

要求：
- 显式配置优先
- 可安全降级时降级
- 不可表达时写入 diagnostics

- [ ] **Step 4: 运行测试，确认通过**

Run: `pytest tests/core/api_format/transformers/test_reasoning_transformer.py -v`

- [ ] **Step 5: 提交 `reasoning`**

---

### Task 8: 集成验证与回归检查

**Files:**
- Test: `tests/core/api_format/transformers/test_pipeline.py`
- Test: `tests/core/api_format/transformers/test_tooluse_transformer.py`
- Test: `tests/core/api_format/transformers/test_reasoning_transformer.py`
- Test: `tests/core/api_format/conversion/test_registry_canonical.py`
- Test: `tests/core/api_format/conversion/test_cli_conversion.py`

- [ ] **Step 1: 运行 transformer 新增测试**

Run: `pytest tests/core/api_format/transformers -v`

- [ ] **Step 2: 运行现有协议转换关键回归**

Run: `pytest tests/core/api_format/conversion/test_registry_canonical.py tests/core/api_format/conversion/test_cli_conversion.py -v`

- [ ] **Step 3: 如有必要补最小修复并重跑测试**

- [ ] **Step 4: 汇总变更与验证结果**

- [ ] **Step 5: 提交 MVP**

---

## Notes

- 第一个可交付版本只做 `tooluse` 与 `reasoning`
- `enhancetool` / `sampling` / `maxtoken` / `cleancache` 在本计划之后继续推进
- 所有生产代码必须遵循 TDD：先写失败测试，再写最小实现
