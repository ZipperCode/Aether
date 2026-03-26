from __future__ import annotations

from src.core.api_format.conversion.internal import InstructionSegment, InternalMessage, InternalRequest, Role, TextBlock
from src.core.api_format.transformers.base import TransformContext
from src.core.api_format.transformers.builtin.cleancache import CleanCacheTransformer


def test_cleancache_transformer_removes_claude_cache_control_hints() -> None:
    transformer = CleanCacheTransformer()
    internal = InternalRequest(
        model="claude-3-7-sonnet",
        messages=[
            InternalMessage(
                role=Role.USER,
                content=[TextBlock(text="hello", extra={"cache_control": {"type": "ephemeral"}})],
            )
        ],
        instructions=[
            InstructionSegment(
                role=Role.SYSTEM,
                text="sys",
                extra={"cache_control": {"type": "ephemeral"}},
            )
        ],
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="claude:chat", provider_format="claude:chat"),
    )

    assert "cache_control" not in out.instructions[0].extra
    assert "cache_control" not in out.messages[0].content[0].extra


def test_cleancache_transformer_removes_gemini_cached_content() -> None:
    transformer = CleanCacheTransformer()
    internal = InternalRequest(
        model="gemini-2.5-pro",
        messages=[],
        extra={"gemini": {"cached_content": "cachedContents/abc"}, "other": {"keep": True}},
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="gemini:generate", provider_format="gemini:generate"),
    )

    assert "cached_content" not in out.extra["gemini"]
    assert out.extra["other"] == {"keep": True}


def test_cleancache_transformer_records_diagnostics_for_removed_cache_hints() -> None:
    transformer = CleanCacheTransformer()
    internal = InternalRequest(
        model="claude-3-7-sonnet",
        messages=[
            InternalMessage(
                role=Role.USER,
                content=[TextBlock(text="hello", extra={"cache_control": {"type": "ephemeral"}})],
            )
        ],
        instructions=[
            InstructionSegment(
                role=Role.SYSTEM,
                text="sys",
                extra={"cache_control": {"type": "ephemeral"}},
            )
        ],
        extra={"gemini": {"cached_content": "cachedContents/abc"}},
    )
    ctx = TransformContext(stage="request", client_format="claude:chat", provider_format="claude:chat")

    out = transformer.transform_request(internal, ctx)

    assert "cache_control" not in out.instructions[0].extra
    assert "cache_control" not in out.messages[0].content[0].extra
    assert "cached_content" not in out.extra["gemini"]
    assert ctx.diagnostics == [
        {
            "stage": "request",
            "transformer": "cleancache",
            "code": "cache_hint_removed",
            "message": "cache hint was removed from instruction segment",
            "severity": "info",
            "details": {"location": "instruction"},
        },
        {
            "stage": "request",
            "transformer": "cleancache",
            "code": "cache_hint_removed",
            "message": "cache hint was removed from content block",
            "severity": "info",
            "details": {"location": "message"},
        },
        {
            "stage": "request",
            "transformer": "cleancache",
            "code": "cache_hint_removed",
            "message": "cached content reference was removed from gemini extra payload",
            "severity": "info",
            "details": {"location": "gemini.cached_content"},
        },
    ]
