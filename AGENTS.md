# Repository Guidelines

## 项目结构与模块组织
- 后端主代码位于 `src/`，按 `api/`、`services/`、`models/`、`core/` 等分层组织。
- 后端测试位于 `tests/`，测试文件遵循 `test_*.py`。
- 前端位于 `frontend/`（Vue 3 + Vite + TypeScript），页面在 `src/views/`，可复用逻辑在 `src/composables/`。
- 代理节点位于 `aether-proxy/`（Rust），与主服务独立构建。
- 数据库迁移在 `alembic/`，文档和架构图在 `docs/`。

## 构建、测试与开发命令
- 后端依赖安装：`uv sync`
- 启动本地依赖（DB/Redis）：`docker compose -f docker-compose.build.yml up -d postgres redis`
- 启动后端开发服务：`./dev.sh`
- 后端测试：`pytest`（含 `--cov=src`）
- 前端开发：`cd frontend && npm install && npm run dev`
- 前端构建：`cd frontend && npm run build`
- 前端测试：`cd frontend && npm run test:run`
- Rust 代理测试：`cd aether-proxy && cargo test`

## 代码风格与命名规范
- Python：4 空格缩进，`black`（100 列）+ `isort`，建议提交前运行 `black . && isort .`。
- 类型检查：后端使用 `mypy`，前端使用 `npm run type-check`。
- TypeScript/Vue：使用 `eslint`，执行 `cd frontend && npm run lint`。
- 命名约定：Python 模块/函数使用 `snake_case`，类使用 `PascalCase`；Vue 组件文件使用 `PascalCase.vue`。

## 测试指南
- 后端框架：`pytest` + `pytest-asyncio` + `pytest-cov`。
- 覆盖率输出：终端缺失行与 `htmlcov/` 报告；新增功能应补充单元测试，修复缺陷应附带回归测试。
- 前端框架：`vitest`，建议组件与 composable 按功能就近添加测试。

## 提交与 Pull Request 规范
- 参考历史提交，采用 Conventional Commits：`feat`、`fix`、`perf`、`docs`，可加 scope（如 `feat(pool): ...`）。
- 提交信息应描述“改了什么 + 为什么改”。
- PR 必须包含：变更摘要、影响范围、测试结果（命令与结论）；涉及 UI 变更请附截图。
- 若涉及迁移或配置变更，请在 PR 描述中注明升级/回滚步骤与风险。

## 迁移与配置变更约定
- 数据库结构改动必须通过 Alembic 迁移提交，避免直接手改线上库结构。
- 建议在本地先执行迁移链验证：`alembic upgrade head`，必要时再执行 `alembic downgrade -1` 做回滚检查。
- 新增环境变量时同步更新 `.env.example` 与相关文档，默认值和用途要写清楚。

## 安全与配置提示
- 基于 `.env.example` 创建本地 `.env`，禁止提交真实密钥。
- 首次部署前运行 `python generate_keys.py` 生成密钥并写入环境变量。
