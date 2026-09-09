# 修复 Antigravity 工具参数 Schema 兼容

## Goal

修复公开 Gemini 请求转发到 Antigravity 时，工具参数中的 `$schema` 导致 Google 返回 400 的问题；不要求用户修改官方客户端。

## Background

- 线上 0.7.32 的请求 `930a9417-b194-4763-b631-5fc58253870b` 使用公开 Gemini `/v1beta/models/gemini-3.8-flash:streamGenerateContent` 入站，再转 Antigravity 私有 v1internal。
- 17 次失败、16 个账号均返回 `INVALID_ARGUMENT`；12 个错误指向 `function_declarations[0..11].parameters` 中未知字段 `$schema`。
- 未保存原始入站/出站请求，不能断言客户端最初使用 `parameters` 还是 `parametersJsonSchema`。
- 用户已于本轮明确要求修复。轻量单模块修复，不需要新的产品或架构选择。

## Requirements

1. 只在 Antigravity 私有出站封装边界适配工具参数 Schema；普通 Gemini 同格式内容不变。
2. 支持现有 `parameters`、`parametersJsonSchema`、`parameters_json_schema` 输入以及两种 function declarations 字段拼写；保留现有 `parameters` 优先级，不丢失参数定义。
3. 删除参数 Schema 节点的 `$schema` 元声明，覆盖参数根节点及嵌套对象、数组、组合 Schema；不得把业务属性名 `$schema` 或示例/默认值中的同名数据当成元声明删除。
4. 不删除 `$ref`、`$defs`、约束或未知扩展，不折叠 `oneOf`/`anyOf`，不添加占位参数。保留这些结构不等于新增其上游兼容支持。
5. 普通 Gemini body 与已有 Antigravity envelope 都通过同一出站处理，不能重复包裹 envelope；输入对象不能被原地改变。

## Acceptance Criteria

- [x] 已证实的 12 工具 `$schema` 错误有脱敏合成回归，修复前失败、修复后通过。
- [x] 三种参数字段输入得到保留实际定义的私有 `parameters`，其 Schema 元声明被移除；已有优先级不变。
- [x] 嵌套对象/数组/组合的 `$schema` 被处理，合法同名业务属性和不应改写的数据保持不变。
- [x] `$ref`/`$defs`、联合类型分支及其他字段不会被静默丢弃或简化。
- [x] 普通 Gemini 透传边界、既有 envelope 与输入不可变性有针对性证据。
- [x] 范围化 Rust 测试与格式检查通过，独立 Trellis 检查完成。

## Out of Scope

不部署、不推送、不重启、不改线上配置/数据库，不发起计费模型请求；不改路由/重试/日志策略，不新增依赖，不承诺完整 JSON Schema 方言转换。真实官方客户端线上复验属于部署后的单独步骤。
