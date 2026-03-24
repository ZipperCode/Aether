from __future__ import annotations

import argparse
from typing import Any

from sqlalchemy import select
from sqlalchemy.orm import joinedload, load_only

from src.database import create_session
from src.models.database import Provider, ProviderAPIKey
from src.services.provider_keys.status_snapshot_store import sync_provider_key_status_snapshot


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Backfill provider_api_keys.status_snapshot from existing OAuth/account/quota fields."
    )
    parser.add_argument(
        "--batch-size",
        type=int,
        default=200,
        help="Number of provider keys to process per commit.",
    )
    parser.add_argument(
        "--include-existing",
        action="store_true",
        help="Recompute rows that already have status_snapshot instead of only filling missing rows.",
    )
    return parser.parse_args()


def build_batch_stmt(
    *,
    batch_size: int,
    last_id: str | None,
    include_existing: bool,
) -> Any:
    stmt = (
        select(ProviderAPIKey)
        .options(joinedload(ProviderAPIKey.provider).load_only(Provider.provider_type))
        .order_by(ProviderAPIKey.id)
        .limit(batch_size)
    )
    if last_id is not None:
        stmt = stmt.where(ProviderAPIKey.id > last_id)
    if not include_existing:
        stmt = stmt.where(ProviderAPIKey.status_snapshot.is_(None))
    return stmt


def main() -> int:
    args = parse_args()
    batch_size = max(1, int(args.batch_size or 200))
    include_existing = bool(args.include_existing)

    db = create_session()
    processed = 0
    updated = 0
    last_id: str | None = None

    try:
        while True:
            batch = (
                db.execute(
                    build_batch_stmt(
                        batch_size=batch_size,
                        last_id=last_id,
                        include_existing=include_existing,
                    )
                )
                .scalars()
                .all()
            )
            if not batch:
                break

            for key in batch:
                last_id = str(key.id)
                processed += 1
                previous = getattr(key, "status_snapshot", None)
                current = sync_provider_key_status_snapshot(key)
                if current != previous:
                    updated += 1

            db.commit()
            print(
                f"processed={processed} updated={updated} last_id={last_id}",
                flush=True,
            )

        print(
            f"backfill finished: processed={processed} updated={updated} include_existing={include_existing}",
            flush=True,
        )
        return 0
    finally:
        db.close()


if __name__ == "__main__":
    raise SystemExit(main())
