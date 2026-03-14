# Key 级模型同步按钮设计

## 背景与问题
当前 Provider 的密钥模型权限刷新流程依赖切换 `自动获取上游可用模型` 开关，步骤繁琐。第三方上游模型频繁变化，需要一个“单次强制同步”入口，直接以最新上游模型覆盖 Key 的模型权限。

## 目标
- 在密钥管理面板，为每个 Key 提供“同步模型”按钮。
- 点击后强制从上游拉取模型，并覆盖该 Key 的 `allowed_models`。
- 不修改 `auto_fetch_models` 开关，也不应用包含/排除规则。
- 同步后刷新该 Key 显示数据（`last_models_fetch_at`/`last_models_fetch_error` 等）。

## 方案概述（推荐）
新增后端接口：`POST /api/admin/provider-keys/{key_id}/sync-models`，由后端完成拉取、覆盖、状态更新与缓存失效。前端在 Key 行新增按钮，触发调用并展示结果。

## 交互设计
- 位置：`Provider 详情`抽屉 -> `密钥管理`面板 -> 每个 Key 行操作区（与“模型权限”“设置代理节点”并列）。
- 行为：点击按钮弹出确认框：
  - 文案：`将从上游获取模型并覆盖当前模型权限，是否继续？`
- 状态：
  - 请求中按钮 loading + disabled。
  - 成功 toast：`已同步 X 个模型`。
  - 失败 toast：显示后端错误。

## 后端设计
- 新增路由：`POST /api/admin/provider-keys/{key_id}/sync-models`
- 主要逻辑：
  1. 校验 key 存在、provider 存在且有可用 endpoint 或自定义 fetcher。
  2. 强制实时从上游获取模型（跳过缓存）。
  3. 将上游模型 ID 列表写入 `ProviderAPIKey.allowed_models`（全量覆盖，不应用过滤规则）。
  4. 更新 `last_models_fetch_at` 与 `last_models_fetch_error`。
  5. 触发 `on_key_allowed_models_changed` 做缓存失效与模型关联检查。
- 返回：`{ success: bool, models_count: int, error?: string }`。
- 错误处理：
  - 拉取失败：不修改 `allowed_models`，返回错误信息。
  - 上游返回空列表：允许覆盖为空（若需保护机制再调整）。

## 前端设计
- API：在 `frontend/src/api/admin.ts` 增加 `syncProviderKeyModels(keyId: string)`。
- UI：在 `ProviderDetailDrawer` 的 Key 行操作区新增按钮。
- 点击后调用接口，成功后刷新 key 列表与当前 key 数据。

## 数据流
1. 点击 Key 行“同步模型”
2. 前端调用 `POST /api/admin/provider-keys/{key_id}/sync-models`
3. 后端拉取上游模型并覆盖 `allowed_models`
4. 后端返回结果
5. 前端刷新 key 列表/状态展示

## 测试
- 后端：接口单测（成功、失败、空列表）。
- 前端：按钮调用与 loading 状态基本测试（如现有测试结构允许）。

## 风险与限制
- 上游返回空列表会导致 Key 允许模型为空（等价于“禁止模型”），目前按需求允许。
- OAuth 类型上游拉取可能失败，需要明确错误提示。

