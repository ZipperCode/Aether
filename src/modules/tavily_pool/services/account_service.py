"""账号服务。"""

from __future__ import annotations

import csv
import io
import json
import re
from dataclasses import dataclass
from typing import Any

from sqlalchemy import select
from sqlalchemy.orm import Session

from src.modules.tavily_pool.models import TavilyAccount, TavilyAccountRuntimeState, TavilyToken
from src.modules.tavily_pool.repositories.account_repo import TavilyAccountRepository
from src.modules.tavily_pool.schemas import TavilyAccountRead
from src.modules.tavily_pool.services.crypto import TavilyCryptoService, mask_token

EMAIL_RE = re.compile(r"^[^@\s]+@[^@\s]+\.[^@\s]+$")


@dataclass
class _ParsedImportRow:
    row: int
    email: str
    password: str | None
    api_key: str | None
    notes: str | None
    source: str


class TavilyAccountService:
    def __init__(self, db: Session) -> None:
        self.db = db
        self.repo = TavilyAccountRepository(db)
        self.crypto = TavilyCryptoService()

    def create_account(
        self,
        *,
        email: str,
        password: str,
        api_key: str | None = None,
        source: str = "manual",
        notes: str | None = None,
    ) -> TavilyAccountRead:
        account = TavilyAccount(
            email=email.strip(),
            password_encrypted=self.crypto.encrypt(password),
            source=source,
            notes=notes,
        )
        created = self.repo.create(account)
        self._get_or_create_runtime_state(created.id)
        normalized_api_key = api_key.strip() if isinstance(api_key, str) and api_key.strip() else None
        self._attach_api_key(account_id=created.id, api_key=normalized_api_key)
        self.db.commit()
        self.db.refresh(created)
        state = self._get_or_create_runtime_state(created.id)
        return self._to_read(created, state)

    def list_accounts(self) -> list[TavilyAccountRead]:
        rows = (
            self.db.query(TavilyAccount, TavilyAccountRuntimeState)
            .outerjoin(TavilyAccountRuntimeState, TavilyAccountRuntimeState.account_id == TavilyAccount.id)
            .all()
        )
        return [self._to_read(account, state) for account, state in rows]

    def update_status(self, account_id: str, status: str) -> TavilyAccountRead:
        account = self.repo.get(account_id)
        if account is None:
            raise ValueError("Account not found")

        normalized = status.strip().lower()
        if normalized not in {"active", "disabled"}:
            raise ValueError("Unsupported status")

        state = self._get_or_create_runtime_state(account.id)
        state.status = normalized
        self.db.commit()
        self.db.refresh(account)
        self.db.refresh(state)
        return self._to_read(account, state)

    def delete_account(self, account_id: str) -> None:
        account = self.repo.get(account_id)
        if account is None:
            raise ValueError("Account not found")
        self.db.delete(account)
        self.db.commit()

    def import_accounts(
        self,
        *,
        file_type: str,
        content: str,
        merge_mode: str = "skip",
    ) -> dict[str, Any]:
        if merge_mode not in {"skip", "overwrite", "error"}:
            raise ValueError("Unsupported merge_mode")

        rows, parse_errors = self._parse_rows(file_type=file_type, content=content)
        stats = {
            "total": len(rows) + len(parse_errors),
            "created": 0,
            "updated": 0,
            "skipped": 0,
            "failed": 0,
            "api_keys_created": 0,
        }
        errors: list[dict[str, Any]] = list(parse_errors)
        stats["failed"] = len(parse_errors)

        if merge_mode == "error":
            try:
                for row in rows:
                    result, created_keys = self._upsert_row(row=row, merge_mode=merge_mode)
                    stats[result] += 1
                    stats["api_keys_created"] += created_keys
                self.db.commit()
            except ValueError as exc:
                self.db.rollback()
                stats["failed"] = 1
                errors.append({"row": getattr(exc, "row", 0), "email": getattr(exc, "email", None), "reason": str(exc)})
            return {"stats": stats, "errors": errors}

        for row in rows:
            try:
                result, created_keys = self._upsert_row(row=row, merge_mode=merge_mode)
                stats[result] += 1
                stats["api_keys_created"] += created_keys
                self.db.commit()
            except ValueError as exc:
                self.db.rollback()
                stats["failed"] += 1
                errors.append({"row": row.row, "email": row.email, "reason": str(exc)})
            except Exception as exc:  # noqa: BLE001
                self.db.rollback()
                stats["failed"] += 1
                errors.append({"row": row.row, "email": row.email, "reason": f"Unexpected error: {exc}"})

        return {"stats": stats, "errors": errors}

    def _parse_rows(self, *, file_type: str, content: str) -> tuple[list[_ParsedImportRow], list[dict[str, Any]]]:
        if file_type == "json":
            raw_rows, parse_errors = self._parse_json_rows(content)
        elif file_type == "csv":
            raw_rows, parse_errors = self._parse_csv_rows(content)
        else:
            raise ValueError("Unsupported file_type")

        # 合并同文件重复邮箱，避免内部重复冲突。
        deduped: dict[str, _ParsedImportRow] = {}
        for row in raw_rows:
            key = row.email.lower()
            if key not in deduped:
                deduped[key] = row
                continue

            prev = deduped[key]
            deduped[key] = _ParsedImportRow(
                row=prev.row,
                email=prev.email,
                password=row.password or prev.password,
                api_key=row.api_key or prev.api_key,
                notes=row.notes if row.notes is not None else prev.notes,
                source=row.source or prev.source,
            )

        return list(deduped.values()), parse_errors

    def _parse_json_rows(self, content: str) -> tuple[list[_ParsedImportRow], list[dict[str, Any]]]:
        try:
            payload = json.loads(content)
        except json.JSONDecodeError as exc:
            raise ValueError(f"Invalid JSON: {exc.msg}") from exc

        if not isinstance(payload, list):
            raise ValueError("JSON content must be an array")

        rows: list[_ParsedImportRow] = []
        errors: list[dict[str, Any]] = []
        for idx, item in enumerate(payload, start=1):
            try:
                rows.append(self._normalize_row(item=item, row=idx))
            except ValueError as exc:
                errors.append(
                    {
                        "row": idx,
                        "email": item.get("email") if isinstance(item, dict) else None,
                        "reason": str(exc),
                    }
                )
        return rows, errors

    def _parse_csv_rows(self, content: str) -> tuple[list[_ParsedImportRow], list[dict[str, Any]]]:
        reader = csv.DictReader(io.StringIO(content))
        expected_headers = {"email", "password", "api_key", "notes", "source"}
        current_headers = set(reader.fieldnames or [])
        compatible_headers = {"email", "password", "tokens", "notes", "source"}
        if not expected_headers.issubset(current_headers) and not compatible_headers.issubset(current_headers):
            raise ValueError("CSV header missing required fields: email,password,api_key,notes,source")

        rows: list[_ParsedImportRow] = []
        errors: list[dict[str, Any]] = []
        for line_no, item in enumerate(reader, start=2):
            try:
                rows.append(self._normalize_row(item=item, row=line_no))
            except ValueError as exc:
                errors.append(
                    {
                        "row": line_no,
                        "email": item.get("email") if isinstance(item, dict) else None,
                        "reason": str(exc),
                    }
                )
        return rows, errors

    def _normalize_row(self, *, item: Any, row: int) -> _ParsedImportRow:
        if not isinstance(item, dict):
            raise ValueError(f"Row {row}: item must be object")

        email_raw = str(item.get("email", "")).strip().lower()
        if not email_raw or not EMAIL_RE.match(email_raw):
            raise ValueError(f"Invalid email at row {row}")

        password_raw = item.get("password")
        password = str(password_raw).strip() if isinstance(password_raw, str) and password_raw.strip() else None

        api_key: str | None = None
        api_key_raw = item.get("api_key")
        if isinstance(api_key_raw, str):
            api_key = api_key_raw.strip() or None

        # backward compatible with legacy `tokens`
        if api_key is None and "tokens" in item:
            tokens_raw = item.get("tokens")
            if isinstance(tokens_raw, list):
                cleaned = [str(token).strip() for token in tokens_raw if str(token).strip()]
                if len(cleaned) > 1:
                    raise ValueError(f"Only single api_key is supported at row {row}")
                api_key = cleaned[0] if cleaned else None
            elif isinstance(tokens_raw, str):
                segments = [segment.strip() for segment in tokens_raw.split("|") if segment.strip()]
                if len(segments) > 1:
                    raise ValueError(f"Only single api_key is supported at row {row}")
                api_key = segments[0] if segments else None
            elif tokens_raw not in (None, ""):
                raise ValueError(f"Invalid api_key at row {row}")

        notes_raw = item.get("notes")
        notes = str(notes_raw).strip() if isinstance(notes_raw, str) and notes_raw.strip() else None
        source_raw = item.get("source")
        source = str(source_raw).strip() if isinstance(source_raw, str) and source_raw.strip() else "import"

        return _ParsedImportRow(
            row=row,
            email=email_raw,
            password=password,
            api_key=api_key,
            notes=notes,
            source=source,
        )

    def _upsert_row(self, *, row: _ParsedImportRow, merge_mode: str) -> tuple[str, int]:
        account = self.db.execute(select(TavilyAccount).where(TavilyAccount.email == row.email)).scalar_one_or_none()
        if account is None:
            if row.password is None:
                raise ValueError("Password is required for new account")
            account = TavilyAccount(
                email=row.email,
                password_encrypted=self.crypto.encrypt(row.password),
                source=row.source,
                notes=row.notes,
            )
            self.db.add(account)
            self.db.flush()
            self._get_or_create_runtime_state(account.id)
            created_keys = self._attach_api_key(account_id=account.id, api_key=row.api_key)
            return "created", created_keys

        if merge_mode == "error":
            exc = ValueError("Account already exists")
            setattr(exc, "row", row.row)
            setattr(exc, "email", row.email)
            raise exc

        if merge_mode == "overwrite":
            if row.password is not None:
                account.password_encrypted = self.crypto.encrypt(row.password)
            account.notes = row.notes
            account.source = row.source
            created_keys = self._attach_api_key(account_id=account.id, api_key=row.api_key)
            return "updated", created_keys

        # skip: 保留现有账号字段，仅补充新 API Key
        created_keys = self._attach_api_key(account_id=account.id, api_key=row.api_key)
        return "skipped", created_keys

    def _attach_api_key(self, *, account_id: str, api_key: str | None) -> int:
        if not api_key:
            return 0

        existing_tokens = (
            self.db.execute(select(TavilyToken).where(TavilyToken.account_id == account_id)).scalars().all()
        )
        existing_plain = set()
        for token in existing_tokens:
            try:
                existing_plain.add(self.crypto.decrypt(token.token_encrypted))
            except Exception:  # noqa: BLE001
                # 若历史数据异常，则退化为使用掩码去重，避免导入中断。
                existing_plain.add(token.token_masked)

        active_exists = any(token.is_active for token in existing_tokens)
        created = 0
        if api_key in existing_plain:
            return 0

        entity = TavilyToken(
            account_id=account_id,
            token_encrypted=self.crypto.encrypt(api_key),
            token_masked=mask_token(api_key),
            is_active=not active_exists,
        )
        self.db.add(entity)
        created += 1
        return created

    def _get_or_create_runtime_state(self, account_id: str) -> TavilyAccountRuntimeState:
        state = self.db.get(TavilyAccountRuntimeState, account_id)
        if state is not None:
            return state
        state = TavilyAccountRuntimeState(account_id=account_id)
        self.db.add(state)
        self.db.flush()
        return state

    @staticmethod
    def _to_read(account: TavilyAccount, state: TavilyAccountRuntimeState | None) -> TavilyAccountRead:
        return TavilyAccountRead(
            id=account.id,
            email=account.email,
            status=state.status if state is not None else "active",
            health_status=state.health_status if state is not None else "unknown",
            fail_count=int(state.fail_count or 0) if state is not None else 0,
            usage_plan=state.usage_plan if state is not None else None,
            usage_account_used=state.usage_account_used if state is not None else None,
            usage_account_limit=state.usage_account_limit if state is not None else None,
            usage_account_remaining=state.usage_account_remaining if state is not None else None,
            usage_synced_at=state.usage_synced_at if state is not None else None,
            usage_sync_error=state.usage_sync_error if state is not None else None,
            source=account.source,
            notes=account.notes,
            created_at=account.created_at,
            updated_at=account.updated_at,
        )
