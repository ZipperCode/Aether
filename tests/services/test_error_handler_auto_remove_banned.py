from __future__ import annotations

import sys
import types
from types import SimpleNamespace
from typing import Any, cast

import pytest

from src.services.orchestration.error_handler import ErrorHandlerService


class _FakeDB:
    def __init__(self) -> None:
        self.deleted: list[object] = []
        self.commit_count = 0

    def delete(self, obj: object) -> None:
        self.deleted.append(obj)

    def commit(self) -> None:
        self.commit_count += 1


def _build_key() -> SimpleNamespace:
    return SimpleNamespace(
        id="k1",
        provider_id="p1",
        name="k1-name",
        auth_type="oauth",
        auth_config=None,
        oauth_invalid_at=None,
        oauth_invalid_reason=None,
        is_active=True,
    )


def test_mark_oauth_key_blocked_auto_remove_enabled_skips_verification_state(
    monkeypatch: Any,
) -> None:
    db = _FakeDB()
    service = ErrorHandlerService(db=cast(Any, db))
    key = _build_key()
    provider = SimpleNamespace(
        provider_type="codex",
        config={"pool_advanced": {"auto_remove_banned_keys": True}},
    )

    monkeypatch.setattr(
        ErrorHandlerService,
        "_schedule_auto_cleanup_after_delete",
        staticmethod(lambda **kwargs: None),
    )

    service._mark_oauth_key_blocked(cast(Any, key), "req-1", provider=cast(Any, provider))

    assert db.commit_count == 1
    assert db.deleted == []
    assert key.is_active is True
    assert str(key.oauth_invalid_reason).startswith("[ACCOUNT_BLOCK] ")


def test_mark_oauth_key_blocked_auto_remove_disabled() -> None:
    db = _FakeDB()
    service = ErrorHandlerService(db=cast(Any, db))
    key = _build_key()
    provider = SimpleNamespace(config={"pool_advanced": {"auto_remove_banned_keys": False}})

    service._mark_oauth_key_blocked(cast(Any, key), "req-1", provider=cast(Any, provider))

    assert db.commit_count == 1
    assert db.deleted == []
    assert key.is_active is True
    assert str(key.oauth_invalid_reason).startswith("[ACCOUNT_BLOCK] ")


def test_mark_oauth_key_blocked_auto_remove_enabled_for_deactivated_account(
    monkeypatch: Any,
) -> None:
    db = _FakeDB()
    service = ErrorHandlerService(db=cast(Any, db))
    key = _build_key()
    provider = SimpleNamespace(
        provider_type="codex",
        config={"pool_advanced": {"auto_remove_banned_keys": True}},
    )

    monkeypatch.setattr(
        ErrorHandlerService,
        "_schedule_auto_cleanup_after_delete",
        staticmethod(lambda **kwargs: None),
    )

    service._mark_oauth_key_blocked(
        cast(Any, key),
        "req-1",
        reason="account has been deactivated",
        provider=cast(Any, provider),
    )

    assert db.commit_count == 1
    assert db.deleted == [key]
    assert key.is_active is True
    assert key.oauth_invalid_reason == "[ACCOUNT_BLOCK] account has been deactivated"


@pytest.mark.asyncio
async def test_verify_oauth_before_account_block_skips_when_refresh_marks_token_expired(
    monkeypatch: Any,
) -> None:
    db = _FakeDB()
    service = ErrorHandlerService(db=cast(Any, db))
    key = _build_key()
    endpoint = SimpleNamespace()

    fake_module = types.ModuleType("src.services.provider.auth")

    async def _fake_get_provider_auth(*_args: Any, **_kwargs: Any) -> None:
        key.oauth_invalid_reason = "[OAUTH_EXPIRED] token expired"

    fake_module.get_provider_auth = _fake_get_provider_auth
    monkeypatch.setitem(sys.modules, "src.services.provider.auth", fake_module)

    should_mark = await service._verify_oauth_before_account_block(
        endpoint=cast(Any, endpoint),
        key=cast(Any, key),
        request_id="req-1",
        candidate_reason="Google 要求验证账号",
    )

    assert should_mark is False
