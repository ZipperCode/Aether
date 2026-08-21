# 实施计划

1. 抽取共享模型能力/API 格式族模块，并让公开模型过滤复用它；保持当前缺省与无效声明语义。
2. 在 `infer_admin_model_endpoint_binding` 中加入 Global Model 元数据推断分支，位置在现有显式/发现/映射证据之后、单 Endpoint 兜底之前。
3. 将公开模型目录过滤拆分为“权限与状态过滤”和可选的“模型族过滤”，避免复制过滤条件。
4. 标准 OpenAI `/v1/models` 使用跨模型族过滤；Provider 限制从跨模型族静态关联计算。
5. 标准 OpenAI `/v1/models/{id}` 使用同一可见性规则；Claude、Gemini、Codex 保持原路径。
6. 复核所有新增或调整注释，确保解释自动推断优先级和目录发布边界。
7. 运行目标文件 `rustfmt --check`、精确 `git diff --check`、CodeGraph 影响检查和局部静态审查；不运行大模块编译或新增单元测试。

## 回滚点

- 共享模块抽取必须先保持旧调用结果一致，再接入两个新行为；若抽取扩大行为差异，退回只导出最小纯函数。
- Endpoint 推断只新增第四优先级分支，不重排或删除现有分支。
- 标准 OpenAI 目录的跨族逻辑必须由明确分支控制，不能改变 Claude、Gemini、Codex 路径。
