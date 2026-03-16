from __future__ import annotations

from src.modules.tavily_pool.models import TavilyToken


def _setup_sqlite_env(tmp_path, monkeypatch):
    db_path = tmp_path / "tavilies.db"
    monkeypatch.setenv("TAVILY_POOL_DB_PATH", str(db_path))
    monkeypatch.setenv("TAVILY_POOL_CRYPTO_KEY", "tavily-pool-pool-service-test-key")

    from src.modules.tavily_pool.sqlite import get_engine, init_schema

    engine = get_engine(reset=True)
    init_schema(engine)


def test_pool_lease_round_robin_and_auto_disable_on_failures(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.services.pool_service import TavilyPoolService
    from src.modules.tavily_pool.services.token_service import TavilyTokenService
    from src.modules.tavily_pool.sqlite import get_session_factory, reset_engine

    try:
        with get_session_factory()() as db:
            account_service = TavilyAccountService(db)
            token_service = TavilyTokenService(db)
            pool_service = TavilyPoolService(db)

            account_a = account_service.create_account(email="pool-a@example.com", password="pwd")
            account_b = account_service.create_account(email="pool-b@example.com", password="pwd")
            token_a = token_service.create_token(account_a.id, "tvly-pool-token-a-0001")
            token_b = token_service.create_token(account_b.id, "tvly-pool-token-b-0001")

            lease1 = pool_service.lease_token()
            lease2 = pool_service.lease_token()
            lease3 = pool_service.lease_token()

            assert lease1.token_id == token_a.id
            assert lease2.token_id == token_b.id
            assert lease3.token_id == token_a.id

            pool_service.report_result(token_id=token_a.id, success=False, endpoint="/search")
            pool_service.report_result(token_id=token_a.id, success=False, endpoint="/search")
            pool_service.report_result(token_id=token_a.id, success=False, endpoint="/search")

            stored_token_a = db.get(TavilyToken, token_a.id)
            assert stored_token_a is not None
            assert stored_token_a.is_active is False

            lease_after_disable = pool_service.lease_token()
            assert lease_after_disable.token_id == token_b.id
    finally:
        reset_engine()


def test_pool_stats_overview_aggregates_usage_logs(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.services.pool_service import TavilyPoolService
    from src.modules.tavily_pool.services.token_service import TavilyTokenService
    from src.modules.tavily_pool.sqlite import get_session_factory, reset_engine

    try:
        with get_session_factory()() as db:
            account_service = TavilyAccountService(db)
            token_service = TavilyTokenService(db)
            pool_service = TavilyPoolService(db)

            account = account_service.create_account(email="stats@example.com", password="pwd")
            token = token_service.create_token(account.id, "tvly-stats-token-0001")

            pool_service.report_result(token_id=token.id, success=True, endpoint="/search", latency_ms=100)
            pool_service.report_result(token_id=token.id, success=False, endpoint="/extract", latency_ms=200)

            stats = pool_service.stats_overview()
            assert stats["total_requests"] == 2
            assert stats["success_requests"] == 1
            assert stats["failed_requests"] == 1
            assert stats["success_rate"] == 0.5
            assert stats["avg_latency_ms"] == 150
    finally:
        reset_engine()
