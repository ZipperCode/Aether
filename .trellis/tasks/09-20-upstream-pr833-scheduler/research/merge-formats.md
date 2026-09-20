# Formats / Antigravity merge receipt

- Worktree: `C:/Users/Zipper/.codex/worktrees/sync-upstream-pr833/Aether`.
- Parents: local `e34c05e8970d46444c3e1480ef94384521fb1fad`; upstream `ba7c9f8b270cce63b0515299076b30129d7d64b4`.
- Owned source stable: **2026-09-20 00:21:13 UTC**. No staging, commit, whole-workspace formatter, or Cargo invocation by this agent.

## Files resolved / corrected

Eight merge conflicts resolved semantically:

- `crates/aether-ai/formats/src/formats/gemini/generate_content/{response,stream}.rs`
- `crates/aether-ai/formats/src/formats/openai/request_contract.rs`
- `crates/aether-ai/formats/src/formats/openai/responses/mod.rs`
- `crates/aether-ai/formats/src/formats/registry.rs`
- `crates/aether-ai/formats/src/formats/shared/stream_core/format_matrix.rs`
- `crates/aether-ai/formats/src/protocol/canonical.rs`
- `crates/aether-provider/transport/src/antigravity/request.rs`

Additional integration correction: `crates/aether-ai/formats/src/formats/conversion/response.rs` updates the local raw-reasoning assertion to the merged upstream `content`-only contract. Other upstream files in these packages remain automatic merge results.

## Preserved contracts / integration fixes

- Gemini tool identity, signature carrier direction/exact value, original part indices, immediate visible thought emission, signature-only non-commit, terminal tool errors/usage, and same-format raw reconstruction remain local implementations.
- Antigravity's exact HTTP 400 corrupted/Base64 classifier and field-specific repair are unchanged; no speculative first-send signature cleanup. Upstream unsigned thought removal applies only to Claude models.
- Public Gemini passthrough remains outside private Schema lowering; retained the twelve-tool regression that checks both public preservation and private envelope normalization.
- Responses message item normalization remains scoped to official OpenAI/Codex providers through the existing local helper, including Compact behavior. Integrated deterministic long `call_id` normalization while preserving call/result pairing.
- Preserved decoded-carrier detection instead of upstream prefix-only rejection. Updated xAI regression to prove valid carrier removal and preservation of provider ciphertext that merely resembles the carrier prefix.
- Integrated raw reasoning into `content` while keeping provider summaries; no duplicate synthesized `summary` content.
- Fixed sync and stream guards that otherwise rejected newly mapped Gemini grounding. Citations preserve original part indices, camel/snake metadata aliases and UTF-8 byte-to-character offsets. Metadata-only terminal frames work without weakening existing malformed content or terminal-error handling.
- Responses adjacent-text coalescing now reuses `offset_openai_annotation_indices` so part-relative citations remain correct after text concatenation. New sync/stream tests cover a nonzero prefix, multibyte text and a citation belonging to a later part; unanchored citations are emitted once.

## Validation

Passed locally:

- `rustfmt --edition 2021 --config skip_children=true` on the nine edited Rust files.
- `git diff --check -- crates/aether-ai/formats crates/aether-provider/transport/src/antigravity`.
- `python docs/api/generate_format_field_coverage.py --check`.
- No remaining conflict markers under either owned path.

Root owns Cargo validation; results pending when this receipt was written. Suggested exact commands in the prepared Linux builder:

```text
cargo test --locked --offline -p aether-ai-formats --lib
cargo test --locked --offline -p aether-provider-transport --lib antigravity
cargo test --locked --offline -p aether-provider-transport --lib same_format_gemini
```

Root's gateway integration gate should retain `antigravity_signature`, Gemini signature round-trip and recent Responses SSE prefetch regression filters.

## Required root spec update / follow-up

`antigravity-tool-schema-contract.md` describes superseded metadata-only cleanup. Per root instruction, upstream #832 is intentionally integrated at the private boundary:

- Parameters alias precedence remains unchanged; bounded `normalize_tool_parameters` applies to both private Gemini and private Claude models.
- Keep supported protobuf Schema subset; lower string consts to enums, nullable type unions, catch-all dictionaries, int64 string constraints, local references with conjunctive siblings. Unsupported constraints, unresolved/external/cyclic references and boolean/depth-limited schemas may relax; this is not lossless JSON Schema preservation.
- Budget is shared across declarations: 4,096 charged nodes and 1 MiB serialized work; exhaustion produces `ToolSchemaBudgetExceeded` without mutating caller input. Depth 64 terminates expansion by relaxing that node; it is distinct from budget exhaustion.
- Claude additionally folds exact string-literal unions, otherwise carries alternatives as description guidance; never arbitrarily picks a branch. Tool callers remain responsible for original-schema argument validation.
- Removed the now-unused metadata-only helper and its obsolete lossless-structure test; upstream schema-node/property-name/literal-data/budget/idempotence regressions supersede it. The twelve-tool public/private separation regression remains.
- PR #836 was not imported. No live provider validation or production diagnosis is claimed.

Memory was used only to locate historical exact-rejection and private-boundary contracts; current repository specs and source were verified before implementation.
