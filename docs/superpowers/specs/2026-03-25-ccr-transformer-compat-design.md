# Aether 协议兼容层增强设计

**日期：** 2026-03-25

**主题：** 对齐 `claude-code-router` 的 transformer pipeline 能力，在保留现有 canonical conversion 架构的前提下，增强 Aether 的协议兼容与工具调用适配能力。

---

## 1. 背景

当前 Aether 已具备较完整的协议转换基础：

- 统一的 `normalizer -> internal -> normalizer` canonical conversion
- 已覆盖 `openai:chat`、`openai:cli`、`claude:chat`、`claude:cli`、`gemini:*`
- 已支持 request / response / stream / tool calling 转换
- 已有 provider 扩展能力：`variant`、`body_rules`、`envelope`

但与 `claude-code-router` 相比，Aether 仍缺少一层显式、可配置、可扩展的语义兼容层：

- 无显式 `transformers` 配置链
- 兼容逻辑分散在 normalizer、repair、variant、body rules 中
- response / stream 方向缺统一兼容修补入口
- 难以产品化表达 `tooluse`、`enhancetool`、`reasoning` 等能力

---

## 2. 目标

### 2.1 主要目标

1. 在现有 canonical conversion 架构上新增 `TransformerPipeline`
2. 让 request / response / stream / error 都能挂载兼容策略
3. 第一阶段优先对齐 Claude/Anthropic 场景
4. 配置模型尽量接近 `claude-code-router` 的 transformer 思路
5. 保持多协议通用，不做 `claude-only` 特化架构

### 2.2 非目标

1. 不重写现有 normalizer 体系
2. 不复制 `claude-code-router` 的全部实现细节
3. 不把所有兼容逻辑都迁移到 transformer；provider 私有包裹仍留在 `envelope`
4. 第一阶段不追求一次覆盖所有 provider 私有怪异行为

---

## 3. 现状结论

### 3.1 已具备能力

- OpenAI/Claude/Gemini 协议互转
- OpenAI Chat `tool_calls` 与 Claude `tool_use/tool_result` 转换
- OpenAI Responses `function_call/function_call_output` 与 Claude 工具块转换
- 部分 reasoning / thinking 字段映射
- 部分 token / max_tokens / cache 字段兼容处理

### 3.2 关键缺口

- 缺 `transformers` 一等公民配置
- 缺 request / response / stream / error 统一兼容策略层
- 缺显式 `tooluse` / `enhancetool` / `reasoning` / `sampling` / `maxtoken` / `cleancache`
- 缺自定义 transformer 注册机制

### 3.3 核心判断

Aether 现在不是“不能实现 CCR 的兼容能力”，而是“底层转换能力已有，缺少产品化的兼容框架”。

---

## 4. 设计原则

1. **保留 canonical internal model**
2. **Transformer 只处理语义兼容，不替代 normalizer**
3. **Body rules 只保留静态字段覆写职责**
4. **Provider envelope 只处理私有包裹、签名、拆包、额外头**
5. **所有流式兼容逻辑基于 internal stream event，而不是直接改原始 chunk**
6. **不启用 transformer 时，现有行为必须保持不变**

---

## 5. 目标架构

### 5.1 组件职责

- `Normalizer`
  - 协议结构映射
- `Transformer`
  - 语义兼容、参数修补、降级、增强
- `Body Rules`
  - endpoint 级静态字段覆写
- `Variant`
  - provider 特定格式输出差异
- `Envelope`
  - provider 私有包裹与拆包

### 5.2 请求链路

1. client payload
2. source normalizer -> `InternalRequest`
3. request transformer pipeline
4. target normalizer -> provider payload
5. endpoint `body_rules`
6. provider `envelope.wrap_request`

### 5.3 响应链路

1. upstream payload
2. provider `envelope.unwrap_response`
3. provider normalizer -> `InternalResponse`
4. response transformer pipeline
5. client normalizer -> client payload

### 5.4 流式链路

1. upstream chunk
2. provider `envelope.unwrap`
3. provider normalizer -> `InternalStreamEvent`
4. stream transformer pipeline
5. client normalizer -> client chunk

### 5.5 错误链路

1. upstream error
2. normalizer -> `InternalError`
3. error transformer pipeline
4. client normalizer -> client error payload

---

## 6. 模块拆分

建议新增模块：

- `src/core/api_format/transformers/base.py`
- `src/core/api_format/transformers/pipeline.py`
- `src/core/api_format/transformers/registry.py`
- `src/core/api_format/transformers/config.py`
- `src/core/api_format/transformers/builtin/tooluse.py`
- `src/core/api_format/transformers/builtin/enhancetool.py`
- `src/core/api_format/transformers/builtin/reasoning.py`
- `src/core/api_format/transformers/builtin/sampling.py`
- `src/core/api_format/transformers/builtin/maxtoken.py`
- `src/core/api_format/transformers/builtin/cleancache.py`

建议扩展现有配置模型：

- endpoint 级：新增 `transformers`
- provider 级：新增 `default_transformers`
- registry 级：支持内置与自定义 transformer 注册

---

## 7. 数据结构

### 7.1 Transformer 配置

建议配置结构：

```json
{
  "transformers": [
    {"name": "tooluse"},
    {"name": "enhancetool", "config": {"strict": false}},
    {"name": "reasoning", "config": {"budget_policy": "adaptive"}},
    {"name": "sampling"},
    {"name": "maxtoken"},
    {"name": "cleancache"}
  ]
}
```

### 7.2 Context

```python
@dataclass(slots=True)
class TransformContext:
    stage: Literal["request", "response", "stream", "error"]
    client_format: str
    provider_format: str
    provider_type: str | None
    target_variant: str | None
    model: str | None
    is_stream: bool
    endpoint_id: str | None
    request_id: str | None
    transformer_config: dict[str, Any]
```

### 7.3 Transformer 接口

```python
class FormatTransformer(Protocol):
    NAME: str

    def transform_request(
        self,
        internal: InternalRequest,
        ctx: TransformContext,
    ) -> InternalRequest: ...

    def transform_response(
        self,
        internal: InternalResponse,
        ctx: TransformContext,
    ) -> InternalResponse: ...

    def transform_stream_event(
        self,
        event: InternalStreamEvent,
        ctx: TransformContext,
    ) -> list[InternalStreamEvent]: ...

    def transform_error(
        self,
        internal: InternalError,
        ctx: TransformContext,
    ) -> InternalError: ...
```

---

## 8. 配置合并规则

配置来源：

1. 系统默认 transformer
2. provider 默认 transformer
3. endpoint 显式 transformer
4. 运行时强制补丁

合并规则：

1. 同名 transformer 后者覆盖前者 `config`
2. `enabled: false` 可显式禁用默认 transformer
3. 最终执行顺序按合并后的稳定顺序执行
4. 未注册 transformer 在配置加载时直接报错

---

## 9. 第一阶段内置 transformer

### 9.1 `tooluse`

职责：

- 统一工具定义 schema
- 统一 `tool_choice`
- 统一 `parallel_tool_calls`
- 修复 tool call id / tool result 关联
- 协调 OpenAI Chat、OpenAI Responses、Claude Messages 的工具语义

### 9.2 `enhancetool`

职责：

- 修复空 arguments
- 修复非法 JSON arguments
- 缺失 `call_id` 自动补齐
- 修复 tool result 序列异常
- 对工具 schema 做最小安全增强

### 9.3 `reasoning`

职责：

- 统一 `reasoning_effort`
- 映射 Claude `thinking`
- 映射 Gemini `thinkingConfig`
- 提供降级与清理策略

### 9.4 `sampling`

职责：

- 统一 `temperature`
- 统一 `top_p`
- 统一 `top_k`
- 统一 `n`

### 9.5 `maxtoken`

职责：

- 统一 `max_tokens`
- 统一 `max_completion_tokens`
- 结合 `output_limit`
- 目标协议需要时自动补默认值
- 需要时执行 clamp

### 9.6 `cleancache`

职责：

- 清理 `prompt_cache_key`
- 清理 Anthropic `cache_control`
- 清理 provider 不兼容缓存字段

---

## 10. 与现有机制的边界

### 10.1 Normalizer

保留协议结构转换职责，不再继续承载越来越多的“策略性兼容修补”。

### 10.2 Body Rules

继续用于 endpoint 静态字段改写，但不再承担：

- tool 语义兼容
- reasoning 兼容
- stream 事件兼容

### 10.3 Variant

继续保留 provider 格式变体职责，但不作为语义兼容主入口。

### 10.4 Envelope

继续只承担 provider 私有包裹、签名、拆包、额外头。

---

## 11. 实施阶段

### Phase 0

- 定义 transformer 抽象
- 定义 registry / pipeline / config / context
- 不改变现有行为

### Phase 1

- 在 request / response / stream / error 主链路接入 pipeline
- 增加 logging / metrics / 开关

### Phase 2

- 实现 `tooluse`

### Phase 3

- 实现 `enhancetool`

### Phase 4

- 实现 `reasoning`

### Phase 5

- 实现 `sampling` / `maxtoken` / `cleancache`

### Phase 6

- 实现自定义 transformer 插件机制

---

## 12. 测试矩阵

需要覆盖：

1. request transform
2. response transform
3. stream transform
4. error transform
5. config merge
6. disabled transformer
7. order-sensitive transformer
8. provider defaults + endpoint override

重点场景：

- `openai:chat -> claude:chat` + tools
- `openai:cli -> claude:chat` + `function_call_output`
- `claude:chat -> openai:chat` + `tool_use`
- `claude:chat -> openai:cli` + `tool_result`
- `reasoning_effort <-> thinking`
- `max_tokens` / `max_completion_tokens` clamp
- cache 字段清理
- text/tool 混排流式事件

---

## 13. 风险与控制

### 风险 1：职责重复

风险：
normalizer、transformer、body_rules、variant 之间逻辑重叠。

控制：
明确 transformer 只改 internal 语义；body_rules 只改 payload；envelope 只改 provider 私有包裹。

### 风险 2：输出顺序不稳定

风险：
request / response / stream 链路顺序不同会导致行为难预测。

控制：
固定执行顺序并加测试覆盖。

### 风险 3：流式状态机破坏

风险：
直接改原始 chunk 容易破坏上下文状态。

控制：
stream transformer 只处理 `InternalStreamEvent`。

---

## 14. 最小可交付版本

MVP 建议只做：

1. `TransformerPipeline`
2. `tooluse`
3. `reasoning`

这是对齐 Claude/Anthropic 场景且最接近 `claude-code-router` 使用价值的最小闭环。
