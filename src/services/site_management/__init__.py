from __future__ import annotations

from typing import Any

__all__ = [
    "SiteManagementLogService",
    "SiteSnapshotService",
    "SiteAccountSyncService",
    "SiteAccountOpsService",
]


def __getattr__(name: str) -> Any:
    if name == "SiteManagementLogService":
        from .log_service import SiteManagementLogService

        return SiteManagementLogService
    if name == "SiteSnapshotService":
        from .snapshot_service import SiteSnapshotService

        return SiteSnapshotService
    if name == "SiteAccountSyncService":
        from .site_account_sync_service import SiteAccountSyncService

        return SiteAccountSyncService
    if name == "SiteAccountOpsService":
        from .site_account_ops_service import SiteAccountOpsService

        return SiteAccountOpsService
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
