# Bug Analysis: 首次完整 CI 的合同传播与验证边界

## 1. Root Cause Category

- B/C：新投影未同步保留余额字段，候选错误登记遗漏执行端已使用的类别；历史baseline源错误提前加入增量迁移字段。
- D/E：本地定向用例不覆盖完整CI；CI强制彩色输出打断纯文本断言；旧夹具未同步capture级别、bound credentials、null墓碑、OAuth Profile发现与目录证据。

## 2. Why Fixes Failed

- 先前编译和部分回归通过不等于全部5456项Gateway行为通过；未把这一边界隐藏为“平台差异”。
- Gateway no-fail-fast本轮一次暴露35个失败；Data/Rest快速停止仍留下2222条未运行，下一轮需要完整执行。
- 35个本地定向用例一次运行后33通过；剩余2项共同指出真实错误类别登记遗漏，因此修共享源，而非把断言改为未分类。

## 3. Prevention Mechanisms

| Priority | Mechanism | Specific Action | Status |
| --- | --- | --- | --- |
| P0 | 真实环境回归 | Cargo解析夹具显式plain输出，并在CI四环境变量下运行 | DONE |
| P0 | 单一合同拥有者 | 余额共享projection、候选错误共享registry修复并原位回归 | DONE |
| P1 | 完整失败收集 | Data/Rest保留全目标并no-fail-fast，不忽略失败 | DONE |
| P1 | 精确SHA验证 | 本地最小验证后正常push，Linux完整CI是最终证据 | PENDING |

## 4. Systematic Expansion

- 已核对真实共享调用方和35项失败清单，不新增通用兼容层、测试跳过、权限设置或发布动作。
- 历史可执行migration和当前bootstrap/generated SQL未改，原始严格一致性测试保持。
- 当前项目没有 `src/templates/markdown/spec`，不创建不属于Aether的模板副本。

## 5. Knowledge Capture

- [x] 已在现有Gateway quality spec记录CI颜色确定性与全失败收集。
- [x] 已记录余额/候选诊断共享投影的签名、边界与回归。
- [x] 原始CI完整35失败矩阵和本地批次结果保存在任务research。
- [ ] 最终新SHA的CI终态和新发现继续补证，不能把待验证标成成功。
