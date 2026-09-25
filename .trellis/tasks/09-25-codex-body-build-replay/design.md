# Design

## Confirmed boundaries

1. The shared Codex image projector rejects a valid, already normalized output_format=png. Permit the supported output format field and retain the common projector's canonical value in Codex output. Do not duplicate its enum/model validation or remove unrelated unsupported-field checks.
2. The request-level synchronous Images JSON heartbeat shell is constructed even when its candidate vector is empty. Guard that condition before shell creation and return the existing NoPath outcome, allowing the normal proxy runtime-miss path to record usage and return an actual error status. Do not retrofit postcommit accounting into an empty shell. Preserve established whitespace heartbeat and in-band error behavior for real attempts.
3. Candidate persistence drops fixed internal diagnostic kind/source. Preserve those safe diagnostic fields through the existing projection without widening unrelated metadata or ordinary-user output.
4. ImageDecision intentionally disables request-level runtime-miss diagnostics,
   and candidate persistence strips model names. The normal failure writer can
   therefore record unknown even with an explicit JSON model. Reuse its existing
   original-request JSON parse once and read a trimmed string model only after
   the current diagnostic/candidate sources are exhausted. Reuse that parsed
   value for the existing capture field; no extra parsing or capture expansion.

## Ownership

- Formats and data contracts: one implement agent owns image/request.rs and candidates/types.rs plus their inline tests.
- Gateway: a second agent owns image heartbeat execution, the existing runtime-miss usage writer and focused gateway tests. Shared task/spec integration remains with the parent.
- No production mutations, schema changes, frontend redesign, new dependency, general diagnostic framework, model capability expansion, automatic retry or deployment. The earlier user-authorized push/CI/tag workflow follows the verified repair as a separate release task.

## Evidence limits

The captured 13:06 generation request proves output_format rejection. The later edits requests prove skipped-only usage loss but their body fields were not captured. Local tests prove construction and accounting behavior, not acceptance by a live paid Codex account.
