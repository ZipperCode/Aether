from __future__ import annotations

from dataclasses import dataclass
from typing import Iterable

from alembic.config import Config
from alembic.script import ScriptDirectory
from sqlalchemy import inspect, text
from sqlalchemy.engine import Engine


HEAD_SENTINELS: dict[str, set[str]] = {
    "provider_api_keys": {"auth_type", "status_snapshot"},
    "usage": {"provider_request_body", "client_response_body", "billing_status"},
    "proxy_nodes": {"remote_config"},
    "user_sessions": {"refresh_token_hash"},
    "wallet_daily_usage_ledgers": {"billing_date"},
    "webdav_sources": {"name"},
}


@dataclass(frozen=True)
class AlembicProbeInput:
    expected_head: str
    known_revisions: set[str]
    current_revisions: list[str]
    tables: set[str]
    table_columns: dict[str, set[str]]
    has_alembic_version_table: bool


@dataclass(frozen=True)
class AlembicProbeResult:
    status: str
    summary: str
    expected_head: str
    current_revisions: list[str]
    can_upgrade_directly: bool
    missing_sentinels: list[str]
    recommendations: list[str]


def _collect_missing_sentinels(
    tables: set[str], table_columns: dict[str, set[str]]
) -> list[str]:
    missing: list[str] = []
    for table_name, required_columns in HEAD_SENTINELS.items():
        if table_name not in tables:
            missing.append(f"{table_name} (table)")
            continue
        existing_columns = table_columns.get(table_name, set())
        for column_name in sorted(required_columns):
            if column_name not in existing_columns:
                missing.append(f"{table_name}.{column_name}")
    return missing


def classify_alembic_state(probe: AlembicProbeInput) -> AlembicProbeResult:
    if not probe.has_alembic_version_table:
        if not probe.tables:
            return AlembicProbeResult(
                status="empty_database",
                summary="数据库为空，尚未初始化 Alembic 版本表。",
                expected_head=probe.expected_head,
                current_revisions=[],
                can_upgrade_directly=True,
                missing_sentinels=[],
                recommendations=[
                    "直接执行 alembic upgrade head",
                ],
            )
        return AlembicProbeResult(
            status="unmanaged_schema",
            summary="数据库已有业务表，但没有 alembic_version，属于未受 Alembic 管理的 schema。",
            expected_head=probe.expected_head,
            current_revisions=[],
            can_upgrade_directly=False,
            missing_sentinels=[],
            recommendations=[
                "先备份数据库",
                "将当前库 schema 与全新升级到 head 的库进行对比",
                "确认结构一致后再手工 stamp",
            ],
        )

    if not probe.current_revisions:
        return AlembicProbeResult(
            status="missing_revision_rows",
            summary="存在 alembic_version 表，但没有 revision 记录。",
            expected_head=probe.expected_head,
            current_revisions=[],
            can_upgrade_directly=False,
            missing_sentinels=[],
            recommendations=[
                "先备份数据库",
                "检查 alembic_version 是否被误清空",
            ],
        )

    unknown_revisions = [rev for rev in probe.current_revisions if rev not in probe.known_revisions]
    if unknown_revisions:
        return AlembicProbeResult(
            status="orphan_revision",
            summary="数据库记录的 revision 不在当前仓库迁移链中。",
            expected_head=probe.expected_head,
            current_revisions=probe.current_revisions,
            can_upgrade_directly=False,
            missing_sentinels=[],
            recommendations=[
                "先备份数据库",
                "不要直接 stamp latest",
                "创建一个全新数据库升级到 head 后做 schema diff",
            ],
        )

    if probe.expected_head in probe.current_revisions:
        missing_sentinels = _collect_missing_sentinels(probe.tables, probe.table_columns)
        if missing_sentinels:
            return AlembicProbeResult(
                status="head_schema_drift",
                summary="数据库 revision 已是 head，但缺少若干后期 schema 哨兵，疑似历史上被错误 stamp。",
                expected_head=probe.expected_head,
                current_revisions=probe.current_revisions,
                can_upgrade_directly=False,
                missing_sentinels=missing_sentinels,
                recommendations=[
                    "先备份数据库",
                    "不要再次 stamp",
                    "对比全新 head 库与当前库的 schema 差异",
                ],
            )
        return AlembicProbeResult(
            status="current_head",
            summary="数据库已处于当前 head，且关键 schema 哨兵齐全。",
            expected_head=probe.expected_head,
            current_revisions=probe.current_revisions,
            can_upgrade_directly=True,
            missing_sentinels=[],
            recommendations=[
                "无需执行额外迁移",
            ],
        )

    return AlembicProbeResult(
        status="upgradeable",
        summary="数据库 revision 仍在当前仓库迁移链内，可以正常增量升级。",
        expected_head=probe.expected_head,
        current_revisions=probe.current_revisions,
        can_upgrade_directly=True,
        missing_sentinels=[],
        recommendations=[
            "执行 alembic upgrade head",
        ],
    )


def load_known_revisions(alembic_ini_path: str) -> tuple[str, set[str]]:
    config = Config(alembic_ini_path)
    script = ScriptDirectory.from_config(config)
    heads = script.get_heads()
    expected_head = heads[0]
    revisions = {revision.revision for revision in script.walk_revisions()}
    return expected_head, revisions


def probe_database_state(
    engine: Engine,
    expected_head: str,
    known_revisions: set[str],
) -> AlembicProbeResult:
    inspector = inspect(engine)
    tables = set(inspector.get_table_names())
    table_columns = {
        table_name: {column["name"] for column in inspector.get_columns(table_name)}
        for table_name in tables
    }
    has_alembic_version_table = "alembic_version" in tables
    current_revisions: list[str] = []

    if has_alembic_version_table:
        with engine.connect() as conn:
            rows = conn.execute(text("SELECT version_num FROM alembic_version")).fetchall()
        current_revisions = [str(row[0]) for row in rows if row and row[0]]

    return classify_alembic_state(
        AlembicProbeInput(
            expected_head=expected_head,
            known_revisions=known_revisions,
            current_revisions=current_revisions,
            tables=tables,
            table_columns=table_columns,
            has_alembic_version_table=has_alembic_version_table,
        )
    )
