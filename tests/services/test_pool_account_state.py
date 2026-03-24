"""Tests for pool account-state resolution helpers."""

from __future__ import annotations

from src.services.provider.pool.account_state import (
    build_provider_key_status_snapshot,
    resolve_pool_account_state,
    should_auto_remove_account_state,
)


def test_resolve_from_kiro_banned_metadata() -> None:
    state = resolve_pool_account_state(
        provider_type="kiro",
        upstream_metadata={"kiro": {"is_banned": True, "ban_reason": "account suspended"}},
        oauth_invalid_reason=None,
    )
    assert state.blocked is True
    assert state.code == "account_banned"
    assert state.label == "账号封禁"
    assert state.reason == "account suspended"


def test_resolve_from_antigravity_forbidden_metadata() -> None:
    state = resolve_pool_account_state(
        provider_type="antigravity",
        upstream_metadata={"antigravity": {"is_forbidden": True, "forbidden_reason": "403"}},
        oauth_invalid_reason=None,
    )
    assert state.blocked is True
    assert state.code == "account_forbidden"
    assert state.label == "访问受限"
    assert state.reason == "403"


def test_resolve_from_structured_oauth_reason_verification() -> None:
    state = resolve_pool_account_state(
        provider_type="codex",
        upstream_metadata=None,
        oauth_invalid_reason="[ACCOUNT_BLOCK] Google requires verification",
    )
    assert state.blocked is True
    assert state.code == "account_blocked"
    assert state.label == "账号异常"
    assert state.reason == "Google requires verification"


def test_resolve_from_structured_oauth_reason_verification_chinese() -> None:
    state = resolve_pool_account_state(
        provider_type="codex",
        upstream_metadata=None,
        oauth_invalid_reason="[ACCOUNT_BLOCK] Google 要求验证账号",
    )
    assert state.blocked is True
    assert state.code == "account_verification"
    assert state.label == "需要验证"
    assert state.reason == "Google 要求验证账号"


def test_resolve_from_structured_oauth_reason_suspended() -> None:
    state = resolve_pool_account_state(
        provider_type="codex",
        upstream_metadata=None,
        oauth_invalid_reason="[ACCOUNT_BLOCK] account suspended by admin",
    )
    assert state.blocked is True
    assert state.code == "account_suspended"
    assert state.label == "账号封禁"
    assert state.reason == "account suspended by admin"


def test_resolve_from_keyword_oauth_reason_disabled() -> None:
    state = resolve_pool_account_state(
        provider_type=None,
        upstream_metadata={},
        oauth_invalid_reason="organization has been disabled by admin",
    )
    assert state.blocked is True
    assert state.code == "account_disabled"
    assert state.label == "账号停用"


def test_resolve_from_keyword_oauth_reason_verification() -> None:
    state = resolve_pool_account_state(
        provider_type=None,
        upstream_metadata={},
        oauth_invalid_reason="validation_required: please verify your identity",
    )
    assert state.blocked is True
    assert state.code == "account_verification"
    assert state.label == "需要验证"


def test_resolve_healthy_state() -> None:
    state = resolve_pool_account_state(
        provider_type="codex",
        upstream_metadata={"codex": {"primary_used_percent": 30}},
        oauth_invalid_reason="Token expired",
    )
    assert state.blocked is False
    assert state.code is None


def test_bare_forbidden_not_treated_as_account_block() -> None:
    """HTTP 403 'Forbidden' from token refresh should not be misclassified."""
    state = resolve_pool_account_state(
        provider_type="codex",
        upstream_metadata={},
        oauth_invalid_reason="Forbidden",
    )
    assert state.blocked is False


def test_kiro_oauth_reason_text_detected_as_suspended() -> None:
    state = resolve_pool_account_state(
        provider_type="kiro",
        upstream_metadata={},
        oauth_invalid_reason="账户已封禁: Terms of Service violation",
    )
    assert state.blocked is True
    assert state.code == "account_suspended"
    assert state.label == "账号封禁"


def test_antigravity_oauth_reason_text_detected_as_disabled() -> None:
    state = resolve_pool_account_state(
        provider_type="antigravity",
        upstream_metadata={},
        oauth_invalid_reason="账户访问被禁止: 403 Forbidden",
    )
    assert state.blocked is True
    assert state.code == "account_disabled"
    assert state.label == "账号停用"


def test_resolve_from_structured_oauth_reason_workspace_deactivated() -> None:
    state = resolve_pool_account_state(
        provider_type="codex",
        upstream_metadata=None,
        oauth_invalid_reason="[ACCOUNT_BLOCK] 工作区已停用 (deactivated_workspace)",
    )
    assert state.blocked is True
    assert state.code == "workspace_deactivated"
    assert state.label == "工作区停用"


def test_resolve_from_structured_oauth_reason_token_invalidated() -> None:
    state = resolve_pool_account_state(
        provider_type="codex",
        upstream_metadata=None,
        oauth_invalid_reason="[OAUTH_EXPIRED] Your authentication token has been invalidated. Please try signing in again.",
    )
    assert state.blocked is True
    assert state.code == "oauth_expired"
    assert state.label == "Token 失效"


def test_refresh_failed_prefix_does_not_block_even_with_scary_keywords() -> None:
    state = resolve_pool_account_state(
        provider_type="codex",
        upstream_metadata=None,
        oauth_invalid_reason="[REFRESH_FAILED] Token 续期失败 (401): account_deactivated",
    )
    assert state.blocked is False


def test_request_failed_prefix_does_not_block() -> None:
    state = resolve_pool_account_state(
        provider_type="codex",
        upstream_metadata=None,
        oauth_invalid_reason="[REQUEST_FAILED] Codex 账户访问受限 (403)",
    )
    assert state.blocked is False


def test_auto_remove_state_excludes_token_expired_and_verification() -> None:
    expired = resolve_pool_account_state(
        provider_type="codex",
        upstream_metadata=None,
        oauth_invalid_reason="[OAUTH_EXPIRED] token invalidated",
    )
    verification = resolve_pool_account_state(
        provider_type="codex",
        upstream_metadata=None,
        oauth_invalid_reason="[ACCOUNT_BLOCK] Google 要求验证账号",
    )
    disabled = resolve_pool_account_state(
        provider_type="codex",
        upstream_metadata=None,
        oauth_invalid_reason="[ACCOUNT_BLOCK] account has been deactivated",
    )

    assert should_auto_remove_account_state(expired) is False
    assert should_auto_remove_account_state(verification) is False
    assert should_auto_remove_account_state(disabled) is True


def test_build_provider_key_status_snapshot_separates_account_block_from_oauth_state() -> None:
    snapshot = build_provider_key_status_snapshot(
        auth_type="oauth",
        oauth_expires_at=2_000_000_000,
        oauth_invalid_at=1_900_000_000,
        oauth_invalid_reason="[ACCOUNT_BLOCK] 工作区已停用 (deactivated_workspace)",
        provider_type="codex",
        upstream_metadata=None,
        now_ts=1_800_000_000,
    )

    assert snapshot.account.blocked is True
    assert snapshot.account.code == "workspace_deactivated"
    assert snapshot.account.label == "工作区停用"
    assert snapshot.oauth.code == "valid"
    assert snapshot.oauth.requires_reauth is False


def test_build_provider_key_status_snapshot_keeps_refresh_failure_visible_beside_account_block() -> (
    None
):
    snapshot = build_provider_key_status_snapshot(
        auth_type="oauth",
        oauth_expires_at=2_000_000_000,
        oauth_invalid_at=1_900_000_000,
        oauth_invalid_reason=(
            "[ACCOUNT_BLOCK] 工作区已停用 (deactivated_workspace)\n"
            "[REFRESH_FAILED] Token 续期失败 (400): refresh_token_reused"
        ),
        provider_type="codex",
        upstream_metadata=None,
        now_ts=1_800_000_000,
    )

    assert snapshot.account.blocked is True
    assert snapshot.account.code == "workspace_deactivated"
    assert snapshot.oauth.code == "invalid"
    assert snapshot.oauth.label == "已失效"
    assert snapshot.oauth.reason == "Token 续期失败 (400): refresh_token_reused"
    assert snapshot.oauth.requires_reauth is True


def test_build_provider_key_status_snapshot_marks_quota_exhausted() -> None:
    snapshot = build_provider_key_status_snapshot(
        auth_type="oauth",
        oauth_expires_at=2_000_000_000,
        oauth_invalid_at=None,
        oauth_invalid_reason=None,
        provider_type="codex",
        upstream_metadata={
            "codex": {
                "primary_used_percent": 100.0,
                "secondary_used_percent": 20.0,
                "updated_at": 1_800_000_000,
                "plan_type": "team",
            }
        },
        now_ts=1_800_000_000,
    )

    assert snapshot.quota.code == "exhausted"
    assert snapshot.quota.exhausted is True
    assert snapshot.quota.reason == "Codex 周限额剩余 0%"
