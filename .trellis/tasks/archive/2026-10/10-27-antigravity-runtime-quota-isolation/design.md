# Design

## Source fact and scope

Persist resettable Antigravity runtime quota observations in the existing shared RuntimeState key/value backend, not `status_snapshot.scheduling`, not fetched `quota_by_model`, and not account-scoped Pool scores. The identity is Provider + actual Key + credential generation + exact selected upstream model. Model names use trimmed full case-insensitive equality. No prefix/family grouping is inferred: the current quota-discovery shape provides exact-model evidence, while broad Gemini/Claude display groups do not establish model membership. An error explicitly associated with a different model cannot poison this candidate.

A scoped observation contains an absolute expiration deadline and a unique observation ID. Use real upstream reset metadata or numeric Retry-After when available; otherwise use configured quota cooldown (300 seconds when zero), bounded to 1 second through 31 days. Before quota fetch, capture existing exact-model records. After successful credential-fenced persistence, clear only byte-identical captured records whose exact model is positive in both incoming and strongly re-read durable metadata, under the shared Provider/Key projection lock. Newer failures, stale/failed refresh and unrelated model success cannot clear the fact. Existing KV TTL and shared backend are reused; no database schema or admin JSON changes.

## Classification and effects

At shared orchestration failure analysis, identify Antigravity quota exhaustion separately from generic rate limiting and permanent account-quota bans. Administrator configured strong quota/account rules and authentication/account-suspension failures retain precedence. Resettable model-quota failures remain visible as candidate failures and retry another actual account, but skip account-wide adaptive RPM, health/circuit, OAuth invalidation, Pool cooldown and score-exhaustion feedback. Recording must validate actual model/account/credential context; late responses from replaced credentials cannot restrict the replacement.

Integrate every current sync, native stream and shared stream failure effect entrypoint. Do not handle only PoolError after earlier health/adaptive effects have already run. Lease release and candidate diagnostics must remain complete on both classified and nonclassified paths.

## Scheduling

Ordinary selection consumes the shared scoped observation after existing strong Key reads, separately from manual account bans. Actual Pool/sticky pages consume the same helper after strong hydration, before hot-pool fallback/window truncation. PoolGroup representatives remain unfiltered by their own account restriction. Redis read failures retain state-unavailable diagnostics rather than implying exhausted quota. Existing native-format mapping and per-account target resolution stay unchanged.

Candidate-page and resolved-page invalidation after recording allows current-process fallback to observe the learned model restriction immediately. Runtime reads use the configured shared backend so other gateway instances cannot reuse account-eligible cached metadata to ignore an active scoped fact.

## Compatibility and recovery

No changes for other Providers, explicit manual runtime bans, generic account cooldown, auth failures or administrative rules. No retrospective production data cleanup or deployment. Historical account-wide feedback is preserved; this cutover prevents new incorrectly scoped feedback. Reset expiry and positive persisted refresh restore model eligibility without changing unrelated hard restrictions or importing models.

## Proof

Extend the existing localhost Antigravity HTTP harness in place. Seed positive fetched quota; cause a real upstream generation quota429 rather than preseeded zero quota. Prove the failing account is skipped for the same actual model on the next request, another account serves the target, Claude remains immediately available on the first account, and the reverse direction also holds. Cover ordinary/Pool/sticky modes and native sync/SSE, refresh/expiry, failed refresh and account-wide ban preservation. Parent runs baseline before production gates are modified, then integrated tests/smoke and review.
