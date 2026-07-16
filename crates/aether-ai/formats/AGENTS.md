# AETHER AI FORMATS KNOWLEDGE BASE

## OVERVIEW

Pure, typed conversion between OpenAI, Claude, Gemini, embedding, and rerank wire formats through canonical request, response, and stream representations.

## WHERE TO LOOK

| Task | Location | Notes |
|---|---|---|
| Format identity/aliases | `src/formats/id.rs` | `FormatId`, normalization, same-format matching |
| Canonical request/response | `src/protocol/canonical.rs` | Provider-neutral content and generation fields |
| Canonical stream events | `src/protocol/stream.rs` | Separate streaming pipeline |
| Parse/emit/guard dispatch | `src/formats/registry.rs` | Registry selects adapters; adapters own wire details |
| Context/errors/reports | `src/formats/context.rs` | Typed fail-closed errors and conversion reports |
| Capability matrix | `src/formats/matrix.rs` | Request/response conversion decisions |
| Contract authority | `docs/api/format-passthrough-contract.md` | Runtime passthrough versus pure conversion |
| Audited schema coverage | `docs/api/provider-interface-definitions.md`, `format-field-coverage-matrix.md` | Input plus generated output |

## CONVENTIONS

- Conversion shape is wire JSON -> provider adapter -> canonical -> target adapter -> wire JSON.
- Registry owns exhaustive dispatch and strict guards; provider modules own actual wire parsing/emission.
- Pure conversion performs parse, emit, provider mapping, and `ConversionReport` only.
- Preserve unknown fields/enums for canonical same-format audit replay. Compare JSON-normalized values, not raw bytes.
- Unknown or unrepresentable cross-format fields return typed `FormatError`; add an explicit audited mapping before emitting them.

## ANTI-PATTERNS

- Never call canonical conversion for runtime same-format traffic; that branch lives in `aether-provider-transport` and preserves parsed JSON directly.
- Never silently drop a cross-format field or encode failure as `None`.
- Never let pure conversion override model/stream, apply body rules/directives, patch from the original request, or perform transport policy.
- Never use the generated field-coverage matrix as a runtime allowlist.
- Never call wire-specific canonical helpers directly from the registry; go through the provider surface adapters.
- Do not coerce unknown cross-format stream events into a finish reason; terminate with the target error form.

## COMMANDS

```bash
cargo test -p aether-ai-formats
cargo test -p aether-ai-formats field_coverage_matrix_covers_all_documented_provider_schema_fields
cargo test -p aether-ai-formats registry_does_not_call_wire_specific_canonical_functions_directly
python3 docs/api/generate_format_field_coverage.py --check
```

Run `python3 docs/api/generate_format_field_coverage.py` only when intentionally regenerating the checked-in matrix.
