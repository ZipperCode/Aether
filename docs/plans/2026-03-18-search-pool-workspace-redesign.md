# Search Pool Workspace Redesign Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将搜索池网关后台重构为“总览页 + 单服务工作台”两层结构，并补齐批量导入、Token 编辑、聚合工作台接口等能力，使其管理方式对齐参考项目。

**Architecture:** 后端新增 `services/summary` 与 `services/{service}/workspace` 聚合接口，扩展 key/token CRUD 与导入能力；前端用新的工作台页面替换旧的 `dashboard/keys/tokens` 三页结构。页面视觉沿用 Aether 的布局、卡片和表格体系，但信息架构对齐参考项目的工作台切换方式。

**Tech Stack:** FastAPI, SQLAlchemy, SQLite, Vue 3, TypeScript, Vitest, pytest.

---

### Task 1: 为搜索池工作台补齐后端契约测试

**Files:**
- Modify: `tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py`
- Create: `tests/modules/search_pool_gateway/test_admin_routes_workspace.py`

**Step 1: Write the failing test**

为以下行为新增测试：
- `POST /api/admin/search-pool/keys/import` 支持批量导入 key
- `PUT /api/admin/search-pool/tokens/{id}` 支持编辑 token 配额
- `GET /api/admin/search-pool/services/summary` 返回服务卡片汇总
- `GET /api/admin/search-pool/services/{service}/workspace` 返回单服务工作台聚合数据

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py tests/modules/search_pool_gateway/test_admin_routes_workspace.py -v`
Expected: FAIL because new endpoints and response fields are not implemented.

**Step 3: Write minimal implementation**

扩展 admin routes、schemas、services，使新增接口先满足测试契约。

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py tests/modules/search_pool_gateway/test_admin_routes_workspace.py -v`
Expected: PASS.

### Task 2: 扩展搜索池后端服务与返回模型

**Files:**
- Modify: `src/modules/search_pool_gateway/schemas.py`
- Modify: `src/modules/search_pool_gateway/routes_admin.py`
- Modify: `src/modules/search_pool_gateway/services/key_service.py`
- Modify: `src/modules/search_pool_gateway/services/token_service.py`
- Modify: `src/modules/search_pool_gateway/services/usage_service.py`
- Modify: `src/modules/search_pool_gateway/repositories/key_repo.py`
- Modify: `src/modules/search_pool_gateway/repositories/token_repo.py`

**Step 1: Write the failing test**

围绕以下行为补充细化断言：
- key 行返回 usage 相关字段
- token 行返回更新时间与基础统计字段
- workspace 包含 usage 示例与路由摘要

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/search_pool_gateway -k "workspace or keys_tokens" -v`
Expected: FAIL on missing fields.

**Step 3: Write minimal implementation**

按测试返回最小聚合结构，先用现有 usage log 聚合出总量；无法精确统计的字段先稳定返回 0。

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/search_pool_gateway -k "workspace or keys_tokens" -v`
Expected: PASS.

### Task 3: 重写前端搜索池 API 契约与页面测试

**Files:**
- Modify: `frontend/src/features/search-pool/api.ts`
- Modify: `frontend/src/features/search-pool/types.ts`
- Modify: `frontend/src/views/admin/__tests__/search-pool-pages.spec.ts`
- Create: `frontend/src/features/search-pool/__tests__/workspace.spec.ts`

**Step 1: Write the failing test**

新增测试覆盖：
- 总览页 setup 能拉取 `services/summary`
- 工作台页 setup 能拉取 `workspace`
- 页面使用新的 API 方法，不再依赖旧 `dashboard/keys/tokens` 三页模式

**Step 2: Run test to verify it fails**

Run: `cd frontend && npm run test:run -- search-pool-pages workspace`
Expected: FAIL because new API and page files do not exist yet.

**Step 3: Write minimal implementation**

先补 types/api methods，再让测试通过。

**Step 4: Run test to verify it passes**

Run: `cd frontend && npm run test:run -- search-pool-pages workspace`
Expected: PASS.

### Task 4: 实现总览页与单服务工作台

**Files:**
- Modify: `frontend/src/views/admin/SearchPoolDashboard.vue`
- Create: `frontend/src/views/admin/SearchPoolServiceWorkspace.vue`
- Create: `frontend/src/features/search-pool/components/SearchPoolServiceCard.vue`
- Create: `frontend/src/features/search-pool/components/SearchPoolStatsGrid.vue`
- Create: `frontend/src/features/search-pool/components/SearchPoolTokenDialog.vue`
- Create: `frontend/src/features/search-pool/components/SearchPoolTokenTable.vue`
- Create: `frontend/src/features/search-pool/components/SearchPoolKeyImportDialog.vue`
- Create: `frontend/src/features/search-pool/components/SearchPoolKeyTable.vue`
- Create: `frontend/src/features/search-pool/components/SearchPoolRouteExamples.vue`
- Modify: `frontend/src/router/index.ts`

**Step 1: Write the failing test**

让新页面测试断言：
- 总览页能渲染服务卡片配置
- 工作台页能渲染 key/token/workspace 区块 setup
- 路由新增 `/admin/search-pool/:service`

**Step 2: Run test to verify it fails**

Run: `cd frontend && npm run test:run -- search-pool-pages workspace`
Expected: FAIL.

**Step 3: Write minimal implementation**

按 Aether 风格落组件，保持 UI 信息架构与参考项目一致，但避免复制其视觉语言。

**Step 4: Run test to verify it passes**

Run: `cd frontend && npm run test:run -- search-pool-pages workspace`
Expected: PASS.

### Task 5: 端到端验证与清理旧页面路径

**Files:**
- Modify: `frontend/src/router/index.ts`
- Delete or stop referencing: `frontend/src/views/admin/SearchPoolKeys.vue`, `frontend/src/views/admin/SearchPoolTokens.vue`
- Modify: any affected imports/tests

**Step 1: Write the failing test**

增加路由和引用检查，确保旧页面不再作为入口。

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/search_pool_gateway -v && cd frontend && npm run test:run -- search-pool`
Expected: FAIL until old references are removed.

**Step 3: Write minimal implementation**

清理旧路由，保留新的工作台结构。

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/search_pool_gateway -v && cd frontend && npm run test:run -- search-pool`
Expected: PASS.
