# Tavily Pool Account Import Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 在 Tavily 账号池管理页新增批量导入能力，支持 JSON/CSV，包含导入示例与说明，支持冲突策略并输出逐行错误。

**Architecture:** 前端在 `TavilyPoolList` 接入导入弹窗组件，弹窗负责文件选择、格式切换、示例说明与提交。后端在 Tavily 模块新增统一导入接口，按文件类型解析并归一化数据，再通过账号服务统一执行冲突策略写入。导入结果返回统计与错误明细。

**Tech Stack:** Vue 3 + TypeScript、FastAPI、Pydantic、SQLAlchemy、pytest、vitest

---

### Task 1: 后端导入接口契约测试

**Files:**
- Modify: `tests/modules/tavily_pool/test_routes.py`
- Test: `tests/modules/tavily_pool/test_routes.py`

**Step 1: Write the failing test**

```python
def test_import_accounts_json_success(client):
    resp = client.post(
        "/api/admin/tavily-pool/accounts/import",
        json={
            "file_type": "json",
            "merge_mode": "skip",
            "content": "[...]"
        },
    )
    assert resp.status_code == 200
    assert resp.json()["stats"]["created"] >= 1
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/tavily_pool/test_routes.py::test_import_accounts_json_success -v`
Expected: FAIL with `404` or missing route error.

**Step 3: Write minimal implementation**

- 在 `src/modules/tavily_pool/routes.py` 新增 `POST /accounts/import` 路由并接线服务。

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/tavily_pool/test_routes.py::test_import_accounts_json_success -v`
Expected: PASS

**Step 5: Commit**

```bash
git add tests/modules/tavily_pool/test_routes.py src/modules/tavily_pool/routes.py
git commit -m "feat(tavily-pool): add account import route"
```

### Task 2: 后端解析与冲突策略（TDD）

**Files:**
- Create: `tests/modules/tavily_pool/test_account_import_service.py`
- Modify: `src/modules/tavily_pool/schemas.py`
- Modify: `src/modules/tavily_pool/services/account_service.py`

**Step 1: Write the failing tests**

```python
def test_import_csv_skip_creates_accounts_and_tokens(): ...
def test_import_overwrite_updates_notes_and_adds_tokens(): ...
def test_import_error_mode_rolls_back_on_conflict(): ...
def test_import_returns_row_level_errors_for_invalid_email(): ...
```

**Step 2: Run tests to verify they fail**

Run: `pytest tests/modules/tavily_pool/test_account_import_service.py -v`
Expected: FAIL with missing schema/service methods.

**Step 3: Write minimal implementation**

- 在 `schemas.py` 添加导入请求/响应模型。
- 在 `account_service.py` 添加：
  - JSON/CSV 解析
  - 统一记录归一化
  - `skip/overwrite/error` 冲突处理
  - 逐行错误收集与统计输出

**Step 4: Run tests to verify they pass**

Run: `pytest tests/modules/tavily_pool/test_account_import_service.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add tests/modules/tavily_pool/test_account_import_service.py src/modules/tavily_pool/schemas.py src/modules/tavily_pool/services/account_service.py
git commit -m "feat(tavily-pool): implement csv/json account import service"
```

### Task 3: 前端 API 与导入弹窗（TDD）

**Files:**
- Modify: `frontend/src/features/tavily-pool/api.ts`
- Create: `frontend/src/features/tavily-pool/components/TavilyAccountImportDialog.vue`
- Test: `frontend/src/features/tavily-pool/__tests__/api.spec.ts`（若无则创建）

**Step 1: Write the failing tests**

```ts
it('calls tavily import api with payload', async () => {
  await tavilyPoolApi.importAccounts(payload)
  expect(mockPost).toHaveBeenCalledWith('/api/admin/tavily-pool/accounts/import', payload)
})
```

**Step 2: Run test to verify it fails**

Run: `cd frontend && npm run test:run -- src/features/tavily-pool/__tests__/api.spec.ts`
Expected: FAIL with missing API method.

**Step 3: Write minimal implementation**

- 在 `api.ts` 增加导入方法与类型。
- 新增导入弹窗组件：格式选择、示例、说明、上传、提交。

**Step 4: Run test to verify it passes**

Run: `cd frontend && npm run test:run -- src/features/tavily-pool/__tests__/api.spec.ts`
Expected: PASS

**Step 5: Commit**

```bash
git add frontend/src/features/tavily-pool/api.ts frontend/src/features/tavily-pool/components/TavilyAccountImportDialog.vue frontend/src/features/tavily-pool/__tests__/api.spec.ts
git commit -m "feat(frontend): add tavily account import dialog and api"
```

### Task 4: 接入 Tavily 列表页与结果反馈

**Files:**
- Modify: `frontend/src/views/admin/TavilyPoolList.vue`
- Test: `frontend/src/views/admin/__tests__/TavilyPoolList.spec.ts`（若无则创建）

**Step 1: Write the failing test**

```ts
it('opens import dialog and submits selected file', async () => { ... })
```

**Step 2: Run test to verify it fails**

Run: `cd frontend && npm run test:run -- src/views/admin/__tests__/TavilyPoolList.spec.ts`
Expected: FAIL with missing import button/dialog behavior.

**Step 3: Write minimal implementation**

- 在页面 actions 区加入 `批量导入` 按钮。
- 接入导入弹窗状态、提交动作、结果 toast。

**Step 4: Run test to verify it passes**

Run: `cd frontend && npm run test:run -- src/views/admin/__tests__/TavilyPoolList.spec.ts`
Expected: PASS

**Step 5: Commit**

```bash
git add frontend/src/views/admin/TavilyPoolList.vue frontend/src/views/admin/__tests__/TavilyPoolList.spec.ts
git commit -m "feat(frontend): wire tavily account import into list page"
```

### Task 5: 全量验证与文档收尾

**Files:**
- Modify: `docs/plans/2026-03-16-tavily-pool-import-design.md`（必要时补充实现偏差）

**Step 1: Run backend tests**

Run: `pytest tests/modules/tavily_pool -q`
Expected: PASS

**Step 2: Run frontend checks**

Run: `cd frontend && npm run test:run`
Expected: PASS

**Step 3: Run lint/type-check (frontend)**

Run: `cd frontend && npm run lint && npm run type-check`
Expected: PASS

**Step 4: Verify key manual flow**

Run (manual): 打开 `/admin/tavily-pool`，验证 JSON/CSV 导入与错误提示。
Expected: 能导入，格式错误时能明确提示。

**Step 5: Commit**

```bash
git add docs/plans/2026-03-16-tavily-pool-import-design.md
git commit -m "docs: finalize tavily pool import design notes"
```
