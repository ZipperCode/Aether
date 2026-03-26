from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Literal, Protocol

from src.core.api_format.conversion.internal import InternalError, InternalRequest, InternalResponse
from src.core.api_format.conversion.stream_events import InternalStreamEvent


Stage = Literal["request", "response", "stream", "error"]


@dataclass(slots=True)
class TransformContext:
    stage: Stage
    client_format: str
    provider_format: str | None = None
    provider_type: str | None = None
    target_variant: str | None = None
    model: str | None = None
    is_stream: bool = False
    endpoint_id: str | None = None
    request_id: str | None = None
    transformer_name: str | None = None
    transformer_config: dict[str, Any] = field(default_factory=dict)
    diagnostics: list[dict[str, Any]] = field(default_factory=list)

    def add_diagnostic(
        self,
        *,
        code: str,
        message: str,
        severity: str = "warning",
        details: dict[str, Any] | None = None,
        transformer: str | None = None,
    ) -> None:
        item: dict[str, Any] = {
            "stage": self.stage,
            "transformer": transformer if transformer is not None else self.transformer_name,
            "code": str(code or "").strip(),
            "message": str(message or "").strip(),
            "severity": str(severity or "warning").strip() or "warning",
        }
        if details:
            item["details"] = details
        self.diagnostics.append(item)


class FormatTransformer(Protocol):
    NAME: str

    def transform_request(
        self,
        internal: InternalRequest,
        ctx: TransformContext,
    ) -> InternalRequest: ...

    def transform_response(
        self,
        internal: InternalResponse,
        ctx: TransformContext,
    ) -> InternalResponse: ...

    def transform_stream_event(
        self,
        event: InternalStreamEvent,
        ctx: TransformContext,
    ) -> list[InternalStreamEvent]: ...

    def transform_error(
        self,
        internal: InternalError,
        ctx: TransformContext,
    ) -> InternalError: ...
