from __future__ import annotations

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
