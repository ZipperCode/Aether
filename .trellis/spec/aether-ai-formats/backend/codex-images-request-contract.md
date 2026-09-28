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

### 透明背景（gpt-image-2）：官方支持与保留边界

官方 openai-python `src/openai/types/image_generate_params.py`（main 分支）在
`background` 参数说明中明确：GPT Image 系列模型支持 `transparent` 背景，其中
`gpt-image-2` 与 `gpt-image-2-2026-04-21` 处于预览（preview）支持；使用
`transparent` 时 `output_format` 必须为 `png` 或 `webp`。官方 Codex
`codex-rs/ext/image-generation/src/tool.rs` 同样以 `gpt-image-2` 为固定模型
（`IMAGE_MODEL`），并把工具入参 `transparent_background` 直接映射为
`ImageBackground::Transparent`。据此，2026-09-28 删除公共投影中“GptImage2
拒绝透明背景”的过期限制；该拒绝曾把合法请求转成 503
`provider_request_body_missing`。保留边界：`transparent` 搭配 `jpeg` 输出
（JPEG 无法承载透明像素）继续拒绝；DALL·E 2/3 不接受 `background` 字段的
既有拒绝不变；未显式指定 `output_format` 时按默认 `png` 放行透明背景。

## 4. Validation & Error Matrix

| Input | Result |
| --- | --- |
| Valid output_format on Generate or Edit | Retain the canonical field upstream |
| Missing output_format | Preserve existing default behavior |
| Unsupported output format | Existing common rejection, no upstream request |
| Other unsupported Codex field | Existing fail-closed behavior |
| Transparent background + png/webp output (GPT image models, incl. gpt-image-2) | Retain `background: transparent` upstream |
| Transparent background + jpeg output | Existing common rejection, no upstream request |

## 5. Good / Base / Bad Cases

Good: explicit PNG survives the public projection and Codex adaptation.
Base: an image request without output_format is unchanged.
Bad: public projection succeeds but a stale private field list skips the account.

## 6. Tests Required

Use the sanitized captured request and a synthetic Edit with an image reference.
Assert the selected format reaches the resulting JSON for PNG/JPEG/WebP and an
invalid format still fails. The captured regression must fail before the repair.
The gpt-image-2 transparent-background regression must keep PNG/WebP through the
public and Codex projections and keep JPEG rejected.

## 7. Wrong vs Correct

Wrong: add exemptions separately to management tests and each planner.
Correct: extend the shared Codex projection and preserve common validation.
