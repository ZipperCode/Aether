# Antigravity account discovery repair

## Boundaries

- Provider transport owns client identity and request envelopes. Reuse the existing identity owner; do not add a network refresh service or duplicate header construction in handlers.
- Model-fetch owns upstream discovery decoding and per-account cache/metadata persistence.
- The admin provider query owns aggregation across the requested accounts. The union is a response projection, never a replacement for account-local availability.

## Identity

Use the supported Antigravity CLI family `antigravity/cli/<version> <os>/<arch>`, with official release `1.4.7` as the default and the existing account client-version override applied consistently. Release provenance: https://github.com/google-antigravity/antigravity-cli/releases/tag/1.4.7 (2026-10-24); the CLI UA shape is also captured in existing gateway frontdoor/CLI fixtures. Reuse `VersionedClientIdentity` validation without adding global mutable profile state or unused setters. Use this identity in initialization, loadCodeAssist, fetchAvailableModels, quota reads and generation where those currently share the Antigravity identity. Preserve unrelated request-body semantics.

Do not change account subscription evidence or infer entitlement from version. Existing raw entitlement candidate fallback remains unchanged. No credentials are needed for deterministic local verification; a real upstream same-account A/B is separate and must not be claimed without observation.

## Account aggregation

Remove Antigravity-specific first-success termination. Process each selected eligible account using the existing error, warning, deduplication and account-cache mechanisms. Stop consuming or producing the obsolete provider-wide cache whose contents were populated from a single account. Retain per-account caching and force-refresh behavior. Do not fan the provider union back into individual account records.

## Disabled models

At the upstream response boundary exclude entries explicitly carrying `disabled: true` before projecting either model rows or quota metadata. Keep unknown enabled upstream IDs. Do not synthesize models from quota buckets, baseline lists, names or aliases. Where existing cached projections need the same filtering, preserve upstream evidence rather than guessing entitlement.

## Compatibility and risks

No HTTP payload schema or database migration. Provider-wide queries perform more per-account discovery calls when caches are cold; this is necessary to produce the requested catalog and uses the existing sequential execution contract. Partial failure keeps successful account results and existing warnings. Identity changes affect requests, not access rights. Do not advise endpoint header_rules as a workaround: existing Antigravity generation rejects them.

## Validation

Keep regression coverage for mixed account tiers, first successful empty catalog, duplicate IDs with distinct endpoint evidence, selected-account isolation, partial failure, and disabled/unknown enabled IDs. Delete old implementation-only identity pinning assertions rather than re-pin new literals. Execute real loopback HTTP requests to inspect identity and verify returned discovery results, with synthetic accounts only. Reuse existing suites/harnesses and remove throwaway smoke artifacts after the run.
