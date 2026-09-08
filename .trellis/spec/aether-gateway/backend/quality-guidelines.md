# Quality Guidelines

> Code quality standards for backend development.

---

## Overview

<!--
Document your project's quality standards here.

Questions to answer:
- What patterns are forbidden?
- What linting rules do you enforce?
- What are your testing requirements?
- What code review standards apply?
-->

(To be filled by the team)

---

## Scenario: Bounded upstream response bodies

### 1. Scope / Trigger

- Apply this contract whenever an `ExecutionPlan` may buffer an upstream response or decode a compressed response body.
- Recheck every transport path when either execution-only response-limit header changes; direct HTTP, tunnel relay, and scoped model-fetch requests share the same resolver.

### 2. Signatures

- Public execution contract: `aether_contracts::EXECUTION_REQUEST_MAX_RESPONSE_BODY_BYTES_HEADER` (`x-aether-execution-max-response-body-bytes`).
- Gateway-scoped contract: `EXECUTION_RESPONSE_BODY_LIMIT_HEADER` (`x-aether-execution-response-body-limit-bytes`).
- Operator cap: `AETHER_MAX_INTERNAL_BUFFERED_BODY_MB`.
- Resolver: `execution_plan_response_body_limit_bytes(&ExecutionPlan) -> usize`.
- Scoped injector: `with_upstream_response_body_limit(&ExecutionPlan, usize) -> ExecutionPlan`.

### 3. Contracts

- A valid public execution-header value is parsed as an exact byte cap. It preserves existing callers such as provider quota probes, including intentionally tiny test limits.
- A gateway-scoped value must be positive and is clamped to `64 KiB..=64 MiB`; an invalid scoped value fails closed to the `8 MiB` scoped default.
- `AETHER_MAX_INTERNAL_BUFFERED_BODY_MB` is converted from MiB to bytes. An absent, zero, or invalid value leaves the operator cap unlimited.
- The effective cap is the minimum of the valid public execution cap, the normalized gateway-scoped cap, and the operator cap. One scope must never raise another scope's stricter limit.
- Both execution-only headers are consumed by the gateway and must be removed before forwarding provider request headers.
- The same effective cap applies to wire bytes and to bytes after supported content decoding.

### 4. Validation & Error Matrix

| Condition | Required result |
|---|---|
| Public header is a valid `u64` representable as `usize` | Use the exact value as one cap |
| Public header is malformed or overflows `usize` | Ignore it and retain the remaining caps |
| Scoped header is missing | Use the operator cap unless the public cap is stricter |
| Scoped header is invalid or zero | Use the `8 MiB` scoped default, then apply stricter caps |
| Scoped header is below `64 KiB` or above `64 MiB` | Clamp it to the nearest scoped bound |
| Wire body crosses the effective cap | Return `UpstreamResponseTooLarge { phase: Wire, ... }` (`response_too_large`) |
| Decoded body crosses the effective cap | Return `UpstreamResponseTooLarge { phase: Decoded, ... }` (`response_too_large`) |

### 5. Good / Base / Bad Cases

- Good: public `1 MiB`, scoped `8 MiB`, operator `64 MiB` resolves to `1 MiB`.
- Base: no per-plan headers resolves to the operator cap.
- Bad: reading only the scoped header silently disables the public quota-probe limit and permits oversized or gzip-expanded bodies.

### 6. Tests Required

- `direct_sync_rejects_declared_response_larger_than_limit`: rejects a declared wire body above the public cap.
- `direct_sync_rejects_streamed_response_crossing_limit`: rejects chunked accumulation above the public cap.
- `direct_sync_rejects_gzip_expansion_crossing_limit`: rejects decoded expansion above the public cap.
- Scoped parsing/injection tests must assert clamping, invalid fallback, global-cap precedence, and case-insensitive replacement.
- Request-header materialization tests must assert that neither execution-only header reaches the provider.

### 7. Wrong vs Correct

#### Wrong

```rust
// Ignores the shared execution contract used by existing callers.
effective_response_body_limit_bytes(scoped_header, operator_limit)
```

#### Correct

```rust
let scoped_limit = effective_response_body_limit_bytes(scoped_header, operator_limit);
public_limit.map_or(scoped_limit, |limit| limit.min(scoped_limit))
```

---

## Forbidden Patterns

### Bounded authentication maintenance and candidate materialization

- OAuth token refresh and account self-check share the process-wide
  `AETHER_AUTH_MAINTENANCE_CONCURRENCY` semaphore. The normalized value defaults
  to `4` and is bounded to `1..=64`.
- A maintenance permit must be acquired before a strong Key/transport read and
  must remain owned through the single upstream authentication or quota call.
  RAII permit ownership is required so cancellation and error paths release the
  slot without manual bookkeeping.
- Maintenance scans first use the lightweight auth-maintenance projection. It
  must not return encrypted credentials, `upstream_metadata`,
  `status_snapshot`, or request bodies. A complete Key is loaded by ID only
  after a permit is acquired and is released before the next candidate starts.
- Ordinary Chat, Responses, family, and same-format text requests use the
  paged dynamic attempt source. Do not restore eager `Vec` plan/report builders
  that construct a request body for every candidate. Static materialization is
  limited to specialized image/file/video bridges whose persistence contract
  requires it.
- If a routing policy requires global ranking, preserve that ordering contract
  explicitly; do not remove the policy-wide collection as an incidental memory
  optimization. Any replacement must provide an equivalent bounded ranking
  algorithm and regression coverage.

### Required checks for this contract

- Unit tests cover default/invalid/upper-bound concurrency, 6,000 candidates,
  combined OAuth/self-check occupancy, and cancelled waiters.
- PostgreSQL and memory repositories expose the same lightweight
  projection and tests assert that secret/heavy result columns are absent.
- Read-only candidate fixtures must seal API keys with
  `AppState::seal_provider_catalog_key_api_key` for each actual Provider/Key ID.
  Cloning one legacy Fernet envelope requires a migration writer and invalidates
  a fixture intended to measure lazy reads without persistence side effects.
- Source/architecture tests keep ordinary text planners on the dynamic source
  and keep OAuth away from the full provider-key-by-provider listing method.

<!-- Patterns that should never be used and why -->

(To be filled by the team)

---

## Required Patterns

<!-- Patterns that must always be used -->

(To be filled by the team)

---

## Testing Requirements

<!-- What level of testing is expected -->

(To be filled by the team)

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)

---

## Scenario: Shared gateway CI commands and build-version inputs

### 1. Scope / Trigger

- Apply when changing gateway build scripts, CI commands, toolchains, cache inputs,
  target selection, or local pre-push verification.
- A focused native check is not proof of Linux CI, real PostgreSQL, or release
  compatibility; retain the separate existing gates and exact-SHA release flow.

### 2. Signatures

```text
python tools/ci.py preflight
python tools/ci.py gateway
python tools/ci.py preflight --dry-run
python tests/ci_contract_test.py
python tests/gateway_build_watch_test.py
```

`preflight` runs format, gateway Clippy and gateway tests in that order.
The gateway command is `cargo nextest run -p aether-gateway --lib --bins
--no-fail-fast --locked`; it still builds distinct lib/bin test artifacts.

### 3. Contracts

- `tools/ci.py` owns the local/CI argument arrays and child profile overrides:
  incremental, dev debug and test debug are `0`; gateway test stack is 16 MiB.
- Linux mold flags are job-scoped before cache restoration. Local execution
  inherits its host linker; the entry does not install tools or modify `.env`.
- Gateway tests collect all failures in the selected lib/bin targets and return
  nonzero on failure. Do not retry or silently suppress failing tests.
- CI setup selects the repository's pinned Rust version. Build/toolchain,
  VSCodex and shared-check changes must trigger both push and PR verification.
- Version watches use Git-resolved HEAD and symbolic-ref paths, not an assumed
  `.git` directory. A packed-only branch temporarily watches its packed ref and
  closest existing parent, then returns to exact-file watches once loose.
- Source archives retain the stable `build.rs` and explicit environment watches.
  Keep version precedence and tunnel-tag exclusion unchanged; do not add broad
  Git/index/tag watching as an incidental speed fix.

### 4. Validation & Error Matrix

| Input or change | Required result |
| --- | --- |
| Unchanged normal/linked checkout | Build script remains Fresh, with zero executions |
| Current branch/HEAD or explicit version advances | Execute the script once and publish the new version |
| Packed branch becomes loose | Detect the change, then retain only exact-file watches |
| Source archive lacks Git | Stable package/version fallback without missing watch paths |
| Gateway has several failing tests | Collect them in one run and return failure |
| `--dry-run` | Print actual command/env plans without executing them or claiming PASS |

### 5. Good / Base / Bad Cases

- Good: a gateway contract change is checked through the same entry locally and
  in CI, so sibling fixture failures are visible together.
- Base: an unchanged worktree reuses its build-script result.
- Bad: checking only compilation after changing runtime contracts, repeatedly
  fixing the first remote failure, or claiming a command merge removes all
  compilation cost or proves an unmeasured CI speedup.

### 6. Tests Required

- The command fixture invokes the real dispatcher with mocked subprocesses and
  checks targets, order, env, failure propagation and nonexecuting dry-run.
- The build-watch fixture uses a dependency-free temporary Cargo package and
  observes actual script execution counts/version output for checkout,
  worktree, packed/detached refs, archive and explicit version inputs.
- Parse the workflow and retain all required job IDs and aggregate conditions.
  Configuration-only changes do not require rebuilding the full gateway.
- Report remote runtime improvement only after a comparable actual Actions run;
  separate compile/link time, test execution and summed runner time.

### 7. Wrong vs Correct

```text
Wrong: watch ../../.git/HEAD unconditionally; copy different local/CI commands.
Correct: resolve real Git inputs; call the same gateway check entry from both.
```
