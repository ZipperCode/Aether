from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.services.orchestration.request_dispatcher import RequestDispatcher
from src.services.request.executor import ExecutionContext, ExecutionResult


@pytest.mark.asyncio
async def test_request_dispatcher_passes_user_id_to_executor() -> None:
    db = MagicMock()

    request_executor = MagicMock()
    response = SimpleNamespace()
    context = ExecutionContext(
        candidate_id="c1",
        candidate_index=0,
        provider_id="p1",
        endpoint_id="e1",
        key_id="k1",
        user_id="u1",
        api_key_id="ak1",
        is_cached_user=False,
        elapsed_ms=123,
    )
    request_executor.execute = AsyncMock(
        return_value=ExecutionResult(response=response, context=context)
    )

    dispatcher = RequestDispatcher(
        db=db,
        request_executor=request_executor,
        cache_scheduler=None,
    )

    candidate = SimpleNamespace(
        provider=SimpleNamespace(id="p1", name="provider-1"),
        endpoint=SimpleNamespace(id="e1"),
        key=SimpleNamespace(id="k1", cache_ttl_minutes=0),
    )

    with patch(
        "src.services.orchestration.request_dispatcher.RequestCandidateService.update_candidate_status"
    ):
        await dispatcher.dispatch(
            candidate=candidate,
            candidate_index=0,
            retry_index=0,
            candidate_record_id="c1",
            user_api_key=SimpleNamespace(id="ak1", user_id="u1"),
            user_id="u1",
            request_func=AsyncMock(),
            request_id="r1",
            api_format="openai:chat",
            model_name="gpt-4.1-mini",
            affinity_key="ak1",
            global_model_id="gm1",
            attempt_counter=1,
            max_attempts=1,
            is_stream=False,
        )

    request_executor.execute.assert_awaited_once()
    assert request_executor.execute.call_args.kwargs["user_id"] == "u1"
