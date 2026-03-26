# Alembic 版本链整理与升级策略

更新时间：2026-03-26

## 结论

当前仓库的 Alembic 迁移链本身是可用的：

- 当前唯一 head：`e8f1a2b3c4d5`
- 当前基线 revision：`20251210_baseline`
- 当前存在多个历史 merge 节点，但已经被合并为单一 head

我已在 2026-03-26 实际验证：

```bash
DATABASE_URL='postgresql://aiwriter:aiwriter_dev@127.0.0.1:55432/aether_migration_probe' uv run alembic upgrade head
DATABASE_URL='postgresql://aiwriter:aiwriter_dev@127.0.0.1:55432/aether_migration_probe' uv run alembic current
```

结果：

- 空数据库可以从 `0` 正常升级到 `e8f1a2b3c4d5`
- 因此“当前迁移链在全新空库上必然冲突”这个判断，不符合当前仓库状态

## 当前版本链

关键节点：

- `20251210_baseline`
  - 当前可追溯链路的统一基线
- `c2d5e0f7a9b1`
  - 2026-03-06 合并一次历史多头
- `f6a7b8c9d0e1`
  - 2026-03-12 合并 site management / provider cleanup 分支
- `0a052bc5a42c`
  - 2026-03-13 合并 3 个 head
- `e8f1a2b3c4d5`
  - 2026-03-25 最终单 head

检查命令：

```bash
uv run alembic heads
uv run alembic history --verbose
```

## 为什么你会感觉“从 0 开始也冲突”

最常见不是 Alembic 链损坏，而是下面几类情况：

### 1. 不是“真空库”，而是复用了旧数据卷

`docker-compose.build.yml` 里 PostgreSQL 使用的是命名卷：

```yaml
volumes:
  postgres_data:
```

这意味着：

- `docker compose down` 不会删除数据库数据
- 看起来像“重启部署”，实际上还是旧库
- 如果旧库里 `alembic_version`、表结构、历史脏数据还在，就会继续冲突

真正从 0 开始要用：

```bash
docker compose -f docker-compose.build.yml down -v
docker volume ls | grep aether
docker compose -f docker-compose.build.yml up -d postgres redis
```

### 2. 数据库里记录的是“仓库已不存在的旧 revision”

典型报错：

```text
Can't locate revision identified by '...'
```

这表示：

- 数据库的 `alembic_version` 还停留在旧 revision
- 但仓库里迁移文件已经被 baseline/整理过
- 此时不能直接假设数据库结构已经等于当前 head

### 3. 旧库被错误地 `stamp` 到最新 head

过去 `deploy.sh` 在遇到 `Can't locate revision` 时，会：

1. 删除 `alembic_version`
2. 直接 `stamp` 到当前最新 head

这很危险，因为：

- `stamp` 只改版本号，不执行真实迁移
- 如果旧库结构落后，后续代码会把“未升级的库”当成“已升级完成”
- 随后会在运行期、下次迁移、或新增迁移时爆出各种重复列/缺字段/约束冲突

## 推荐处理策略

### 场景 A：全新部署

适用条件：

- 不需要保留任何旧数据
- 可以删除 Docker volume / 整个数据库

步骤：

```bash
docker compose -f docker-compose.build.yml down -v
docker compose -f docker-compose.build.yml up -d postgres redis
DATABASE_URL='postgresql://postgres:<password>@localhost:5432/aether' uv run alembic upgrade head
```

然后再启动应用。

### 场景 B：旧版本库，revision 仍可识别

适用条件：

- `uv run alembic current` 能看到 revision
- `uv run alembic heads` 只有一个 head

步骤：

```bash
uv run alembic current
uv run alembic upgrade head
```

这种情况属于正常增量升级。

### 场景 C：旧版本库，revision 已丢失

适用条件：

- 报 `Can't locate revision`
- 库里还有真实业务数据

安全做法：

1. 先备份数据库
2. 导出当前 schema
3. 用当前仓库创建一个全新数据库并执行 `alembic upgrade head`
4. 比对旧库和新库 schema
5. 再决定是：
   - 迁移数据到新库
   - 还是确认 schema 已对齐后手工 `stamp`

不建议直接：

```bash
DELETE FROM alembic_version;
alembic stamp e8f1a2b3c4d5
```

除非你已经确认：

- 旧库结构和当前 head 完全一致
- 只是版本号丢了

### 场景 D：开发环境“反复切分支”后迁移冲突

步骤：

```bash
uv run alembic heads
uv run alembic current
```

如果是本地开发库：

- 最省事的方式通常是直接删除开发库重建
- 不要拿已经被多个 feature branch 反复升级/降级过的库，继续当“标准基线”

## 仓库内需要特别注意的点

### 1. `alembic/versions/README.md` 已过时

这个文件仍写着：

- baseline revision: `aether_baseline`

但当前真实 baseline 是：

- `20251210_baseline`

因此排障时应以实际迁移文件和 `alembic history` 输出为准，不要以这个旧 README 为准。

### 2. mergepoint 上不要使用 `downgrade -1`

当前链路有多个 mergepoint，`README.md` 里的提示是正确的：

```bash
alembic downgrade <target_revision>
```

不要在 mergepoint 上使用：

```bash
alembic downgrade -1
```

否则可能出现 `Ambiguous walk`。

## 建议的运维规范

### 一键体检当前数据库状态

可以直接运行：

```bash
uv run python scripts/check_alembic_state.py
```

它会输出：

- `empty_database`
- `unmanaged_schema`
- `upgradeable`
- `orphan_revision`
- `head_schema_drift`
- `current_head`

其中最需要人工介入的是：

- `orphan_revision`
- `head_schema_drift`
- `unmanaged_schema`

### 部署前检查

```bash
uv run alembic heads
uv run alembic history --verbose
```

### 生产升级前

```bash
uv run alembic current
```

### 碰到 `Can't locate revision`

先做：

```bash
备份数据库
```

再决定是否：

- 新库重建 + 数据迁移
- 或经过 schema 对比后手动 stamp

不要把“清掉 alembic_version 并强行 stamp latest”当成默认操作。
