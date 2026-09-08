# 修复 Chat 流式首段错误后提供商切换并发布

## 目标与授权

用户要求：排查到问题直接修复，完成 review、模拟测试；通过后提交推送 GitHub，精确 SHA 的必需 CI 成功后推送新 tag。完整交付，不把诊断或本地测试当作完成。

## 已确认背景

- 实例 192.168.2.212:8084 运行 v0.7.31，revision 74abb40cfd8f4c9df93ce657c8bbcbf9996acda6。
- 样本 f4c37f2e-14d3-4c19-98fa-f208e9628af9，2026-09-09 00:04 +08，流式 k3/openai:chat：othersapi 0/0、0/1 均 retryable_upstream_status/400，lfree 2/0 为 server_error/400。sticky_key_attempts=2 仅控制首位同 Key 重试，无两提供商总上限。
- frontdoor 在 00:04:14.719 记 HTTP 200，最终 candidate 在 00:04:14.763 结算失败；保存的响应仅有 provider server_error，无正常 choices，内部消息包装 NVIDIA 400。
- 归档代码重写 upstream_response 状态及 content-type；保存的 400/application/json 不是原始线头证明。必须先本地复现，不能把历史推断当事实。

## 要求与验收

- [x] A1：本地可控 HTTP/SSE 提供商复现早停，修改前回归失败证明失败点一致，不调用收费模型。
- [x] A2：Chat 在客户端可见有效事件前识别首段错误，按既有故障转移策略继续；至少三个不同提供商模拟证明第二家错误后第三家真正收到请求并成功，不靠增加重试次数。
- [x] A3：正常首段不丢失/重复，分块错误不误判成功；有效内容输出后错误仍在本流终止，不拼接下一家回答。
- [x] A4：保留同 Key 重试、显式 stop 规则、候选单次结算及 usage 归属；修复共享实际调用链，不留重复路径。
- [x] A5：独立 review/修正、模拟回归及最小相关检查通过，命令和证据可追溯。
- [x] A6：中文提交并正常推送 origin/master；精确 SHA 全部必需 GitHub CI 成功后创建并推送下一未占用应用补丁 tag。
- [x] A7：跟进 tag 的既有 release 工作流和产物，记录 tag/commit/CI/release 关系，清理并收口。

## 边界与决策

不修改线上 provider、路由、日志级别、数据库，不重启/部署，不外发真实报文和凭据，不强推/移动已有 tag，不改独立 tunnel 版本。沿用现有提交门和 failover，不新增配置/依赖/安全加固/通用重构。用户已授权实施、提交、推送、新 tag，无待决产品选择，不新增批准关卡。
