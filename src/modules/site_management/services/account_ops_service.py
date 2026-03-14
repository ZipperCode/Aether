"""Account operations service -- checkin / balance queries for SiteAccount.

Decoupled from the legacy Provider model; all configuration is resolved
exclusively from the SiteAccount's own fields and ``config`` JSON column.
"""
from __future__ import annotations

from dataclasses import asdict, is_dataclass
from datetime import datetime, timezone
from typing import TYPE_CHECKING, Any

from src.core.crypto import CryptoService
from src.modules.site_management.models import SiteAccount
from src.services.provider_ops.execution_engine import OpsExecutionEngine, OpsExecutionTarget
from src.services.provider_ops.types import (
    ActionResult,
    ActionStatus,
    ConnectorAuthType,
    ProviderActionType,
)

if TYPE_CHECKING:
    from src.services.provider_ops.architectures.base import ProviderConnector


class AccountOpsService:
    """站点账号操作服务（签到/余额），不依赖 Provider。"""

    _CHECKIN_NOT_SUPPORTED_INDICATORS = (
        "签到功能未启用",
        "签到功能未开放",
        "签到未开放",
        "not enabled",
        "not support",
    )

    def __init__(self, db: Any):
        import asyncio

        try:
            asyncio.get_running_loop()
        except RuntimeError:
            try:
                asyncio.get_event_loop()
            except RuntimeError:
                asyncio.set_event_loop(asyncio.new_event_loop())
        self.db = db
        self.crypto = CryptoService()
        self.execution_engine = OpsExecutionEngine()
        self._connectors: dict[str, ProviderConnector] = {}

    # ------------------------------------------------------------------
    # public API
    # ------------------------------------------------------------------

    async def execute_action(
        self,
        account_id: str,
        action_type: ProviderActionType,
        action_config: dict[str, Any] | None = None,
    ) -> ActionResult:
        account = self._get_account(account_id)
        if not account:
            return ActionResult(
                status=ActionStatus.NOT_CONFIGURED,
                action_type=action_type,
                message="站点账号不存在",
            )
        if not account.is_active:
            return ActionResult(
                status=ActionStatus.NOT_CONFIGURED,
                action_type=action_type,
                message="站点账号已停用",
            )

        target = self._build_target(account)
        if not target:
            return ActionResult(
                status=ActionStatus.NOT_CONFIGURED,
                action_type=action_type,
                message="站点账号缺少可用配置",
            )

        connector = await self._ensure_connected(target)
        if not connector:
            return ActionResult(
                status=ActionStatus.AUTH_FAILED,
                action_type=action_type,
                message="连接失败，请检查账号凭据",
            )

        result = await self.execution_engine.execute(
            connector=connector,
            target=target,
            action_type=action_type,
            action_config=action_config,
        )
        self._persist_last_result(account, action_type=action_type, result=result)
        self.db.commit()
        return result

    async def query_balance(
        self,
        account_id: str,
        action_config: dict[str, Any] | None = None,
    ) -> ActionResult:
        return await self.execute_action(
            account_id=account_id,
            action_type=ProviderActionType.QUERY_BALANCE,
            action_config=action_config,
        )

    async def checkin(
        self,
        account_id: str,
        action_config: dict[str, Any] | None = None,
    ) -> ActionResult:
        account = self._get_account(account_id)
        if not account:
            return ActionResult(
                status=ActionStatus.NOT_CONFIGURED,
                action_type=ProviderActionType.CHECKIN,
                message="站点账号不存在",
            )

        architecture_id = str(account.architecture_id or "").strip().lower()
        if architecture_id == "new_api":
            merged = dict(action_config or {})
            merged["checkin_only"] = True
            result = await self.execute_action(
                account_id=account_id,
                action_type=ProviderActionType.QUERY_BALANCE,
                action_config=merged,
            )
            checkin_result = self._to_checkin_result_from_new_api(result)
            self._persist_last_result(
                account,
                action_type=ProviderActionType.CHECKIN,
                result=checkin_result,
            )
            self.db.commit()
            return checkin_result

        return await self.execute_action(
            account_id=account_id,
            action_type=ProviderActionType.CHECKIN,
            action_config=action_config,
        )

    # ------------------------------------------------------------------
    # account lookup
    # ------------------------------------------------------------------

    def _get_account(self, account_id: str) -> SiteAccount | None:
        return self.db.query(SiteAccount).filter(SiteAccount.id == account_id).first()

    # ------------------------------------------------------------------
    # target building (no Provider lookups)
    # ------------------------------------------------------------------

    def _build_target(self, account: SiteAccount) -> OpsExecutionTarget | None:
        account_config = account.config if isinstance(account.config, dict) else {}

        architecture_id = str(
            account.architecture_id
            or account_config.get("architecture_id")
            or self._infer_default_architecture(account.auth_type)
        ).strip()
        if not architecture_id:
            return None

        base_url = str(
            account.base_url
            or account.site_url
            or account_config.get("base_url")
            or ""
        ).strip()
        if not base_url:
            return None

        auth_type = self._resolve_auth_type(
            architecture_id=architecture_id,
            account_auth_type=str(account.auth_type or "").strip().lower(),
            account_config=account_config,
        )
        if auth_type is None:
            return None

        connector_config = self._resolve_connector_config(account_config)
        actions = self._resolve_actions(account_config)
        credentials = self._resolve_credentials(account)

        return OpsExecutionTarget(
            target_id=str(account.id),
            architecture_id=architecture_id,
            base_url=base_url,
            auth_type=auth_type,
            connector_config=connector_config,
            credentials=credentials,
            actions=actions,
        )

    # ------------------------------------------------------------------
    # connector management
    # ------------------------------------------------------------------

    async def _ensure_connected(self, target: OpsExecutionTarget) -> ProviderConnector | None:
        connector = self._connectors.get(target.target_id)
        if connector:
            try:
                if await connector.is_authenticated():
                    return connector
            except Exception:
                pass
            self._connectors.pop(target.target_id, None)

        try:
            connector = self.execution_engine.create_connector(target)
        except Exception:
            return None

        success = await connector.connect(target.credentials)
        if not success:
            return None
        self._connectors[target.target_id] = connector
        return connector

    # ------------------------------------------------------------------
    # result persistence
    # ------------------------------------------------------------------

    def _persist_last_result(
        self,
        account: SiteAccount,
        *,
        action_type: ProviderActionType,
        result: ActionResult,
    ) -> None:
        now = datetime.now(timezone.utc)
        if action_type == ProviderActionType.QUERY_BALANCE:
            account.last_balance_status = result.status.value
            account.last_balance_message = result.message
            account.last_balance_at = now
            total, currency = self._extract_balance_total_and_currency(result.data)
            account.last_balance_total = total
            account.last_balance_currency = currency
        elif action_type == ProviderActionType.CHECKIN:
            account.last_checkin_status = result.status.value
            account.last_checkin_message = result.message
            account.last_checkin_at = now
        account.updated_at = now

    # ------------------------------------------------------------------
    # static helpers
    # ------------------------------------------------------------------

    @staticmethod
    def _extract_balance_total_and_currency(data: Any) -> tuple[float | None, str | None]:
        if data is None:
            return None, None
        if is_dataclass(data) and not isinstance(data, type):
            payload = asdict(data)
        elif isinstance(data, dict):
            payload = data
        else:
            payload = {}

        total = payload.get("total_available")
        if total is None:
            total = payload.get("balance")
        try:
            total_val = float(total) if total is not None else None
        except (TypeError, ValueError):
            total_val = None
        currency = payload.get("currency")
        if currency is not None:
            currency = str(currency)
        return total_val, currency

    @staticmethod
    def _infer_default_architecture(auth_type: str | None) -> str:
        normalized = str(auth_type or "").strip().lower()
        if normalized in {"cookie", "access_token"}:
            return "new_api"
        return "generic_api"

    # ------------------------------------------------------------------
    # resolve helpers (account-only, no Provider fallback)
    # ------------------------------------------------------------------

    @staticmethod
    def _resolve_auth_type(
        *,
        architecture_id: str,
        account_auth_type: str,
        account_config: dict[str, Any],
    ) -> ConnectorAuthType | None:
        if architecture_id == "new_api":
            return ConnectorAuthType.API_KEY

        auth_type_value = (
            account_config.get("connector", {}).get("auth_type")
            if isinstance(account_config.get("connector"), dict)
            else None
        )
        if auth_type_value is None:
            if account_auth_type in {"cookie", "access_token"}:
                auth_type_value = "api_key"
            elif account_auth_type:
                auth_type_value = account_auth_type
            else:
                auth_type_value = "api_key"

        try:
            return ConnectorAuthType(str(auth_type_value))
        except Exception:
            return None

    @staticmethod
    def _resolve_connector_config(
        account_config: dict[str, Any],
    ) -> dict[str, Any]:
        account_connector = account_config.get("connector", {})
        if isinstance(account_connector, dict):
            account_connector_config = account_connector.get("config")
            if isinstance(account_connector_config, dict):
                return dict(account_connector_config)
        return {}

    @staticmethod
    def _resolve_actions(
        account_config: dict[str, Any],
    ) -> dict[str, dict[str, Any]]:
        account_actions = account_config.get("actions")
        if isinstance(account_actions, dict):
            return dict(account_actions)
        return {}

    def _resolve_credentials(
        self,
        account: SiteAccount,
    ) -> dict[str, Any]:
        if isinstance(account.credentials, dict) and account.credentials:
            return self._decrypt_provider_credentials(account.credentials)
        return {}

    # ------------------------------------------------------------------
    # checkin helpers
    # ------------------------------------------------------------------

    @staticmethod
    def _to_checkin_result_from_new_api(result: ActionResult) -> ActionResult:
        payload = result.data if isinstance(result.data, dict) else {}
        checkin_success = payload.get("checkin_success")
        message = str(result.message or "")

        if result.status != ActionStatus.SUCCESS:
            status = result.status
        elif AccountOpsService._contains_checkin_not_supported_signal(message):
            status = ActionStatus.NOT_SUPPORTED
        elif checkin_success is True:
            status = ActionStatus.SUCCESS
        elif checkin_success is None:
            status = ActionStatus.ALREADY_DONE
        else:
            status = ActionStatus.UNKNOWN_ERROR

        return ActionResult(
            status=status,
            action_type=ProviderActionType.CHECKIN,
            data=payload or None,
            message=result.message,
            executed_at=result.executed_at,
            response_time_ms=result.response_time_ms,
            raw_response=result.raw_response,
            cache_ttl_seconds=result.cache_ttl_seconds,
            retry_after_seconds=result.retry_after_seconds,
        )

    @staticmethod
    def _contains_checkin_not_supported_signal(message: str | None) -> bool:
        text = str(message or "").strip().lower()
        if not text:
            return False
        return any(ind in text for ind in AccountOpsService._CHECKIN_NOT_SUPPORTED_INDICATORS)

    # ------------------------------------------------------------------
    # crypto helper
    # ------------------------------------------------------------------

    def _decrypt_provider_credentials(self, credentials: dict[str, Any]) -> dict[str, Any]:
        sensitive_fields = {
            "api_key",
            "password",
            "refresh_token",
            "session_token",
            "session_cookie",
            "token_cookie",
            "auth_cookie",
            "cookie_string",
            "cookie",
        }
        result: dict[str, Any] = {}
        for key, value in credentials.items():
            if key in sensitive_fields and isinstance(value, str):
                try:
                    result[key] = self.crypto.decrypt(value)
                except Exception:
                    result[key] = value
            else:
                result[key] = value
        return result
