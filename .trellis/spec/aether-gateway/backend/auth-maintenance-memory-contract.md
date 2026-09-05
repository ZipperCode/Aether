# Authentication Maintenance Memory Contract

## 1. Scope / Trigger

This contract applies to OAuth token refresh, account self-check, Provider
Catalog reads used by either worker, and ordinary text candidate execution.
It is required when a deployment has many Provider Keys or accepts large
request bodies, because maintenance memory and candidate body memory must be
bounded by active work rather than total Key count.

## 2. Signatures

```rust
const AETHER_AUTH_MAINTENANCE_CONCURRENCY: usize = 4; // default, normalized to 1..=64

pub(crate) fn shared_auth_maintenance_gate() -> AuthMaintenanceGate;

async fn list_auth_maintenance_candidates_by_provider_ids(
    &self,
    provider_ids: &[String],
) -> Result<Vec<StoredProviderCatalogAuthMaintenanceCandidate>, DataLayerError>;

async fn list_provider_catalog_keys_by_ids_strong(
    &self,
    key_ids: &[String],
) -> Result<Vec<StoredProviderCatalogKey>, GatewayError>;
```
`StoredProviderCatalogAuthMaintenanceCandidate` contains only `id`,
`provider_id`, `is_active`, `auth_type`, `has_auth_config`, OAuth expiry, and
OAuth invalid-state fields. It must not contain credential ciphertext,
`upstream_metadata`, `status_snapshot`, or a request body.

## 3. Contracts

- OAuth refresh and account self-check clone one process-wide
  `AuthMaintenanceGate`; the configured value comes from
  `AETHER_AUTH_MAINTENANCE_CONCURRENCY`, defaults to `4`, and clamps to
  `1..=64`. Invalid or blank values use the default.
- A worker may scan lightweight candidate projections before waiting, but it
  must acquire an owned permit before loading a full Key, decrypting a
  transport snapshot, or issuing a quota/OAuth operation. The permit remains
  held until those full objects are dropped.
- OAuth refresh uses the projection repository method and then performs a
  single-Key strong read after permit acquisition. It must not call a
  provider-wide full-Key list method.
- Account self-check keeps its Provider interval, per-Provider selection
  limit, and Pool score behavior. A Key deleted, disabled, or runtime-blocked
  after projection is not counted as an executed self-check.
- The cached Provider Catalog wrapper delegates the maintenance projection
  directly to its inner repository. It must not cache a full catalog snapshot
  for this scan, because per-Key writes would repeatedly invalidate it.
- Ordinary Chat, Responses, and same-format candidates use the paged dynamic
  attempt source. Only the attempt currently returned by `next_attempt()` may
  materialize a provider request body or report context; draining or stopping
  the source must not build bodies for remaining candidates.

## 4. Validation & Error Matrix

| Condition | Required behavior |
|---|---|
| Missing, blank, invalid, or oversized concurrency env | Use `4`, or clamp valid values to `1..=64` |
| Lightweight candidate scan | Return only eligibility fields; exclude credentials and large JSON |
| Full Key read | Occurs only after a shared permit is acquired and is limited to the current Key |
| OAuth/self-check overlap | Both consume the same process-wide permit pool |
| Task cancellation or early return | RAII permit release makes the slot immediately reusable |
| Key becomes absent/inactive/runtime-blocked after projection | Skip execution and exclude it from executed self-check counts |
| Ordinary candidate source is stopped | Drop unmaterialized candidates without creating request bodies |
| Special image/file/video bridge | Existing static candidate behavior may remain when its contract requires it |

## 5. Good / Base / Bad Cases

- Good: 6,000 OAuth candidates remain lightweight, four full credentials are
  in flight at most, and each completed attempt releases its permit and
  transport before the next one proceeds.
- Base: a single-Endpoint Provider with a few Keys keeps the existing refresh,
  self-check, and Pool score results while using the same gate.
- Bad: loading all complete Keys, decrypting every transport, or cloning a
  500 KiB request body once per candidate before execution.

## 6. Tests Required

- Gate unit tests assert default parsing, `1..=64` normalization, shared
  OAuth/self-check capacity, 6,000 candidates, and cancellation release.
- Data adapter tests for memory, SQLite, PostgreSQL, and MySQL assert the
  projection fields and verify large credential/status columns are absent from
  the SELECT projection.
- Gateway tests assert OAuth source-method selection, single-Key strong reads,
  post-projection eligibility revalidation, self-check summary counts, and
  cache bypass.
- Candidate tests use thousands of candidates and a request body of at least
  500 KiB; they assert one body for the first attempt and zero bodies for
  drained candidates.
- Run `cargo fmt --all --check`, targeted tests, affected-package
  `cargo check --all-targets`, and a source Docker Compose build before a
  completion claim.

## 7. Wrong vs Correct

### Wrong

```rust
let keys = catalog.list_keys_by_provider_ids(provider_ids).await?;
for key in keys {
    let body = request_body.clone();
    spawn_refresh(key, body);
}
```

### Correct

```rust
let candidates = catalog
    .list_auth_maintenance_candidates_by_provider_ids(provider_ids)
    .await?;
for candidate in candidates {
    let _permit = shared_auth_maintenance_gate().acquire().await?;
    let key = state.list_provider_catalog_keys_by_ids_strong(&[candidate.id]).await?;
    run_one_refresh(key).await?;
}
```
