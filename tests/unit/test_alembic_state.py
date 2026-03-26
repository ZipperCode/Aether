from src.utils.alembic_state import (
    AlembicProbeInput,
    classify_alembic_state,
)


def test_classify_empty_database_without_version_table() -> None:
    result = classify_alembic_state(
        AlembicProbeInput(
            expected_head="e8f1a2b3c4d5",
            known_revisions={"20251210_baseline", "e8f1a2b3c4d5"},
            current_revisions=[],
            tables=set(),
            table_columns={},
            has_alembic_version_table=False,
        )
    )

    assert result.status == "empty_database"
    assert result.can_upgrade_directly is True


def test_classify_normal_database_on_known_revision() -> None:
    result = classify_alembic_state(
        AlembicProbeInput(
            expected_head="e8f1a2b3c4d5",
            known_revisions={"20251210_baseline", "e8f1a2b3c4d5"},
            current_revisions=["20251210_baseline"],
            tables={"users", "providers", "alembic_version"},
            table_columns={"users": {"id", "email"}},
            has_alembic_version_table=True,
        )
    )

    assert result.status == "upgradeable"
    assert result.can_upgrade_directly is True


def test_classify_orphan_revision_when_db_revision_missing_from_repo() -> None:
    result = classify_alembic_state(
        AlembicProbeInput(
            expected_head="e8f1a2b3c4d5",
            known_revisions={"20251210_baseline", "e8f1a2b3c4d5"},
            current_revisions=["legacy_rev_123"],
            tables={"users", "providers", "alembic_version"},
            table_columns={"users": {"id", "email"}},
            has_alembic_version_table=True,
        )
    )

    assert result.status == "orphan_revision"
    assert result.can_upgrade_directly is False


def test_classify_suspicious_head_when_schema_is_missing_late_sentinels() -> None:
    result = classify_alembic_state(
        AlembicProbeInput(
            expected_head="e8f1a2b3c4d5",
            known_revisions={"20251210_baseline", "e8f1a2b3c4d5"},
            current_revisions=["e8f1a2b3c4d5"],
            tables={"users", "providers", "provider_api_keys", "usage", "alembic_version"},
            table_columns={
                "provider_api_keys": {"id", "name"},
                "usage": {"id", "request_id"},
            },
            has_alembic_version_table=True,
        )
    )

    assert result.status == "head_schema_drift"
    assert result.can_upgrade_directly is False
    assert any("status_snapshot" in item for item in result.missing_sentinels)


def test_classify_head_ok_when_late_sentinels_exist() -> None:
    result = classify_alembic_state(
        AlembicProbeInput(
            expected_head="e8f1a2b3c4d5",
            known_revisions={"20251210_baseline", "e8f1a2b3c4d5"},
            current_revisions=["e8f1a2b3c4d5"],
            tables={
                "users",
                "providers",
                "provider_api_keys",
                "usage",
                "proxy_nodes",
                "user_sessions",
                "wallet_daily_usage_ledgers",
                "webdav_sources",
                "alembic_version",
            },
            table_columns={
                "provider_api_keys": {"id", "name", "auth_type", "status_snapshot"},
                "usage": {"id", "request_id", "provider_request_body", "client_response_body", "billing_status"},
                "proxy_nodes": {"id", "remote_config"},
                "user_sessions": {"id", "refresh_token_hash"},
                "wallet_daily_usage_ledgers": {"id", "billing_date"},
                "webdav_sources": {"id", "name"},
            },
            has_alembic_version_table=True,
        )
    )

    assert result.status == "current_head"
    assert result.can_upgrade_directly is True
