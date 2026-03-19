from __future__ import annotations

from datetime import datetime, timezone

import pytest

import src.plugins.rate_limit.token_bucket as token_bucket_module
from src.plugins.rate_limit.token_bucket import RedisTokenBucketBackend, TokenBucketStrategy


@pytest.mark.asyncio
async def test_token_bucket_cleans_up_expired_buckets(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("RATE_LIMIT_BACKEND", "memory")
    strategy = TokenBucketStrategy()
    strategy.configure({"bucket_expiry": 1, "cleanup_interval": 0})

    await strategy.check_limit("api_key:stale")
    strategy.buckets["api_key:stale"].last_access_time -= 3600

    await strategy.check_limit("api_key:fresh")

    assert "api_key:stale" not in strategy.buckets
    assert "api_key:fresh" in strategy.buckets


@pytest.mark.asyncio
async def test_token_bucket_reconfigures_existing_bucket_when_rate_limit_changes(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("RATE_LIMIT_BACKEND", "memory")
    strategy = TokenBucketStrategy()

    await strategy.check_limit("user:42", rate_limit=120)
    bucket = strategy.buckets["user:42"]
    bucket.tokens = 90

    await strategy.check_limit("user:42", rate_limit=30)

    updated_bucket = strategy.buckets["user:42"]
    assert updated_bucket.capacity == 30
    assert updated_bucket.refill_rate == 0.5
    assert updated_bucket.tokens <= 30


@pytest.mark.asyncio
async def test_token_bucket_treats_non_positive_dynamic_rate_limit_as_unlimited(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("RATE_LIMIT_BACKEND", "memory")
    strategy = TokenBucketStrategy()

    result = await strategy.check_limit("public_ip:test", rate_limit=0)
    consumed = await strategy.consume("public_ip:test", amount=1, rate_limit=0)

    assert result.allowed is True
    assert consumed is True
    assert "public_ip:test" not in strategy.buckets


class _FakeRedisClient:
    async def hmget(self, _key: str, *_fields: str) -> list[None]:
        return [None, None]

    def register_script(self, _script: str):  # type: ignore[no-untyped-def]
        async def _runner(*args, **kwargs):  # type: ignore[no-untyped-def]
            return [1, 0, 0]

        return _runner


@pytest.mark.asyncio
async def test_token_bucket_retries_redis_backend_probe_after_initial_miss(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("RATE_LIMIT_BACKEND", "auto")
    strategy = TokenBucketStrategy()
    strategy._redis_retry_interval = 0

    fake_redis = _FakeRedisClient()
    calls = {"count": 0}

    def _fake_get_redis_client_sync():  # type: ignore[no-untyped-def]
        calls["count"] += 1
        if calls["count"] == 1:
            return None
        return fake_redis

    monkeypatch.setattr(
        token_bucket_module,
        "get_redis_client_sync",
        _fake_get_redis_client_sync,
    )

    await strategy.check_limit("public_ip:first")
    assert strategy._redis_backend is None

    await strategy.check_limit("public_ip:second")
    assert strategy._redis_backend is not None
    assert calls["count"] == 2


@pytest.mark.asyncio
async def test_redis_token_bucket_missing_bucket_reports_reset_now() -> None:
    backend = RedisTokenBucketBackend(_FakeRedisClient())

    result = await backend.peek("public_ip:test", capacity=60, refill_rate=1.0, amount=1)

    assert result.allowed is True
    assert result.remaining == 60
    assert result.reset_at is not None
    assert abs((result.reset_at - datetime.now(timezone.utc)).total_seconds()) < 2
