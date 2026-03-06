"""
New API 余额查询操作
"""

from datetime import datetime, timedelta
from typing import Any

import httpx

from src.core.cache_service import CacheService
from src.core.logger import logger
from src.services.provider_ops.actions.balance import BalanceAction
from src.services.provider_ops.types import ActionResult
from src.services.provider_ops.types import ActionStatus
from src.services.provider_ops.types import BalanceInfo


class NewApiBalanceAction(BalanceAction):
    """
    New API 风格的余额查询

    特点：
    - 使用 /api/user/self 端点
    - quota 单位是 1/500000 美元
    - 支持查询前自动签到（通过基类的模板方法）
    """

    display_name = "查询余额"
    description = "查询 New API 账户余额信息"
    CHECKIN_CACHE_PREFIX = "provider_ops:checkin:done"
    _ALREADY_CHECKED_INDICATORS = ("already", "已签到", "已经签到", "今日已签", "重复签到")
    _AUTH_FAIL_INDICATORS = ("未登录", "请登录", "login", "unauthorized", "无权限", "权限不足")
    _MANUAL_VERIFICATION_INDICATORS = ("turnstile", "captcha", "验证码", "校验", "verify")

    async def execute(self, client: httpx.AsyncClient):
        """
        执行操作。

        checkin_only=True 时仅执行签到，不进行余额查询（用于站点管理手动签到任务）。
        """
        if not self.config.get("checkin_only"):
            return await super().execute(client)

        checkin_result = await self._do_checkin(client)
        if checkin_result is None:
            return self._make_success_result(
                data={"checkin_success": None},
                message="未执行签到",
            )

        if checkin_result.get("cookie_expired"):
            return self._make_error_result(
                ActionStatus.AUTH_EXPIRED,
                checkin_result.get("message") or "Cookie 已失效",
            )

        success = checkin_result.get("success")
        message = checkin_result.get("message") or ""
        if success is False:
            if checkin_result.get("auth_failed"):
                data = {
                    "manual_verification_required": bool(
                        checkin_result.get("needs_manual_verification")
                    )
                }
                return ActionResult(
                    status=ActionStatus.AUTH_FAILED,
                    action_type=self.action_type,
                    data=data,
                    message=message or "签到认证失败",
                    cache_ttl_seconds=0,
                )
            return self._make_error_result(
                ActionStatus.UNKNOWN_ERROR,
                message or "签到失败",
            )

        return self._make_success_result(
            data={"checkin_success": success},
            message=message or "签到成功",
        )

    def _parse_balance(
        self,
        data: Any,
    ) -> BalanceInfo:
        """解析 New API 余额信息"""
        # New API 响应格式: {"success": true, "data": {...}}
        user_data = data.get("data", {}) if isinstance(data, dict) else {}

        # 获取 quota 除数（默认 500000，New API 的标准）
        quota_divisor = self.config.get("quota_divisor", 500000)

        # 提取原始值
        # 注意：New API 中 quota 是剩余额度（total_available），不是总额度
        raw_quota = self._to_float(user_data.get("quota"))
        raw_used = self._to_float(user_data.get("used_quota"))

        # 转换为美元
        total_available = raw_quota / quota_divisor if raw_quota is not None else None
        total_used = raw_used / quota_divisor if raw_used is not None else None

        return self._create_balance_info(
            total_available=total_available,
            total_used=total_used,
            currency=self.config.get("currency", "USD"),
        )

    async def _do_checkin(self, client: httpx.AsyncClient) -> dict[str, Any] | None:
        """
        执行签到（静默，不抛出异常）

        New API 签到通常需要认证（Cookie 或 API Key + New-Api-User）。
        失败时仅记录日志，不影响余额查询。

        Returns:
            签到结果字典，包含 success 和 message 字段；
            如果功能未开放返回 None；
            如果 Cookie 失效返回 {"cookie_expired": True}
        """
        site = client.base_url.host or str(client.base_url)
        checkin_endpoint = self.config.get("checkin_endpoint", "/api/user/checkin")
        checkin_cache_key = self._build_checkin_cache_key(site)

        # 签到前先做当日缓存检查，避免重复调用签到接口
        if await CacheService.get(checkin_cache_key):
            logger.debug(f"[{site}] 已命中当日签到缓存，跳过重复签到请求")
            return {"success": None, "message": "今日已签到（缓存）"}

        # 检查认证信息（通过 service 层注入的标志）
        has_cookie = self.config.get("_has_cookie", False)
        has_user_id = self.config.get("_has_user_id", False)
        if not has_cookie and not has_user_id:
            logger.debug(f"[{site}] 未配置 Cookie 且缺少用户 ID，无法使用 access_token 签到")
            return {"success": False, "message": "access_token 签到需要 user_id"}

        # access_token 模式下，先查询本月签到状态，避免重复调用 checkin 接口导致 401/风控。
        if not has_cookie:
            checked_in_today = await self._fetch_checked_in_today_status(client, site)
            if checked_in_today is True:
                await CacheService.set(
                    checkin_cache_key, {"done": True}, self._seconds_until_next_day()
                )
                logger.debug(f"[{site}] 预检查显示今日已签到，跳过 checkin 接口调用")
                return {"success": None, "message": "今日已签到（状态接口）"}

        try:
            response = await client.post(checkin_endpoint, json={})

            # 404 表示签到功能未开放
            if response.status_code == 404:
                logger.debug(f"[{site}] 签到功能未开放")
                return None

            # 401/403 通常表示未授权；Cookie 模式下大多意味着 Cookie 已失效
            if response.status_code in (401, 403):
                if has_cookie:
                    logger.warning(f"[{site}] Cookie 已失效（签到返回 {response.status_code}）")
                    return {"cookie_expired": True, "message": "Cookie 已失效"}
                # access_token 模式下，部分站点在已签到场景会返回 401/403（风控/重复签到保护），
                # 再次用状态接口确认，避免误判为认证失败。
                checked_in_today = await self._fetch_checked_in_today_status(client, site)
                if checked_in_today is True:
                    await CacheService.set(
                        checkin_cache_key, {"done": True}, self._seconds_until_next_day()
                    )
                    logger.debug(
                        f"[{site}] access_token checkin 返回 {response.status_code}，但状态接口显示已签到"
                    )
                    return {"success": None, "message": "今日已签到（状态接口）"}
                logger.warning(f"[{site}] access_token 签到认证失败（{response.status_code}）")
                response_message = self._extract_response_message(response)
                needs_manual_verification = self._contains_manual_verification_signal(
                    response_message
                )
                return {
                    "success": False,
                    "message": f"access_token 签到认证失败（{response.status_code}）",
                    "auth_failed": True,
                    "needs_manual_verification": needs_manual_verification,
                }

            try:
                data = response.json()
                message = data.get("message", "")
                success = data.get("success", False)
                message_lower = str(message).lower()

                if success:
                    logger.debug(f"[{site}] 签到成功: {message}")
                    await CacheService.set(
                        checkin_cache_key, {"done": True}, self._seconds_until_next_day()
                    )
                    return {"success": True, "message": message or "签到成功"}
                else:
                    # 检查是否是"已签到"的情况
                    is_already = any(
                        ind.lower() in message_lower for ind in self._ALREADY_CHECKED_INDICATORS
                    )
                    if is_already:
                        logger.debug(f"[{site}] 今日已签到: {message}")
                        await CacheService.set(
                            checkin_cache_key, {"done": True}, self._seconds_until_next_day()
                        )
                        return {"success": None, "message": message or "今日已签到"}

                    # 检查是否是认证失败（未登录、无权限、验证码等）
                    is_auth_fail = any(
                        ind.lower() in message_lower for ind in self._AUTH_FAIL_INDICATORS
                    ) or self._contains_manual_verification_signal(message)
                    if is_auth_fail:
                        # Cookie 模式下，这类提示通常意味着 Cookie 已失效或需要重新登录。
                        if has_cookie:
                            logger.warning(f"[{site}] Cookie 已失效（签到认证失败）: {message}")
                            return {"cookie_expired": True, "message": message or "Cookie 已失效"}

                        logger.warning(f"[{site}] access_token 签到认证失败: {message}")
                        return {
                            "success": False,
                            "message": message or "access_token 签到认证失败",
                            "auth_failed": True,
                            "needs_manual_verification": self._contains_manual_verification_signal(
                                message
                            ),
                        }

                    # 其他失败情况
                    logger.debug(f"[{site}] 签到失败: {message}")
                    return {"success": False, "message": message or "签到失败"}
            except Exception as e:
                content_type = response.headers.get("content-type", "")
                body_preview = response.text[:120].replace("\n", " ").strip()
                logger.debug(
                    f"[{site}] 签到响应解析失败: {e}, status={response.status_code}, "
                    f"content_type={content_type}, body_preview={body_preview}"
                )
                return {"success": False, "message": "响应解析失败"}

        except Exception as e:
            # 签到失败不影响余额查询
            logger.debug(f"[{site}] 签到请求失败（不影响余额查询）: {e}")
            return None

    async def _fetch_checked_in_today_status(
        self,
        client: httpx.AsyncClient,
        site: str,
    ) -> bool | None:
        """查询本月签到状态，返回今日是否已签到。"""
        month = datetime.now().strftime("%Y-%m")
        endpoint = f"/api/user/checkin?month={month}"
        try:
            response = await client.get(endpoint)
            if response.status_code in (401, 403, 404, 500):
                logger.debug(
                    f"[{site}] 签到状态接口返回异常: status={response.status_code}, endpoint={endpoint}"
                )
                return None
            payload = response.json()
            stats = payload.get("data", {}).get("stats", {}) if isinstance(payload, dict) else {}
            checked_in_today = bool(stats.get("checked_in_today"))
            logger.debug(f"[{site}] 签到状态接口: checked_in_today={checked_in_today}")
            return checked_in_today
        except Exception as exc:
            logger.debug(f"[{site}] 查询签到状态失败: {exc}")
            return None

    def _build_checkin_cache_key(self, site: str) -> str:
        provider_id = str(self.config.get("_provider_id") or "").strip() or site
        day = datetime.now().strftime("%Y%m%d")
        return f"{self.CHECKIN_CACHE_PREFIX}:{provider_id}:{day}"

    def _contains_manual_verification_signal(self, message: str | None) -> bool:
        text = str(message or "").strip().lower()
        if not text:
            return False
        return any(ind in text for ind in self._MANUAL_VERIFICATION_INDICATORS)

    def _extract_response_message(self, response: httpx.Response) -> str:
        try:
            payload = response.json()
            if isinstance(payload, dict):
                return str(payload.get("message") or "")
        except Exception:
            pass
        return str(response.text or "")

    @staticmethod
    def _seconds_until_next_day() -> int:
        now = datetime.now()
        tomorrow = (now + timedelta(days=1)).replace(hour=0, minute=0, second=0, microsecond=0)
        return max(60, int((tomorrow - now).total_seconds()))

    @classmethod
    def get_config_schema(cls) -> dict[str, Any]:
        """获取操作配置 schema"""
        return {
            "type": "object",
            "properties": {
                "endpoint": {
                    "type": "string",
                    "title": "API 路径",
                    "description": "余额查询 API 路径",
                    "default": "/api/user/self",
                },
                "method": {
                    "type": "string",
                    "title": "请求方法",
                    "enum": ["GET", "POST"],
                    "default": "GET",
                },
                "quota_divisor": {
                    "type": "number",
                    "title": "额度除数",
                    "description": "将原始额度值转换为美元的除数",
                    "default": 500000,
                },
                "currency": {
                    "type": "string",
                    "title": "货币单位",
                    "default": "USD",
                },
            },
            "required": [],
        }
