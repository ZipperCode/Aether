from __future__ import annotations


def _session_factory(tmp_path, monkeypatch):
    db_path = tmp_path / "search_pool_gateway.db"
    monkeypatch.setenv("SEARCH_POOL_GATEWAY_DB_PATH", str(db_path))

    from src.modules.search_pool_gateway.sqlite import get_engine, get_session_factory

    get_engine(reset=True)
    return get_session_factory()


def test_key_repo_creates_and_lists_by_service(tmp_path, monkeypatch):
    session_factory = _session_factory(tmp_path, monkeypatch)

    from src.modules.search_pool_gateway.repositories.key_repo import GatewayKeyRepository

    with session_factory() as db:
        repo = GatewayKeyRepository(db)
        repo.create(service="tavily", raw_key="tvly-raw-1", key_masked="tvly-abc***0001", email="a@example.com")
        repo.create(service="firecrawl", raw_key="fc-raw-1", key_masked="fc-abc***0001", email="b@example.com")

        tavily = repo.list_keys(service="tavily")
        firecrawl = repo.list_keys(service="firecrawl")

        assert len(tavily) == 1
        assert tavily[0].service == "tavily"
        assert len(firecrawl) == 1
        assert firecrawl[0].service == "firecrawl"
