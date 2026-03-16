from __future__ import annotations

import json

from src.modules.tavily_pool.models import TavilyAccount, TavilyToken


def test_script_create_accounts_persists_to_sqlite(tmp_path, monkeypatch):
    db_path = tmp_path / "tavilies.db"
    data_path = tmp_path / "accounts.json"

    monkeypatch.setenv("TAVILY_POOL_DB_PATH", str(db_path))
    monkeypatch.setenv("TAVILY_POOL_CRYPTO_KEY", "tavily-script-key")

    data_path.write_text(
        json.dumps(
            [
                {
                    "email": "script1@example.com",
                    "password": "p1",
                    "token": "tvly-script-0001",
                    "source": "script",
                },
                {
                    "email": "script2@example.com",
                    "password": "p2",
                    "token": "tvly-script-0002",
                    "source": "script",
                },
            ]
        ),
        encoding="utf-8",
    )

    from scripts.tavily.create_accounts import create_accounts_from_json
    from src.modules.tavily_pool.sqlite import get_session_factory, reset_engine

    try:
        result = create_accounts_from_json(data_path)
        assert result == {"success": 2, "failed": 0}

        session_factory = get_session_factory()
        with session_factory() as db:
            assert db.query(TavilyAccount).count() == 2
            assert db.query(TavilyToken).count() == 2
    finally:
        reset_engine()
