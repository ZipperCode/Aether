# 技术设计

## Data Flow

`ModelsTab 行动作 -> 能力检测对话框 -> typed provider-query API -> 固定 target/reference 候选 -> 服务端生成题集 -> 现有 execution runtime -> 客观解析与评分 -> typed result -> 对话框结果页`

首版保持单个有界 HTTP 请求。现有 `aether-task-runtime` 只有进程级 supervisor，没有可复用的按请求结果注册表；因此不为本功能新增后台任务、持久状态或轮询协议。

## API Contract

新增 `POST /api/admin/provider-query/test-model-capability`，权限为 `admin:provider_query:write`。

请求：

```json
{
  "provider_id": "target-provider-id",
  "model_id": "target-provider-model-id",
  "endpoint_id": "target-endpoint-id",
  "api_key_id": "target-key-id",
  "mode": "quick",
  "language": "bilingual",
  "use_saved_reference": true,
  "request_id": "provider-capability-..."
}
```

- `mode`: `quick | verify`
- `language`: `zh | en | bilingual`
- 客户端不传模型名、题目、答案、种子或任意请求体；后端从已存模型与候选解析真实模型和格式。
- 无效 ID、非文本格式、相同 target/reference 或失效参考返回 4xx；已开始但覆盖不足的运行返回 200 + `inconclusive`。

响应：

```text
run_id, suite_version, seed, mode, language, verdict, inconclusive_reason
target, reference?
target_metrics, reference_metrics?
comparison?
items[]
elapsed_ms
```

`target/reference` 包含 provider/model/endpoint/Key 的内部 ID、requested model 和 effective model。`metrics` 包含 planned/scored/correct、coverage、score、Wilson 上下界、五维评分、失败分类、耗时及可用的 input/output tokens 和 cost。逐题结果只返回题目 ID、维度、语言、期望选项、解析后的选项、状态、正确性、延迟和可用 usage；不返回完整上游请求/响应或推理。

`verdict`：

- `profile_only`
- `no_large_deviation`
- `needs_verification`
- `no_significant_deviation`
- `significant_deviation`
- `inconclusive`

## Reference Mapping

复用提供商模型现有 JSON `config`，保存：

```json
{
  "capability_test_reference": {
    "provider_id": "...",
    "model_id": "...",
    "endpoint_id": "...",
    "api_key_id": "..."
  }
}
```

前端通过现有模型 PATCH 合并配置，不覆盖其他 `config` 键。后端每次运行重新验证四个引用均存在、启用、相互隶属并支持文本生成；不存储密钥明文，不做自动替换。

## Suite Generation and Scoring

- `suite_version = capability-v1`。
- 五个维度：`quantitative`、`logical`、`algorithmic`、`language`、`instruction`。
- quick 为每维 8 题；verify 为每维 20 题。双语在每个维度内中英文各半。
- 服务端用 UUID v4 生成 seed，再用仓库已有 UUID v5 哈希派生每个题目参数和选项顺序，不增加随机数依赖。
- 题目由构造保证唯一答案，统一为 A-D 四选一。提示只要求最终选项；解析器仅接受明确的单一最终答案或最后一行 `FINAL/答案: X`，歧义文本为 `unparseable`。
- 正确率以已解析题为分母，覆盖率为已解析题/计划题。总体分为五个维度正确率的等权平均；同时给出二项比例 95% Wilson 区间。
- 同题配对只使用 target/reference 都已解析的题：`b = reference 对且 target 错`，`c = target 对且 reference 错`，执行单侧精确 McNemar（二项尾概率）。
- quick：双方覆盖率至少 90%、分差至少 15pp 且 `p < 0.05` -> `needs_verification`，否则 `no_large_deviation`。
- verify：双方覆盖率至少 95%、分差至少 10pp 且 `p < 0.01` -> `significant_deviation`，否则 `no_significant_deviation`。
- 未配置 reference -> `profile_only`；覆盖不足、配对不足、总超时或条件不一致 -> `inconclusive`。

## Execution Boundary

- 从现有模型测试候选构建与 adapter 中抽取/放宽最小共享 helper，不复制鉴权、模型映射或 transport 逻辑。
- target/reference 各解析一个明确候选；所有题复用该候选，不允许候选 fallback。
- 请求为非流式、无工具、无搜索；各格式使用其支持的确定性参数，temperature 为 0（支持时）且输出上限为 1024。实际请求轮廓随结果记录。
- 全部上游调用共享并发上限 4。runner 不叠加重试；保留现有 pinned execution 的 transport 语义。
- quick 总时限 10 分钟，verify 20 分钟；请求取消或超时时停止未完成 future。
- 支持现有 adapter 能稳定产生文本的 OpenAI Chat/Responses、Claude Messages、Gemini GenerateContent；其他格式返回 unsupported。

## Frontend

- 在模型行现有联通测试按钮旁增加独立动作，复用 endpoint/Key 能力过滤和首选逻辑。
- 新对话框自行持有最小请求状态，不把评分语义塞进 `useModelTest`。
- 配置页：模式、语言、target endpoint/Key、reference toggle 与首次 reference provider/model/endpoint/Key 选择；显示 40/100 题以及启用参考后的 80/200 次最大调用数。
- 运行页：预计时间、取消和不确定进度提示；不新建流式进度协议。
- 结果页：verdict、免责声明、target/reference 总分与五维分、覆盖率、失败分类、耗时、可用 usage/cost；`needs_verification` 显示 100 题复核入口并使用新 seed。

## Compatibility and Rollback

- 新 API、按钮和 config 子键均为增量；现有模型测试与现有模型 config 消费方不变。
- 无数据库迁移。回滚时删除新入口/API/module 即可；遗留的未知 `capability_test_reference` config 键会被现有代码忽略。
