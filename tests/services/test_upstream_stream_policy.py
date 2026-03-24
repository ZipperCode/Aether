from __future__ import annotations

import sys
import types
from dataclasses import dataclass
from types import SimpleNamespace

from src.services.provider.adapters.codex.context import (
    CodexRequestContext,
    set_codex_request_context,
)
from src.services.provider.stream_policy import (
    UpstreamStreamPolicy,
    enforce_stream_mode_for_upstream,
    get_upstream_stream_policy,
)


@dataclass
class _DummyEndpoint:
    api_format: str
    config: dict | None = None
    provider: object | None = None
    provider_id: str | None = None


class _FakeQuery:
    def __init__(self, row: object | None) -> None:
        self._row = row

    def filter(self, *_args: object, **_kwargs: object) -> "_FakeQuery":
        return self

    def first(self) -> object | None:
        return self._row


class _FakeSessionCtx:
    def __init__(self, row: object | None) -> None:
        self._row = row

    def __enter__(self) -> "_FakeSessionCtx":
        return self

    def __exit__(self, exc_type: object, exc: object, tb: object) -> bool:
        _ = exc_type, exc, tb
        return False

    def query(self, _model: object) -> _FakeQuery:
        return _FakeQuery(self._row)


def test_get_upstream_stream_policy_defaults_to_auto() -> None:
    ep = _DummyEndpoint(
        api_format="openai:chat", config=None, provider=SimpleNamespace(provider_type="custom")
    )
    assert get_upstream_stream_policy(ep) == UpstreamStreamPolicy.AUTO


def test_get_upstream_stream_policy_codex_openai_cli_forces_stream() -> None:
    ep = _DummyEndpoint(
        api_format="openai:cli",
        config=None,
        provider=SimpleNamespace(provider_type="codex"),
    )
    assert get_upstream_stream_policy(ep) == UpstreamStreamPolicy.FORCE_STREAM


def test_get_upstream_stream_policy_codex_ignores_force_non_stream_config() -> None:
    ep = _DummyEndpoint(
        api_format="openai:cli",
        config={"upstream_stream_policy": "force_non_stream"},
        provider=SimpleNamespace(provider_type="codex"),
    )
    assert get_upstream_stream_policy(ep) == UpstreamStreamPolicy.FORCE_STREAM


def test_get_upstream_stream_policy_codex_compact_forces_non_stream() -> None:
    ep = _DummyEndpoint(
        api_format="openai:cli",
        config=None,
        provider=SimpleNamespace(provider_type="codex"),
    )
    try:
        set_codex_request_context(CodexRequestContext(is_compact=True))
        assert get_upstream_stream_policy(ep) == UpstreamStreamPolicy.FORCE_NON_STREAM
    finally:
        set_codex_request_context(None)


def test_get_upstream_stream_policy_uses_explicit_provider_type_without_touching_endpoint_provider() -> (
    None
):
    class _DetachedEndpoint:
        api_format = "openai:cli"
        config = None

        @property
        def provider(self) -> object:
            raise RuntimeError("detached endpoint provider should not be lazy-loaded")

    ep = _DetachedEndpoint()

    assert (
        get_upstream_stream_policy(ep, provider_type="codex") == UpstreamStreamPolicy.FORCE_STREAM
    )


def test_get_upstream_stream_policy_codex_openai_compact_defaults_to_auto() -> None:
    ep = _DummyEndpoint(
        api_format="openai:compact",
        config=None,
        provider=SimpleNamespace(provider_type="codex"),
    )
    assert get_upstream_stream_policy(ep) == UpstreamStreamPolicy.AUTO


def test_get_upstream_stream_policy_uses_provider_id_lookup_without_touching_endpoint_provider(
    monkeypatch,
) -> None:
    class _DetachedEndpoint:
        api_format = "openai:cli"
        config = None
        provider_id = "provider-1"

        @property
        def provider(self) -> object:
            raise RuntimeError("detached endpoint provider should not be lazy-loaded")

    fake_provider = SimpleNamespace(id="provider-1", provider_type="codex", proxy=None)
    fake_database = types.ModuleType("src.database")
    fake_database.create_session = lambda: _FakeSessionCtx(fake_provider)
    fake_models = types.ModuleType("src.models.database")
    fake_models.Provider = type("Provider", (), {"id": "id"})

    monkeypatch.setitem(sys.modules, "src.database", fake_database)
    monkeypatch.setitem(sys.modules, "src.models.database", fake_models)

    assert get_upstream_stream_policy(_DetachedEndpoint()) == UpstreamStreamPolicy.FORCE_STREAM


def test_enforce_stream_mode_for_upstream_openai_chat_sets_stream_options_usage() -> None:
    body = {"stream": False}
    out = enforce_stream_mode_for_upstream(
        body,
        provider_api_format="openai:chat",
        upstream_is_stream=True,
    )
    assert out["stream"] is True
    assert out["stream_options"]["include_usage"] is True


def test_enforce_stream_mode_for_upstream_gemini_drops_stream_field() -> None:
    body = {"stream": True, "foo": "bar"}
    out = enforce_stream_mode_for_upstream(
        body,
        provider_api_format="gemini:chat",
        upstream_is_stream=False,
    )
    assert "stream" not in out
    assert out["foo"] == "bar"


def test_enforce_stream_mode_for_upstream_openai_compact_drops_stream_field() -> None:
    body = {"stream": True, "foo": "bar"}
    out = enforce_stream_mode_for_upstream(
        body,
        provider_api_format="openai:compact",
        upstream_is_stream=True,
    )
    assert "stream" not in out
    assert out["foo"] == "bar"


def test_enforce_stream_mode_for_upstream_codex_compact_keeps_stream_absent() -> None:
    body = {"stream": True, "foo": "bar"}
    try:
        set_codex_request_context(CodexRequestContext(is_compact=True))
        out = enforce_stream_mode_for_upstream(
            body,
            provider_api_format="openai:cli",
            upstream_is_stream=False,
        )
    finally:
        set_codex_request_context(None)

    assert "stream" not in out
    assert out["foo"] == "bar"
