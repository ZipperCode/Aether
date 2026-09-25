# Codex WebSocket 非 ASCII 请求头修复

## 问题与证据

`UpstreamBindingIdentity::from_decision` 在 turn-scoped 排除前对每个头执行 ASCII-only `HeaderValue::to_str()`，合法 UTF-8 元数据因此本地报 `codex_websocket_headers_invalid`。`rewrite_header_turn_metadata` 使用普通 JSON 重序列化，会把客户端 ASCII Unicode 转义还原为原始 Unicode。已用脱敏 Rust 程序复现：ASCII 输入→serde_json 重写含 Unicode→HeaderValue::from_str 成功→to_str 失败。

## 验收

- Codex turn metadata 重写为 ASCII-safe JSON，中文、非 BMP 字符与未知嵌套字段 JSON 语义不变；保留原有身份收敛与禁用/Compact 排除语义。
- WS 连接身份接受合法非 ASCII 头值，按字节准确比较/指纹；turn-scoped 头不造成误拒或重绑定，凭据与稳定头变化仍被识别，ASCII 输入既有指纹保持兼容。
- 非法头名、CR/LF 等控制字符仍拒绝，不能通过 lossy 转码、删头、关闭 WS 或吞掉错误修复。
- 使用纯合成身份/凭据、本机上游验证 WS 握手与续接；不调用真实 OpenAI，不复制用户输入中的个人数据或凭据。
- 用户暂停发布 TAG；本次不自动 commit/push/tag，不修改 CI 配置。

## 实施与归属

1. transport owner 修改 `crates/aether-provider/transport/src/codex_fingerprint.rs`：优先复用现有 ASCII-safe JSON helper；没有则私有实现 Unicode UTF-16 escape（包含 surrogate pair），ASCII 快路径避免无谓复制。只处理 JSON 头编码，不改 body 的 JSON 语义。
2. binding owner 修改 `apps/aether-gateway/src/handlers/proxy/websocket/responses/binding.rs`：先按认证优先/turn-scoped 规则筛选，再以合法原始头字节构造身份与指纹，保留摘要的长度编码及 credential-generation 语义。
3. 同两 owner 各维护自己的行为回归；binding owner 可独占既有 WS 集成测试文件验证真实本机握手与续接。不得与 transport owner 编辑相同文件。
4. 主会话统一运行格式、相关 cargo tests（Windows `RUST_MIN_STACK=16777216`）、真实 WS smoke；验证后更新既有协议文档/spec。agent 不执行 build/lint/tests/formatter。
