from fastapi import FastAPI
from fastapi.testclient import TestClient

from src.api.admin.provider_sync import router
from src.database import get_db
from src.models.database import Provider
from src.utils.auth_utils import require_admin


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


def test_trigger_sync_returns_summary() -> None:
    provider = _provider(
        "p1",
        "https://anyrouter.top",
        {
            "provider_ops": {
                "architecture_id": "anyrouter",
                "connector": {
                    "auth_type": "cookie",
                    "config": {},
                    "credentials": {"session_cookie": "old"},
                },
            }
        },
    )
    db = _FakeSession([provider])

    app = FastAPI()
    app.include_router(router)
    app.dependency_overrides[require_admin] = lambda: object()
    app.dependency_overrides[get_db] = lambda: db

    client = TestClient(app)
    resp = client.post(
        "/api/admin/provider-sync/trigger",
        json={
            "url": "https://dav.example.com/backup.json",
            "username": "u",
            "password": "p",
            "backup": {
                "version": "2.0",
                "accounts": {
                    "accounts": [
                        {
                            "site_url": "https://anyrouter.top/path",
                            "cookieAuth": {"sessionCookie": "session=new"},
                        }
                    ]
                },
            },
        },
    )

    assert resp.status_code == 200
    data = resp.json()
    assert data["matched_providers"] == 1
    assert data["updated_providers"] == 1
