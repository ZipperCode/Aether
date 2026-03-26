from __future__ import annotations

from src.core.api_format.conversion.internal import (
    InternalError,
    InternalRequest,
    InternalResponse,
    ToolResultBlock,
    ToolUseBlock,
)
from src.core.api_format.conversion.stream_events import InternalStreamEvent
from src.core.api_format.transformers.base import TransformContext


class ToolUseTransformer:
    NAME = "tooluse"

    def transform_request(
        self,
        internal: InternalRequest,
        ctx: TransformContext,
    ) -> InternalRequest:
        pending_tool_ids: list[str] = []
        auto_counter = 0

        def next_tool_id() -> str:
            nonlocal auto_counter
            auto_counter += 1
            return f"call_auto_{auto_counter}"

        for message in internal.messages:
            for block in message.content:
                if isinstance(block, ToolUseBlock):
                    tool_id = str(block.tool_id or "").strip()
                    if not tool_id:
                        tool_id = next_tool_id()
                        block.tool_id = tool_id
                        ctx.add_diagnostic(
                            code="tool_id_generated",
                            message="missing tool_use id was generated",
                            severity="info",
                            details={"tool_name": block.tool_name, "tool_id": tool_id},
                            transformer=self.NAME,
                        )
                    pending_tool_ids.append(tool_id)
                    continue

                if isinstance(block, ToolResultBlock):
                    tool_use_id = str(block.tool_use_id or "").strip()
                    if tool_use_id:
                        block.tool_use_id = tool_use_id
                        if tool_use_id in pending_tool_ids:
                            pending_tool_ids.remove(tool_use_id)
                        continue
                    if pending_tool_ids:
                        block.tool_use_id = pending_tool_ids.pop(0)
                        ctx.add_diagnostic(
                            code="tool_result_linked",
                            message="tool_result was linked to the latest pending tool_use",
                            severity="warning",
                            details={"tool_use_id": block.tool_use_id},
                            transformer=self.NAME,
                        )
                    else:
                        block.tool_use_id = next_tool_id()
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
