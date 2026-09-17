# Implementation receipt

## Implemented

- Added strict recognition for HTTP 400 `INVALID_ARGUMENT` errors that name exactly `request.contents[n].parts[m].thought_signature`, `TYPE_BYTES`, and `Base64 decoding failed`.
- Kept the legacy `Corrupted thought signature` path unchanged. The Base64 path replaces only existing root signature aliases in the reported `role=model` part; it never changes unrelated model parts, user/tool parts, nested tool data, or the original input capture.
- Extended only the private Antigravity stream recovery prefetch from 16 KiB to 64 KiB, so the confirmed 16,766-character synthetic echo remains parseable before diagnostics are truncated. Normal error prefetch behavior is unchanged.
- Allowed the existing candidate projection to retain the fixed safe diagnostic `Base64 decoding failed`.

## Regression coverage

- Provider parser/repair: exact trigger and structured-error exclusions, wrong status/error/field, user/tool/out-of-range/unsigned repair targets, input immutability.
- Direct stream, regular sync, and fixed-target plan tests now use a synthetic 16,766-character non-canonical Base64 signature and a larger-than-16-KiB Google-style error. They assert exactly two same-account sends and only the reported part changes.

## Validation

- Passed: `rustfmt --edition 2021 --check` for all seven changed Rust files.
- Passed: `git diff --check`.
- Implementer did not run Cargo. Root subsequently compiled the final source and passed provider request tests (10), same-format Gemini tests (5), candidate projection (1), gateway signature regressions (10) and stream error collection/classification tests (4). See the parent validation receipt for commands and limits.

## Review integration
- Independent checker added the shared classifier and 64 KiB bound to final stream failure handling; repeated Base64 rejection now preserves provider-scope fallback and stop policy.
- The shared fixture now includes both long message and structured Google field-violation echoes. Parser negatives cover malformed numeric paths. Root fixed two test-only constant scope references exposed by compilation.

Suggested serialized Docker filters:

```text
cargo test -p aether-provider-transport antigravity::request::tests::base64_thought_signature
cargo test -p aether-data-contracts antigravity_signature_recovery_survives_candidate_projection
cargo test -p aether-gateway antigravity_signature_stream_retries_same_candidate_once
cargo test -p aether-gateway antigravity_signature_sync_recovery_and_provider_scope
cargo test -p aether-gateway antigravity_signature_admin_fixed_plan_recovery
```
