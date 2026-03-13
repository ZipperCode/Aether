import asyncio

from src.api.admin.provider_sync import SyncTriggerRequest, trigger_sync
from src.models.database import Provider
from src.modules.site_management.services.log_service import SiteManagementLogService


class _FakeQuery:
    def __init__(self, providers: list[Provider]) -> None:
        self._providers = providers

    def all(self) -> list[Provider]:
        return self._providers


class _FakeSession:
    def __init__(self, providers: list[Provider]) -> None:
        self.providers = providers
        self.commit_calls = 0

    def query(self, model: type[Provider]) -> _FakeQuery:
        assert model is Provider
        return _FakeQuery(self.providers)

    def commit(self) -> None:
        self.commit_calls += 1


def _provider(provider_id: str, website: str, config: dict | None) -> Provider:
    return Provider(
        id=provider_id,
        name=f"provider-{provider_id}",
        website=website,
        provider_type="custom",
        config=config,
    )


def _run_trigger_sync(db: _FakeSession, payload: SyncTriggerRequest) -> dict:
    return asyncio.run(trigger_sync(payload, db=db, _=object()))


def _sync_result(**overrides: int | bool):
    return type(
        "Result",
        (),
        {
            "total_accounts": 1,
            "total_providers": 1,
            "matched_providers": 1,
            "updated_providers": 1,
            "skipped_no_provider_ops": 0,
            "skipped_no_cookie": 0,
            "skipped_not_changed": 0,
            "dry_run": False,
            **overrides,
        },
    )()


def test_trigger_sync_defaults_auto_create_provider_ops_to_true(monkeypatch) -> None:
    provider = _provider("p1", "https://anyrouter.top", None)
    db = _FakeSession([provider])
    captured: dict[str, object] = {}

    monkeypatch.setattr(
        SiteManagementLogService,
        "record_sync_run",
        staticmethod(lambda **kwargs: type("Run", (), {"id": "run-1"})()),
    )
    monkeypatch.setattr(
        "src.services.system.config.SystemConfigService.get_config",
        classmethod(
            lambda cls, *_args, **_kwargs: (_ for _ in ()).throw(
                AssertionError("should not read legacy system config")
            )
        ),
    )

    def fake_sync_from_backup_object(self, session, backup, dry_run, auto_create_provider_ops):
        captured["auto_create_provider_ops"] = auto_create_provider_ops
        captured["dry_run"] = dry_run
        return _sync_result()

    monkeypatch.setattr(
        "src.api.admin.provider_sync.AllApiHubSyncService.sync_from_backup_object",
        fake_sync_from_backup_object,
    )

    payload = SyncTriggerRequest(
        url="https://dav.example.com/backup.json",
        username="u",
        password="p",
        backup={"version": "2.0", "accounts": {"accounts": []}},
    )
    data = _run_trigger_sync(db, payload)

    assert data["run_id"] == "run-1"
    assert captured["auto_create_provider_ops"] is True
    assert captured["dry_run"] is False


def test_trigger_sync_honors_auto_create_provider_ops_override(monkeypatch) -> None:
    provider = _provider("p1", "https://anyrouter.top", None)
    db = _FakeSession([provider])
    captured: dict[str, object] = {}

    monkeypatch.setattr(
        SiteManagementLogService,
        "record_sync_run",
        staticmethod(lambda **kwargs: type("Run", (), {"id": "run-1"})()),
    )
    monkeypatch.setattr(
        "src.services.system.config.SystemConfigService.get_config",
        classmethod(
            lambda cls, *_args, **_kwargs: (_ for _ in ()).throw(
                AssertionError("should not read legacy system config")
            )
        ),
    )

    def fake_sync_from_backup_object(self, session, backup, dry_run, auto_create_provider_ops):
        captured["auto_create_provider_ops"] = auto_create_provider_ops
        return _sync_result(updated_providers=0, skipped_no_provider_ops=1)

    monkeypatch.setattr(
        "src.api.admin.provider_sync.AllApiHubSyncService.sync_from_backup_object",
        fake_sync_from_backup_object,
    )

    payload = SyncTriggerRequest(
        url="https://dav.example.com/backup.json",
        username="u",
        password="p",
        auto_create_provider_ops=False,
        backup={"version": "2.0", "accounts": {"accounts": []}},
    )
    data = _run_trigger_sync(db, payload)

    assert data["updated_providers"] == 0
    assert data["skipped_no_provider_ops"] == 1
    assert captured["auto_create_provider_ops"] is False
