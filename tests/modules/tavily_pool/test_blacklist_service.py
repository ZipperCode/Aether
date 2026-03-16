from __future__ import annotations

from datetime import datetime, timedelta, timezone


def _setup_sqlite_env(tmp_path, monkeypatch):
    db_path = tmp_path / "tavilies.db"
    monkeypatch.setenv("TAVILY_POOL_DB_PATH", str(db_path))
    monkeypatch.setenv("TAVILY_POOL_CRYPTO_KEY", "tavily-pool-blacklist-test-key")

    from src.modules.tavily_pool.sqlite import get_engine, init_schema

    engine = get_engine(reset=True)
    init_schema(engine)


def test_quota_exhausted_failure_marks_account_blacklisted(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.models import TavilyAccountRuntimeState, TavilyBlacklistState
    from src.modules.tavily_pool.services.pool_service import TavilyPoolService
    from src.modules.tavily_pool.services.token_service import TavilyTokenService
    from src.modules.tavily_pool.sqlite import get_session_factory, reset_engine

    try:
        with get_session_factory()() as db:
            account_service = TavilyAccountService(db)
            token_service = TavilyTokenService(db)
            pool_service = TavilyPoolService(db)

            account = account_service.create_account(email="quota@example.com", password="pwd")
            token = token_service.create_token(account.id, "tvly-quota-token-0001")

            pool_service.report_result(token_id=token.id, success=False, error_message="HTTP 429: quota exceeded")

            updated = account_service.repo.get(account.id)
            assert updated is not None
            state = db.get(TavilyBlacklistState, account.id)
            assert state is not None
            assert state.status == "active"
            assert state.reason == "quota_exhausted"
            runtime = db.get(TavilyAccountRuntimeState, account.id)
            assert runtime is not None
            assert runtime.status == "disabled"
    finally:
        reset_engine()


def test_continuous_failures_mark_blacklist_when_enabled(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)
    monkeypatch.setenv("TAVILY_POOL_BLACKLIST_ON_CONTINUOUS_FAIL", "true")
    monkeypatch.setenv("TAVILY_POOL_BLACKLIST_FAIL_THRESHOLD", "2")

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.models import TavilyAccountRuntimeState, TavilyBlacklistState
    from src.modules.tavily_pool.services.pool_service import TavilyPoolService
    from src.modules.tavily_pool.services.token_service import TavilyTokenService
    from src.modules.tavily_pool.sqlite import get_session_factory, reset_engine

    try:
        with get_session_factory()() as db:
            account_service = TavilyAccountService(db)
            token_service = TavilyTokenService(db)
            pool_service = TavilyPoolService(db)

            account = account_service.create_account(email="fail@example.com", password="pwd")
            token = token_service.create_token(account.id, "tvly-fail-token-0001")

            pool_service.report_result(token_id=token.id, success=False, error_message="temporary upstream error")
            pool_service.report_result(token_id=token.id, success=False, error_message="temporary upstream error")

            updated = account_service.repo.get(account.id)
            assert updated is not None
            state = db.get(TavilyBlacklistState, account.id)
            assert state is not None
            assert state.status == "active"
            assert state.reason == "continuous_failures"
    finally:
        reset_engine()


def test_blacklist_scan_deletes_account_after_retention_if_still_unavailable(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)
    monkeypatch.setenv("TAVILY_POOL_BLACKLIST_RETENTION_DAYS", "31")

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.services.blacklist_service import TavilyBlacklistService
    from src.modules.tavily_pool.models import TavilyAccountRuntimeState, TavilyBlacklistState
    from src.modules.tavily_pool.services.token_service import TavilyTokenService
    from src.modules.tavily_pool.sqlite import get_session_factory, reset_engine

    try:
        with get_session_factory()() as db:
            account_service = TavilyAccountService(db)
            token_service = TavilyTokenService(db)
            blacklist_service = TavilyBlacklistService(db)

            account = account_service.create_account(email="cleanup@example.com", password="pwd")
            token_service.create_token(account.id, "tvly-cleanup-token-0001")

            entity = account_service.repo.get(account.id)
            assert entity is not None
            db.add(
                TavilyBlacklistState(
                    account_id=entity.id,
                    status="active",
                    reason="quota_exhausted",
                    blacklisted_at=datetime.now(timezone.utc) - timedelta(days=32),
                )
            )
            db.commit()

            monkeypatch.setattr(TavilyBlacklistService, "_probe_api_key", lambda self, _api_key: (False, "quota", 12))

            result = blacklist_service.scan_and_cleanup()
            assert result["deleted"] == 1
            assert account_service.repo.get(account.id) is None
    finally:
        reset_engine()


def test_blacklist_scan_releases_account_when_probe_success(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.services.blacklist_service import TavilyBlacklistService
    from src.modules.tavily_pool.models import TavilyAccountRuntimeState, TavilyBlacklistState
    from src.modules.tavily_pool.services.token_service import TavilyTokenService
    from src.modules.tavily_pool.sqlite import get_session_factory, reset_engine

    try:
        with get_session_factory()() as db:
            account_service = TavilyAccountService(db)
            token_service = TavilyTokenService(db)
            blacklist_service = TavilyBlacklistService(db)

            account = account_service.create_account(email="release@example.com", password="pwd")
            token_service.create_token(account.id, "tvly-release-token-0001")

            entity = account_service.repo.get(account.id)
            assert entity is not None
            db.add(
                TavilyBlacklistState(
                    account_id=entity.id,
                    status="active",
                    reason="quota_exhausted",
                    blacklisted_at=datetime.now(timezone.utc) - timedelta(days=2),
                )
            )
            db.commit()

            monkeypatch.setattr(TavilyBlacklistService, "_probe_api_key", lambda self, _api_key: (True, "", 9))

            result = blacklist_service.scan_and_cleanup()
            assert result["released"] == 1

            refreshed = account_service.repo.get(account.id)
            assert refreshed is not None
            state = db.get(TavilyBlacklistState, account.id)
            assert state is not None
            assert state.status == "released"
            runtime = db.get(TavilyAccountRuntimeState, account.id)
            assert runtime is not None
            assert runtime.status == "active"
    finally:
        reset_engine()
