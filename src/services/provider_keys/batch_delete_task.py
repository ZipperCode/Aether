"""Pool Key 批量删除异步任务。

接口立即返回 task_id，后台执行删除，前端轮询进度。
任务状态存内存字典，重启丢失无影响。
"""

from __future__ import annotations

import asyncio
import time
import uuid
from dataclasses import dataclass, field

from sqlalchemy import delete as sa_delete

from src.core.logger import logger

# 任务状态
STATUS_PENDING = "pending"
STATUS_RUNNING = "running"
STATUS_COMPLETED = "completed"
STATUS_FAILED = "failed"

# 任务完成后保留时长（秒）
_TASK_RETAIN_SECONDS = 600

# 删除 ProviderAPIKey 时的分批大小
_DELETE_BATCH_SIZE = 500

# 同时存在的最大任务数（含已完成但未过期的）
_MAX_TASKS = 100


@dataclass
class BatchDeleteTask:
    task_id: str
    provider_id: str
    status: str = STATUS_PENDING
    total: int = 0
    deleted: int = 0
    message: str = ""
    created_at: float = field(default_factory=time.monotonic)
    finished_at: float | None = None


# 模块级任务注册表
_tasks: dict[str, BatchDeleteTask] = {}
# 持有后台 asyncio.Task 引用，防止 GC 回收
_running_tasks: set[asyncio.Task[None]] = set()


def _evict_finished_tasks() -> None:
    """清除已过期的已完成任务，为新任务腾出空间。"""
    now = time.monotonic()
    expired = [
        tid
        for tid, t in _tasks.items()
        if t.finished_at is not None and (now - t.finished_at) > _TASK_RETAIN_SECONDS
    ]
    for tid in expired:
        _tasks.pop(tid, None)


def submit_batch_delete(provider_id: str, key_ids: list[str]) -> str:
    """提交批量删除任务，返回 task_id。

    Raises:
        RuntimeError: 并发任务数超过 _MAX_TASKS 上限。
    """
    _evict_finished_tasks()
    if len(_tasks) >= _MAX_TASKS:
        raise RuntimeError(f"too many batch-delete tasks ({len(_tasks)}), try again later")
    task_id = uuid.uuid4().hex[:16]
    task = BatchDeleteTask(
        task_id=task_id,
        provider_id=provider_id,
        total=len(key_ids),
    )
    _tasks[task_id] = task
    bg = asyncio.create_task(
        _run_batch_delete(task_id, provider_id, key_ids),
        name=f"batch-delete-{task_id}",
    )
    _running_tasks.add(bg)
    bg.add_done_callback(_running_tasks.discard)
    return task_id


def get_batch_delete_task(task_id: str) -> BatchDeleteTask | None:
    return _tasks.get(task_id)


def _sync_delete(
    provider_id: str,
    key_ids: list[str],
    task: BatchDeleteTask,
) -> int:
    """在线程中执行的同步删除逻辑，避免阻塞事件循环。

    每批 cleanup + DELETE + commit 作为独立事务，减少锁持有时间。
    """
    from src.database import create_session
    from src.models.database import ProviderAPIKey
    from src.services.provider_keys.key_side_effects import cleanup_key_references

    db = create_session()
    try:
        affected = 0
        for i in range(0, len(key_ids), _DELETE_BATCH_SIZE):
            batch = key_ids[i : i + _DELETE_BATCH_SIZE]
            try:
                cleanup_key_references(db, batch)
                result = db.execute(
                    sa_delete(ProviderAPIKey).where(
                        ProviderAPIKey.provider_id == provider_id,
                        ProviderAPIKey.id.in_(batch),
                    )
                )
                rowcount = getattr(result, "rowcount", 0) or 0
                affected += int(rowcount)
                db.commit()
            except Exception:
                try:
                    db.rollback()
                except Exception:
                    pass
                raise
            # NOTE: task.deleted is written from the worker thread and read
            # from the event-loop thread. Safe on CPython (GIL protects simple
            # attribute assignment) but not guaranteed by the language spec.
            task.deleted = affected
        return affected
    finally:
        try:
            db.close()
        except Exception:
            pass


async def _run_batch_delete(
    task_id: str,
    provider_id: str,
    key_ids: list[str],
) -> None:
    task = _tasks.get(task_id)
    if task is None:
        return

    task.status = STATUS_RUNNING

    try:
        affected = await asyncio.to_thread(_sync_delete, provider_id, key_ids, task)

        # 副作用（缓存失效等）是异步操作，在事件循环中执行
        if affected > 0:
            try:
                from src.database import get_db_context
                from src.services.provider_keys.key_side_effects import (
                    run_delete_key_side_effects,
                )

                with get_db_context() as db:
                    await run_delete_key_side_effects(
                        db=db,
                        provider_id=provider_id,
                        deleted_key_allowed_models=None,
                    )
            except Exception as exc:
                logger.error("batch delete side effects failed: {}", exc)

        task.status = STATUS_COMPLETED
        task.deleted = affected
        task.message = f"{affected} keys deleted"
        logger.info(
            "[BATCH_DELETE_TASK] completed task={} provider={} total={} deleted={}",
            task_id,
            provider_id[:8],
            len(key_ids),
            affected,
        )

    except asyncio.CancelledError:
        task.status = STATUS_FAILED
        task.message = "task cancelled (shutdown)"
        logger.warning(
            "[BATCH_DELETE_TASK] cancelled task={} provider={} deleted={}",
            task_id,
            provider_id[:8],
            task.deleted,
        )
    except Exception as exc:
        task.status = STATUS_FAILED
        task.message = str(exc)
        logger.error(
            "[BATCH_DELETE_TASK] failed task={} provider={}: {}",
            task_id,
            provider_id[:8],
            exc,
        )
    finally:
        task.finished_at = time.monotonic()

    # 延迟清除已完成的任务
    try:
        await asyncio.sleep(_TASK_RETAIN_SECONDS)
    except asyncio.CancelledError:
        pass
    _tasks.pop(task_id, None)
