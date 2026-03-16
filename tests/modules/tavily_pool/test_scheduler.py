from __future__ import annotations

from src.modules.tavily_pool.models import TavilyHealthCheck, TavilyMaintenanceRun


def test_scheduler_jobs_record_health_and_maintenance_runs(tmp_path, monkeypatch):
    db_path = tmp_path / "tavilies.db"
    monkeypatch.setenv("TAVILY_POOL_DB_PATH", str(db_path))
    monkeypatch.setenv("TAVILY_POOL_CRYPTO_KEY", "tavily-scheduler-key")

    from src.modules.tavily_pool.sqlite import get_engine, get_session_factory, init_schema, reset_engine

    try:
        init_schema(get_engine(reset=True))
        session_factory = get_session_factory()

        from src.modules.tavily_pool.services.account_service import TavilyAccountService

        with session_factory() as db:
            account = TavilyAccountService(db).create_account(
                email="worker@example.com",
                password="pass-001",
            )
            from src.modules.tavily_pool.services.token_service import TavilyTokenService

            TavilyTokenService(db).create_token(account.id, "tvly-health-0001")

        from src.modules.tavily_pool.services.scheduler import TavilyPoolScheduler

        scheduler = TavilyPoolScheduler()
        scheduler.run_health_check_once()
        scheduler.run_maintenance_once()

        with session_factory() as db:
            assert db.query(TavilyHealthCheck).count() >= 1
            assert db.query(TavilyMaintenanceRun).count() >= 1
    finally:
        reset_engine()
