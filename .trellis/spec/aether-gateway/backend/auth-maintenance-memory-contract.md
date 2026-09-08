# Authentication Maintenance Memory Contract

## 1. Scope / Trigger

This contract applies to AntiGravity bearer resolution, OAuth maintenance,
model-fetch startup, Pool score rebuilds, Provider Catalog reads used by those
workers, and ordinary text candidate execution. It is required when a
deployment has many Provider Keys, because memory and database transfer must
be bounded by active attempts rather than total Key count.

## 2. Signatures

```rust
const AETHER_AUTH_MAINTENANCE_CONCURRENCY: usize = 4; // normalized to 1..=64

async fn find_system_config_revision_strong(
    &self,
    key: &str,
) -> Result<Option<u64>, DataLayerError>;

async fn find_system_config_value_strong(
    &self,
    key: &str,
) -> Result<Option<StoredSystemConfigValue>, DataLayerError>;

async fn list_auth_maintenance_candidates_by_provider_ids(
    &self,
    provider_ids: &[String],
) -> Result<Vec<StoredProviderCatalogAuthMaintenanceCandidate>, DataLayerError>;

async fn list_model_fetch_candidates_by_provider_ids(
    &self,
    provider_ids: &[String],
) -> Result<Vec<StoredProviderCatalogModelFetchCandidate>, DataLayerError>;

async fn resolve_and_rank_logical_local_candidate_snapshots(
    /* compact candidates and ranking context */
) -> Result<Vec<RankedLocalExecutionCandidate>, GatewayError>;

async fn hydrate_ranked_local_execution_candidate(
    /* one selected ranked candidate and full gates */
) -> Result<Option<EligibleLocalExecutionCandidate>, GatewayError>;
```

`StoredProviderCatalogAuthMaintenanceCandidate` contains only auth
eligibility fields. `StoredProviderCatalogModelFetchCandidate` contains only
model-discovery eligibility, filters, allowed models, API formats, and
metadata for active auto-fetch Keys. Neither projection may contain API keys,
auth config, quota/status snapshots, request bodies, or complete Provider and
Endpoint configuration.

## 3. Contracts

- AntiGravity bearer resolution first performs a strong revision-only read.
  An unchanged revision reuses the immutable `Arc` snapshot without reading
  or cloning the JSON value. A mismatch is serialized by one async
  singleflight lock, rechecks revision, then performs one atomic
  `{revision, value}` read and owned parse.
- Parsed, JSON-null, and invalid config outcomes are all cached by revision.
  Invalid input remains fail-closed with the same error, but must not cause
  repeated full reads or parses.
- Ordinary system-config deletion writes a JSON-null tombstone, clears its
  description, and increments revision. Normal find/list/admin APIs hide the
  tombstone; revision-only and atomic revision/value reads retain visibility
  so delete/recreate cannot reuse an old bearer snapshot.
- A successful string compare-and-set also increments revision in the same
  memory/PostgreSQL mutation. A rejected comparison leaves both value and
  revision unchanged, so the immutable auth snapshot cannot miss a CAS write.
- Bearer-derived auth-context cache keys use a one-way bearer hash and must not
  retain the raw authorization bearer.
- OAuth refresh and account self-check share one process-wide
  `AuthMaintenanceGate`. A worker may scan lightweight candidates first, but
  it acquires a permit before loading a full Key, decrypting transport, or
  issuing a quota/OAuth operation, and holds it until full objects are dropped.
- Routed and same-format text candidates aggregate and globally rank
  `RankedLocalExecutionCandidate` values only. `StoredMinimalCandidateRoutingFacts`
  carries compact Pool, auth-channel, conversion-priority, and proxy-affinity
  facts; it must not carry proxy URLs, credentials, raw auth JSON, Provider
  config, or `GatewayProviderTransportSnapshot`.
- `CandidateResolvedPageSnapshot` is transport-free. `next_attempt()` hydrates
  only the selected candidate and reruns existing full transport gates. A
  rejected candidate records the same skip reason and advances through the
  already globally-ranked fallback chain.
- Model-fetch target collection and Provider-wide whitelist reconciliation
  independently read `list_model_fetch_candidates_by_provider_ids`, preserving
  current-batch writes and concurrent admin changes without loading full Keys.
- Per-Key model discovery still persists and associates its result, but
  Provider-wide whitelist availability reconciliation runs once per successful
  Provider per batch, not once per successful Key.
- Pool score rebuild selects IDs from maintenance summaries and performs only
  single-Key strong reads. It must not pre-load a multi-Key full catalog. The
  worker performs exactly one startup rebuild: after explicit startup work it
  consumes the interval's immediate tick before entering the periodic loop.

## 4. Validation & Error Matrix

| Condition | Required behavior |
|---|---|
| Unchanged bearer-config revision | Reuse cached outcome; do not read or parse the full value |
| Changed bearer-config revision | One singleflight leader reads/parses; followers reuse its outcome |
| Null or invalid bearer config | Cache the negative/error outcome for that revision and fail closed |
| Delete then recreate system config | Tombstone and recreate each increment revision; old snapshot cannot match |
| Lightweight candidate scan | Exclude credentials and large JSON |
| Full Key read | Occur only for the current permitted or attempted Key |
| Routed candidate invalid during hydration | Continue in existing global order and hydrate only the next attempt |
| Model-fetch batch has many Keys for one Provider | Persist each Key result; reconcile Provider whitelist once |
| Inactive or non-auto-fetch model candidate | Do not load `upstream_metadata` |
| Pool score startup | Run one rebuild with no multi-Key full catalog read |
| Task cancellation or early return | RAII releases permits and full attempt state |

## 5. Good / Base / Bad Cases

- Good: 10,000 bearer hashes and 128 concurrent requests at one revision cause
  one full JSON load/parse.
- Good: 2,048 routed candidates cause zero transport reads during ranking and
  one read per actual fallback attempt.
- Good: 50 model-fetch Keys for one Provider issue one Provider-wide
  reconciliation and use two compact projection reads; Pool score reads each
  selected Key individually during one startup round.
- Base: a Provider with a few Keys keeps existing refresh, ordering, fallback,
  model-discovery, and score results through the same paths.
- Bad: loading every complete Key, decrypting every candidate transport, or
  invalidating and reloading the full Provider Key catalog once per fetched Key.

## 6. Tests Required

- Bearer tests assert 10,000 hashes with 128 concurrent refreshes perform one
  full load/parse, a new revision is recognized within the same second,
  cross-node revocation is observed, and null/invalid outcomes are cached.
- System-config tests assert revision-only SQL excludes `value`, delete creates
  an incrementing tombstone, repeated delete returns false, and recreate
  advances revision for memory and PostgreSQL. String CAS must advance revision
  only on success; live PostgreSQL round trips require a dedicated test database.
- Candidate tests use 2,048 candidates and assert zero full transport reads
  before ranking, one read for the first attempt, and a second read only after
  the first candidate becomes invalid.
- Model-fetch tests assert the compact projection excludes secret/heavy
  columns in memory and PostgreSQL, and Provider reconciliation occurs
  once per successful Provider.
- Pool-score tests reject multi-Key full reads, assert one startup round, and
  preserve score results.
- Run formatting, targeted tests, affected-package `cargo check --all-targets`,
  and a source Docker Compose build before completion. Runtime validation must
  compare peak RSS, allocator allocated/resident, database query counts, and
  network transfer at realistic Key counts.

## 7. Wrong vs Correct

### Wrong

```rust
let revisioned = state
    .find_system_config_value_with_revision_strong(CONFIG_KEY)
    .await?;
for candidate in routed_candidates {
    let transport = read_transport(candidate).await?;
    ranked.push(rank(candidate, transport));
}
```

### Correct

```rust
let revision = state.find_system_config_revision_strong(CONFIG_KEY).await?;
let config = antigravity_snapshot.load_or_parse(revision).await?;

let ranked = rank_compact_candidates(routed_candidates).await?;
while let Some(candidate) = ranked.next() {
    if let Some(attempt) = hydrate_ranked_local_execution_candidate(candidate).await? {
        return Ok(attempt);
    }
}
```
