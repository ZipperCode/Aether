# Design
Trace all callers of the existing detector/repair functions and AntigravitySignatureRecovery first. Extend the shared recovery boundary rather than adding per-handler fallbacks.
Recognize explicit protocol evidence: actual 400 + INVALID_ARGUMENT + the exact model-content part-root signature path and Base64/TYPE_BYTES rejection. Prefer structured field violations where supplied; account for the observed long JSON error/message and diagnostic capture limits without broad text matching.
For a Base64 rejection use the reported positions to modify only existing model signature fields. For the pre-existing unlocated Corrupted thought signature rejection retain its authorized behavior. Keep the existing provider-specific compatibility sentinel; verify its wire behavior with synthetic endpoint tests. Do not invent a signature or attempt to recover lost opaque bytes.
Any added diagnostic category must remain a fixed safe value and survive existing candidate projection; no raw signature/error body belongs in compact metadata.
API/schema and normal same-format passthrough remain unchanged. Main session updates the existing contract after implementation review.
