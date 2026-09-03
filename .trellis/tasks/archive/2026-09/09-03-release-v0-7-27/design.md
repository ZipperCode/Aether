# 发布与 CI 修复设计

## 边界

- 产品行为以已合并的 Key 强读、统一准入和预提交门禁为准，不为旧测试增加旁路。
- 测试通过现有 `provider_catalog_for_plan` 提供真实 Provider/Endpoint/Key；本地 tunnel 必须在最终 `GatewayDataState` 安装后注册。
- 等待类测试必须使用本地确定性传输，并带有限超时，避免 GitHub runner 无限占用。

## CI 与发布门禁

1. 已用 WSL Rust 1.95.0 和独立 `/tmp` target 验证已发现的失败用例；最终认证以 GitHub `Rust CI` 为准。
2. 每次修复均提交并推送 `master`，只接受精确 SHA 的 `Rust CI` 成功。
3. CI 成功后再次确认 `v0.7.27` 本地与远端均不存在，再创建 annotated tag。
4. 推送 tag 后只接受精确 tag/SHA 的 `Release Aether` 成功，并核验 Release assets。

## 回滚边界

- CI 未通过时不创建 tag。
- Release workflow 失败时保留已发布 tag，不移动或覆盖 tag；先报告并修复后发布新版本，除非用户另行授权删除。
