# Alembic 多头合并设计（本地库）

## 背景与目标
- 问题：`alembic upgrade head` 报错 “Multiple head revisions”
- 目标：通过新增 merge 迁移，合并为单一 head，保证 `alembic upgrade head` 可用
- 范围：仅本地库，迁移文件提交进仓库

## 现状与根因
- 当前存在 3 个 head：
  - `f0c3a7b9d1e2`
  - `a9b1c2d3e4f5`
  - `364680d1bc99`
- Alembic 无法自动选择 head，导致 `upgrade head` 失败

## 方案对比
1. 新增 merge 迁移（推荐）
   - 优点：标准做法、一次修复、后续稳定
   - 缺点：新增迁移文件
2. 仅改启动命令为 `upgrade heads`
   - 优点：最快
   - 缺点：多头状态仍在、后续易复发
3. 手动指定某分支 head
   - 优点：无需改文件
   - 缺点：容易遗漏其他分支迁移

## 设计方案（采用）
- 新增一个 merge migration，`down_revision` 同时指向 3 个 head
- 该迁移只合并历史链，不引入新的 schema 变更
- 结果：`alembic heads` 仅剩 1 个 head，`alembic upgrade head` 通过

## 变更范围
- 新增：`alembic/versions/2026xxxx_xxxx_merge_heads.py`

## 验证方案
- `alembic heads` 仅显示 1 个 head
- `alembic upgrade head` 正常执行

## 风险与回滚
- 风险：merge 迁移若依赖写错，会继续出现多头
- 回滚：删除 merge 迁移并回到多头状态（仅本地）
