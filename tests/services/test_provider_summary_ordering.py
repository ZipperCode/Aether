from __future__ import annotations

from datetime import datetime, timedelta, timezone

from sqlalchemy import Boolean, Column, DateTime, Integer, String, create_engine, select
from sqlalchemy.orm import Session, declarative_base

from src.api.admin.providers.summary import _provider_summary_ordering

Base = declarative_base()


class DemoProvider(Base):
    __tablename__ = "demo_providers"

    id = Column(String(36), primary_key=True)
    name = Column(String(100), nullable=False)
    is_active = Column(Boolean, nullable=False)
    provider_priority = Column(Integer, nullable=False)
    created_at = Column(DateTime(timezone=True), nullable=False)


def test_provider_summary_ordering_puts_inactive_providers_last() -> None:
    engine = create_engine("sqlite:///:memory:")
    try:
        Base.metadata.create_all(engine)

        with Session(engine) as db:
            base_time = datetime(2026, 3, 15, tzinfo=timezone.utc)
            db.add_all(
                [
                    DemoProvider(
                        id="inactive-top-priority",
                        name="inactive-top-priority",
                        is_active=False,
                        provider_priority=0,
                        created_at=base_time,
                    ),
                    DemoProvider(
                        id="active-lower-priority",
                        name="active-lower-priority",
                        is_active=True,
                        provider_priority=20,
                        created_at=base_time + timedelta(seconds=1),
                    ),
                    DemoProvider(
                        id="active-higher-priority-newer",
                        name="active-higher-priority-newer",
                        is_active=True,
                        provider_priority=5,
                        created_at=base_time + timedelta(seconds=3),
                    ),
                    DemoProvider(
                        id="active-higher-priority-older",
                        name="active-higher-priority-older",
                        is_active=True,
                        provider_priority=5,
                        created_at=base_time + timedelta(seconds=2),
                    ),
                ]
            )
            db.commit()

            ordered = (
                db.execute(select(DemoProvider).order_by(*_provider_summary_ordering(DemoProvider)))
                .scalars()
                .all()
            )

        assert [provider.id for provider in ordered] == [
            "active-higher-priority-older",
            "active-higher-priority-newer",
            "active-lower-priority",
            "inactive-top-priority",
        ]
    finally:
        engine.dispose()
