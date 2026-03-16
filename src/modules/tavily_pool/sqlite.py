"""Tavily Pool 模块的独立 SQLite 连接与初始化。"""

from __future__ import annotations

import os
import sqlite3
from pathlib import Path
from threading import Lock

from sqlalchemy import Engine, create_engine, event, text
from sqlalchemy.orm import Session, sessionmaker

from src.core.logger import logger
from src.modules.tavily_pool.models import TavilyPoolBase

DEFAULT_DB_PATH = "/sqlites/tavilies.db"

_engine: Engine | None = None
_session_factory: sessionmaker[Session] | None = None
_schema_initialized = False
_schema_lock = Lock()
_wal_lock = Lock()
_wal_configured = False


def reset_engine() -> None:
    global _engine
    global _session_factory
    global _schema_initialized
    global _wal_configured

    if _engine is not None:
        _engine.dispose()
    _engine = None
    _session_factory = None
    _schema_initialized = False
    _wal_configured = False


def _build_db_url() -> str:
    db_path = Path(os.getenv("TAVILY_POOL_DB_PATH", DEFAULT_DB_PATH))
    db_path.parent.mkdir(parents=True, exist_ok=True)
    return f"sqlite:///{db_path}"


def get_engine(*, reset: bool = False) -> Engine:
    global _engine
    global _session_factory

    if reset:
        reset_engine()

    if _engine is None:
        _engine = create_engine(
            _build_db_url(),
            connect_args={"check_same_thread": False},
            future=True,
        )

        @event.listens_for(_engine, "connect")
        def _on_connect(dbapi_connection, _connection_record) -> None:  # type: ignore[no-untyped-def]
            cursor = dbapi_connection.cursor()
            cursor.execute("PRAGMA foreign_keys=ON")
            cursor.execute("PRAGMA busy_timeout=5000")
            _try_enable_wal_mode(cursor)
            cursor.close()

    return _engine


def get_session_factory() -> sessionmaker[Session]:
    global _session_factory

    if _session_factory is None:
        engine = get_engine()
        _ensure_schema_initialized(engine)
        _session_factory = sessionmaker(bind=engine, autoflush=False, autocommit=False, future=True)
    return _session_factory


def init_schema(engine: Engine | None = None) -> None:
    global _schema_initialized
    target_engine = engine or get_engine()
    TavilyPoolBase.metadata.create_all(bind=target_engine)
    _ensure_runtime_columns(target_engine)
    _schema_initialized = True


def _ensure_schema_initialized(engine: Engine) -> None:
    global _schema_initialized

    if _schema_initialized:
        return
    with _schema_lock:
        if _schema_initialized:
            return
        TavilyPoolBase.metadata.create_all(bind=engine)
        _ensure_runtime_columns(engine)
        _schema_initialized = True


def _table_columns(engine: Engine, table_name: str) -> set[str]:
    with engine.connect() as conn:
        result = conn.execute(text(f"PRAGMA table_info({table_name})"))
        return {str(row[1]) for row in result.fetchall()}


def _ensure_columns(engine: Engine, table_name: str, columns: dict[str, str]) -> None:
    existing = _table_columns(engine, table_name)
    missing = {name: ddl for name, ddl in columns.items() if name not in existing}
    if not missing:
        return

    with engine.begin() as conn:
        for name, ddl in missing.items():
            conn.execute(text(f"ALTER TABLE {table_name} ADD COLUMN {name} {ddl}"))


def _ensure_runtime_columns(engine: Engine) -> None:
    _ensure_columns(
        engine,
        "tavily_account_runtime_states",
        {
            "status": "VARCHAR(32) NOT NULL DEFAULT 'active'",
            "daily_limit": "INTEGER NOT NULL DEFAULT 0",
            "daily_used": "INTEGER NOT NULL DEFAULT 0",
            "health_status": "VARCHAR(32) NOT NULL DEFAULT 'unknown'",
            "fail_count": "INTEGER NOT NULL DEFAULT 0",
            "usage_plan": "VARCHAR(128)",
            "usage_account_used": "INTEGER",
            "usage_account_limit": "INTEGER",
            "usage_account_remaining": "INTEGER",
            "usage_synced_at": "DATETIME",
            "usage_sync_error": "TEXT",
            "last_used_at": "DATETIME",
            "health_checked_at": "DATETIME",
        },
    )
    _ensure_columns(
        engine,
        "tavily_tokens",
        {
            "consecutive_fail_count": "INTEGER NOT NULL DEFAULT 0",
            "last_checked_at": "DATETIME",
            "last_success_at": "DATETIME",
            "last_response_ms": "INTEGER",
            "last_error": "TEXT",
        },
    )


def _try_enable_wal_mode(cursor) -> None:  # type: ignore[no-untyped-def]
    global _wal_configured

    if _wal_configured:
        return

    with _wal_lock:
        if _wal_configured:
            return
        try:
            cursor.execute("PRAGMA journal_mode=WAL")
            _wal_configured = True
        except sqlite3.OperationalError as exc:
            message = str(exc).lower()
            if "database is locked" not in message:
                raise
            # 多 worker 并发初始化时，WAL 切换可能瞬时锁冲突；跳过本次设置避免请求失败。
            logger.warning("tavily_pool sqlite WAL pragma skipped due to lock contention")
            _wal_configured = True
