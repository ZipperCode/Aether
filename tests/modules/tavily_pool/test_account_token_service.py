from __future__ import annotations

from src.modules.tavily_pool.models import TavilyToken


def _setup_sqlite_env(tmp_path, monkeypatch):
    db_path = tmp_path / "tavilies.db"
    monkeypatch.setenv("TAVILY_POOL_DB_PATH", str(db_path))
    monkeypatch.setenv("TAVILY_POOL_CRYPTO_KEY", "tavily-pool-test-crypto-key")

    from src.modules.tavily_pool.sqlite import get_engine, init_schema

    engine = get_engine(reset=True)
    init_schema(engine)
    return engine


def test_create_account_and_token_persisted_with_masked_value(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.services.token_service import TavilyTokenService
    from src.modules.tavily_pool.sqlite import get_session_factory

    from src.modules.tavily_pool.sqlite import reset_engine

    try:
        session_factory = get_session_factory()
        with session_factory() as session:
            account_service = TavilyAccountService(session)
            token_service = TavilyTokenService(session)

            account = account_service.create_account(
                email="alice@example.com",
                password="pass-123",
                source="script",
            )
            token = token_service.create_token(account_id=account.id, token="tvly-secret-token-0001")

            session.flush()
            stored = session.get(TavilyToken, token.id)

            assert stored is not None
            assert stored.token_encrypted != "tvly-secret-token-001"
            assert token.token_masked.startswith("tvly")
            assert token.token_masked.endswith("0001")
    finally:
        reset_engine()


def test_activate_token_deactivates_other_active_tokens(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.services.token_service import TavilyTokenService
    from src.modules.tavily_pool.sqlite import get_session_factory

    from src.modules.tavily_pool.sqlite import reset_engine

    try:
        session_factory = get_session_factory()
        with session_factory() as session:
            account_service = TavilyAccountService(session)
            token_service = TavilyTokenService(session)
            account = account_service.create_account(email="bob@example.com", password="pwd")

            token1 = token_service.create_token(account.id, "tvly-token-1111")
            token2 = token_service.create_token(account.id, "tvly-token-2222")
            token_service.activate_token(token2.id)

            tokens = token_service.list_tokens(account.id)
            active_tokens = [token for token in tokens if token.is_active]
            inactive_ids = {token.id for token in tokens if not token.is_active}

            assert len(active_tokens) == 1
            assert active_tokens[0].id == token2.id
            assert token1.id in inactive_ids
    finally:
        reset_engine()


def test_create_account_with_api_key_creates_active_token(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.services.token_service import TavilyTokenService
    from src.modules.tavily_pool.sqlite import get_session_factory

    from src.modules.tavily_pool.sqlite import reset_engine

    try:
        session_factory = get_session_factory()
        with session_factory() as session:
            account_service = TavilyAccountService(session)
            token_service = TavilyTokenService(session)

            account = account_service.create_account(
                email="with-key@example.com",
                password="pass-123",
                api_key="tvly-create-with-key-0001",
                source="manual",
            )

            tokens = token_service.list_tokens(account.id)
            assert len(tokens) == 1
            assert tokens[0].is_active is True
            assert tokens[0].token_masked.startswith("tvly")
            assert tokens[0].token_masked.endswith("0001")
    finally:
        reset_engine()
