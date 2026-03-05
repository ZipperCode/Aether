from types import SimpleNamespace

import pytest

import src.main as main_module


class _FakeDb:
    def __init__(self) -> None:
        self.closed = False

    def close(self) -> None:
        self.closed = True


class _FakeRegistry:
    def __init__(self, enabled_map: dict[str, bool]) -> None:
        self.enabled_map = enabled_map

    def is_enabled(self, name: str, db: object) -> bool:
        assert db is not None
        return self.enabled_map.get(name, False)


@pytest.mark.anyio
async def test_run_module_lifecycle_hooks_only_runs_enabled(monkeypatch: pytest.MonkeyPatch) -> None:
    events: list[str] = []

    async def startup_a() -> None:
        events.append("a")

    async def startup_b() -> None:
        events.append("b")

    modules = [
        SimpleNamespace(metadata=SimpleNamespace(name="mod_a"), on_startup=startup_a, on_shutdown=None),
        SimpleNamespace(metadata=SimpleNamespace(name="mod_b"), on_startup=startup_b, on_shutdown=None),
    ]

    fake_db = _FakeDb()
    monkeypatch.setattr(main_module, "create_session", lambda: fake_db)

    registry = _FakeRegistry({"mod_a": True, "mod_b": False})

    await main_module._run_module_lifecycle_hooks(modules, registry, phase="startup")

    assert events == ["a"]
    assert fake_db.closed is True
