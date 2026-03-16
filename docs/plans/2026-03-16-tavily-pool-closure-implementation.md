# Tavily Pool Closure Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 Tavily 账号池从基础 CRUD 升级为可生产使用的可用性闭环（真实健康检查、失效治理、配额同步、管理面板入口）。

**Architecture:** 在现有 `src/modules/tavily_pool` 内补齐服务层能力，不引入新模块。健康检查改为真实 Tavily API 探测，配额同步独立服务并可手动/定时触发，路由层补充账号与 token 管理接口，前端在现有页面增加同步入口与关键状态展示。

**Tech Stack:** FastAPI, SQLAlchemy, SQLite, httpx, Vue3 + TypeScript, pytest, vitest。

---

### Task 1: 补齐后端回归测试（先红）
- 修改 `tests/modules/tavily_pool/test_routes.py`
- 新增/修改 `tests/modules/tavily_pool/test_scheduler.py`

步骤：
1. 先写失败用例：覆盖 `usage/sync`、账号禁用、token 删除、健康检查状态更新。
2. 运行 `pytest tests/modules/tavily_pool/test_routes.py -q`，确认失败。

### Task 2: 实现后端 P0 能力
- 修改 `src/modules/tavily_pool/models.py`
- 修改 `src/modules/tavily_pool/schemas.py`
- 修改 `src/modules/tavily_pool/services/health_service.py`
- 新增 `src/modules/tavily_pool/services/usage_service.py`
- 修改 `src/modules/tavily_pool/services/token_service.py`
- 修改 `src/modules/tavily_pool/services/account_service.py`
- 修改 `src/modules/tavily_pool/routes.py`
- 修改 `src/modules/tavily_pool/services/scheduler.py`

步骤：
1. 实现最小代码让失败测试通过。
2. 扩充 scheduler 加 `usage` 定时同步。
3. 运行 `pytest tests/modules/tavily_pool -q`。

### Task 3: 前端补必要入口（同步+状态）
- 修改 `frontend/src/features/tavily-pool/types.ts`
- 修改 `frontend/src/features/tavily-pool/api.ts`
- 修改 `frontend/src/views/admin/TavilyPoolList.vue`
- 修改 `frontend/src/views/admin/TavilyPoolDetail.vue`

步骤：
1. 加“同步额度”按钮与状态字段展示。
2. 运行 `cd frontend && npm run lint && npm run type-check`。

### Task 4: 全量验证
1. `pytest tests/modules/tavily_pool -q`
2. `cd frontend && npm run lint && npm run type-check && npm run test:run`
3. 记录失败项与剩余风险。
