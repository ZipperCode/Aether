# 研究摘要

## 本地现状

- 前端入口：`frontend/src/features/providers/components/provider-tabs/ModelsTab.vue` 的模型行调用 `useModelTest`，现有对话框允许编辑任意请求体，语义仅为联通与候选诊断。
- 前端 API：`frontend/src/api/endpoints/providers.ts` 的 `/test-model` 与 `/test-model-failover` 使用 10 分钟 timeout；新能力合同应独立，不能扩展旧 attempts 结构。
- 后端入口：`apps/aether-gateway/src/handlers/admin/provider/query/models/model_test.rs` 已拥有候选构建、endpoint/Key 过滤、模型映射、格式 adapter 和 execution runtime 同步执行能力。
- 现有 failover 顺序尝试候选直到成功；能力测试必须固定单一候选，避免不同题落到不同 Key。
- 提供商模型已有 `config: Record<String, Value>` / `Record<string, unknown>`，可以保存 reference IDs，无需迁移。
- `aether-task-runtime` 是进程级 supervisor，不是带结果存储和查询 API 的通用作业系统；首版同步实现更小。

## 外部研究

- EvalScope: https://github.com/modelscope/evalscope — 支持 OpenAI-compatible/Anthropic 与 IQuiz，但完整 runner 过重且服务模式同步阻塞。
- MMLU-Pro: https://github.com/TIGER-AI-Lab/MMLU-Pro
- GPQA: https://github.com/idavidrein/gpqa
- IFEval: https://github.com/google-research/google-research/tree/master/instruction_following_eval
- LiveBench: https://github.com/LiveBench/LiveBench
- LLM API Audit: https://github.com/sunblaze-ucb/llm-api-audit 与 https://arxiv.org/abs/2504.04715 — 纯软件黑盒方法不能提供模型身份保证。
- RUT: https://arxiv.org/abs/2506.06975 — 需要可信正品模型对照，当前 GitHub 实现不可直接复用。
- TRAP: https://github.com/parameterlab/trap — 需要目标模型特定白盒探针，不适合首版通用中转检测。

## 已确认产品决策

- 双层检测：能力快筛 + 可选官方直连对照。
- 运行时随机客观题，不使用固定公开题。
- 40 题快筛 + 100 题复核。
- 中文/英文/双语可选，默认双语。
- 首次选择并保存官方参考；不能按名字自动猜。
- 结果只报告能力偏离，不报告身份认证或确认掺水。
