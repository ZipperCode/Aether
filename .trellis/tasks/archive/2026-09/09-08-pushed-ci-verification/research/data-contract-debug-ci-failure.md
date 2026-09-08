# Data-contract CAS Debug CI failure receipt

- Status: READY, 2026-09-08. This assignment changed only `crates/aether-data/contracts/src/repository/provider_catalog/types.rs` and this receipt. The earlier color-fixture assignment was not reopened.
- Remote baseline: `948c1c16f2f9927b37a8570b86767128abd6b7c8`; Rust CI run `34210248162`, Workspace Rest job `102009266040`.

## Confirmed failure and root cause

The completed job log was inspected through `gh api repos/ZipperCode/Aether/actions/jobs/102009266040/logs`, emitting only the named test's narrow failure context. At `2026-09-08T09:36:40.2997868Z`, the test panicked at `types.rs:1224:9`:

```text
debug output: ProviderCatalogKeySchedulingStateCasUpdate { key_id: "key-debug", expected_auth_type: "oauth", updated_at_unix_secs: Some(125), .. }
```

The exact failed assertion was the helper's unconditional `debug.contains("[REDACTED]")`, before its canary non-disclosure checks. It was not a type-name assertion, and the shown scheduling output did not disclose any credential or scheduling evidence.

The existing scheduling CAS Debug implementation deliberately emits only `key_id`, `expected_auth_type`, and `updated_at_unix_secs`, then uses `finish_non_exhaustive()`. Its expected API-key ciphertext, expected auth-config ciphertext, previous scheduling object, and replacement scheduling object are entirely absent. The OAuth credential fence, credential replacement CAS, OAuth runtime CAS, and associated metadata expectation instead expose the existing redacted placeholders through their custom Debug implementations. Those implementations and the other related CAS Debug implementations were inspected; no runtime behavior change was required.

## Minimal correction and preserved contract

- Replace only scheduling's unsuitable generic-marker helper call with an exact assertion of its existing safe, non-exhaustive Debug output.
- Retain all four distinct sensitive scheduling canary values in the constructed input. Exact output equality rejects any leaked canary, extra sensitive field, or expansion to a derived full-struct Debug representation, while checking the three intended diagnostic fields.
- Leave `assert_debug_redacts()` unchanged, including its placeholder requirement and every secret non-disclosure assertion for the OAuth fence, credential replacement, and OAuth runtime cases.
- Keep the same test and all four CAS/fence cases. Add/update Chinese explanations identifying the omission-versus-placeholder distinction.
- No production Debug, serialization, repository/CAS semantics, credential treatment, helper relaxation, dependency, or workflow change. No new redaction layer or compatibility behavior.

## Exact targeted verification

Root granted this writer the exclusive main Cargo target interval; the runtime-data writer deferred Cargo throughout it. Native Windows, repository-pinned Rust 1.95.0, fresh `pwsh -NoProfile`, working directory `D:\Project\GitHub\Aether`:

```powershell
cargo test -p aether-data-contracts --lib provider_catalog_cas_debug_output_redacts_credential_fences --locked
```

- Before editing: FAIL, exit 1; `0 passed; 1 failed; 0 ignored; 224 filtered out`. Same `types.rs:1224:9` assertion and safe scheduling Debug output as CI. Compilation completed in 1m 13s; execution finished in 0.00s. Session `10227` completed.
- After editing: PASS, exit 0; `1 passed; 0 failed; 0 ignored; 224 filtered out`. Incremental compilation completed in 10.27s; execution finished in 0.00s. Session `94733` completed.
- `rustfmt --edition 2021 --check crates/aether-data/contracts/src/repository/provider_catalog/types.rs`: PASS.
- `git diff --check -- crates/aether-data/contracts/src/repository/provider_catalog/types.rs`: PASS; only Git's normal LF-to-CRLF checkout advisory was emitted.

No other test filter or suite was run. The passing targeted test executes scheduling, OAuth credential fence, credential replacement, and OAuth runtime assertions together; its original failure had stopped before the latter cases.

## Handoff and limits

- Main Cargo target ownership was returned to Root immediately after both sessions exited; no Cargo process remains owned by this assignment, and no additional Cargo run will be started.
- No separate target, temporary fixture, database, service, external provider, or persistent diagnostic process was created. Normal ignored Cargo artifacts remain in the shared repository target; other owners' work is preserved.
- Final exact-SHA Linux CI and any tests not reached in the first fail-fast Workspace Rest job remain Root's combined-push follow-up. This native targeted result is not an overall CI success claim.
- No staging, commit, push, workflow operation, release, deployment, or change outside the two assigned files.
