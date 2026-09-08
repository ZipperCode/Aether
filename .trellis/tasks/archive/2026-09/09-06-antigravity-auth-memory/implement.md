# Implementation Plan

1. Update system-config data contracts, atomic revision persistence, migrations, and all adapters; add focused adapter tests.
2. Implement Antigravity allowlist revision snapshot/singleflight and bearer-hash cache keys in gateway auth resolution; add large-list and same-second update tests while retaining cross-node revocation coverage.
3. Refactor routed candidate ranking/materialization to lightweight snapshots, lazy transport reads, projection-based ranking facts, and transport-free resolved-page cache values; add 2,048-candidate global-order/fallback tests.
4. Run formatting, affected gateway/data tests, and `cargo check --all-targets`; inspect metrics/pressure output for bounded peak and post-drain baseline.
5. Run Trellis quality check and record evidence before archiving the task.

## Validation environment constraint

- 按用户要求，禁止在宿主机执行 Cargo/CMake 本地构建；编译、部署和运行时
  验证统一通过 `docker compose -f docker-compose.yml -f docker-compose.local.yml`
  的容器构建与服务健康检查完成。

## Current execution status

- Completed: revision-only strong reads plus atomic `{revision, value}` loads,
  async singleflight immutable allowlist snapshots, bearer-hash auth cache keys,
  null/invalid negative snapshots, and revision-preserving delete tombstones.
- Completed: compact routed ranking facts, transport-free resolved-page values,
  lazy `next_attempt()` hydration for ordinary and same-format text paths, and
  globally ordered fallback after a selected transport becomes invalid.
- Completed: compact model-fetch Key projections in memory/SQLite/PostgreSQL/MySQL,
  once-per-Provider whitelist reconciliation, and Pool-score startup with no
  multi-Key full read or duplicate immediate cycle.

## Latest Docker verification evidence

- `docker compose -f docker-compose.yml -f docker-compose.local.yml build app`：通过，生成镜像 `aether-app:latest`（digest `sha256:193ad0a2884171a8499c7f33cf79f0a6e663b0a6728499f62922093973f3e083`）。
- `docker compose -f docker-compose.yml -f docker-compose.local.yml up -d app`：通过，`aether-app` 状态为 `Up (healthy)`。
- `GET /_gateway/health`：HTTP 200；启动日志确认 revision migration `20260906000000` 已应用。
- PostgreSQL 容器确认 `system_configs.revision` 字段存在；未执行宿主机 Cargo/CMake 构建。

## Final Docker verification evidence

- 10,000-entry allowlist with 128 concurrent refreshes plus revision change:
  2/2 passed. Existing cross-node revocation: 1/1 passed. Null/invalid
  revision snapshots: 1/1 passed.
- System-config revision-only/tombstone coverage: 8/8 data tests plus 2/2
  memory revision/delete-reinsert tests; SQLite includes a live CRUD round trip.
- Routed 2,048-candidate real `next_attempt()` regression passed: ranking reads
  zero transports; invalid first candidate and successful fallback read two
  transports total.
- Model-fetch gateway tests: 55/55; `aether-model-fetch`: 79/79; compact
  projection tests passed for memory and all three SQL adapters. Pool-score:
  2/2.
- Docker `cargo check --locked --all-targets` passed for the eight affected
  packages. Standalone `rustfmt --check` passed for all 107 changed Rust files;
  `git diff --check` passed. `cargo-nextest` was not installed in the builder,
  so precise filtered `cargo test` commands were used instead.
- Final app image after auth changes:
  `sha256:936afc867c89ca107a1c8f00ac09087679b43fe0d04a4233e643aad3e80aca79`.
  Normal local app, PostgreSQL, and Redis are healthy; `/_gateway/health`
  returns HTTP 200; current app RSS sample was 25.77 MiB.

## 8,356-Key runtime comparison

| Stage | 50-target time | RX | Peak RSS | Provider-wide full-Key queries |
|---|---:|---:|---:|---:|
| Original | 4m43s | 5.87 GiB | 3.125 GiB | 50 / 417,800 rows |
| Once-per-Provider reconcile | 1m41s | 739 MiB | 1.362 GiB | 2 / 16,712 rows |
| Compact projection + bounded Pool startup | 73.914s | 167 MB | 568.59 MiB | 0 |

- Final allocator peak: 197.79 MiB allocated and 421.99 MiB resident; drain
  baseline: 32.57 MiB allocated and 76.71 MiB resident.
- Compact model-fetch projection was 2,597,053 bytes versus 115,194,397 bytes
  for full rows (44.36x smaller).
- Compared with the original run: time -73.88%, RX -97.35%, peak RSS -82.23%.
- The task probe container and `aether_auth_memory_probe_0906` database were
  deleted after validation. The original local `aether` database was not
  modified.

## Remaining boundary

- The destructive whole-config purge path still physically clears
  `system_configs`. A purge immediately followed by recreating the same key,
  without an intervening read that clears a live process snapshot, can reuse
  an initial row revision. Changing full purge/backup semantics is outside the
  ordinary system-config delete contract fixed here and requires a separate
  product decision.
