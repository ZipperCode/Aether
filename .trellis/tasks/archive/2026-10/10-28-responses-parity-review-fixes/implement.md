# Implementation plan

1. Read exact changed symbols and current tests; start the task after review of this PRD/design.
2. Implement reasoning/carrier and failure-terminal repairs with focused tests.
3. Implement Vertex partialArgs owner/escape/candidate-finish repairs with focused tests.
4. Implement tool inventory/choice/history restoration repairs and direct instruction preservation with focused tests.
5. Run focused tests after all edits, then full crate/transport/gateway/lint/format/coverage checks once.
6. Run a temporary executable smoke covering every acceptance boundary; remove temporary harness.
7. Update the existing protocol contract, inspect status/diff, commit in one coherent repair commit, archive task and record journal. Do not push.

## Verification results

- `cargo test -p aether-ai-formats --locked`: 1,201 passed.
- `cargo test -p aether-provider-transport responses --locked`: 17 passed.
- Formats Clippy with `--all-targets -- -D warnings` passed.
- `python3 tools/ci.py preflight`: rustfmt and gateway Clippy passed; gateway nextest reported 5,745 passed, 3 skipped.
- Executable public-API smoke passed instruction preservation, signed/reasoning-only replay, done-only deduplication, carrier rejection, complete failed terminals, unfinished hosted search, Vertex ambiguity/Unicode/candidate closure, discovered tools and pinned history identity.
- The real HTTP gateway regression passed two-turn discovered namespace restoration and foreign-key history rejection against a loopback upstream. No live provider calls were made.
- A separate replay executable reproduced `messages[].openai_responses.id` rejection before the final fix and passed afterward. The fix extends the existing explicit message-metadata downgrade to Chat and Gemini; unknown fields still fail closed.
- Bounded reviews of core reasoning/failure, Vertex and tools/history completed. The Vertex review's empty-close ambiguity finding was fixed and exercised.

## Root cause and prevention

- Lifecycle ownership was inferred across reasoning blocks and parallel Vertex calls. Per-item identities, candidate-scoped closure and unique-owner checks now cover all continuation markers.
- Request conversion and response restoration previously rebuilt different tool inventories. The planner now pins a tool-only projection of the expanded, API-key-scoped request.
- Hand-built history fixtures omitted metadata produced by the real response converter. The permanent regression now records actual converted output and exercises the next request through the HTTP planner.
