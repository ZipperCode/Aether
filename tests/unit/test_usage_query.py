from __future__ import annotations

from sqlalchemy import create_engine, select
from sqlalchemy.orm import Session

from src.models.database import Usage
from src.services.usage.query import input_context_expr


def _make_usage(
    request_id: str,
    api_format: str,
    input_tokens: int,
    cache_read_input_tokens: int,
) -> Usage:
    return Usage(
        request_id=request_id,
        provider_name="provider-a",
        model="test-model",
        api_format=api_format,
        input_tokens=input_tokens,
        cache_read_input_tokens=cache_read_input_tokens,
    )


def test_input_context_expr_always_uses_input_plus_cache_read() -> None:
    engine = create_engine("sqlite:///:memory:")
    Usage.__table__.create(engine)

    with Session(engine) as session:
        session.add_all(
            [
                _make_usage("openai-ok", "openai:chat", 100, 20),
                _make_usage("gemini-ok", "gemini:chat", 80, 30),
                _make_usage("claude-ok", "claude:chat", 100, 20),
            ]
        )
        session.commit()

        rows = session.execute(
            select(Usage.request_id, input_context_expr().label("ctx")).order_by(Usage.request_id)
        ).all()

    assert {r[0]: r[1] for r in rows} == {
        "claude-ok": 120,
        "gemini-ok": 110,
        "openai-ok": 120,
    }
    engine.dispose()


def test_input_context_expr_keeps_same_formula_for_anomalous_openai_compatible_usage() -> None:
    engine = create_engine("sqlite:///:memory:")
    Usage.__table__.create(engine)

    with Session(engine) as session:
        session.add(
            _make_usage(
                "openai-anomalous",
                "openai:chat",
                input_tokens=10,
                cache_read_input_tokens=300,
            )
        )
        session.commit()

        total_input_context = session.execute(
            select(input_context_expr()).where(Usage.request_id == "openai-anomalous")
        ).scalar_one()

    assert total_input_context == 310
    engine.dispose()
