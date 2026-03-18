from __future__ import annotations


def _session_factory(tmp_path, monkeypatch):
    db_path = tmp_path / "search_pool_gateway.db"
    monkeypatch.setenv("SEARCH_POOL_GATEWAY_DB_PATH", str(db_path))
    monkeypatch.setenv("SEARCH_POOL_GATEWAY_CRYPTO_KEY", "search-pool-gateway-test-key")

    from src.modules.search_pool_gateway.sqlite import get_engine, get_session_factory

    get_engine(reset=True)
    return get_session_factory()


def test_pool_round_robin_and_disable_after_three_failures(tmp_path, monkeypatch):
    session_factory = _session_factory(tmp_path, monkeypatch)

    from src.modules.search_pool_gateway.services.key_pool import ServiceKeyPool
    from src.modules.search_pool_gateway.services.key_service import GatewayKeyService

    with session_factory() as db:
        service = GatewayKeyService(db)
        first = service.create_key(service="tavily", raw_key="tvly-k1-abcdef1234567890")
        second = service.create_key(service="tavily", raw_key="tvly-k2-abcdef1234567890")
        first_id = first.id
        second_id = second.id

    pool = ServiceKeyPool(session_factory)
    k1 = pool.get_next_key("tavily")
    k2 = pool.get_next_key("tavily")

    assert k1 is not None
    assert k2 is not None
    assert k1.id != k2.id

    pool.report_result("tavily", first_id, success=False)
    pool.report_result("tavily", first_id, success=False)
    pool.report_result("tavily", first_id, success=False)

    with session_factory() as db:
        from src.modules.search_pool_gateway.repositories.key_repo import GatewayKeyRepository

        repo = GatewayKeyRepository(db)
        first_row = repo.get(first_id)
        second_row = repo.get(second_id)
        assert first_row is not None and first_row.active is False
        assert second_row is not None and second_row.active is True
