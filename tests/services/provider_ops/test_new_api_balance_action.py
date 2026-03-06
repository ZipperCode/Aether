from __future__ import annotations

from dataclasses import dataclass
from json import JSONDecodeError
import sys
import types

import pytest

_service_stub = types.ModuleType("src.services.provider_ops.service")
_service_stub.ProviderOpsService = object  # type: ignore[attr-defined]
sys.modules.setdefault("src.services.provider_ops.service", _service_stub)

_cache_stub = types.ModuleType("src.core.cache_service")


class _CacheServiceStub:
    @staticmethod
    async def get(_key: str):
        return None

    @staticmethod
    async def set(_key: str, _value, _ttl_seconds: int = 60):
        return True


_cache_stub.CacheService = _CacheServiceStub  # type: ignore[attr-defined]
sys.modules.setdefault("src.core.cache_service", _cache_stub)

from src.services.provider_ops.actions.new_api_balance import NewApiBalanceAction
from src.services.provider_ops.types import ActionStatus


class _Resp:
    def __init__(
        self,
        status_code: int = 200,
        payload: dict | None = None,
        text: str = "<html>forbidden</html>",
    ) -> None:
        self.status_code = status_code
        self.headers = {"content-type": "text/html"}
        self.text = text
        self._payload = payload

    def json(self) -> dict:
        if self._payload is None:
            raise JSONDecodeError("Expecting value", "", 0)
        return self._payload


@dataclass
class _BaseURL:
    host: str


class _Client:
    def __init__(
        self,
        host: str = "example.com",
        response: _Resp | None = None,
        get_responses: list[_Resp] | None = None,
    ) -> None:
        self.base_url = _BaseURL(host=host)
        self.response = response or _Resp(status_code=200)
        self.get_responses = list(get_responses or [])
        self.post_calls = 0
        self.get_calls = 0

    async def post(self, _endpoint: str, **_kwargs) -> _Resp:
        self.post_calls += 1
        return self.response

    async def get(self, _endpoint: str, **_kwargs) -> _Resp:
        self.get_calls += 1
        if self.get_responses:
            return self.get_responses.pop(0)
        return _Resp(status_code=404)


@pytest.mark.asyncio
async def test_checkin_access_token_mode_requires_user_id() -> None:
    client = _Client()
    action = NewApiBalanceAction(config={"_has_cookie": False, "_has_user_id": False})
    result = await action._do_checkin(client)
    assert result == {"success": False, "message": "access_token 签到需要 user_id"}
    assert client.post_calls == 0


@pytest.mark.asyncio
async def test_checkin_non_json_response_fails_for_access_token_mode() -> None:
    client = _Client()
    action = NewApiBalanceAction(config={"_has_cookie": False, "_has_user_id": True})
    result = await action._do_checkin(client)
    assert result == {"success": False, "message": "响应解析失败"}
    assert client.post_calls == 1


@pytest.mark.asyncio
async def test_checkin_non_json_response_keeps_failure_for_cookie_mode() -> None:
    action = NewApiBalanceAction(config={"_has_cookie": True})
    result = await action._do_checkin(_Client())
    assert result == {"success": False, "message": "响应解析失败"}


@pytest.mark.asyncio
async def test_checkin_skips_request_when_already_checked_in_cache_hit(mocker) -> None:
    mocker.patch(
        "src.services.provider_ops.actions.new_api_balance.CacheService.get",
        new=mocker.AsyncMock(return_value={"done": True}),
    )
    set_mock = mocker.patch(
        "src.services.provider_ops.actions.new_api_balance.CacheService.set",
        new=mocker.AsyncMock(return_value=True),
    )
    client = _Client()
    action = NewApiBalanceAction(config={"_has_cookie": True, "_provider_id": "p-1"})

    result = await action._do_checkin(client)

    assert result == {"success": None, "message": "今日已签到（缓存）"}
    assert client.post_calls == 0
    set_mock.assert_not_awaited()


@pytest.mark.asyncio
async def test_checkin_success_writes_daily_cache(mocker) -> None:
    mocker.patch(
        "src.services.provider_ops.actions.new_api_balance.CacheService.get",
        new=mocker.AsyncMock(return_value=None),
    )
    set_mock = mocker.patch(
        "src.services.provider_ops.actions.new_api_balance.CacheService.set",
        new=mocker.AsyncMock(return_value=True),
    )
    client = _Client(response=_Resp(status_code=200, payload={"success": True, "message": "ok"}))
    action = NewApiBalanceAction(config={"_has_cookie": True, "_provider_id": "p-1"})

    result = await action._do_checkin(client)

    assert result == {"success": True, "message": "ok"}
    assert client.post_calls == 1
    set_mock.assert_awaited()


@pytest.mark.asyncio
async def test_execute_checkin_only_fails_when_no_cookie_and_no_user_id() -> None:
    client = _Client()
    action = NewApiBalanceAction(config={"checkin_only": True, "_has_cookie": False})

    result = await action.execute(client)

    assert result.status == ActionStatus.UNKNOWN_ERROR
    assert result.message == "access_token 签到需要 user_id"
    assert client.post_calls == 0


@pytest.mark.asyncio
async def test_checkin_access_token_skips_when_status_endpoint_already_checked(mocker) -> None:
    mocker.patch(
        "src.services.provider_ops.actions.new_api_balance.CacheService.get",
        new=mocker.AsyncMock(return_value=None),
    )
    set_mock = mocker.patch(
        "src.services.provider_ops.actions.new_api_balance.CacheService.set",
        new=mocker.AsyncMock(return_value=True),
    )
    client = _Client(
        response=_Resp(status_code=200, payload={"success": True, "message": "should not call post"}),
        get_responses=[
            _Resp(status_code=200, payload={"data": {"stats": {"checked_in_today": True}}}),
        ],
    )
    action = NewApiBalanceAction(config={"_has_cookie": False, "_has_user_id": True, "_provider_id": "p-1"})

    result = await action._do_checkin(client)

    assert result == {"success": None, "message": "今日已签到（状态接口）"}
    assert client.post_calls == 0
    assert client.get_calls == 1
    set_mock.assert_awaited()


@pytest.mark.asyncio
async def test_checkin_access_token_401_but_status_already_checked_returns_skipped(mocker) -> None:
    mocker.patch(
        "src.services.provider_ops.actions.new_api_balance.CacheService.get",
        new=mocker.AsyncMock(return_value=None),
    )
    set_mock = mocker.patch(
        "src.services.provider_ops.actions.new_api_balance.CacheService.set",
        new=mocker.AsyncMock(return_value=True),
    )
    client = _Client(
        response=_Resp(status_code=401, payload={"success": False, "message": "unauthorized"}),
        get_responses=[
            _Resp(status_code=200, payload={"data": {"stats": {"checked_in_today": False}}}),
            _Resp(status_code=200, payload={"data": {"stats": {"checked_in_today": True}}}),
        ],
    )
    action = NewApiBalanceAction(config={"_has_cookie": False, "_has_user_id": True, "_provider_id": "p-1"})

    result = await action._do_checkin(client)

    assert result == {"success": None, "message": "今日已签到（状态接口）"}
    assert client.post_calls == 1
    assert client.get_calls == 2
    set_mock.assert_awaited()


@pytest.mark.asyncio
async def test_checkin_access_token_401_not_checked_keeps_auth_failed_without_manual_verification(
    mocker,
) -> None:
    mocker.patch(
        "src.services.provider_ops.actions.new_api_balance.CacheService.get",
        new=mocker.AsyncMock(return_value=None),
    )
    set_mock = mocker.patch(
        "src.services.provider_ops.actions.new_api_balance.CacheService.set",
        new=mocker.AsyncMock(return_value=True),
    )
    client = _Client(
        response=_Resp(status_code=401, payload={"success": False, "message": "unauthorized"}),
        get_responses=[
            _Resp(status_code=200, payload={"data": {"stats": {"checked_in_today": False}}}),
            _Resp(status_code=200, payload={"data": {"stats": {"checked_in_today": False}}}),
        ],
    )
    action = NewApiBalanceAction(config={"checkin_only": True, "_has_cookie": False, "_has_user_id": True})

    result = await action.execute(client)

    assert result.status == ActionStatus.AUTH_FAILED
    assert result.message == "access_token 签到认证失败（401）"
    assert isinstance(result.data, dict)
    assert result.data.get("manual_verification_required") is False
    set_mock.assert_not_awaited()


@pytest.mark.asyncio
async def test_checkin_access_token_turnstile_message_marks_manual_verification(
    mocker,
) -> None:
    mocker.patch(
        "src.services.provider_ops.actions.new_api_balance.CacheService.get",
        new=mocker.AsyncMock(return_value=None),
    )
    client = _Client(
        response=_Resp(status_code=200, payload={"success": False, "message": "Turnstile token 校验失败"}),
        get_responses=[
            _Resp(status_code=200, payload={"data": {"stats": {"checked_in_today": False}}}),
        ],
    )
    action = NewApiBalanceAction(config={"checkin_only": True, "_has_cookie": False, "_has_user_id": True})

    result = await action.execute(client)

    assert result.status == ActionStatus.AUTH_FAILED
    assert result.message == "Turnstile token 校验失败"
    assert isinstance(result.data, dict)
    assert result.data.get("manual_verification_required") is True
