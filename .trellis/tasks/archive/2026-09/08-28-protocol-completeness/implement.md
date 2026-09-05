# Implementation Plan

## 1. Gemini request mapping

- [x] Extend `gemini_generation_config` in `crates/aether-ai/formats/src/protocol/canonical.rs` for penalties and logprobs.
- [x] Extend `canonical_generation_config_to_gemini` in `crates/aether-ai/formats/src/formats/gemini/generate_content/request.rs`.
- [x] Update `GEMINI_MAPPED_GENERATION_CONFIG_KEYS` and source field-path reporting.
- [x] Remove stale Gemini target rejections in `crates/aether-ai/formats/src/formats/registry.rs` while retaining genuine lossy guards.

## 2. Cross-format response safety

- [x] Add source-specific Gemini/Claude response-extension validation to `registry.rs`.
- [x] Add only lossless citation/grounding projections supported by current canonical target shapes.
- [x] Ensure provider-only top-level, candidate/block, and usage fields fail closed with exact paths.
- [x] Parse official Claude `output_config.format` into canonical response-format semantics where representable.
- [x] Confirm same-format validation exits before the new guards.

## 3. Gemini tool identity

- [x] Add per-name pending call queues while parsing Gemini request `contents`.
- [x] Generate deterministic message/part-scoped IDs for calls without IDs.
- [x] Reuse matched IDs for ID-less function responses and preserve explicit IDs.

## 4. Gemini stream framing

- [x] Add a shared public Gemini stream-mode resolver using request path/query from report context.
- [x] Add SSE/JSON-array modes to `GeminiClientEmitter` and cover normal, finish, empty, tool-flush, and error emission paths.
- [x] Initialize the emitter from report context in the standard stream matrix and sync-to-stream bridge.
- [x] Wrap native same-format Gemini SSE records directly in a JSON array, and use standard rewriting for converted traffic.
- [x] Set client response content type from the selected mode in `apps/aether-gateway/src/execution_runtime/stream/execution.rs`.
- [x] Keep `/v1internal:streamGenerateContent` and Gemini Interactions unchanged.

## 5. Gemini models list/detail

- [x] Replace duplicated static model-value builders with one gateway projection.
- [x] Retain identity fields, add accurate generate/stream methods, and derive countTokens from eligible native Gemini candidates.
- [x] Remove unsupported universal numeric metadata.
- [x] Keep list pagination, detail lookup, auth filtering, provider restrictions, and model restrictions unchanged.
- [x] Update existing assertions that encode the old static payload; do not add new test suites.

## 6. Documentation

- [x] Update `docs/api/generate_format_field_coverage.py` classifications.
- [x] Regenerate `docs/api/format-field-coverage-matrix.md`.
- [x] Review the diff for stale claims that Gemini lacks penalties/logprobs.

## 7. Quality gates

- [x] Run `cargo fmt --all --check` or targeted `rustfmt --check` if workspace formatting would be too broad.
- [x] Run `git diff --check -- <touched files>`.
- [x] Run focused `rg`/source assertions for route mode, mapped field names, and removed hard-coded model metadata.
- [x] Do not run workspace or large-module compilation.
- [x] Do not create new unit tests.
- [x] Review the final diff for unrelated changes and confirm no commit was created.

## Rollback points

1. Request/validator changes can be reverted independently from stream framing.
2. Stream emitter/rewrite/header changes form one atomic rollback unit.
3. Model list/detail projection changes form one atomic rollback unit.
4. Generated documentation is rolled back with the mapping change that produced it.
