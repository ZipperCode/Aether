"""Tavily Pool 模块的独立 SQLite 连接与初始化。"""

from __future__ import annotations

import os
from pathlib import Path

from sqlalchemy import Engine, create_engine, event
from sqlalchemy.orm import Session, sessionmaker

from src.modules.tavily_pool.models import TavilyPoolBase

DEFAULT_DB_PATH = "/sqlites/tavilies.db"

_engine: Engine | None = None
_session_factory: sessionmaker[Session] | None = None


def reset_engine() -> None:
    global _engine
    global _session_factory

    if _engine is not None:
        _engine.dispose()
    _engine = None
    _session_factory = None


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
            cursor.execute("PRAGMA journal_mode=WAL")
            cursor.execute("PRAGMA busy_timeout=5000")
            cursor.close()

    return _engine


def get_session_factory() -> sessionmaker[Session]:
    global _session_factory

    if _session_factory is None:
        _session_factory = sessionmaker(bind=get_engine(), autoflush=False, autocommit=False, future=True)
    return _session_factory


def init_schema(engine: Engine | None = None) -> None:
    target_engine = engine or get_engine()
    TavilyPoolBase.metadata.create_all(bind=target_engine)
