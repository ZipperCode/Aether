# Implementation plan

1. Read the PRD, design, formats ownership rules and Codex HTTP Responses contract. Baseline worktree was clean. Reuse prior exercised defect evidence; do not repeat investigation without a new gap.
2. Start the approved task. Dispatch core lifecycle/replay, Vertex assembler and tool adaptation slices together under one shared-file integration owner. Workers implement focused regressions but skip all builds/tests/lints/formatters during implementation.
3. Core integrates Vertex audit allowances and tool request/response hooks once helpers are defined. No concurrent edits to shared canonical/registry/emitters. Preserve the native path and reject unsupported material semantics.
4. Run one post-integration validation pass: Rust formatting; cargo test -p aether-ai-formats; cargo check -p aether-gateway (include migration of exhaustive event consumers); provider transport focused carrier/native-path tests if changed. Fix discovered issues and rerun only failed/affected checks.
5. Execute a throwaway Rust smoke outside the repo using real request conversion, actual AiSurfaceStreamRewriter byte chunks and local HTTP fake upstream if gateway boundary changes. Exercise failed partial output; signed/redacted Claude sync+stream replay with tool result; Vertex nested/interleaved/parallel argument streams; tool search discovery, calls and results for Chat/Claude. Observe exact terminal objects/arguments and retained native unknown data. Do not call live providers.
6. After smoke proof, update the existing protocol contract and API coverage inputs if semantics change; regenerate only the supported audit generator, check coverage. Read-only reviewer checks final implementation for protocol loss and native rewrites.
7. Isolate task files and code changes, commit with the repository convention after all applicable checks pass. Do not push without explicit authorization. Finish/archive the Trellis task per project workflow.

## Risks
- Shared emitters and canonical validators are large: one integration owner prevents lost edits.
- A signed reasoning carrier must not be mistaken for vendor ciphertext or reused in the wrong origin.
- Partial tool JSON must not produce a successful tool completion on failure.
- Tool search is stateful within supplied history; do not invent discovered tools or silently drop unsupported grammars.
