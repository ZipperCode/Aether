# 同步本地 tunnel 路由回执

## 改动

- `apps/aether-gateway/src/execution_runtime/transport.rs`
  - 新增 `execute_sync_plan_via_local_tunnel_with_response_started`，复用现有本地
    tunnel 同步正文、超时、鉴权和响应观察实现；无本地 tunnel 时返回 `None`。
  - 本地 tunnel 内部在提供回调时交付 `DirectSyncResponseStarted`，避免重复写 OAuth
    成功效果；无回调的既有 admin/普通路径保持原行为。
- `apps/aether-gateway/src/execution_runtime/sync/execution.rs`
  - `execute_direct_sync_runtime_candidate` 先尝试本地 tunnel 路径，命中后沿用候选
    lifecycle/diagnostic 回调并返回同一 `ExecutionResult`；未命中继续既有 Windsurf/直连路径。

## 验证

- 主机 `cargo check -p aether-gateway --lib`：未完成，`boring-sys2` 的 CMake 选择
  `Visual Studio 18 2026` 不存在（环境失败，未作为代码结论）。
- Docker builder `aether-gateway-builder:trellis-check`，容器
  `aether-antigravity-sync-route-verify`：`cargo test --locked -p aether-gateway --lib
  antigravity_signature -- --nocapture` 退出码 0，9 passed / 0 failed / 5427 filtered，
  覆盖 direct/tunnel sync、stream 与 admin。
- Docker builder `aether-gateway-builder:trellis-check`，容器
  `aether-antigravity-sync-route-check`：`cargo check --locked -p aether-gateway --lib`
  退出码 0（dev profile，9m03s）。

## 边界

未修改公共 API、schema、配置、依赖或非 tunnel 路径；未执行远程部署/推理。
