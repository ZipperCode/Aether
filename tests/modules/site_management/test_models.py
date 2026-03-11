"""Tests for site management module models."""
from src.modules.site_management.models import (
    WebDavSource,
    SiteAccount,
    SiteSourceSnapshot,
    SiteSyncRun,
    SiteSyncItem,
    SiteCheckinRun,
    SiteCheckinItem,
)


def test_webdav_source_table_and_fields() -> None:
    """WebDavSource has correct table name and all required fields."""
    assert WebDavSource.__tablename__ == "webdav_sources"
    for field in ("id", "name", "url", "username", "password", "is_active",
                  "sync_enabled", "last_sync_at", "last_sync_status",
                  "created_at", "updated_at"):
        assert hasattr(WebDavSource, field), f"Missing field: {field}"


def test_site_account_has_webdav_source_id_not_provider_id() -> None:
    """SiteAccount has webdav_source_id and does NOT define provider_id.

    NOTE: The legacy SiteAccount in src/models/database.py still shares the same
    table via extend_existing, so hasattr may see old columns.  We inspect the
    *class source* to ensure the new module does not declare them.
    """
    import inspect

    assert hasattr(SiteAccount, "webdav_source_id")
    src = inspect.getsource(SiteAccount)
    assert "provider_id" not in src, "provider_id should not be defined in new SiteAccount"
    assert "source_snapshot_id" not in src, "source_snapshot_id should not be defined in new SiteAccount"
    # source_type is not defined in the new SiteAccount class
    assert "source_type" not in src or "source_type" not in [
        line.strip().split("=")[0].strip()
        for line in src.splitlines()
        if "=" in line and not line.strip().startswith(("#", '"', "'"))
    ], "source_type should not be defined as a column in new SiteAccount"


def test_site_account_retained_fields() -> None:
    """SiteAccount retains all operational fields."""
    assert SiteAccount.__tablename__ == "site_accounts"
    for field in ("id", "domain", "site_url", "architecture_id", "base_url",
                  "auth_type", "credentials", "config", "checkin_enabled",
                  "balance_sync_enabled", "is_active", "last_checkin_status",
                  "last_checkin_at", "last_balance_status", "last_balance_total",
                  "last_balance_at", "created_at", "updated_at"):
        assert hasattr(SiteAccount, field), f"Missing field: {field}"


def test_site_account_unique_constraint_defined() -> None:
    """SiteAccount has composite unique constraint on (webdav_source_id, domain)."""
    constraints = [c for c in SiteAccount.__table_args__
                   if hasattr(c, 'name') and c.name == "uq_site_accounts_source_domain"]
    assert len(constraints) == 1


def test_site_source_snapshot_has_webdav_source_id() -> None:
    """SiteSourceSnapshot has webdav_source_id, no source_url in its own definition.

    NOTE: The legacy SiteSourceSnapshot in database.py still declares source_url
    on the shared table.  We inspect the class source instead of hasattr.
    """
    import inspect

    assert hasattr(SiteSourceSnapshot, "webdav_source_id")
    src = inspect.getsource(SiteSourceSnapshot)
    assert "source_url" not in src, "source_url should not be defined in new SiteSourceSnapshot"


def test_site_sync_run_has_webdav_source_id() -> None:
    """SiteSyncRun has webdav_source_id."""
    assert hasattr(SiteSyncRun, "webdav_source_id")


def test_unchanged_models_fields() -> None:
    """SiteSyncItem, SiteCheckinRun, SiteCheckinItem retain original fields."""
    assert SiteSyncItem.__tablename__ == "site_sync_items"
    assert SiteCheckinRun.__tablename__ == "site_checkin_runs"
    assert SiteCheckinItem.__tablename__ == "site_checkin_items"


def test_no_provider_imports_in_module() -> None:
    """Module models.py must not import Provider."""
    import inspect
    import src.modules.site_management.models as models_module
    source = inspect.getsource(models_module)
    # Check that Provider is not imported (import/from statements only);
    # occurrences in docstrings or column names like provider_id are acceptable.
    import_lines = [
        line.strip() for line in source.splitlines()
        if line.strip().startswith(("import ", "from "))
    ]
    for line in import_lines:
        assert "Provider" not in line, f"Provider import found: {line}"
