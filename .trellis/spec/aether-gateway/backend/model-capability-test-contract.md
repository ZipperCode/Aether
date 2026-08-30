# Model Capability Test Contract

## 1. Scope / Trigger

This contract applies to the admin-only model capability test beside the existing connectivity test. It measures observable capability and optional divergence from a user-selected trusted reference; it does not authenticate model identity.

Use this path only for text-generation endpoints. Keep connectivity/failover diagnostics, public benchmarks, scheduled monitoring, and provider enforcement outside this contract.

## 2. Signatures

- HTTP: `POST /api/admin/provider-query/test-model-capability`
- Route: `admin_proxy / provider_query_manage / test_model_capability`
- Permission: `admin:provider_query:write` or `admin:provider_query:admin`
- Handler: `build_admin_provider_query_test_model_capability_response(state, payload)`
- Saved model config:

```json
{
  "capability_test_reference": {
    "provider_id": "provider-id",
    "model_id": "provider-model-id",
    "endpoint_id": "endpoint-id",
    "api_key_id": "key-id"
  }
}
```

Saving this object must merge with the existing ProviderModel `config`; it must not replace sibling keys.

## 3. Contracts

### Request

```json
{
  "provider_id": "target-provider-id",
  "model_id": "target-provider-model-id",
  "endpoint_id": "target-endpoint-id",
  "api_key_id": "target-key-id",
  "mode": "quick",
  "language": "bilingual",
  "use_saved_reference": true,
  "request_id": "provider-capability-..."
}
```

- `mode`: `quick` generates 40 questions; `verify` generates 100.
- `language`: `zh`, `en`, or `bilingual`; bilingual is balanced within each dimension.
- The browser never supplies prompts, answers, seed, arbitrary headers/body, or a model name.
- Unknown fields are rejected. All four target IDs are required and non-empty. `request_id` is optional and at most 128 characters.

### Execution

- Suite version is `capability-v1` with equal `quantitative`, `logical`, `algorithmic`, `language`, and `instruction` dimensions.
- A fresh UUID v4 seed drives deterministic UUID v5 derivation and option ordering. Target and reference receive identical questions.
- Each subject is pinned to exactly one active ProviderModel, endpoint, Key, mapping, and effective model. There is no candidate or Key failover.
- Supported formats are `openai:chat`, `openai:responses`, `claude:messages`, and `gemini:generate_content` through the existing provider-query adapters and aggregated response bodies.
- Requests are non-streaming, temperature `0`, no tools/search, and at most `1024` output tokens. Shared concurrency is `4`; runner deadlines are 10 minutes for quick and 20 minutes for verify.

### Response

The typed response owns:

- run metadata: `run_id`, `suite_version`, `seed`, `mode`, `language`, `elapsed_ms`, `request_profile`;
- safe target/reference descriptors: internal IDs, requested/effective model, and API format only;
- `target_metrics`, optional `reference_metrics`, optional paired `comparison`, and per-item observations;
- verdict: `profile_only`, `no_large_deviation`, `needs_verification`, `no_significant_deviation`, `significant_deviation`, or `inconclusive`;
- inconclusive reason: `total_timeout`, `target_coverage`, `reference_coverage`, or `paired_coverage`;
- fixed disclaimer that capability behavior is not model identity authentication.

Per-item status is one of `scored`, `network_failure`, `rate_limited`, `timeout`, `filtered`, `refused`, `truncated`, `unparseable`, `upstream_error`, or `cancelled`. Only `scored` enters accuracy. Responses must not contain prompts, endpoint URLs, Key names/credentials, raw headers/bodies, or reasoning text.

Quick requires target/reference/paired coverage of at least 90%; a reference-minus-target score gap of at least 15 percentage points and one-sided exact McNemar `p < 0.05` yields `needs_verification`. Verify uses 95% coverage, at least 10 percentage points, and `p < 0.01` for `significant_deviation`. Exact-threshold `p` values do not pass because the comparison is strict.

## 4. Validation & Error Matrix

| Condition | Result |
|---|---|
| Malformed/unknown request field | `400 Invalid model capability test request` |
| Empty target ID or oversized request ID | `400 Model capability test IDs must be non-empty` |
| ProviderModel missing | `404 Provider model not found` |
| ProviderModel inactive | `400 Provider model is inactive` |
| Explicit non-text model | `400 Provider model does not support text capability testing` |
| Unsupported endpoint format | `400 Endpoint format does not support capability testing` |
| Endpoint/Key/model cannot resolve to exactly one active candidate | `400 Pinned endpoint and API key are not available for this model` |
| Saved reference requested but missing/malformed/stale | `400 Saved capability test reference is required/invalid` |
| Saved reference equals the complete target tuple | `400 Reference must differ from target` |
| Runtime timeout/rate limit/filter/refusal/truncation | Corresponding item status; never counted as a wrong answer |
| Coverage, pairing, or total deadline insufficient | `200` with `verdict=inconclusive` and a machine-readable reason |

Every new local admin route that deserializes JSON must also be registered in `admin_proxy_local_requires_buffered_body`. Missing registration delivers an empty payload to the handler even when route classification is correct.

## 5. Good / Base / Bad Cases

- Good: target plus a saved official reference resolve to two fixed tuples; both receive the same generated suite; the response reports paired statistics and a non-identity disclaimer.
- Base: no reference is enabled; the same suite returns a five-dimension `profile_only` capability profile.
- Bad: a stale saved reference, inactive endpoint, non-text model, ambiguous answer, filtered response, or max-token truncation is rejected or classified explicitly; it is never silently replaced, retried through another candidate, or scored as incorrect.

## 6. Tests Required

- Route/permission: assert route classification, `provider_query` write permission, and buffered-body registration.
- Pure logic: assert 40/100 counts, equal dimensions, bilingual quotas, seed reproducibility, strict A-D parsing, supported request shapes, usage extraction, Wilson/McNemar values, thresholds, and all failure classifications.
- HTTP integration: assert exact ProviderModel binding despite a same-name decoy; fixed endpoint/Key/model with no failover; saved reference receives the same questions; stale reference, inactive endpoint, and non-text model fail closed; no secret or raw body appears in the response.
- Protocol regressions: retain exact aggregation tests for OpenAI Responses, Claude Messages, and Gemini GenerateContent.
- Frontend: type-check the cross-layer contract and test shared capability-format/model-family predicates.

Minimum focused gates:

```text
RUST_MIN_STACK=16777216 cargo test -p aether-gateway capability --lib
cargo clippy -p aether-gateway --lib --bins --examples -- -D warnings
cargo fmt --all --check
cd frontend && npm run type-check
cd frontend && npx vitest run src/features/providers/components/provider-tabs/__tests__/model-test-request.spec.ts
git diff --check
```

## 7. Wrong vs Correct

### Wrong

```text
Classify the POST route, deserialize its local payload, let normal failover select any Key,
and label a low score as confirmed model substitution.
```

This can yield an empty body, mix multiple upstream identities in one score, and overstate what black-box output can prove.

### Correct

```text
Classify route + register buffered body -> validate typed IDs -> pin one target/reference tuple
-> generate one server-side suite -> reuse existing protocol execution/aggregation
-> classify non-scorable outcomes -> report capability divergence with a disclaimer.
```
