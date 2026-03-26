from __future__ import annotations

from src.core.api_format.conversion.internal import InternalError, InternalRequest, InternalResponse
from src.core.api_format.conversion.stream_events import InternalStreamEvent
from src.core.api_format.transformers.base import TransformContext


class CleanCacheTransformer:
    NAME = "cleancache"

    def transform_request(
        self,
        internal: InternalRequest,
        ctx: TransformContext,
    ) -> InternalRequest:
        for segment in internal.instructions:
            if isinstance(segment.extra, dict):
                if segment.extra.pop("cache_control", None) is not None:
                    ctx.add_diagnostic(
                        code="cache_hint_removed",
                        message="cache hint was removed from instruction segment",
                        severity="info",
                        details={"location": "instruction"},
                        transformer=self.NAME,
                    )

        for message in internal.messages:
            for block in message.content:
                if isinstance(getattr(block, "extra", None), dict):
                    if block.extra.pop("cache_control", None) is not None:
                        ctx.add_diagnostic(
                            code="cache_hint_removed",
                            message="cache hint was removed from content block",
                            severity="info",
                            details={"location": "message"},
                            transformer=self.NAME,
                        )

        gemini_extra = internal.extra.get("gemini")
        if isinstance(gemini_extra, dict):
            if gemini_extra.pop("cached_content", None) is not None:
                ctx.add_diagnostic(
                    code="cache_hint_removed",
                    message="cached content reference was removed from gemini extra payload",
                    severity="info",
                    details={"location": "gemini.cached_content"},
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
