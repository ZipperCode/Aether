# Research: Provider / Antigravity 上游同步

- Query: 分类并适配 `fe8ff268d`、`c5ae9c2c7`、`c8d1ae3e7`、`ba11a7221`
- Scope: internal
- Date: 2026-09-05

## Findings

### 提交分类与顺序

- `fe8ff268d`：功能依赖，必须纳入。增加 Antigravity Google userinfo 邮箱身份、Codex 本地消费后额度递减、失败详情刷新保留旧额度，以及 Pool 页快照/metadata 合并。
- `c5ae9c2c7`：功能依赖，必须纳入。成功额度刷新后把 `quota_by_model` 的可路由模型写入目录。
- `c8d1ae3e7`：测试依赖。`fe8` 给共享 fixture 增加 `reset_credits` 后，凭据代际拒绝测试必须比较完整原 metadata；无运行时代码。随 `fe8` 纳入或紧随其后应用。
- `ba11a7221`：不需要。仅调整路由、affinity、finalize 等 CI fixture，无编号 3 功能；纳入会碰本地余额调度契约。
- 最小顺序：`fe8`（含本地适配）→ `c5`（含 Endpoint 适配）→ `c8`；排除 `ba11`。四个 SHA 当前均被 `git cherry -v HEAD upstream/main` 标为 `+`。

### 文件与代码模式

- `crates/aether-oauth/src/provider/providers/antigravity.rs:6`：当前适配器仅包装 Generic；`exchange_code` 在 `:55` 直接返回 token，需加入同一网络上下文的 Bearer userinfo 请求，并把 email 同时写入 `auth_config` 与 `raw_payload`。
- `apps/aether-gateway/src/handlers/admin/provider/oauth/state/exchange.rs:43`：切换 Antigravity 专用适配器时必须保留 `:47` 的 Nous 特例；userinfo 测试 URL沿用现有 OAuth override 状态，不新增配置。
- `apps/aether-gateway/src/handlers/admin/provider/oauth/provisioning.rs:213`：新 Key 名优先 email，`:268` 返回 email；因此 `raw_payload` 写回是完整身份链必需。
- `apps/aether-gateway/src/handlers/admin/provider/oauth/quota/shared.rs:575`：只在首次终态 `reset` 时饱和减一、移除首个 credit、标记 `local_consume/pending_refresh`；不得改变现有 generation、reservation、fence、CAS 顺序。共享 fixture 在 `:1774`，旧精确断言在 `:1892`，解释了 `c8` 依赖。
- `crates/aether-admin/src/provider/quota.rs:1779`：仅在现有非陈旧、同凭据/重置代际门禁内特殊合并 `reset_credits`；失败详情可更新诊断，但空 `credits` 不覆盖已知列表/计数。
- `frontend/src/features/providers/components/codex-reset-credit-display.ts:12` 已有时间戳优先合并；`PoolManagement.vue:2421` 目前只读快照，应复用该函数，不另写合并器。
- `apps/aether-gateway/src/handlers/admin/provider/oauth/quota/antigravity.rs:58` 已持有真实 `endpoint`。本地解析器把发现模型持久化到 `antigravity.models`，同步器必须优先读取该字段，并只为上游旧结构兼容回退 `quota_by_model`。目录同步应在 quota 状态成功持久化后执行，失败仅告警、不改 quota 成功结果。
- `apps/aether-gateway/src/handlers/admin/provider/shared/payloads.rs:347` 的本地 DTO 比上游多必填 struct 字段 `models`；不能照搬只含 `model_ids` 的 literal。应构造 `AdminImportProviderModelSource`，把每个发现模型绑定到当前 `endpoint.id`。`request/models.rs:730` 会校验 Endpoint 归属并走精确 `discovered` 绑定；这保留本地 Endpoint 证据契约。
- `crates/aether-model-fetch/src/strategy.rs:1138` 的解析与新目录同步必须共用导出的大小写无关 routable 谓词，继续排除 `chat_23310/chat_20706`。
- 所有新增/修改手写函数、方法、常量/字段和测试须补实质中文说明；尤其 userinfo URL/字段/override/enrich、reset merge/decrement、discovered sync、routable predicate。

### 最小验证

```text
cargo fmt --all --check
cargo check -p aether-oauth -p aether-admin -p aether-model-fetch -p aether-gateway
cargo test -p aether-oauth --lib antigravity_exchange_fetches_google_email_for_account_identity
cargo test -p aether-admin --lib codex_quota_failed_reset_credit_detail_preserves_last_known_count_and_items
cargo test -p aether-gateway --lib gateway_names_new_antigravity_oauth_account_from_google_userinfo_email
cargo test -p aether-gateway --lib codex_reset_reservation_rejects_replaced_credential_generation
cargo test -p aether-gateway --lib gateway_refreshes_admin_provider_quota_locally_for_antigravity_with_trusted_admin_principal
cd frontend; npx vitest run src/features/providers/components/__tests__/codex-reset-credit-display.spec.ts; npm run type-check
```

## Related Specs / External References

- `.trellis/spec/aether-gateway/backend/model-association-endpoint-contract.md:16`：发现模型须携带 Endpoint 证据。
- `.trellis/spec/aether-provider-pool/backend/balance-scheduling-contract.md`：排除 `ba11`，不改调度。
- 外部资料：无；证据来自当前源码、测试及上述 SHA diff。

## Caveats / Not Found

- userinfo enrichment 只覆盖授权码交换；refresh-token 导入仍依赖现有导入邮箱保留链路。
- 最终检查发现并修复字段适配与测试证据缺口：生产同步器优先读取本地 `models`；测试的独立 Global Model 仓储显式写入 Endpoint→Provider 归属后，再断言精确 Endpoint binding。
