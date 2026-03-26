from __future__ import annotations

import json

from src.core.api_format.conversion.internal import (
    InternalError,
    InternalRequest,
    InternalResponse,
    ToolResultBlock,
    ToolUseBlock,
)
from src.core.api_format.conversion.stream_events import InternalStreamEvent
from src.core.api_format.transformers.base import TransformContext


class EnhanceToolTransformer:
    NAME = "enhancetool"

    def transform_request(
        self,
        internal: InternalRequest,
        ctx: TransformContext,
    ) -> InternalRequest:
        tool_names: dict[str, str] = {}
        auto_name_counter = 0

        def next_tool_name() -> str:
            nonlocal auto_name_counter
            auto_name_counter += 1
            return f"tool_{auto_name_counter}"

        for message in internal.messages:
            for block in message.content:
                if isinstance(block, ToolUseBlock):
                    tool_name = str(block.tool_name or "").strip()
                    if not tool_name:
                        tool_name = next_tool_name()
                        block.tool_name = tool_name
                        ctx.add_diagnostic(
                            code="tool_name_generated",
                            message="missing tool name was generated",
                            severity="warning",
                            details={"tool_id": str(block.tool_id or "").strip() or None, "tool_name": tool_name},
                            transformer=self.NAME,
                        )

                    raw = block.extra.get("raw") if isinstance(block.extra, dict) else None
                    raw_arguments = raw.get("arguments") if isinstance(raw, dict) else None
                    if isinstance(raw_arguments, str) and not block.tool_input:
                        try:
                            parsed = json.loads(raw_arguments)
                        except json.JSONDecodeError:
                            parsed = {}
                            ctx.add_diagnostic(
                                code="tool_arguments_repaired",
                                message="invalid tool arguments JSON was replaced with an empty object",
                                severity="warning",
                                details={"tool_id": str(block.tool_id or "").strip() or None},
                                transformer=self.NAME,
                            )
                        block.tool_input = parsed if isinstance(parsed, dict) else {}

                    tool_id = str(block.tool_id or "").strip()
                    if tool_id:
                        tool_names[tool_id] = tool_name
                    continue

                if isinstance(block, ToolResultBlock):
                    tool_use_id = str(block.tool_use_id or "").strip()
                    if not block.tool_name and tool_use_id in tool_names:
                        block.tool_name = tool_names[tool_use_id]
                        ctx.add_diagnostic(
                            code="tool_result_name_filled",
                            message="tool_result name was backfilled from tool_use",
                            severity="info",
                            details={"tool_use_id": tool_use_id, "tool_name": block.tool_name},
                            transformer=self.NAME,
                        )

                    if block.output is None and block.content_text is None:
                        block.content_text = ""
                        ctx.add_diagnostic(
                            code="tool_result_normalized",
                            message="empty tool_result was normalized to empty text",
                            severity="info",
                            details={"tool_use_id": tool_use_id or None},
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
