from src.models.database import SiteAccount, SiteSourceSnapshot


def test_site_account_model_has_required_fields() -> None:
    assert SiteAccount.__tablename__ == "site_accounts"
    assert hasattr(SiteAccount, "id")
    assert hasattr(SiteAccount, "domain")
    assert hasattr(SiteAccount, "provider_id")
    assert hasattr(SiteAccount, "checkin_enabled")
    assert hasattr(SiteAccount, "balance_sync_enabled")


def test_site_source_snapshot_model_has_required_fields() -> None:
    assert SiteSourceSnapshot.__tablename__ == "site_source_snapshots"
    assert hasattr(SiteSourceSnapshot, "id")
    assert hasattr(SiteSourceSnapshot, "source_url")
    assert hasattr(SiteSourceSnapshot, "payload_hash")
    assert hasattr(SiteSourceSnapshot, "raw_payload")
    assert hasattr(SiteSourceSnapshot, "fetched_at")
