"""Per-service round-robin key pool with failover."""

from __future__ import annotations

from datetime import datetime, timezone
from threading import Lock

from sqlalchemy.orm import sessionmaker

from src.modules.search_pool_gateway.models import GatewayApiKey
from src.modules.search_pool_gateway.services.key_service import SUPPORTED_SERVICES


class ServiceKeyPool:
    def __init__(self, session_factory: sessionmaker, max_fails: int = 3) -> None:
        self._session_factory = session_factory
        self._max_fails = max(1, int(max_fails))
        self._lock = Lock()
        self._keys: dict[str, list[GatewayApiKey]] = {service: [] for service in SUPPORTED_SERVICES}
        self._indexes: dict[str, int] = {service: 0 for service in SUPPORTED_SERVICES}
        self._initialized: set[str] = set()

    def _normalize_service(self, service: str) -> str:
        service_norm = (service or "").strip().lower()
        if service_norm not in SUPPORTED_SERVICES:
            raise ValueError("unsupported service")
        return service_norm

    def reload(self, service: str | None = None) -> None:
        services = [self._normalize_service(service)] if service else sorted(SUPPORTED_SERVICES)
        with self._session_factory() as db:
            with self._lock:
                for item in services:
                    rows = (
                        db.query(GatewayApiKey)
                        .filter(GatewayApiKey.service == item, GatewayApiKey.active.is_(True))
                        .order_by(GatewayApiKey.created_at.asc())
                        .all()
                    )
                    self._keys[item] = rows
                    if self._indexes[item] >= len(rows):
                        self._indexes[item] = 0
                    self._initialized.add(item)

    def get_next_key(self, service: str = "tavily") -> GatewayApiKey | None:
        service_norm = self._normalize_service(service)
        if service_norm not in self._initialized:
            self.reload(service_norm)

        with self._lock:
            keys = self._keys[service_norm]
            if not keys:
                return None
            index = self._indexes[service_norm]
            key = keys[index]
            self._indexes[service_norm] = (index + 1) % len(keys)
            return key

    def report_result(self, service: str, key_id: str, *, success: bool) -> None:
        service_norm = self._normalize_service(service)
        now = datetime.now(timezone.utc)
        with self._session_factory() as db:
            row = db.get(GatewayApiKey, key_id)
            if row is None:
                return

            row.last_used_at = now
            if success:
                row.total_used = int(row.total_used or 0) + 1
                row.consecutive_fails = 0
            else:
                row.total_failed = int(row.total_failed or 0) + 1
                row.consecutive_fails = int(row.consecutive_fails or 0) + 1
                if row.consecutive_fails >= self._max_fails:
                    row.active = False

            db.flush()
            db.commit()

        self.reload(service_norm)
