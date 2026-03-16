from __future__ import annotations

import sqlite3

from sqlalchemy import text
from sqlalchemy import inspect


def test_tavily_sqlite_path_and_bootstrap(tmp_path, monkeypatch):
    db_path = tmp_path / "tavilies.db"
    monkeypatch.setenv("TAVILY_POOL_DB_PATH", str(db_path))

    from src.modules.tavily_pool.sqlite import get_engine, init_schema, reset_engine

    try:
        engine = get_engine(reset=True)
        init_schema(engine)

        assert db_path.exists()
        assert str(engine.url).endswith(str(db_path))

        tables = set(inspect(engine).get_table_names())
        assert {
            "tavily_accounts",
            "tavily_tokens",
            "tavily_health_checks",
            "tavily_maintenance_runs",
            "tavily_maintenance_items",
        }.issubset(tables)
    finally:
        reset_engine()


def test_wal_mode_lock_contention_is_tolerated(monkeypatch):
    from src.modules.tavily_pool import sqlite as sqlite_mod

    sqlite_mod._wal_configured = False

    class _FakeCursor:
        def execute(self, sql: str) -> None:
            if sql == "PRAGMA journal_mode=WAL":
                raise sqlite3.OperationalError("database is locked")

    cursor = _FakeCursor()
    sqlite_mod._try_enable_wal_mode(cursor)
    assert sqlite_mod._wal_configured is True


def test_init_schema_creates_blacklist_state_table_for_legacy_accounts_table(tmp_path, monkeypatch):
    db_path = tmp_path / "legacy-tavilies.db"
    monkeypatch.setenv("TAVILY_POOL_DB_PATH", str(db_path))

    from src.modules.tavily_pool.sqlite import get_engine, init_schema, reset_engine

    try:
        engine = get_engine(reset=True)
        with engine.begin() as conn:
            conn.execute(
                text(
                    """
                    CREATE TABLE tavily_accounts (
                        id VARCHAR(36) PRIMARY KEY,
                        email VARCHAR(255) NOT NULL,
                        password_encrypted TEXT NOT NULL,
                        status VARCHAR(32) NOT NULL DEFAULT 'active',
                        daily_limit INTEGER NOT NULL DEFAULT 0,
                        daily_used INTEGER NOT NULL DEFAULT 0,
                        health_status VARCHAR(32) NOT NULL DEFAULT 'unknown',
                        fail_count INTEGER NOT NULL DEFAULT 0,
                        created_at DATETIME NOT NULL,
                        updated_at DATETIME NOT NULL
                    )
                    """
                )
            )

        init_schema(engine)

        with engine.connect() as conn:
            columns = {
                str(row[1]) for row in conn.execute(text("PRAGMA table_info(tavily_accounts)")).fetchall()
            }
            blacklist_tables = conn.execute(
                text("SELECT name FROM sqlite_master WHERE type='table' AND name='tavily_blacklist_states'")
            ).fetchall()

        assert "blacklist_status" not in columns
        assert "blacklisted_at" not in columns
        assert "blacklist_reason" not in columns
        assert "blacklist_fail_count" not in columns
        assert "last_blacklist_check_at" not in columns
        assert len(blacklist_tables) == 1
    finally:
        reset_engine()
