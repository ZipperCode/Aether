from __future__ import annotations

import importlib.util
from pathlib import Path
from types import ModuleType


REVISION_FILE = Path(
    "alembic/versions/20260307_1200_e4f8d1c2b3a4_add_site_account_tables.py"
)


def _load_revision_module() -> ModuleType:
    assert REVISION_FILE.exists(), f"missing migration file: {REVISION_FILE}"
    spec = importlib.util.spec_from_file_location("site_account_revision", REVISION_FILE)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_site_account_migration_metadata() -> None:
    module = _load_revision_module()
    assert module.revision == "e4f8d1c2b3a4"
    assert module.down_revision == "c2d5e0f7a9b1"


def test_site_account_migration_creates_required_tables() -> None:
    module = _load_revision_module()
    created_tables: list[str] = []

    class _Recorder:
        def create_table(self, table_name: str, *args, **kwargs) -> None:  # noqa: ARG002
            created_tables.append(table_name)

        def create_index(self, *args, **kwargs) -> None:  # noqa: ARG002
            pass

    module.op = _Recorder()
    module.upgrade()

    assert "site_source_snapshots" in created_tables
    assert "site_accounts" in created_tables
