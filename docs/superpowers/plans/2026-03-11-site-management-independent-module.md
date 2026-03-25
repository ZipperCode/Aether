# Site Management Independent Module — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure site management into a self-contained module under `src/modules/site_management/` with multi-WebDav source support and zero Provider coupling.

**Architecture:** Self-contained module with own models, services, routes, and scheduler. Shares only `OpsExecutionEngine` from `provider_ops` and infrastructure from `src/core/`. All Provider references removed. Module registered via existing `ModuleDefinition` auto-discovery.

**Tech Stack:** Python 3.12+ / FastAPI / SQLAlchemy / Alembic / APScheduler / Vue 3 + TypeScript

**Spec:** `docs/superpowers/specs/2026-03-11-site-management-independent-module-design.md`

---

## File Map

### New Files (module)

| File | Responsibility |
|------|---------------|
| `src/modules/site_management/models.py` | ORM: WebDavSource, SiteAccount, SiteSourceSnapshot, log tables |
| `src/modules/site_management/schemas.py` | Pydantic request/response models |
| `src/modules/site_management/routes.py` | FastAPI router for all site-management endpoints |
| `src/modules/site_management/webdav_client.py` | WebDav HTTP client (copy from provider_sync) |
| `src/modules/site_management/services/__init__.py` | Service exports |
| `src/modules/site_management/services/webdav_source_service.py` | WebDavSource CRUD |
| `src/modules/site_management/services/snapshot_service.py` | WebDav snapshot fetch & cache |
| `src/modules/site_management/services/account_sync_service.py` | WebDav → SiteAccount sync (no Provider) |
| `src/modules/site_management/services/account_ops_service.py` | Checkin/balance execution (no Provider) |
| `src/modules/site_management/services/log_service.py` | Sync/checkin log recording |
| `src/modules/site_management/services/scheduler.py` | Scheduled task registration & execution |
| `alembic/versions/YYYYMMDD_HHMM_*_site_mgmt_independent_module.py` | Migration: add webdav_sources, refactor site_accounts |

### New Files (tests)

| File | Responsibility |
|------|---------------|
| `tests/modules/site_management/test_models.py` | Model constraint tests |
| `tests/modules/site_management/test_webdav_source_service.py` | CRUD tests |
| `tests/modules/site_management/test_snapshot_service.py` | Cache logic tests |
| `tests/modules/site_management/test_account_sync_service.py` | Sync logic tests (no Provider) |
| `tests/modules/site_management/test_account_ops_service.py` | Ops execution tests (no Provider) |
| `tests/modules/site_management/test_scheduler.py` | Multi-source scheduling tests |

### New Files (frontend)

| File | Responsibility |
|------|---------------|
| `frontend/src/features/site-management/api.ts` | API client |
| `frontend/src/features/site-management/types.ts` | TypeScript types |
| `frontend/src/features/site-management/components/WebDavSourceFormDialog.vue` | Source create/edit dialog |
| `frontend/src/features/site-management/components/WebDavSourceCard.vue` | Source card display |
| `frontend/src/features/site-management/components/AccountTable.vue` | Account list table |
| `frontend/src/features/site-management/components/AccountDetailDrawer.vue` | Account detail drawer |
| `frontend/src/features/site-management/components/SyncHistoryTable.vue` | Sync history |
| `frontend/src/features/site-management/components/CheckinHistoryTable.vue` | Checkin history |
| `frontend/src/views/admin/SiteSourceList.vue` | Source list page |
| `frontend/src/views/admin/SiteSourceDetail.vue` | Source detail + accounts page |
| `frontend/src/views/admin/SiteSyncHistory.vue` | Sync history page |
| `frontend/src/views/admin/SiteCheckinHistory.vue` | Checkin history page |

### Modified Files

| File | Change |
|------|--------|
| `src/modules/site_management/__init__.py` | Update ModuleDefinition, add on_startup hook |
| `src/models/database.py` | Remove SiteAccount, SiteSourceSnapshot, SiteSyncRun/Item, SiteCheckinRun/Item classes + Provider.site_accounts relationship |
| `src/services/system/maintenance_scheduler.py` | Remove all site_account jobs, imports, time configs |
| `src/api/admin/system.py` | Remove site_account_*_time config handlers |
| `frontend/src/router/index.ts` | Update site-management routes |

### Deleted Files

| File | Reason |
|------|--------|
| `src/services/site_management/` (entire directory) | Migrated to module |
| `src/api/admin/site_management.py` | Migrated to module routes.py |
| `frontend/src/views/admin/SiteManagement.vue` | Replaced by multi-page structure |

---

## Chunk 1: Backend Models & Migration

### Task 1: Create module skeleton with models

**Files:**
- Create: `src/modules/site_management/models.py`
- Create: `src/modules/site_management/services/__init__.py`

- [ ] **Step 1: Create module directory structure**

```bash
mkdir -p src/modules/site_management/services
touch src/modules/site_management/services/__init__.py
```

- [ ] **Step 2: Write models.py with all ORM definitions**

Create `src/modules/site_management/models.py` with these models migrated from `src/models/database.py`:

- `WebDavSource` — NEW table `webdav_sources` (see spec: id, name, url, username, password, is_active, sync_enabled, last_sync_at, last_sync_status, created_at, updated_at)
- `SiteAccount` — MODIFIED from current: remove `provider_id`, `source_snapshot_id`, `source_type`; add `webdav_source_id` FK to `webdav_sources.id` (CASCADE), add `UniqueConstraint("webdav_source_id", "domain")`; remove `provider` relationship, add `webdav_source` relationship
- `SiteSourceSnapshot` — MODIFIED: add `webdav_source_id` FK to `webdav_sources.id` (CASCADE), remove `source_url` (now on WebDavSource); add `webdav_source` relationship
- `SiteSyncRun` — MODIFIED: add `webdav_source_id` column (nullable String(36), not FK to keep loose coupling with log data)
- `SiteSyncItem` — unchanged from current `src/models/database.py:2195`
- `SiteCheckinRun` — unchanged from current `src/models/database.py:2232`
- `SiteCheckinItem` — unchanged from current `src/models/database.py:2264`

Import Base from `src/database` (same pattern as `src/models/database.py`). Use same Column types and conventions.

- [ ] **Step 3: Verify models import without error**

Run: `python -c "from src.modules.site_management.models import WebDavSource, SiteAccount, SiteSourceSnapshot"`
Expected: no import errors

- [ ] **Step 4: Commit**

```bash
git add src/modules/site_management/models.py src/modules/site_management/services/__init__.py
git commit -m "feat(site-mgmt): add module skeleton with ORM models"
```

### Task 2: Create Alembic migration

**Files:**
- Create: `alembic/versions/*_site_mgmt_independent_module.py`

- [ ] **Step 1: Generate migration stub**

```bash
uv run alembic revision --autogenerate -m "site_mgmt_independent_module"
```

- [ ] **Step 2: Edit migration to implement the 8-step strategy**

The migration must handle:
1. Create `webdav_sources` table
2. Add nullable `webdav_source_id` to `site_accounts`
3. Read existing WebDav config from `system_configs` table (`all_api_hub_webdav_url`, `all_api_hub_webdav_username`, `all_api_hub_webdav_password`), insert a default `WebDavSource` if config exists
4. Backfill all existing `site_accounts.webdav_source_id` with the default source ID
5. Alter `webdav_source_id` to NOT NULL
6. Drop `provider_id` column and its index `idx_site_accounts_provider_id`
7. Drop `source_snapshot_id` column
8. Drop `source_type` column
9. Add composite unique constraint `uq_site_accounts_source_domain` on `(webdav_source_id, domain)`
10. Add `webdav_source_id` column to `site_source_snapshots` (nullable, not FK — some historical snapshots may not have a source)
11. Add `webdav_source_id` column to `site_sync_runs` (nullable String(36))

Important: use `op.execute()` with raw SQL for data backfill steps 3-4. Generate a UUID for the default source in Python.

- [ ] **Step 3: Test migration locally**

```bash
uv run alembic upgrade head
```

Expected: migration applies without error

- [ ] **Step 4: Test downgrade**

```bash
uv run alembic downgrade <target_revision>
```

Expected: rollback applies without error

- [ ] **Step 5: Commit**

```bash
git add alembic/versions/*_site_mgmt_independent_module.py
git commit -m "feat(site-mgmt): add migration for independent module schema"
```

### Task 3: Write model constraint tests

**Files:**
- Create: `tests/modules/__init__.py`
- Create: `tests/modules/site_management/__init__.py`
- Create: `tests/modules/site_management/test_models.py`

- [ ] **Step 1: Write tests**

Test cases:
- `test_webdav_source_create` — create WebDavSource, verify fields
- `test_site_account_unique_constraint` — two accounts with same (webdav_source_id, domain) raises IntegrityError
- `test_site_account_different_source_same_domain` — same domain under different sources is allowed
- `test_cascade_delete_source` — deleting WebDavSource cascades to SiteAccount

Use in-memory SQLite session for tests (follow existing test patterns in `tests/`).

- [ ] **Step 2: Run tests**

```bash
pytest tests/modules/site_management/test_models.py -v
```

Expected: all pass

- [ ] **Step 3: Commit**

```bash
git add tests/modules/
git commit -m "test(site-mgmt): add model constraint tests"
```

---

## Chunk 2: Backend Services (Core)

### Task 4: WebDav client

**Files:**
- Create: `src/modules/site_management/webdav_client.py`

- [ ] **Step 1: Copy from `src/services/provider_sync/webdav_client.py`**

Copy the entire file (87 lines) verbatim. No changes needed — it has no Provider dependencies.

- [ ] **Step 2: Verify import**

```bash
python -c "from src.modules.site_management.webdav_client import download_backup_with_meta, WebDavDownloadResult"
```

- [ ] **Step 3: Commit**

```bash
git add src/modules/site_management/webdav_client.py
git commit -m "feat(site-mgmt): add webdav client to module"
```

### Task 5: WebDavSourceService

**Files:**
- Create: `src/modules/site_management/services/webdav_source_service.py`
- Create: `tests/modules/site_management/test_webdav_source_service.py`

- [ ] **Step 1: Write test for CRUD operations**

Test cases:
- `test_create_source` — creates, verifies password is encrypted
- `test_list_sources` — lists active only by default
- `test_update_source` — updates name/url, re-encrypts password if changed
- `test_delete_source` — deletes and cascades to accounts
- `test_get_source_decrypts_password` — returned object has decrypted password

- [ ] **Step 2: Run tests to verify they fail**

```bash
pytest tests/modules/site_management/test_webdav_source_service.py -v
```

Expected: FAIL (module not found)

- [ ] **Step 3: Implement WebDavSourceService**

Class with methods: `create()`, `update()`, `delete()`, `get()`, `list_all()`, `test_connection()`.

Key implementation:
- `create()`: encrypt password via `CryptoService.encrypt()`, create `WebDavSource`, flush + commit
- `update()`: re-encrypt password if changed
- `delete()`: `db.delete(source)` — CASCADE handles accounts/snapshots
- `test_connection()`: call `download_backup_with_meta(url, username, decrypted_password)`, catch exceptions, return bool

- [ ] **Step 4: Run tests to verify they pass**

```bash
pytest tests/modules/site_management/test_webdav_source_service.py -v
```

Expected: all pass

- [ ] **Step 5: Commit**

```bash
git add src/modules/site_management/services/webdav_source_service.py tests/modules/site_management/test_webdav_source_service.py
git commit -m "feat(site-mgmt): add WebDavSourceService with CRUD"
```

### Task 6: SnapshotService

**Files:**
- Create: `src/modules/site_management/services/snapshot_service.py`
- Create: `tests/modules/site_management/test_snapshot_service.py`

- [ ] **Step 1: Write tests for cache behavior**

Test cases:
- `test_fresh_fetch_stores_snapshot` — no cache, downloads, stores to DB
- `test_cache_hit_within_ttl` — existing snapshot within TTL returns cached
- `test_cache_expired_refetches` — expired TTL triggers new download
- `test_force_refresh_bypasses_cache` — force_refresh=True always downloads

- [ ] **Step 2: Run tests to verify they fail**

- [ ] **Step 3: Implement SnapshotService**

Migrate from `src/services/site_management/snapshot_service.py` (130 lines). Changes:
- Constructor accepts `downloader` (same as current)
- `get_webdav_snapshot()` signature changes: accept `webdav_source_id` instead of url/username/password. Load `WebDavSource` from DB, decrypt password, then proceed as before
- `SiteSourceSnapshot` now created with `webdav_source_id`
- `_get_latest_snapshot()`: filter by `webdav_source_id` instead of `source_url`
- Import models from `src.modules.site_management.models` instead of `src.models.database`
- Import `download_backup_with_meta` from `src.modules.site_management.webdav_client`

- [ ] **Step 4: Run tests**

- [ ] **Step 5: Commit**

```bash
git add src/modules/site_management/services/snapshot_service.py tests/modules/site_management/test_snapshot_service.py
git commit -m "feat(site-mgmt): add SnapshotService with cache logic"
```

### Task 7: AccountSyncService (decoupled from Provider)

**Files:**
- Create: `src/modules/site_management/services/account_sync_service.py`
- Create: `tests/modules/site_management/test_account_sync_service.py`

- [ ] **Step 1: Write tests**

Test cases:
- `test_sync_creates_new_accounts` — new domains create SiteAccount records
- `test_sync_updates_existing_account` — same domain updates credentials
- `test_sync_deactivates_missing_domain` — domain absent from snapshot gets is_active=False (if policy enables it)
- `test_sync_respects_unique_constraint` — same domain in same source doesn't duplicate
- `test_no_provider_references` — verify the service module has zero imports from `src.models.database.Provider`

- [ ] **Step 2: Run tests to verify they fail**

- [ ] **Step 3: Implement AccountSyncService**

Migrate from `src/services/site_management/site_account_sync_service.py` (267 lines). Key changes:
- **Delete** `from src.models.database import Provider, SiteAccount` → import `SiteAccount` from `src.modules.site_management.models`
- **Delete** `from src.services.provider_sync.all_api_hub_backup import ...` → copy `ImportedAccount`, `parse_all_api_hub_accounts`, `_normalize_domain`, `_extract_accounts_list` into a local `parsers.py` or inline in this service (they are small pure functions with no Provider dependency)
- **Delete** `from src.services.provider_sync.sync_service import AllApiHubSyncService`
- **Delete** all `provider_by_domain` matching logic
- **Delete** `_extract_provider_architecture_id()` method
- `apply_snapshot()` signature: add `webdav_source_id: str` parameter
- New SiteAccount creation: set `webdav_source_id=webdav_source_id` instead of `provider_id`
- `_apply_account_fields()`: remove `provider` parameter, remove `provider_id` and `provider_arch` logic. Default `architecture_id` to `"new_api"` for `access_token` auth_type, else `None`
- Identity matching: within scope of single source (`db.query(SiteAccount).filter(SiteAccount.webdav_source_id == webdav_source_id)`)

- [ ] **Step 4: Run tests**

- [ ] **Step 5: Commit**

```bash
git add src/modules/site_management/services/account_sync_service.py tests/modules/site_management/test_account_sync_service.py
git commit -m "feat(site-mgmt): add AccountSyncService decoupled from Provider"
```

### Task 8: AccountOpsService (decoupled from Provider)

**Files:**
- Create: `src/modules/site_management/services/account_ops_service.py`
- Create: `tests/modules/site_management/test_account_ops_service.py`

- [ ] **Step 1: Write tests**

Test cases:
- `test_execute_action_builds_target_from_account_fields` — verify OpsExecutionTarget uses account's own architecture_id, base_url, auth_type, credentials
- `test_execute_action_no_provider_lookup` — mock db, verify no Provider query
- `test_checkin_new_api_architecture` — new_api uses query_balance with checkin_only
- `test_persist_balance_result` — result written to last_balance_* fields
- `test_persist_checkin_result` — result written to last_checkin_* fields

- [ ] **Step 2: Run tests to verify they fail**

- [ ] **Step 3: Implement AccountOpsService**

Migrate from `src/services/site_management/site_account_ops_service.py` (399 lines). Key changes:
- **Delete** `from src.models.database import Provider, SiteAccount` → import `SiteAccount` from `src.modules.site_management.models`
- **Delete** `_extract_provider_ops_config()` method entirely (was lines 252-259 querying Provider)
- `_build_target()`: remove `provider_config` parameter and all fallback-to-provider logic. Read architecture_id, base_url, auth_type, credentials, config directly from `account.*` fields only. Remove `provider_config` from `_resolve_auth_type()`, `_resolve_connector_config()`, `_resolve_actions()`, `_resolve_credentials()` — each now only uses `account_config`
- Keep `OpsExecutionEngine` usage unchanged
- Keep `_to_checkin_result_from_new_api()`, `_persist_last_result()`, `_ensure_connected()` unchanged

- [ ] **Step 4: Run tests**

- [ ] **Step 5: Commit**

```bash
git add src/modules/site_management/services/account_ops_service.py tests/modules/site_management/test_account_ops_service.py
git commit -m "feat(site-mgmt): add AccountOpsService decoupled from Provider"
```

### Task 9: LogService

**Files:**
- Create: `src/modules/site_management/services/log_service.py`

- [ ] **Step 1: Implement LogService**

Migrate from `src/services/site_management/log_service.py` (126 lines). Changes:
- Import models from `src.modules.site_management.models` instead of `src.models.database`
- **Delete** `from src.services.provider_sync import ProviderSyncResult` — `record_sync_run()` will accept plain kwargs instead of ProviderSyncResult
- `record_sync_run()`: change signature to accept individual fields (total_accounts, matched, etc.) instead of ProviderSyncResult. Add `webdav_source_id` param, set on SiteSyncRun
- `record_checkin_run()`: unchanged
- `resolve_system_password()`: unchanged

- [ ] **Step 2: Verify import**

```bash
python -c "from src.modules.site_management.services.log_service import SiteManagementLogService"
```

- [ ] **Step 3: Commit**

```bash
git add src/modules/site_management/services/log_service.py
git commit -m "feat(site-mgmt): add LogService to module"
```

### Task 10: Scheduler (extracted from MaintenanceScheduler)

**Files:**
- Create: `src/modules/site_management/services/scheduler.py`
- Create: `tests/modules/site_management/test_scheduler.py`

- [ ] **Step 1: Write tests**

Test cases:
- `test_sync_iterates_all_active_sources` — mock 3 WebDavSources, verify sync called for each
- `test_sync_skips_inactive_sources` — source with is_active=False is skipped
- `test_checkin_queries_enabled_accounts` — only checkin_enabled accounts are processed
- `test_single_source_failure_continues` — one source fails, others still sync

- [ ] **Step 2: Run tests to verify they fail**

- [ ] **Step 3: Implement SiteManagementScheduler**

Extract from `src/services/system/maintenance_scheduler.py`. The scheduler class:
- `start()`: register 3 cron jobs using `get_scheduler()` — same time config keys from SystemConfig
- `stop()`: remove the 3 jobs
- `_perform_site_account_sync()`: iterate `db.query(WebDavSource).filter(WebDavSource.is_active, WebDavSource.sync_enabled)`, call SnapshotService + AccountSyncService for each. Log via LogService
- `_perform_site_account_checkin()`: same as current maintenance_scheduler logic but import SiteAccount from module models
- `_perform_site_account_balance_sync()`: same as current but import from module models

Job IDs: `site_account_sync`, `site_account_checkin`, `site_account_balance_sync`

- [ ] **Step 4: Run tests**

- [ ] **Step 5: Commit**

```bash
git add src/modules/site_management/services/scheduler.py tests/modules/site_management/test_scheduler.py
git commit -m "feat(site-mgmt): add SiteManagementScheduler"
```

---

## Chunk 3: Backend API & Module Registration

### Task 11: Schemas

**Files:**
- Create: `src/modules/site_management/schemas.py`

- [ ] **Step 1: Define Pydantic models**

Request models:
- `CreateWebDavSourceRequest` (name, url, username, password)
- `UpdateWebDavSourceRequest` (name?, url?, username?, password?)
- `TriggerSyncRequest` (dry_run: bool = False, force_refresh: bool = False)
- `BatchAccountActionRequest` (account_ids: list[str] | None = None — None means all)

Response models:
- `WebDavSourceResponse` (all fields except decrypted password; password shown as masked)
- `SiteAccountResponse` (all fields, credentials masked)
- `SyncRunResponse`, `SyncItemResponse`
- `CheckinRunResponse`, `CheckinItemResponse`
- `PaginatedResponse[T]` (items, total, page, page_size)

- [ ] **Step 2: Commit**

```bash
git add src/modules/site_management/schemas.py
git commit -m "feat(site-mgmt): add Pydantic schemas"
```

### Task 12: Routes

**Files:**
- Create: `src/modules/site_management/routes.py`

- [ ] **Step 1: Implement all API endpoints**

Router prefix: `/api/admin/site-management` (set in ModuleDefinition, not in router)

Endpoint groups — reference the spec API table:

**Source CRUD:**
- `GET /sources` → `WebDavSourceService.list_all()`
- `POST /sources` → `WebDavSourceService.create()`
- `PUT /sources/{source_id}` → `WebDavSourceService.update()`
- `DELETE /sources/{source_id}` → `WebDavSourceService.delete()`
- `POST /sources/{source_id}/test` → `WebDavSourceService.test_connection()`
- `POST /sources/{source_id}/sync` → SnapshotService + AccountSyncService

**Account operations:**
- `GET /sources/{source_id}/accounts` → query SiteAccount by webdav_source_id with pagination
- `POST /sources/{source_id}/accounts/{account_id}/checkin` → AccountOpsService.checkin()
- `POST /sources/{source_id}/accounts/{account_id}/balance` → AccountOpsService.query_balance()
- `POST /sources/{source_id}/accounts/checkin` → batch checkin
- `POST /sources/{source_id}/accounts/balance` → batch balance

**History:**
- `GET /sync-runs` → query SiteSyncRun (optional source_id filter)
- `GET /sync-runs/{run_id}/items` → query SiteSyncItem
- `GET /checkin-runs` → query SiteCheckinRun
- `GET /checkin-runs/{run_id}/items` → query SiteCheckinItem

All endpoints require admin auth (use existing `get_current_admin_user` dependency from `src/api/base/`).

- [ ] **Step 2: Verify router imports**

```bash
python -c "from src.modules.site_management.routes import router; print(router.routes)"
```

- [ ] **Step 3: Commit**

```bash
git add src/modules/site_management/routes.py
git commit -m "feat(site-mgmt): add API routes"
```

### Task 13: Update ModuleDefinition

**Files:**
- Modify: `src/modules/site_management/__init__.py`

- [ ] **Step 1: Update module registration**

Change `router_factory` to import from `src.modules.site_management.routes` (was `src.api.admin.site_management`).

Add `on_startup` hook to start the scheduler:
```python
async def _on_startup() -> None:
    from src.modules.site_management.services.scheduler import SiteManagementScheduler
    scheduler = SiteManagementScheduler()
    await scheduler.start()
```

Add `on_shutdown` hook to stop the scheduler.

Update `services/__init__.py` to export all services.

- [ ] **Step 2: Verify module auto-discovery works**

```bash
python -c "from src.modules import ALL_MODULES; print([m.metadata.name for m in ALL_MODULES])"
```

Expected: `site_management` appears in list

- [ ] **Step 3: Commit**

```bash
git add src/modules/site_management/__init__.py src/modules/site_management/services/__init__.py
git commit -m "feat(site-mgmt): update ModuleDefinition with startup hook"
```

---

## Chunk 4: Cleanup Old Code

### Task 14: Remove old site_management services

**Files:**
- Delete: `src/services/site_management/` (entire directory)

- [ ] **Step 1: Delete directory**

```bash
rm -rf src/services/site_management/
```

- [ ] **Step 2: Commit**

```bash
git add -A src/services/site_management/
git commit -m "refactor(site-mgmt): remove old services directory"
```

### Task 15: Remove old API routes

**Files:**
- Delete: `src/api/admin/site_management.py`

- [ ] **Step 1: Delete file**

```bash
rm src/api/admin/site_management.py
```

- [ ] **Step 2: Commit**

```bash
git add -A src/api/admin/site_management.py
git commit -m "refactor(site-mgmt): remove old API routes file"
```

### Task 16: Remove models from database.py and Provider coupling

**Files:**
- Modify: `src/models/database.py`

- [ ] **Step 1: Remove Provider.site_accounts relationship**

Delete lines around 799:
```python
    site_accounts = relationship(
        "SiteAccount",
        back_populates="provider",
        passive_deletes=True,
    )
```

- [ ] **Step 2: Remove all Site* model classes**

Delete these classes from `src/models/database.py`:
- `SiteSyncRun` (line 2159)
- `SiteSyncItem` (line 2195)
- `SiteCheckinRun` (line 2232)
- `SiteCheckinItem` (line 2264)
- `SiteSourceSnapshot` (line 2298)
- `SiteAccount` (line 2328)

- [ ] **Step 3: Verify database.py still imports cleanly**

```bash
python -c "from src.models.database import Provider, User"
```

- [ ] **Step 4: Commit**

```bash
git add src/models/database.py
git commit -m "refactor(site-mgmt): remove Site* models from database.py"
```

### Task 17: Clean up maintenance_scheduler.py

**Files:**
- Modify: `src/services/system/maintenance_scheduler.py`

- [ ] **Step 1: Remove all site_account code**

Delete:
- Imports: `SiteAccount`, `SiteAccountOpsService`, `SiteAccountSyncService`, `SiteManagementLogService`, `SiteSnapshotService`, `CheckinItemLog`
- Job ID constants: `SITE_ACCOUNT_SYNC_JOB_ID`, `SITE_ACCOUNT_CHECKIN_JOB_ID`, `SITE_ACCOUNT_BALANCE_SYNC_JOB_ID`
- Time config methods: `_get_site_account_sync_time()`, `_get_site_account_checkin_time()`, `_get_site_account_balance_sync_time()`
- Update methods: `update_site_account_sync_time()`, `update_site_account_checkin_time()`, `update_site_account_balance_sync_time()`
- Job registration in `start_all()`: the 3 `scheduler.add_cron_job()` calls for site_account
- Scheduled wrappers: `_scheduled_site_account_sync()`, `_scheduled_site_account_checkin()`, `_scheduled_site_account_balance_sync()`
- Perform methods: `_perform_site_account_sync()`, `_perform_site_account_checkin()`, `_perform_site_account_balance_sync()`

- [ ] **Step 2: Verify file parses**

```bash
python -c "from src.services.system.maintenance_scheduler import MaintenanceScheduler"
```

- [ ] **Step 3: Commit**

```bash
git add src/services/system/maintenance_scheduler.py
git commit -m "refactor(site-mgmt): remove site_account code from MaintenanceScheduler"
```

### Task 18: Clean up system.py config handlers

**Files:**
- Modify: `src/api/admin/system.py`

- [ ] **Step 1: Remove site_account time config handlers**

Delete the 3 blocks around lines 682-705 that handle `site_account_sync_time`, `site_account_checkin_time`, `site_account_balance_sync_time`.

- [ ] **Step 2: Verify file parses**

```bash
python -c "from src.api.admin.system import router"
```

- [ ] **Step 3: Commit**

```bash
git add src/api/admin/system.py
git commit -m "refactor(site-mgmt): remove site_account config handlers from system API"
```

### Task 19: Fix remaining import references

- [ ] **Step 1: Search for stale references**

```bash
grep -rn "from src.services.site_management" src/ --include="*.py"
grep -rn "from src.models.database import.*SiteAccount" src/ --include="*.py"
grep -rn "from src.api.admin.site_management" src/ --include="*.py"
```

- [ ] **Step 2: Fix any remaining references found**

Each reference should be either deleted or redirected to `src.modules.site_management.*`.

- [ ] **Step 3: Run full test suite**

```bash
pytest --tb=short -q
```

Expected: all existing tests pass (some old site_management tests may need updating — see next task)

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor(site-mgmt): fix all stale import references"
```

### Task 20: Update/remove old tests

**Files:**
- Delete or update: `tests/api/admin/test_site_management_api.py`
- Delete or update: `tests/api/admin/test_site_management_site_account_ops_api.py`
- Delete or update: `tests/services/site_management/`
- Delete: `tests/services/test_maintenance_scheduler_site_account_jobs.py`
- Delete: `tests/models/test_site_account_models.py`
- Delete: `tests/alembic/test_site_account_migration.py`

- [ ] **Step 1: Delete old test files that test removed code**

```bash
rm -rf tests/api/admin/test_site_management_api.py
rm -rf tests/api/admin/test_site_management_site_account_ops_api.py
rm -rf tests/services/site_management/
rm -rf tests/services/test_maintenance_scheduler_site_account_jobs.py
rm -rf tests/models/test_site_account_models.py
rm -rf tests/alembic/test_site_account_migration.py
```

- [ ] **Step 2: Run full test suite**

```bash
pytest --tb=short -q
```

Expected: no failures related to site_management

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "refactor(site-mgmt): remove old site_management tests"
```

---

## Chunk 5: Frontend

### Task 21: API client & types

**Files:**
- Create: `frontend/src/features/site-management/api.ts`
- Create: `frontend/src/features/site-management/types.ts`

- [ ] **Step 1: Define TypeScript types**

`types.ts`:
- `WebDavSource` interface (id, name, url, username, is_active, sync_enabled, last_sync_at, last_sync_status, created_at, updated_at, account_count)
- `SiteAccount` interface (id, webdav_source_id, domain, site_url, architecture_id, base_url, auth_type, checkin_enabled, balance_sync_enabled, is_active, last_checkin_*, last_balance_*, created_at, updated_at)
- `SyncRun`, `SyncItem`, `CheckinRun`, `CheckinItem` interfaces
- `PaginatedResponse<T>` generic interface

- [ ] **Step 2: Implement API client**

`api.ts`: axios-based functions matching all API routes from spec. Use existing axios instance pattern from `frontend/src/api/`.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/features/site-management/
git commit -m "feat(site-mgmt): add frontend API client and types"
```

### Task 22: WebDav source list page

**Files:**
- Create: `frontend/src/features/site-management/components/WebDavSourceCard.vue`
- Create: `frontend/src/features/site-management/components/WebDavSourceFormDialog.vue`
- Create: `frontend/src/views/admin/SiteSourceList.vue`

- [ ] **Step 1: Implement WebDavSourceCard.vue**

Card showing: name, URL (truncated), account count, last_sync_at + status badge, action buttons (Sync, Edit, Delete).

- [ ] **Step 2: Implement WebDavSourceFormDialog.vue**

Dialog with form: name, url, username, password (masked input). Create and edit modes. Test connection button.

- [ ] **Step 3: Implement SiteSourceList.vue**

Page: header with "Add Source" button, grid of WebDavSourceCards. Empty state when no sources.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/features/site-management/components/WebDavSourceCard.vue frontend/src/features/site-management/components/WebDavSourceFormDialog.vue frontend/src/views/admin/SiteSourceList.vue
git commit -m "feat(site-mgmt): add WebDav source list page"
```

### Task 23: Source detail & account management page

**Files:**
- Create: `frontend/src/features/site-management/components/AccountTable.vue`
- Create: `frontend/src/features/site-management/components/AccountDetailDrawer.vue`
- Create: `frontend/src/views/admin/SiteSourceDetail.vue`

- [ ] **Step 1: Implement AccountTable.vue**

Table columns: domain, auth_type, is_active, last_checkin (status + time), last_balance (total + currency + time). Row actions: Checkin, Balance. Batch selection for bulk ops. Search by domain.

- [ ] **Step 2: Implement AccountDetailDrawer.vue**

Drawer showing: full account info, masked credentials, execution config (architecture_id, base_url, auth_type), last execution details.

- [ ] **Step 3: Implement SiteSourceDetail.vue**

Page: source info header (name, url, sync status), AccountTable below. Sync button in header. Breadcrumb navigation back to source list.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/features/site-management/components/AccountTable.vue frontend/src/features/site-management/components/AccountDetailDrawer.vue frontend/src/views/admin/SiteSourceDetail.vue
git commit -m "feat(site-mgmt): add source detail and account management page"
```

### Task 24: History pages

**Files:**
- Create: `frontend/src/features/site-management/components/SyncHistoryTable.vue`
- Create: `frontend/src/features/site-management/components/CheckinHistoryTable.vue`
- Create: `frontend/src/views/admin/SiteSyncHistory.vue`
- Create: `frontend/src/views/admin/SiteCheckinHistory.vue`

- [ ] **Step 1: Implement SyncHistoryTable.vue**

Table: timestamp, source name, status, total/matched/created/updated counts. Expandable rows for item details.

- [ ] **Step 2: Implement CheckinHistoryTable.vue**

Table: timestamp, trigger source, status, total/success/failed/skipped. Expandable rows for item details.

- [ ] **Step 3: Implement page wrappers**

`SiteSyncHistory.vue` and `SiteCheckinHistory.vue`: page header + filter by source dropdown + history table.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/features/site-management/components/ frontend/src/views/admin/SiteSyncHistory.vue frontend/src/views/admin/SiteCheckinHistory.vue
git commit -m "feat(site-mgmt): add sync and checkin history pages"
```

### Task 25: Router & cleanup old frontend

**Files:**
- Modify: `frontend/src/router/index.ts`
- Delete: `frontend/src/views/admin/SiteManagement.vue`

- [ ] **Step 1: Update router**

Replace the single `site-management` route with:
```typescript
{ path: '/admin/site-management', component: () => import('@/views/admin/SiteSourceList.vue') },
{ path: '/admin/site-management/:sourceId', component: () => import('@/views/admin/SiteSourceDetail.vue') },
{ path: '/admin/site-management/history/sync', component: () => import('@/views/admin/SiteSyncHistory.vue') },
{ path: '/admin/site-management/history/checkin', component: () => import('@/views/admin/SiteCheckinHistory.vue') },
```

- [ ] **Step 2: Delete old SiteManagement.vue**

```bash
rm frontend/src/views/admin/SiteManagement.vue
```

- [ ] **Step 3: Verify frontend builds**

```bash
cd frontend && npm run build
```

Expected: builds without errors

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(site-mgmt): update router, delete old SiteManagement.vue"
```

---

## Chunk 6: Final Verification

### Task 26: End-to-end smoke test

- [ ] **Step 1: Start backend**

```bash
docker compose -f docker-compose.build.yml up -d postgres redis
uv run alembic upgrade head
./dev.sh
```

- [ ] **Step 2: Verify module loads**

Check server logs for `Discovered module: site_management` message.

- [ ] **Step 3: Test API endpoints**

```bash
# Create source
curl -X POST http://localhost:8084/api/admin/site-management/sources \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"name":"Test","url":"https://example.com/dav","username":"user","password":"pass"}'

# List sources
curl http://localhost:8084/api/admin/site-management/sources \
  -H "Authorization: Bearer <token>"
```

- [ ] **Step 4: Verify no Provider coupling**

```bash
grep -rn "from src.models.database import.*Provider" src/modules/site_management/ --include="*.py"
grep -rn "from src.services.provider_sync" src/modules/site_management/ --include="*.py"
grep -rn "from src.services.provider_ops.service" src/modules/site_management/ --include="*.py"
```

Expected: zero matches for all three

- [ ] **Step 5: Run full test suite**

```bash
pytest --tb=short -q
```

Expected: all tests pass

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "feat(site-mgmt): complete independent module implementation"
```
