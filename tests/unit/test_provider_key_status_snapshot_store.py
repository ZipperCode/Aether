from __future__ import annotations

import pytest

from src.models.database import Provider, ProviderAPIKey, _provider_api_key_before_insert
from src.services.provider_keys import status_snapshot_store as module


def test_provider_api_key_before_insert_populates_status_snapshot(
    monkeypatch: "pytest.MonkeyPatch",
) -> None:
    provider = Provider(
        id="provider-1",
        name="Codex Pool",
        provider_type="codex",
    )
    key = ProviderAPIKey(
        id="key-1",
        provider_id="provider-1",
        provider=provider,  # type: ignore[arg-type]
        api_key="enc-access-token",
        auth_type="oauth",
        auth_config='{"expires_at":2100000000}',
        name="codex-user",
        oauth_invalid_reason=(
            "[ACCOUNT_BLOCK] 工作区已停用 (deactivated_workspace)\n"
            "[REFRESH_FAILED] refresh_token_reused"
        ),
    )

    monkeypatch.setattr(module.crypto_service, "decrypt", lambda value: value)

    _provider_api_key_before_insert(None, None, key)

    assert isinstance(key.status_snapshot, dict)
    assert key.status_snapshot["oauth"]["code"] == "invalid"
    assert key.status_snapshot["oauth"]["label"] == "已失效"
    assert key.status_snapshot["oauth"]["reason"] == "refresh_token_reused"
    assert key.status_snapshot["account"]["code"] == "workspace_deactivated"
    assert key.status_snapshot["account"]["blocked"] is True


def test_resolve_provider_key_status_snapshot_prefers_persisted_snapshot_layers(
    monkeypatch: "pytest.MonkeyPatch",
) -> None:
    provider = Provider(
        id="provider-1",
        name="Codex Pool",
        provider_type="codex",
    )
    key = ProviderAPIKey(
        id="key-2",
        provider_id="provider-1",
        provider=provider,  # type: ignore[arg-type]
        api_key="enc-access-token",
        auth_type="oauth",
        auth_config='{"expires_at":100}',
        name="codex-user",
        upstream_metadata=None,
        oauth_invalid_reason=None,
        status_snapshot={
            "oauth": {
                "code": "valid",
                "label": "有效",
                "expires_at": 100,
            },
            "account": {
                "code": "workspace_deactivated",
                "label": "工作区停用",
                "reason": "persisted",
                "blocked": True,
                "source": "persisted",
            },
            "quota": {
                "code": "exhausted",
                "label": "额度耗尽",
                "reason": "persisted quota",
                "exhausted": True,
                "usage_ratio": 1.0,
            },
        },
    )

    monkeypatch.setattr(module.crypto_service, "decrypt", lambda value: value)

    snapshot = module.resolve_provider_key_status_snapshot(
        key,
        now_ts=200,
    )

    assert snapshot.oauth.code == "expired"
    assert snapshot.oauth.label == "已过期"
    assert snapshot.account.code == "workspace_deactivated"
    assert snapshot.account.reason == "persisted"
    assert snapshot.quota.code == "exhausted"
    assert snapshot.quota.reason == "persisted quota"
