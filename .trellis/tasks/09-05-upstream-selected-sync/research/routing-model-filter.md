# Research: routing selected-model provider filter

- Query: 分析上游 `1eb2d10de` 的行为、依赖、当前分支重叠、最小集成与验证方式。
- Scope: mixed
- Date: 2026-09-05

## Findings

### 结论

- 上游功能提交为 `1eb2d10decef56150ec51c7ad9c14d183ae7ef44`（父提交 `18d78dd6c973c4508673760838dc42400a867552`），共 4 个前端文件、`+102/-2`。
- **可独立适配，类别 1 不需要纳入任何父提交。** `ddcbeb3ae99e4145c2a56ad261ab1d73b1c6bfdb` 和当前代码均已具备 `ProviderSummaryQuery.model_id`、Provider summary 后端过滤及路由策略类型；`18d78dd6` 是类别 3/测试修复合并节点，不是类别 1 的功能或编译依赖。
- 当前分支已实现上游补丁的一部分，而且更严格：父组件已把启用 GlobalModel ID 传给编辑器；编辑器已在客户端限定为“关联当前模型 + Provider 启用 + 至少一个启用 Key”。缺口是 `loadProviders()` 仍请求全量 Provider、模型/模式变化不重查服务端、并发请求没有防止旧响应覆盖新选择。
- 不应直接照搬或无冲突 cherry-pick：上游使用 `modelId`，当前本地契约使用 `globalModelId`；上游会新增当前已存在的 ID 解析/传递逻辑，并与本地客户端资格过滤重叠。应手工适配并保留本地更严格过滤和隐藏 Provider 覆盖值的往返语义。

### 上游行为

`1eb2d10de` 的目标行为是：

1. 默认统一 Provider 排序和 `global_key` 排序继续请求全量 Provider。
2. 非 `*` 模型且优先级模式为 `provider` 时，必须以 **GlobalModel ID** 传 `model_id`，不能把模型名称误当 ID。
3. 父组件尚未解析到 GlobalModel ID 时返回空列表，不短暂回退全量 Provider。
4. 模型、GlobalModel ID 或优先级模式变化时重新加载；递增 request ID，忽略过期成功/失败响应。

上游文件集：

- `frontend/src/features/routing/utils/providerQuery.ts`：新增查询构造函数，返回 `ProviderSummaryQuery | null`。
- `frontend/src/features/routing/__tests__/providerQuery.spec.ts`：覆盖模型 Provider 模式、统一/Key 模式及 ID 未就绪三种分支。
- `frontend/src/features/routing/components/RoutingPriorityPolicyEditor.vue`：消费查询、监听选择变化、丢弃旧响应。
- `frontend/src/views/admin/RoutingProfiles.vue`：按模型名解析 GlobalModel ID 并传入编辑器。

### 当前代码证据与语义重叠

- `frontend/src/views/admin/RoutingProfiles.vue:705` 已渲染按模型编辑器，`:global-model-id="activeGlobalModelId"` 位于 `:708`；`activeGlobalModelId` 已在 `:908-914` 从启用全局模型目录解析 ID。因此上游 view hunk 已等价吸收，无需再新增 `globalModelIdFor()`。
- `frontend/src/features/routing/components/RoutingPriorityPolicyEditor.vue:369-379` 已声明并中文说明 `globalModelId`；`:408-414` 区分默认/按模型作用域并规范化 ID。
- 同文件 `:451-468` 的 `providerRows` 已保留本地更严格资格过滤；这层过滤必须继续存在，因为后端 `model_id` 只过滤有效模型关联，不额外要求 Provider 启用或有启用 Key。
- 同文件 `:520-526` 当前模式 watcher 只加载 Key；`:581-593` 的 `loadProviders()` 固定调用 `{ page: 1, page_size: 9999 }`，是类别 1 尚未补齐的根缺口。
- `frontend/src/api/endpoints/providers.ts:29-36` 已定义 `ProviderSummaryQuery.model_id?: string`；`:62-94` 已把参数交给 `/api/admin/providers/summary`。
- `apps/aether-gateway/src/handlers/admin/provider/crud/reads.rs:51-86` 已解析 `model_id`；`apps/aether-gateway/src/handlers/admin/provider/summary/aggregates.rs:92-95,115-123,165-170` 按有效 GlobalModel 关联过滤 Provider。
- 后端已有契约回归：`apps/aether-gateway/src/tests/control/admin/providers.rs:559-661` 请求 `model_id=gpt-5` 并断言只返回对应 Provider。
- 本地已有更强的组件回归：`frontend/src/features/routing/components/__tests__/RoutingPriorityPolicyEditor.spec.ts:140-171` 验证按 GlobalModel ID、缺失 ID、默认统一模式的可见范围；`:173-200` 起验证隐藏 Provider 的覆盖值不被排序操作删除。该测试目前 mock 返回全量数据，尚未断言查询参数或异步竞态。

### 完整依赖与最小集成

- 功能提交依赖集：仅 `1eb2d10decef56150ec51c7ad9c14d183ae7ef44`；无需 `fe8ff268d`、`c5ae9c2c`、`c8d1ae3e`、`ba11a722` 或两个合并提交。
- Ponytail 最小适配可只改 2 个现有文件：
  - `RoutingPriorityPolicyEditor.vue`：沿用 `globalModelId`，按上游三分支构造查询；ID 未就绪时清空；在模型/ID/模式变化时重载；以 request ID 忽略旧响应；保留现有 `providerRows` 过滤。
  - `RoutingPriorityPolicyEditor.spec.ts`：在现有真实组件测试中断言 `getProvidersSummary` 参数、未就绪不请求、统一/Key 模式不带 `model_id`，并继续验证本地资格过滤与隐藏覆盖值。
- 不必修改 `RoutingProfiles.vue`。也不必新增一次性 helper；若主集成者希望尽量保留上游文件/测试映射，可新增原 `providerQuery.ts` 和 `providerQuery.spec.ts`，但仍须把参数名适配为本地 `globalModelId`，且不能删除现有组件回归。
- 不需要 backend、API 类型、Endpoint 绑定或配置改动；与当前 Provider Model Endpoint 精确绑定文件无写集重叠。

### 中文说明影响

- 若保留上游 `buildRoutingProviderSummaryQuery`，其上游中文 JSDoc 已说明 GlobalModel ID、统一排序和 Key 排序语义，可沿用并补清 `null` 的含义。
- 当前 `activeGlobalModelId`/`globalModelId` 已有中文说明，优先复用；不要新增上游未注释的 `globalModelIdFor()`。
- `loadProviders()` 将被实质修改，按项目规则应补中文说明，明确查询范围、ID 未就绪为空及旧响应丢弃语义。匿名 watcher 无需另造具名抽象。

### 精确目标验证

在仓库根目录执行：

```powershell
Push-Location frontend
npm run test:run -- src/features/routing/components/__tests__/RoutingPriorityPolicyEditor.spec.ts
# 仅在保留上游独立 helper/test 时执行下一项
npm run test:run -- src/features/routing/__tests__/providerQuery.spec.ts
npm run type-check
npm exec eslint -- --no-fix src/features/routing/components/RoutingPriorityPolicyEditor.vue src/features/routing/components/__tests__/RoutingPriorityPolicyEditor.spec.ts
Pop-Location
cargo nextest run -p aether-gateway gateway_handles_admin_providers_summary_list_locally_with_trusted_admin_principal
```

不要用 `npm run lint` 做检查；项目脚本包含 `--fix`，会修改文件。

## Files Found

- `frontend/src/features/routing/components/RoutingPriorityPolicyEditor.vue` — 当前编辑器、本地资格过滤和待补服务端查询入口。
- `frontend/src/features/routing/components/__tests__/RoutingPriorityPolicyEditor.spec.ts` — 当前模型作用域与隐藏覆盖值回归，可直接扩展。
- `frontend/src/views/admin/RoutingProfiles.vue` — 已有 GlobalModel ID 解析与 prop 传递。
- `frontend/src/api/endpoints/providers.ts` — 已有 `model_id` 查询类型与 API 传输。
- `apps/aether-gateway/src/handlers/admin/provider/crud/reads.rs` — Provider summary 查询参数入口。
- `apps/aether-gateway/src/handlers/admin/provider/summary/aggregates.rs` — 服务端有效模型关联过滤实现。
- `apps/aether-gateway/src/tests/control/admin/providers.rs` — 后端 `model_id` 过滤回归。

## External References

- 上游提交：<https://github.com/fawney19/Aether/commit/1eb2d10decef56150ec51c7ad9c14d183ae7ef44>
- 基线到目标比较：<https://github.com/fawney19/Aether/compare/ddcbeb3ae99e4145c2a56ad261ab1d73b1c6bfdb...1eb2d10decef56150ec51c7ad9c14d183ae7ef44>

## Related Specs

- `frontend/AGENTS.md` — feature/view/API ownership、严格类型、Vitest 就近与目标测试规则。
- `.trellis/spec/guides/code-reuse-thinking-guide.md` — 已有测试/逻辑优先复用，单调用点不新增抽象。
- `.trellis/spec/guides/cross-layer-thinking-guide.md` — GlobalModel name/ID 与组件/API 边界必须保持语义区分。
- 当前 `.trellis/spec/` 没有独立 frontend package spec；本类别不修改 backend，因此无需引入 backend spec 写集。

## Caveats / Not Found

- 手工适配能满足产品行为，但产生的 patch-id 很可能不同于上游原补丁；因此 **不能预先保证** `git cherry HEAD upstream/main` 会把 `1eb2d10de` 标成等价。主集成者必须在最终历史上实测；若仍显示 `+`，应明确选择保留上游 patch 身份的集成策略，或调整“等价吸收”的验收表达，不能仅凭功能测试宣称该项通过。
- 未执行任何 cherry-pick、merge、构建或测试；本文件只记录只读核验结论。
