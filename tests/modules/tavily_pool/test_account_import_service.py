from __future__ import annotations


def _setup_sqlite_env(tmp_path, monkeypatch):
    db_path = tmp_path / "tavilies.db"
    monkeypatch.setenv("TAVILY_POOL_DB_PATH", str(db_path))
    monkeypatch.setenv("TAVILY_POOL_CRYPTO_KEY", "tavily-pool-import-test-key")

    from src.modules.tavily_pool.sqlite import get_engine, init_schema

    engine = get_engine(reset=True)
    init_schema(engine)


def test_import_accounts_json_creates_accounts_and_tokens(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.sqlite import get_session_factory, reset_engine

    try:
        with get_session_factory()() as db:
            service = TavilyAccountService(db)
            result = service.import_accounts(
                file_type="json",
                content='[{"email":"json1@example.com","password":"pwd1","api_key":"tvly-json-0001"}]',
                merge_mode="skip",
            )

            assert result["stats"]["total"] == 1
            assert result["stats"]["created"] == 1
            assert result["stats"]["failed"] == 0
    finally:
        reset_engine()


def test_import_accounts_csv_overwrite_updates_notes(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.sqlite import get_session_factory, reset_engine

    try:
        with get_session_factory()() as db:
            service = TavilyAccountService(db)
            service.create_account(email="csv@example.com", password="old", notes="old-note")

            result = service.import_accounts(
                file_type="csv",
                content=(
                    "email,password,api_key,notes,source\n"
                    "csv@example.com,newpwd,tvly-csv-0001,new-note,import\n"
                ),
                merge_mode="overwrite",
            )

            accounts = service.list_accounts()
            assert result["stats"]["updated"] == 1
            assert any(item.email == "csv@example.com" and item.notes == "new-note" for item in accounts)
    finally:
        reset_engine()


def test_import_accounts_error_mode_rolls_back(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.sqlite import get_session_factory, reset_engine

    try:
        with get_session_factory()() as db:
            service = TavilyAccountService(db)
            service.create_account(email="dup@example.com", password="old")

            result = service.import_accounts(
                file_type="json",
                content='[{"email":"dup@example.com","password":"new"}]',
                merge_mode="error",
            )

            assert result["stats"]["failed"] == 1
            assert len(service.list_accounts()) == 1
    finally:
        reset_engine()


def test_import_accounts_collects_row_errors_for_invalid_email(tmp_path, monkeypatch):
    _setup_sqlite_env(tmp_path, monkeypatch)

    from src.modules.tavily_pool.services.account_service import TavilyAccountService
    from src.modules.tavily_pool.sqlite import get_session_factory, reset_engine

    try:
        with get_session_factory()() as db:
            service = TavilyAccountService(db)
            result = service.import_accounts(
                file_type="csv",
                content=(
                    "email,password,tokens,notes,source\n"
                    "invalid-email,pwd,\"tvly-1\",note,import\n"
                ),
                merge_mode="skip",
            )

            assert result["stats"]["failed"] == 1
            assert result["errors"][0]["row"] == 2
    finally:
        reset_engine()
