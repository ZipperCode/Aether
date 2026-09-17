# Antigravity Base64 thought-signature recovery
## Requirements
- R1.1: Recover the confirmed Google HTTP 400 INVALID_ARGUMENT rejection naming a contents[n].parts[m].thought_signature field and TYPE_BYTES/Base64 decoding failure.
- R1.2: Retain existing Corrupted thought signature behavior. First sends, normal signatures and non-Antigravity routes retain current semantics.
- R1.3: For the new field-specific failure, repair only rejected model part-root signature fields; preserve other signatures, text, tools, tool arguments/results/order, original client input and capture.
- R1.4: Reuse the candidate-local single recovery allowance, same endpoint/key/auth/model/URL and remaining deadline. Preserve failover stop policy, single billing/health/terminal ownership and safe diagnostics.
## Acceptance
Synthetic long-error regression reproduces the incident and succeeds only after one correctly targeted resend. Negative cases include other byte fields, wrong status/provider, out-of-range paths, user/tool fields, malformed/unsupported errors and no modifiable signature. Stream, sync and fixed-target callers continue using shared recovery.
## Scope
Provider boundary and existing consumers/projection/tests only. No client modifications, signature caches, proactive cleanup, base64 repair, dependencies, deployment or unrelated upstream changes.
