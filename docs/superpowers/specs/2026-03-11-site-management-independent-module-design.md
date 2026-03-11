# Site Management Independent Module Design

## Background

Current site management is spread across multiple locations:
- `src/services/site_management/` (services)
- `src/api/admin/site_management.py` (routes)
- `src/models/database.py` (SiteAccount model with Provider FK)
- `src/services/system/maintenance_scheduler.py` (scheduled tasks)

Problems:
1. Only supports a single WebDav source (config stored in SystemConfig)
2. SiteAccount is coupled to Provider via `provider_id` FK and `AllApiHubSyncService` sync
3. Code scattered across api/services/models layers, not self-contained

## Decision Record

| Decision | Choice |
|----------|--------|
| Architecture | Self-contained module under `src/modules/site_management/` (Option C) |
| Multi-WebDav | Independent `webdav_sources` table |
| Provider coupling | Fully severed (delete `provider_id` FK, remove all Provider references) |
| Account uniqueness | `(webdav_source_id, domain)` composite unique constraint |
| Ops config location | Stays on SiteAccount (architecture_id, auth_type, credentials, etc.) |
| Frontend | Split into multiple pages (source list, source detail/accounts, history) |

## Module Directory Structure

```
src/modules/site_management/
├── __init__.py                     # ModuleDefinition registration
├── models.py                       # ORM: WebDavSource, SiteAccount, log tables
├── schemas.py                      # Pydantic request/response models
├── routes.py                       # FastAPI router
├── services/
│   ├── __init__.py
│   ├── webdav_source_service.py    # WebDavSource CRUD
│   ├── snapshot_service.py         # WebDav snapshot fetch & cache
│   ├── account_sync_service.py     # WebDav → SiteAccount sync
│   ├── account_ops_service.py      # Checkin/balance execution
│   ├── log_service.py              # Sync/checkin logging
│   └── scheduler.py                # Scheduled task registration & execution
└── webdav_client.py                # WebDav HTTP client
```

## Data Model

### New Table: `webdav_sources`

| Column | Type | Constraints | Notes |
|--------|------|-------------|-------|
| id | String(36) | PK | uuid4 |
| name | String(100) | NOT NULL | Display name |
| url | String(500) | NOT NULL | WebDav URL |
| username | String(200) | NOT NULL | WebDav username |
| password | Text | NOT NULL | Encrypted via CryptoService |
| is_active | Boolean | default=True | Enable/disable |
| sync_enabled | Boolean | default=True | Participate in scheduled sync |
| last_sync_at | DateTime | nullable | Last sync timestamp |
| last_sync_status | String(20) | nullable | success/failed |
| created_at | DateTime | | |
| updated_at | DateTime | | |

Relationships: `accounts` (SiteAccount, passive_deletes), `snapshots` (SiteSourceSnapshot, passive_deletes)

### Modified Table: `site_accounts`

Changes from current schema:
- **Remove** `provider_id` FK (fully decoupled from Provider)
- **Remove** `source_snapshot_id` FK
- **Add** `webdav_source_id` FK → `webdav_sources.id`, `ondelete="CASCADE"`, NOT NULL
- **Add** `UniqueConstraint("webdav_source_id", "domain")`

Retained fields: `domain`, `site_url`, `architecture_id`, `base_url`, `auth_type`, `credentials`, `config`, `checkin_enabled`, `balance_sync_enabled`, `is_active`, all `last_checkin_*` / `last_balance_*` fields, timestamps.

### Modified Table: `site_source_snapshots`

- **Add** `webdav_source_id` FK → `webdav_sources.id`, `ondelete="CASCADE"`
- **Remove** `source_url` (sourced from WebDavSource)

### Log Tables

`site_sync_runs`: add `webdav_source_id` field. `site_sync_items`, `site_checkin_runs`, `site_checkin_items` unchanged.

### Migration Strategy

1. Create `webdav_sources` table
2. Add nullable `webdav_source_id` to `site_accounts`
3. Read existing WebDav config from SystemConfig, insert default `WebDavSource` record
4. Backfill all existing `site_accounts` with default source ID
5. Make `webdav_source_id` NOT NULL
6. Drop `provider_id` column
7. Add composite unique constraint `(webdav_source_id, domain)`
8. Add `webdav_source_id` to `site_source_snapshots`

## Service Layer

### WebDavSourceService

CRUD for WebDav sources. Password encrypted via CryptoService. `test_connection()` validates URL/credentials.

### SnapshotService

Migrated from existing `SiteSnapshotService`. Accepts `webdav_source_id`, reads URL/credentials from `WebDavSource`. Cache TTL from SystemConfig.

### AccountSyncService

Migrated from existing `SiteAccountSyncService`. **Removed:**
- Provider query and domain→provider matching
- `AllApiHubSyncService` calls

Simplified flow: WebDav JSON → parse accounts → match by domain within source → create/update/deactivate SiteAccount records.

### AccountOpsService

Migrated from existing `SiteAccountOpsService`. **Removed:**
- `from src.models.database import Provider`
- `_extract_provider_ops_config()` Provider lookup

All execution config read from SiteAccount fields. Continues to use shared `OpsExecutionEngine` and `OpsExecutionTarget`.

### Scheduler

Extracted from `maintenance_scheduler.py`. Three jobs:
- `site_account_sync`: iterate all active WebDavSources, sync each
- `site_account_checkin`: query all checkin_enabled accounts, execute with semaphore(3)
- `site_account_balance_sync`: query all balance_sync_enabled accounts, execute with semaphore(3)

Registered via `ModuleDefinition.on_startup` hook instead of injecting into MaintenanceScheduler.

## Dependency Boundary

**Allowed imports:**
- `src/core/*` (CryptoService, logger, exceptions, CacheService)
- `src/database/*` (create_session, Base)
- `src/services/provider_ops/execution_engine.py` (OpsExecutionEngine, OpsExecutionTarget)
- `src/services/provider_ops/types.py` (ActionResult, ActionStatus, ProviderActionType)
- `src/services/system/scheduler.py` (get_scheduler)
- `src/services/system/config.py` (SystemConfigService)

**Forbidden imports:**
- `src/models/database.py` Provider/ProviderAPIKey
- `src/services/provider_sync/*`
- `src/services/provider_ops/service.py` (ProviderOpsService)

## API Routes

Prefix: `/api/admin/site-management`

### WebDav Source Management

| Method | Path | Description |
|--------|------|-------------|
| GET | /sources | List all sources |
| POST | /sources | Create source |
| PUT | /sources/{source_id} | Update source |
| DELETE | /sources/{source_id} | Delete source (CASCADE) |
| POST | /sources/{source_id}/test | Test WebDav connection |
| POST | /sources/{source_id}/sync | Manual sync (optional dry_run) |

### Account Management

| Method | Path | Description |
|--------|------|-------------|
| GET | /sources/{source_id}/accounts | List accounts (paginated, searchable) |
| POST | /sources/{source_id}/accounts/{id}/checkin | Manual checkin |
| POST | /sources/{source_id}/accounts/{id}/balance | Manual balance query |
| POST | /sources/{source_id}/accounts/checkin | Batch checkin |
| POST | /sources/{source_id}/accounts/balance | Batch balance query |

### Execution History

| Method | Path | Description |
|--------|------|-------------|
| GET | /sync-runs | Sync history (filterable by source_id) |
| GET | /sync-runs/{run_id}/items | Sync detail items |
| GET | /checkin-runs | Checkin history |
| GET | /checkin-runs/{run_id}/items | Checkin detail items |

## Frontend

### File Structure

```
frontend/src/features/site-management/
├── components/
│   ├── WebDavSourceFormDialog.vue
│   ├── WebDavSourceCard.vue
│   ├── AccountTable.vue
│   ├── AccountDetailDrawer.vue
│   ├── SyncHistoryTable.vue
│   └── CheckinHistoryTable.vue
├── api.ts
└── types.ts
```

### Page Routes

| Route | Page |
|-------|------|
| /admin/site-management | WebDav source list (card layout) |
| /admin/site-management/:sourceId | Source detail (account table + actions) |
| /admin/site-management/history/sync | Sync execution history |
| /admin/site-management/history/checkin | Checkin execution history |

## Cleanup List

| File/Directory | Action |
|----------------|--------|
| `src/services/site_management/` | Delete entire directory |
| `src/api/admin/site_management.py` | Delete |
| `src/models/database.py` SiteAccount/SiteSourceSnapshot/SiteSyncRun/SiteSyncItem/SiteCheckinRun/SiteCheckinItem | Move to module models.py, remove from database.py |
| `src/models/database.py` Provider.site_accounts relationship | Delete |
| `src/services/system/maintenance_scheduler.py` site_account code | Remove 3 jobs, time configs, imports |
| `src/api/admin/system.py` site_account config endpoints | Remove or migrate to module |

## Error Handling

- WebDav connection failure: update `WebDavSource.last_sync_status = "failed"`, log to `site_sync_runs`. Single source failure does not block others.
- Checkin/balance failure: persisted to `SiteAccount.last_*` fields via `ActionResult`. Single account failure does not block batch.
- Module disabled: routes not registered, scheduler not started. No impact on main service.

## Testing

- **Migration tests:** table creation, data backfill, constraint validation
- **Service unit tests:** CRUD, sync logic, ops without Provider dependency, multi-source scheduler
- **Integration tests:** create source → sync → checkin → query balance → view history
