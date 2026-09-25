# Codex Images request projection

## 1. Scope / Trigger

Apply when adapting native Images requests to Codex OAuth. A client DTO's small
field set is not an exhaustive server rejection schema. Verify real rejected
fields and current protocol evidence before tightening the shared projector.

## 2. Signatures

- `build_codex_openai_image_api_provider_request_body` serves normalized requests.
- `project_codex_openai_image_api_request_body` also serves image bridge callers.
- Both use `project_codex_openai_image_api_request_body_with_max_generation_count`.

## 3. Contracts

Common image validation owns supported `output_format` values (`png`, `jpeg`,
`webp`) and normalization. The Codex projector must retain that canonical value
for both generation and edit. Do not silently remove it or repeat enum validation
at each planner/admin caller. Missing output_format remains missing.

Keep the separate model, plan, image-reference, count and unsupported-field
boundaries. This fix is not evidence for new quality/size/mask/stream semantics.
The captured 2026-09-25 failure used `gpt-image-2.5-flare`, `n=1`,
`size=1536x1024`, `output_format=png`; its prompt is not needed for reproduction.

## 4. Validation & Error Matrix

| Input | Result |
| --- | --- |
| Valid output_format on Generate or Edit | Retain the canonical field upstream |
| Missing output_format | Preserve existing default behavior |
| Unsupported output format | Existing common rejection, no upstream request |
| Other unsupported Codex field | Existing fail-closed behavior |

## 5. Good / Base / Bad Cases

Good: explicit PNG survives the public projection and Codex adaptation.
Base: an image request without output_format is unchanged.
Bad: public projection succeeds but a stale private field list skips the account.

## 6. Tests Required

Use the sanitized captured request and a synthetic Edit with an image reference.
Assert the selected format reaches the resulting JSON for PNG/JPEG/WebP and an
invalid format still fails. The captured regression must fail before the repair.

## 7. Wrong vs Correct

Wrong: add exemptions separately to management tests and each planner.
Correct: extend the shared Codex projection and preserve common validation.
