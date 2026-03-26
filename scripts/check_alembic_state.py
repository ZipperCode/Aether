#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path

from sqlalchemy import create_engine

from src.database import get_db_url
from src.utils.alembic_state import load_known_revisions, probe_database_state


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    alembic_ini = repo_root / "alembic.ini"
    expected_head, known_revisions = load_known_revisions(str(alembic_ini))

    engine = create_engine(get_db_url())
    result = probe_database_state(engine, expected_head, known_revisions)

    print(f"status: {result.status}")
    print(f"summary: {result.summary}")
    print(f"expected_head: {result.expected_head}")
    print(f"current_revisions: {', '.join(result.current_revisions) or '(none)'}")
    print(f"can_upgrade_directly: {'yes' if result.can_upgrade_directly else 'no'}")

    if result.missing_sentinels:
      print("missing_sentinels:")
      for item in result.missing_sentinels:
          print(f"  - {item}")

    if result.recommendations:
        print("recommendations:")
        for item in result.recommendations:
            print(f"  - {item}")

    print("json:")
    print(
        json.dumps(
            {
                "status": result.status,
                "summary": result.summary,
                "expected_head": result.expected_head,
                "current_revisions": result.current_revisions,
                "can_upgrade_directly": result.can_upgrade_directly,
                "missing_sentinels": result.missing_sentinels,
                "recommendations": result.recommendations,
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
