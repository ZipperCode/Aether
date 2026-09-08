# Second CI: Kiro safe-region fixture

- Status: READY for the Root integrator; no Cargo command has run in this dispatch.
- Baseline: `9026380d1957e7a57a746a409335f15eb1675515`, Rust CI `34217818968`, Workspace Rest failure supplied by Root.
- Exact failed filter: `provider::providers::kiro::tests::refresh_urls_use_safe_default_for_malicious_auth_region`.
- CI assertion at `crates/aether-oauth/src/provider/providers/kiro.rs:1147:9`: actual last URL `https://q.us-east-1.amazonaws.com/ListAvailableProfiles`, expected `https://prod.us-east-1.auth.desktop.kiro.dev/refreshToken`.

## Root cause and source contract

`KiroProviderOAuthAdapter::refresh_auth_config` first calls social/IDC token refresh and then `with_discovered_profile_arn`. If the refreshed configuration has no Profile, `discover_kiro_profile_arn` calls `discover_kiro_profile_arn_in_region`. This is the same intentional discovery contract documented in `gateway-d-fixes.md`, now exposed by a lower-level OAuth fixture.

The failed social fixture supplied no Profile and used `StaticExecutor`, whose `Option<OAuthHttpRequest>` is overwritten on every request. Its static refresh JSON was also reused as the discovery response. The legitimate discovery call therefore replaced the refresh request before the old assertion inspected it. There is no evidence of unsafe token URL construction: `effective_auth_region` rejects the malicious `attacker.example/` and yields `us-east-1`; `effective_api_region` also yields the default, and the social region list deduplicates to one region.

The actual sequence remains:

1. POST `provider-oauth:kiro-social-refresh` to `https://prod.us-east-1.auth.desktop.kiro.dev/refreshToken` with the existing fake refresh token.
2. POST `provider-oauth:kiro-profile-discovery` to `https://q.us-east-1.amazonaws.com/ListAvailableProfiles` with the newly returned fake access token.

## Bounded changes

- Reuse the existing `RoutingExecutor` request vector and request-ID response routing; add only the observed social-refresh response arm. Keep its IDC/Profile responses and unexpected-request panic unchanged.
- Keep the malicious input and original exact safe refresh URL assertion. Expand the attacker-domain exclusion to both calls; assert exactly two requests in the expected order, both POST, exact discovery URL/Host, original refresh body, new access-token Bearer, and returned Profile.
- Keep all existing malicious/valid region cases, effective-region fallback assertions, production validation, and real network behavior unchanged. No extra helper, dependency, skip, relaxed assertion, or production compatibility path.
- Add Chinese documentation to the modified test, shared test executor, field, and executor method.

## Checks actually run

- `rustfmt --edition 2021 --config skip_children=true crates/aether-oauth/src/provider/providers/kiro.rs`: completed, owned file only.
- Same command with `--check`: PASS after the patch.
- `git diff --check -- crates/aether-oauth/src/provider/providers/kiro.rs`: PASS; only normal Git LF-to-CRLF policy warning.
- Read-only call/caller review: both `RoutingExecutor` consumers and both remaining `StaticExecutor` consumers inspected; the change is entirely under `#[cfg(test)]`. CodeGraph was tried first twice but did not resolve the target, so exact-file source reads supplied the evidence.

## Runtime checks not yet run

Root owns the main target slot. Suggested minimum selects the original failure and the existing IDC discovery sibling sharing `RoutingExecutor`:

```text
cargo nextest run -p aether-oauth --lib --locked --no-fail-fast -E 'test(=provider::providers::kiro::tests::refresh_urls_use_safe_default_for_malicious_auth_region) | test(=provider::providers::kiro::tests::refresh_discovers_missing_idc_profile_arn)'
```

No runtime-PASS claim, full OAuth/Gateway build, CI action, Git staging/commit/push, task-state/spec change, real provider/DB/OAuth operation, settings change, external credential access, or persistent process. No task-owned temporary artifacts to clean. Only this receipt and `crates/aether-oauth/src/provider/providers/kiro.rs` were written; ownership returns to Root when this receipt is delivered.
