# 同步上游路由、Pool 与协议修复

## 目标

将源码上游 `fawney19/Aether` 的路由策略、Provider Pool、网关兼容修复，以及 Gemini / Responses 协议兼容能力同步到当前 fork，同时保留当前 fork 已有功能和行为。

## 背景

- 当前基线为本地与 `origin/master` 共同指向的 `50c96d060442fb1b612a27c587b91dec4f79a613`。
- 已刷新源码上游 `upstream/main` 到 `cae9aa4134b6bfd4b21dab0c535186232002ed34`。
- 当前分支与上游已分叉：本地独有 119 个提交，上游独有 62 个提交，不能直接快进。
- 用户明确选择此前更新清单中的第 1、3、4、5 项，并排除第 2、6 项。

## 范围内需求

### R1 路由策略

- 路由配置成为调度策略的唯一来源，同时为旧调度配置建立等价的系统默认路由组。
- 支持按 API 格式设置 Key 优先级，并由有效路由策略控制转换时是否保留优先级。
- 使用路由策略字段 `sticky_key_attempts` 控制同 Key 尝试次数；默认值为 2。
- 只允许首选候选在同一 Key 上重试，故障转移候选各尝试一次，避免回退链路被重复重试拖住。

### R2 Provider Pool 与 Key 调度

- 支持并实际执行 Key 级并发限制，覆盖相关 HTTP、流式与 WebSocket 执行入口。
- 支持缓存亲和调度，以及首次分配时的单号优先或 LRU 轮号模式。
- 不同模型的额度状态相互隔离。
- Antigravity 额度进度、重置时间和汇总在管理界面及后端载荷中一致。

### R3 网关与提供商兼容修复

- Codex 会话身份和客户端指纹在同一逻辑请求的重试、重新规划及 WebSocket 路径中保持稳定。
- Provider Pool 饱和状态被正确识别和记录，并按既有候选故障转移语义处理。
- Gemini 流式响应中的畸形函数调用在向客户端提交成功状态前被识别，以便尝试下一候选。
- Responses 流转换忽略无业务语义的 `ping` 事件。
- 自定义中继仅在模型或合法 DeepSeek 主机/类型证据成立时应用 DeepSeek reasoning/tool-call 兼容逻辑。

### R4 Gemini 与 Responses 协议兼容

- 支持 Gemini 无 ID 工具调用历史配对、thought signature 保留、工具 Schema 清理和混合工具调用。
- 支持 Responses 附加工具转换到 Chat，并合理降级 reasoning summary。
- Antigravity 工具 Schema 字段和私有搜索工具名与实际上游协议一致。
- Responses compaction 只路由给能处理 Responses 格式的候选。

### R5 集成约束

- 优先复用上游提交；只在本地分叉冲突或依赖裁剪时做最小手工调整，不建立第二套实现。
- 为使 R1-R4 可编译、可运行而必须引入的公共契约或配置持久化片段属于范围内，但不得借此带入无关产品功能。
- 保留本地模型能力检测、Responses 流错误处理、额度调度及其他 fork 独有变更。
- 本次新增或修改的手写函数、方法、具名回调、类型、模型、接口、业务字段和配置字段必须补充或保留实质性中文说明；说明业务语义和必要边界，不修改第三方、生成代码或构建产物。
- 本地完成提交、合并回 `master` 并清理临时 worktree；不推送任何远端。

## 范围外

- Aether VSCodex 远程 Codex 协作模块及其 gateway、前端、sidecar、扩展和部署接线。
- 用户套餐权益撤销流程。
- 通用 Provider Usage API 模板。
- Nightly 发布流程及其安装脚本、工作流调整。
- 与 R1-R4 无依赖关系的管理、发布、文档或格式化改动。

## 验收标准

- [x] AC1：R1 的路由策略、格式级 Key 优先级和首选 Key 重试语义均已同步，并有覆盖关键行为的现有或最小新增检查。
- [x] AC2：R2 的 Key 并发限制、缓存亲和、模型额度隔离及 Antigravity 展示数据链路均已同步。
- [x] AC3：R3 的 Codex 稳定身份、Pool 饱和、Gemini 预提交错误、Responses ping 和 DeepSeek 自定义中继修复均已同步。
- [x] AC4：R4 列出的 Gemini / Responses / Antigravity 协议兼容行为均已同步。
- [x] AC5：VSCodex、套餐撤销、通用 Usage API 和 Nightly 发布功能未进入最终产品差异；必要依赖例外可逐项追溯到 R1-R4。
- [x] AC6：Rust 格式检查、受影响 crate 的编译检查和针对性逻辑测试通过；前端受影响逻辑通过类型检查和必要的针对性测试。
- [x] AC7：最终 `master` 包含已验证的本地提交，`origin/master` 未被推送，临时 worktree 和临时分支已清理。
- [x] AC8：本次新增或修改的手写函数、类型和业务/配置字段均具有符合项目要求的中文说明，且没有为第三方或生成文件制造无关改动。
