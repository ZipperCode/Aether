from __future__ import annotations

from src.modules.tavily_pool.models import TavilyAccountRuntimeState, TavilyToken


def _setup_sqlite_env(tmp_path, monkeypatch):
    db_path = tmp_path / "tavilies.db"
    monkeypatch.setenv("TAVILY_POOL_DB_PATH", str(db_path))
    monkeypatch.setenv("TAVILY_POOL_CRYPTO_KEY", "tavily-pool-health-usage-test-key")

    from src.modules.tavily_pool.sqlite import get_engine, init_schema

    engine = get_engine(reset=True)
    init_schema(engine)


def test_health_check_disables_token_after_three_failures(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.services.health_service import TavilyHealthService
    from src.modules.tavily_pool.services.token_service import TavilyTokenService
    from src.modules.tavily_pool.sqlite import get_session_factory, reset_engine

    try:
        with get_session_factory()() as db:
            account = TavilyAccountService(db).create_account(email="fail@example.com", password="pwd")
            token = TavilyTokenService(db).create_token(account.id, "tvly-fail-token-0001")

            def _always_fail(self, _token):  # noqa: ANN001
                return False, "unauthorized", 20

            monkeypatch.setattr(TavilyHealthService, "_probe_token", _always_fail)

            service = TavilyHealthService(db)
            service.run_health_check()
            service.run_health_check()
            service.run_health_check()

            stored_token = db.get(TavilyToken, token.id)
            stored_runtime = db.get(TavilyAccountRuntimeState, account.id)
            assert stored_token is not None
            assert stored_runtime is not None
            assert stored_token.is_active is False
            assert stored_token.consecutive_fail_count >= 3
            assert stored_runtime.health_status == "fail"
            assert stored_runtime.fail_count >= 3
    finally:
        reset_engine()


def test_usage_sync_updates_account_quota_fields(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.services.token_service import TavilyTokenService
    from src.modules.tavily_pool.services.usage_service import TavilyUsageService
    from src.modules.tavily_pool.sqlite import get_session_factory, reset_engine

    try:
        with get_session_factory()() as db:
            account = TavilyAccountService(db).create_account(email="usage@example.com", password="pwd")
            TavilyTokenService(db).create_token(account.id, "tvly-usage-token-0001")

            def _fake_fetch(self, _token):  # noqa: ANN001
                return {
                    "account": {
                        "current_plan": "free",
                        "plan_usage": 15,
                        "plan_limit": 1000,
                    }
                }

            monkeypatch.setattr(TavilyUsageService, "_fetch_usage", _fake_fetch)

            result = TavilyUsageService(db).run_usage_sync()
            stored_runtime = db.get(TavilyAccountRuntimeState, account.id)

            assert result["synced_accounts"] == 1
            assert stored_runtime is not None
            assert stored_runtime.usage_plan == "free"
            assert stored_runtime.usage_account_used == 15
            assert stored_runtime.usage_account_limit == 1000
            assert stored_runtime.usage_account_remaining == 985
            assert stored_runtime.usage_sync_error is None
    finally:
        reset_engine()
