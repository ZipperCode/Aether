from __future__ import annotations

from src.core.api_format.conversion.internal import (
    InternalError,
    InternalRequest,
    InternalResponse,
    ThinkingConfig,
)
from src.core.api_format.conversion.stream_events import InternalStreamEvent
from src.core.api_format.transformers.base import TransformContext

_EFFORT_TO_BUDGET = {
    "low": 1024,
    "medium": 2048,
    "high": 4096,
    "xhigh": 8192,
}


class ReasoningTransformer:
    NAME = "reasoning"

    def transform_request(
        self,
        internal: InternalRequest,
        ctx: TransformContext,
    ) -> InternalRequest:
        provider_format = str(ctx.provider_format or "").strip().lower()

        effort = internal.extra.get("reasoning_effort")
        if (
            provider_format.startswith("claude:")
            and not internal.thinking
            and isinstance(effort, str)
            and effort in _EFFORT_TO_BUDGET
        ):
            internal.thinking = ThinkingConfig(
                enabled=True,
                budget_tokens=_EFFORT_TO_BUDGET[effort],
                extra={"reasoning_effort": effort},
            )

        if (
            provider_format.startswith("openai:")
            and "reasoning_effort" not in internal.extra
            and internal.thinking
            and internal.thinking.enabled
            and internal.thinking.budget_tokens is not None
        ):
            budget = internal.thinking.budget_tokens
            if budget <= 1024:
                effort = "low"
            elif budget <= 2048:
                effort = "medium"
            else:
                effort = "high"
            internal.extra["reasoning_effort"] = effort

        has_reasoning_controls = (
            isinstance(internal.extra.get("reasoning_effort"), str)
            or (
                internal.thinking is not None
                and internal.thinking.enabled
            )
        )
        if has_reasoning_controls and not provider_format.startswith(("claude:", "gemini:", "openai:")):
            ctx.add_diagnostic(
                code="reasoning_not_supported",
                message="target format does not support reasoning controls",
                severity="warning",
                details={"provider_format": provider_format or None},
                transformer=self.NAME,
            )
        return internal

    def transform_response(
        self,
        internal: InternalResponse,
        ctx: TransformContext,
    ) -> InternalResponse:
        return internal

    def transform_stream_event(
        self,
        event: InternalStreamEvent,
        ctx: TransformContext,
    ) -> list[InternalStreamEvent]:
        return [event]

    def transform_error(
        self,
        internal: InternalError,
        ctx: TransformContext,
    ) -> InternalError:
        return internal
