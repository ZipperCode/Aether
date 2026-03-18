from __future__ import annotations


def test_tavily_pool_module_no_longer_discovered() -> None:
    from src.modules import ALL_MODULES

    names = {m.metadata.name for m in ALL_MODULES}
    assert "tavily_pool" not in names
    assert "search_pool_gateway" in names
