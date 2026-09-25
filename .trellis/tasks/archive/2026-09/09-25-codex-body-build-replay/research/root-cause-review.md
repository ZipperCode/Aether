# Root-cause review

## Why the prior release did not cover this failure

- Model discovery and OAuth plan eligibility can pass while a later body
  projector rejects a normal image option. Existing Codex image tests treated
  the CLI DTO's small field list as the whole private API contract and even
  asserted that output_format=png must be rejected.
- The heartbeat serializer had a NoPath error-body test, but it did not test
  the actual HTTP status or usage lifecycle. The request-level shell accepted
  an empty execution vector, committed 200, then serialized an in-band error
  without going through the proxy's no-plan accounting owner.
- Candidate diagnostics were richer before persistence than afterward:
  kind/source disappeared from the admin projection, leaving only `$` and a
  generic message. HTTP body capture was disabled for the earliest examples.
- The first full-route green attempt exposed a related model loss: ImageDecision
  disables runtime-miss diagnostics, and persisted candidates omit model names.
  The existing usage writer ignored the explicit model in its original JSON
  capture value, so the newly restored failure row was labeled unknown.

## Repair and prevention

- Preserve the common projector's validated output_format in the single Codex
  projector used by formal, admin and bridge callers. Keep a sanitized captured
  request as a regression, with both generation/edit coverage.
- Return existing NoPath before constructing a heartbeat shell when all
  candidates are skipped. Test a real router, exact 503 response, skipped
  unstarted candidate, one failed/void usage and configured body capture.
- Retain bounded kind/source for admin diagnostics and prove repeated
  persistence/public projection behavior. Do not turn capture on implicitly.
- Reuse the failure writer's original JSON value as the final model source,
  after existing diagnostic/candidate precedence. Keep the original ImageDecision
  policy and candidate-to-usage diagnostic nonduplication contract.
- Separate fixture failures from target regressions: a bearer Codex row is
  filtered before candidate preparation. The corrected OAuth fixture must
  actually reach the body projector; otherwise a 503 test can pass on the
  wrong path.

## Evidence boundaries

The recovered 13:06 body proves output_format rejection. The unrecorded 13:09
edits prove the missing-usage defect, not their exact input fields. Pure-text
Docker logs must not be discarded merely because a JSON-only parser finds no
matching records. Read-only investigation and local tests do not prove a live
account's upstream entitlement or deploy the fix.
