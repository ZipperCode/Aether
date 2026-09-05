# 实施计划

- [x] 1. 在 data contracts 定义轻量认证维护候选和仓储读取方法，并在内存、PostgreSQL、MySQL、SQLite 实现不含大字段的投影。
- [x] 2. 在 gateway data/state 包装层暴露强读取方法，确保 provider catalog cache 不缓存该维护扫描。
- [x] 3. 新增进程级共享认证维护 gate，解析并归一化 `AETHER_AUTH_MAINTENANCE_CONCURRENCY`，覆盖 permit 取消释放测试。
- [x] 4. 重写 OAuth token refresh：轻量筛选、取得 permit 后单 Key 强读取、再次校验、按受控并发执行并合并汇总。
- [x] 5. 让 account self-check 使用轻量候选选择，并在单 Key quota/OAuth 操作前取得同一共享 permit。
- [x] 6. 清理普通文本候选的未使用 eager 构造/re-export，增加数千候选 + 500 KiB body 的惰性构造回归和架构约束。
- [x] 7. 运行定向 rustfmt、仓储测试、维护 worker 测试、候选测试和受影响包 `cargo check`；仅按失败证据扩大范围。（`cargo fmt --all --check`、`git diff --check`、gateway auth-maintenance 7/7、四仓储投影测试、候选惰性测试及 `cargo check -p aether-gateway --all-targets` 已通过。）
- [x] 8. 使用当前源码执行真实 `docker compose build`，启动本地 Compose，验证健康、进程内存和高基数场景；停止任务自有容器并清理临时数据。（当前源码第二次 `docker compose -f docker-compose.yml -f docker-compose.local.yml build app` 成功，镜像摘要 `sha256:7cfa4c3b6012efc99d2433fc49684d56d4bf6433b57dd867b16712b93b3ec909`；`up -d --no-build --force-recreate app` 后健康接口 200、`RESTARTS=0`、`OOM=false`。Docker stats 采样约 25.21、48.5、106.1、110.1、90.99 MiB，后续波动而非单调增长；本地持久库保留 73 个 Key，6,000 候选/512 KiB 请求体由 Rust 回归测试覆盖，未向持久库写入测试数据。按用户要求保留本地 Compose 运行供登录验证，未部署远端。）
- [x] 9. 运行 Trellis check，更新相关 spec、提交中文 commit，并报告远端仍未部署。（质量检查回执通过；认证维护/候选内存契约已写入 `.trellis/spec/aether-gateway/backend/`；本地 Docker 复核完成，远端旧版保持未变。）

## Risky Files And Rollback Points

- `crates/aether-data/contracts/src/repository/provider_catalog/**` 与四个仓储实现必须保持方法签名和字段顺序一致。
- `apps/aether-gateway/src/maintenance/runtime/oauth_token_refresh.rs` 必须保留 OAuth 资格、Agent Identity 恢复和凭证变更检测语义。
- `apps/aether-gateway/src/maintenance/runtime/account_self_check.rs` 必须保留 runtime quota block、Pool score 与自动删除语义。
- 删除 eager 候选入口前必须证明普通生产路径均已使用 dynamic attempt source。

## Validation Commands

```text
cargo fmt --all --check
cargo test -p aether-data-contracts <targeted filters>
cargo test -p aether-data-postgres <targeted filters>
cargo test -p aether-data-mysql <targeted filters>
cargo test -p aether-data-sqlite <targeted filters>
cargo test -p aether-gateway --lib <oauth/account/candidate filters>
cargo check -p aether-gateway --all-targets
docker compose build
docker compose up -d
docker compose ps
```
