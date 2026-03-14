from __future__ import annotations

from pathlib import Path


def test_taskmode_import_path_is_core_context() -> None:
    root = Path(__file__).resolve().parents[1]
    targets = [
        root / "src/api/handlers/base/cli_stream_mixin.py",
        root / "src/api/handlers/base/cli_sync_mixin.py",
        root / "src/api/handlers/base/chat_handler_base.py",
    ]

    for path in targets:
        text = path.read_text(encoding="utf-8")
        assert "from src.services.task.context import TaskMode" not in text
        assert "from src.services.task.core.context import TaskMode" in text
