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
    def filter(self, *_args, **_kwargs) -> "_FakeQuery":
        return self

    def limit(self, *_args, **_kwargs) -> "_FakeQuery":
        return self

    def order_by(self, *_args, **_kwargs) -> "_FakeQuery":
        return self

    def all(self):
        return []

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


def test_list_site_accounts_from_webdav() -> None:
    fake_db = _FakeSession()

    app = FastAPI()
    app.include_router(router)
    app.dependency_overrides[require_admin] = lambda: object()
    app.dependency_overrides[get_db] = lambda: fake_db

    backup_text = """
    {
      "accounts": {
        "accounts": [
          {
            "site_url": "https://api.usegemini.xyz",
            "authType": "access_token",
            "account_info": {
              "access_token": "tok_123",
              "user_id": "u-001"
            }
          }
        ]
      }
    }
    """.strip()

    original_get_config = SystemConfigService.get_config
    original_resolve_password = site_management_api.SiteManagementLogService.resolve_system_password
    original_download_backup = site_management_api.download_backup

    SystemConfigService.get_config = classmethod(  # type: ignore[assignment]
        lambda cls, _db, key, default=None: {
            "all_api_hub_webdav_url": "https://dav.example.com/backup.json",
            "all_api_hub_webdav_username": "user",
            "all_api_hub_webdav_password": "enc",
        }.get(key, default)
    )
    site_management_api.SiteManagementLogService.resolve_system_password = staticmethod(  # type: ignore[assignment]
        lambda _raw: "plain-password"
    )

    async def fake_download(_url: str, _username: str, _password: str) -> str:
        return backup_text

    site_management_api.download_backup = fake_download  # type: ignore[assignment]

    try:
        client = TestClient(app)
        resp = client.get("/api/admin/site-management/accounts")
        assert resp.status_code == 200
        data = resp.json()
        assert len(data) == 1
        assert data[0]["domain"] == "api.usegemini.xyz"
        assert data[0]["auth_type"] == "access_token"
        assert data[0]["user_id"] == "u-001"
        assert data[0]["access_token"] == "tok_123"
    finally:
        SystemConfigService.get_config = original_get_config  # type: ignore[assignment]
        site_management_api.SiteManagementLogService.resolve_system_password = original_resolve_password  # type: ignore[assignment]
        site_management_api.download_backup = original_download_backup  # type: ignore[assignment]


def test_apply_site_accounts_sync_with_manual_edit_payload() -> None:
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
            True if key == "enable_all_api_hub_auto_create_provider_ops" else default
        )
    )

    def fake_sync_from_backup(self, db, backup, dry_run=False, auto_create_provider_ops=True):
        captured["backup"] = backup
        captured["dry_run"] = dry_run
        captured["auto_create_provider_ops"] = auto_create_provider_ops
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
                "dry_run": dry_run,
                "item_results": [],
            },
        )()

    site_management_api.AllApiHubSyncService.sync_from_backup_object = fake_sync_from_backup  # type: ignore[assignment]
    site_management_api.SiteManagementLogService.record_sync_run = staticmethod(  # type: ignore[assignment]
        lambda **kwargs: type("Run", (), {"id": "run-edit-1"})()
    )

    try:
        client = TestClient(app)
        resp = client.post(
            "/api/admin/site-management/accounts/apply-sync",
            json={
                "dry_run": True,
                "accounts": [
                    {
                        "site_url": "https://api.usegemini.xyz",
                        "domain": "api.usegemini.xyz",
                        "auth_type": "access_token",
                        "user_id": "u-001",
                        "access_token": "tok_123",
                        "cookie": "sid=abc",
                    }
                ],
            },
        )
        assert resp.status_code == 200
        assert captured["dry_run"] is True
        assert captured["auto_create_provider_ops"] is True
        backup = captured["backup"]
        assert isinstance(backup, dict)
        accounts = backup["accounts"]["accounts"]
        assert accounts[0]["site_url"] == "https://api.usegemini.xyz"
        assert accounts[0]["account_info"]["access_token"] == "tok_123"
        assert accounts[0]["account_info"]["user_id"] == "u-001"
        assert accounts[0]["cookieAuth"]["cookie"] == "sid=abc"
    finally:
        SystemConfigService.get_config = original_get_config  # type: ignore[assignment]
        site_management_api.AllApiHubSyncService.sync_from_backup_object = original_sync_from_backup  # type: ignore[assignment]
        site_management_api.SiteManagementLogService.record_sync_run = original_record  # type: ignore[assignment]
