# Provider / OAuth / HTTP merge receipt

## Scope and source

- Exclusive writer: `merge_provider`; only `crates/aether-provider/**`, `crates/aether-oauth/**`, `crates/aether-http/**` and this receipt.
- Merge ours `b0ad8ff7f7bdb1888abac8c06f47f61e76761431`, theirs `c7e403b410139d12a6189dda9c2bdf0c7c80782e`, base `7892aa94853461c1e634f7a5babbb1280128720f`.
- Read task PRD/design/implement/context manifests and the memory, balance, runtime-quota, model admission and Codex logical identity contracts. Upstream final policy supersedes intermediate opt-in DNS filtering; no provider DNS filter or allowlist was reintroduced here.

## Manual conflict paths (16)

```text
crates/aether-http/Cargo.toml
crates/aether-http/src/lib.rs
crates/aether-oauth/src/provider/providers/antigravity.rs
crates/aether-oauth/src/provider/providers/generic.rs
crates/aether-oauth/src/provider/providers/mod.rs
crates/aether-provider/pool/src/lib.rs
crates/aether-provider/pool/src/presets.rs
crates/aether-provider/pool/src/provider.rs
crates/aether-provider/pool/src/providers/antigravity.rs
crates/aether-provider/pool/src/quota.rs
crates/aether-provider/pool/src/quota_refresh.rs
crates/aether-provider/transport/src/antigravity/request.rs
crates/aether-provider/transport/src/codex_fingerprint.rs
crates/aether-provider/transport/src/generic_oauth/mod.rs
crates/aether-provider/transport/src/request_url/mod.rs
crates/aether-provider/transport/src/vertex/url.rs
```

Additional necessary adapters: Pool `providers/{deepseek,nous,official_api_key,openrouter}.rs` remove the retired `accept_invalid_certs` literal field; requests now use the upstream shared TLS policy. No external caller or root manifest was edited.

## Semantic result

- HTTP retains fork `BoundedBodyCollector`, bounded collection/decompression errors and internal/hop header filter, alongside upstream `ResponseBodyReadError`, bounded control-plane response reader, bounded DNS lookup and header-security helpers. Both families have actual current callers; neither was replaced with a compatibility shim.
- OAuth generic template uses the final tip environment/default-client credential resolution: native default Google clients work without new environment setup; custom clients require matching configured credentials. Test-only overrides and upstream error/JWT limits remain. Existing Codex persisted member fingerprint, access-token-rotation independence and old refresh metadata preservation remain intact.
- Antigravity exchange AND import resolve Google userinfo through the same network context when token payload has no email; existing identity enrichment and tests retained, upstream import tests included.
- `ProviderOutboundRequestContext` owns frozen logical-turn signals. `CodexFingerprintConvergenceContext` aliases that exact type, so HTTP/WS clone/restore has no conversion/reconstruction. Dispatcher signature remains upstream `apply_provider_outbound_request_policies(&transport, &str, &context, &mut BTreeMap<String,String>, &mut Value) -> Vec<ProviderOutboundRequestPolicyResult>`; results are categorical. Opt-in, ordinary auth channel parity, Agent Identity/Compact exclusion, Live headers-only and no removed-cache-key resurrection retained. Removed accidental duplicate type/functions produced by text merge.
- Pool retains balance threshold/freshness rules, ObservationOnly/SubscriptionExhaustionOnly capabilities, official provider adapters, bounded backoff, Zhipu scheduling fact and manual-recovery runtime block. Uses the newer upstream model/family/group quota resolver and timestamp precedence; removed the accidentally duplicated old resolver. Both account and NEW model hard-block entrypoints first honor persistent runtime block, regardless of a model bucket reporting capacity.
- Preserved cache-affinity modes, Nous-specific OAuth refresh, credential-generation cache fences, Gemini/Vertex countTokens, internal Gemini SSE framing and Antigravity private tool-name/schema normalization. Upstream custom path encoding, Vertex origin/resource rules, sensitive Debug boundaries and quota-summary request stay integrated.
- No eager multi-key transport hydration, catalog reread or new retry/configuration path added.

## Checks and handoff

- PASS: scoped `rustfmt --edition 2021 --config skip_children=true --check` over 19 manually touched Rust files (16 conflicts include one TOML, plus four literal adaptations).
- PASS: normal repository `git diff --check` for owned paths. A diagnostic invocation overriding `core.autocrlf=false` was discarded because it treated existing Windows CRLF as whitespace; no repository line-ending settings changed.
- PASS: owned conflict-marker scan = 0; source invariants assert one model resolver, one frozen context alias (no duplicate struct), model hard-block runtime fence, and zero retired quota-request TLS fields.
- Added runnable regression `runtime_quota_block_survives_model_specific_available_quota` in provider-pool. Existing balance/model tests, Codex fingerprint policy tests, Antigravity import/default credential tests and URL tests remain.
- NOT RUN: Cargo compile/test while other writers change shared root manifests/contracts, per dispatch restriction. Integrator should run affected package checks and focused tests for `aether-http`, `aether-oauth`, `aether-provider-pool`, `aether-provider-transport`; especially the new runtime-block test and HTTP/WS frozen-context callers.
- Cross-scope API notices delivered to Root/integrator: shared outbound context/dispatcher; removed quota-request `accept_invalid_certs`; retained GeminiCountTokens and HTTP helper exports. No outstanding interface decision.
- No commit, push, deploy, real database, service, temporary runtime or main-worktree mutation.
