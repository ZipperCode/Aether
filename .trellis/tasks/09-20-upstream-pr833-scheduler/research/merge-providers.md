# Provider integration receipt

Baseline: `e34c05e8970d46444c3e1480ef94384521fb1fad`; upstream: `ba7c9f8b270cce63b0515299076b30129d7d64b4`.

## Resolved files

- `apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/device/authorize.rs`
- `apps/aether-gateway/src/handlers/admin/provider/write/normalize.rs`
- `apps/aether-gateway/src/provider_key_auth.rs`
- `crates/aether-oauth/src/provider/providers/generic.rs`
- `crates/aether-oauth/src/provider/service.rs`
- `crates/aether-provider/pool/src/lib.rs`
- `crates/aether-provider/pool/src/service.rs`
- `crates/aether-provider/transport/src/provider_types.rs`
- `crates/aether-provider/transport/src/request_url/mod.rs`

## Decisions

- Union the xAI and local Nous/official quota provider registrations and their capability/preset tests. Preserve both independent device flows behind the existing administrator/session checks.
- Make `GenericProviderOAuthAdapter::token_set_from_payload` visible to its xAI sibling while retaining the local persistent Codex identity fingerprint implementation.
- Preserve OAuth capability classification and the separate stored-refresh-credential gate. The new xAI semantic-capability test is valid; only the displaced local test comment needed restoration.
- Preserve the Gemini CountTokens branch and use the upstream resolved request base URL in both Gemini count and generation paths. Existing CountTokens and xAI host-selection regressions remain present.
- Reviewed related automatic merges in device polling/import, quota dispatch/shared persistence, Pool payloads and xAI media transport. They retain local modules and reuse existing credential/session and quota persistence owners.

## Completed checks

- Scoped `rustfmt --edition 2021 --config skip_children=true` and matching `--check`: pass for the nine resolved files.
- Scoped `git diff --check`: pass.
- Static check: no conflict markers and all function names from both parents remain in the nine resolved files.
- Exact baseline preservation check: `pool/src/quota.rs`, `quota_sources.rs`, `quota_snapshot.rs`, `providers/official_balance.rs` and gateway `oauth/quota/official_balance/execution.rs` are unchanged after newline normalization.
- No staging, commit, workspace formatter, Cargo compilation or live provider request performed by this worker.

## Root validation / cross-boundary follow-up

- Run the shared Linux builder for `aether-oauth`, `aether-provider-pool` and `aether-provider-transport`; run relevant gateway OAuth, official quota/manual-block and xAI control tests.
- Specific retained tests include `builtin_provider_service_registers_supported_provider_types`, `builtin_service_registers_provider_pool_adapters`, `builtin_service_owns_quota_refresh_support_and_endpoint_selection`, `preset_payload_derives_provider_support_from_capabilities`, `codex_persisted_fingerprint_is_member_scoped_and_token_independent`, `runtime_quota_block_survives_model_specific_available_quota`, `routes_gemini_count_tokens_across_official_and_custom_urls`, `rejects_gemini_count_tokens_on_private_adapters`, `xai_oauth_responses_use_cli_chat_proxy`, `xai_compact_and_using_api_use_official_api` and `refresh_capability_requires_stored_refresh_token`.
- xAI media adds a `transport` field to `ProviderVideoCreateHeadersInput` and a `resolve_video_task_proxy` lookup method; gateway/video compile and tests cover their automatically merged callers outside this worker's scope.
- Scheduler audit reported representative-Key runtime pollution separately to root; this worker made no scheduler policy or strong-read changes.
- No outstanding conflict or known integration defect remains in the owned paths. Runtime test results remain the root's responsibility; static preservation checks are not production validation.
