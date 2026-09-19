# Design

- Change the existing generic SSE precommit classifier at its shared ownership point. Inspect every caller before choosing the smallest exact text-event predicate. Treat only empty/whitespace text before commit as pending; do not apply text trimming to function arguments, identifiers, or arbitrary JSON.
- Keep the original chunks in the existing prefetch buffers. Classification affects when a provider is committed, not the bytes emitted on success.
- Reuse the existing error classification, candidate retry scope, deadlines, capture budgets, first-byte accounting, and committed-stream finalizer. Do not add a retry loop or provider-count override.
- Reuse current gateway mock HTTP fixtures for a two-provider regression with deterministic provider order and actual call counts. Include the opening-space shape observed from xmapi; preserve content and tool boundaries as negative controls.
- If a shared Chat branch is changed while repairing text classification, cover that branch explicitly. Do not expand into unrelated protocols.
- No schema/API/configuration migration. A release/deployment is required for live behavior to change; production remains untouched during this task.
