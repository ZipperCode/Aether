from __future__ import annotations

import argparse
import json
from pathlib import Path

from src.modules.tavily_pool.services.account_service import TavilyAccountService
from src.modules.tavily_pool.services.token_service import TavilyTokenService
from src.modules.tavily_pool.sqlite import get_engine, get_session_factory, init_schema


def create_accounts_from_json(file_path: str | Path) -> dict[str, int]:
    payload = json.loads(Path(file_path).read_text(encoding="utf-8"))
    if not isinstance(payload, list):
        raise ValueError("input json must be an array")

    init_schema(get_engine())
    session_factory = get_session_factory()
    success = 0
    failed = 0

    with session_factory() as db:
        account_service = TavilyAccountService(db)
        token_service = TavilyTokenService(db)
        for item in payload:
            try:
                email = str(item["email"]).strip()
                password = str(item["password"])
                token = str(item["token"])
                source = str(item.get("source", "script"))
                notes = item.get("notes")

                account = account_service.create_account(
                    email=email,
                    password=password,
                    source=source,
                    notes=notes,
                )
                token_service.create_token(account.id, token)
                success += 1
            except Exception:
                failed += 1

    return {"success": success, "failed": failed}


def main() -> None:
    parser = argparse.ArgumentParser(description="Create Tavily accounts and persist to sqlite")
    parser.add_argument("--input", required=True, help="JSON file path")
    args = parser.parse_args()

    result = create_accounts_from_json(args.input)
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
