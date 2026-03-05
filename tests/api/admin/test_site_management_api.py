from fastapi import FastAPI
from fastapi.testclient import TestClient

from src.api.admin import site_management as site_management_api
from src.api.admin.site_management import router
from src.database import get_db
from src.services.system.config import SystemConfigService
from src.utils.auth_utils import require_admin


class _FakeScheduler:
    def __init__(self) -> None:
        self.calls: list[tuple[str, bool]] = []

    async def _perform_provider_checkin(
        self,
        *,
        trigger_source: str = "scheduled",
        ignore_enabled: bool = False,
    ) -> None:
        self.calls.append((trigger_source, ignore_enabled))


class _FakeQuery:
    def order_by(self, *_args, **_kwargs) -> "_FakeQuery":
        return self

    def first(self):
        return ("run-1",)


class _FakeSession:
    def query(self, *_args, **_kwargs) -> _FakeQuery:
        return _FakeQuery()


def test_trigger_site_sync_passes_auto_create_toggle() -> None:
    fake_db = _FakeSession()
    captured: dict[str, object] = {}

    app = FastAPI()
    app.include_router(router)
    app.dependency_overrides[require_admin] = lambda: object()
    app.dependency_overrides[get_db] = lambda: fake_db

    original_get_config = SystemConfigService.get_config
    original_sync_from_backup = site_management_api.AllApiHubSyncService.sync_from_backup_object
    original_record = site_management_api.SiteManagementLogService.record_sync_run

    SystemConfigService.get_config = classmethod(  # type: ignore[assignment]
        lambda cls, _db, key, default=None: (
            False if key == "enable_all_api_hub_auto_create_provider_ops" else default
        )
    )

    def fake_sync_from_backup(self, db, backup, dry_run=False, auto_create_provider_ops=True):
        captured["auto_create_provider_ops"] = auto_create_provider_ops
        return type(
            "Result",
            (),
            {
                "total_accounts": 1,
                "total_providers": 1,
                "matched_providers": 1,
                "updated_providers": 0,
                "skipped_no_provider_ops": 1,
                "skipped_no_cookie": 0,
                "skipped_not_changed": 0,
                "dry_run": dry_run,
                "item_results": [],
            },
        )()

    site_management_api.AllApiHubSyncService.sync_from_backup_object = fake_sync_from_backup  # type: ignore[assignment]
    site_management_api.SiteManagementLogService.record_sync_run = staticmethod(  # type: ignore[assignment]
        lambda **kwargs: type("Run", (), {"id": "run-1"})()
    )

    try:
        client = TestClient(app)
        resp = client.post(
            "/api/admin/site-management/sync/trigger",
            json={"backup": {"version": "2.0", "accounts": {"accounts": []}}, "dry_run": True},
        )
        assert resp.status_code == 200
        assert captured["auto_create_provider_ops"] is False
    finally:
        SystemConfigService.get_config = original_get_config  # type: ignore[assignment]
        site_management_api.AllApiHubSyncService.sync_from_backup_object = original_sync_from_backup  # type: ignore[assignment]
        site_management_api.SiteManagementLogService.record_sync_run = original_record  # type: ignore[assignment]


def test_trigger_checkin_calls_scheduler_manual_mode() -> None:
    fake_scheduler = _FakeScheduler()
    fake_db = _FakeSession()

    app = FastAPI()
    app.include_router(router)
    app.dependency_overrides[require_admin] = lambda: object()
    app.dependency_overrides[get_db] = lambda: fake_db

    original_get_scheduler = site_management_api.get_maintenance_scheduler
    site_management_api.get_maintenance_scheduler = lambda: fake_scheduler
    try:
        client = TestClient(app)
        resp = client.post("/api/admin/site-management/checkin/trigger")

        assert resp.status_code == 200
        data = resp.json()
        assert data["ok"] is True
        assert fake_scheduler.calls == [("manual", True)]
    finally:
        site_management_api.get_maintenance_scheduler = original_get_scheduler
