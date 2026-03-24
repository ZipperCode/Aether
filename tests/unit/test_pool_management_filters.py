from src.api.admin.pool.routes import _detail_is_oauth_invalid, _filter_pool_key_details
from src.api.admin.pool.schemas import PoolKeyDetail


def _detail(
    key_id: str,
    *,
    is_active: bool = True,
    scheduling_status: str = "available",
    account_status_blocked: bool = False,
    account_status_code: str | None = None,
    account_status_label: str | None = None,
    account_status_reason: str | None = None,
    auth_type: str = "api_key",
    oauth_invalid_at: int | None = None,
    oauth_invalid_reason: str | None = None,
    oauth_expires_at: int | None = None,
    cooldown_reason: str | None = None,
    circuit_breaker_open: bool = False,
    cost_limit: int | None = None,
    cost_window_usage: int = 0,
    status_snapshot: dict | None = None,
) -> PoolKeyDetail:
    return PoolKeyDetail(
        key_id=key_id,
        key_name=key_id,
        is_active=is_active,
        auth_type=auth_type,
        oauth_invalid_at=oauth_invalid_at,
        oauth_invalid_reason=oauth_invalid_reason,
        oauth_expires_at=oauth_expires_at,
        scheduling_status=scheduling_status,
        scheduling_reason=scheduling_status or "available",
        scheduling_label=scheduling_status or "available",
        account_status_blocked=account_status_blocked,
        account_status_code=account_status_code,
        account_status_label=account_status_label,
        account_status_reason=account_status_reason,
        cooldown_reason=cooldown_reason,
        circuit_breaker_open=circuit_breaker_open,
        cost_limit=cost_limit,
        cost_window_usage=cost_window_usage,
        status_snapshot=status_snapshot  # type: ignore[arg-type]
        or {
            "oauth": {"code": "none"},
            "account": {"code": "ok", "blocked": False},
            "quota": {"code": "unknown", "exhausted": False},
        },
    )


def test_filter_pool_key_details_require_schedulable_keeps_available_and_degraded() -> None:
    details = [
        _detail("available", scheduling_status="available"),
        _detail("degraded", scheduling_status="degraded"),
        _detail("blocked", scheduling_status="blocked", account_status_blocked=True),
    ]

    filtered = _filter_pool_key_details(details, require_schedulable=True)

    assert [item.key_id for item in filtered] == ["available", "degraded"]


def test_filter_pool_key_details_require_schedulable_uses_fallback_when_status_missing() -> None:
    details = [
        _detail("manual-disabled", scheduling_status="", is_active=False),
        _detail("cooldown", scheduling_status="", cooldown_reason="rate_limited_429"),
        _detail("usable", scheduling_status="", is_active=True),
    ]

    filtered = _filter_pool_key_details(details, require_schedulable=True)

    assert [item.key_id for item in filtered] == ["usable"]


def test_detail_is_oauth_invalid_excludes_account_disabled_state() -> None:
    detail = _detail(
        "disabled-account",
        auth_type="oauth",
        oauth_invalid_at=1,
        oauth_invalid_reason="[ACCOUNT_BLOCK] account has been deactivated",
        account_status_blocked=True,
        account_status_code="account_disabled",
        account_status_label="账号停用",
    )

    assert _detail_is_oauth_invalid(detail) is False


def test_detail_is_oauth_invalid_accepts_token_expired_state() -> None:
    detail = _detail(
        "expired-token",
        auth_type="oauth",
        oauth_invalid_at=1,
        oauth_invalid_reason="[OAUTH_EXPIRED] token invalidated",
        account_status_blocked=True,
        account_status_code="oauth_expired",
        account_status_label="Token 失效",
    )

    assert _detail_is_oauth_invalid(detail) is True


def test_detail_is_oauth_invalid_accepts_refresh_failed_state() -> None:
    detail = _detail(
        "refresh-failed",
        auth_type="oauth",
        oauth_invalid_at=1,
        oauth_invalid_reason="[REFRESH_FAILED] refresh_token_reused",
        account_status_blocked=False,
        account_status_code="oauth_refresh_failed",
        account_status_label="续期失败",
    )

    assert _detail_is_oauth_invalid(detail) is True


def test_detail_is_oauth_invalid_uses_status_snapshot_without_legacy_fields() -> None:
    detail = _detail(
        "snapshot-invalid",
        auth_type="oauth",
        oauth_invalid_at=None,
        oauth_invalid_reason=None,
        account_status_blocked=False,
        account_status_code=None,
        account_status_label=None,
        status_snapshot={
            "oauth": {
                "code": "invalid",
                "label": "已失效",
                "reason": "refresh_token_reused",
                "requires_reauth": True,
            },
            "account": {"code": "workspace_deactivated", "label": "工作区停用", "blocked": True},
            "quota": {"code": "ok", "exhausted": False},
        },
    )

    assert _detail_is_oauth_invalid(detail) is True


def test_filter_pool_key_details_require_schedulable_uses_status_snapshot_account_block() -> None:
    details = [
        _detail(
            "snapshot-blocked",
            scheduling_status="",
            is_active=True,
            account_status_blocked=False,
            status_snapshot={
                "oauth": {"code": "valid"},
                "account": {"code": "account_disabled", "label": "账号停用", "blocked": True},
                "quota": {"code": "ok", "exhausted": False},
            },
        ),
        _detail(
            "snapshot-ok",
            scheduling_status="",
            is_active=True,
            account_status_blocked=False,
            status_snapshot={
                "oauth": {"code": "valid"},
                "account": {"code": "ok", "blocked": False},
                "quota": {"code": "ok", "exhausted": False},
            },
        ),
    ]

    filtered = _filter_pool_key_details(details, require_schedulable=True)

    assert [item.key_id for item in filtered] == ["snapshot-ok"]
