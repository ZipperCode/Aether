# Antigravity Claude cache-control compatibility

## Goal
Allow Claude Messages requests carrying prompt-cache markers to reach Antigravity without weakening universal lossless conversion or modifying fallback candidates.

## Confirmed incident
Request 982d3d18 on the existing 0.7.43 deployment was rejected locally for ephemeral cache markers on system, tool declarations and message content blocks. The final fallback independently failed while streaming. Investigation was read-only; implementation does not authorize production deployment or paid inference.

## Requirements
- Apply an explicit, configurable Antigravity-only compatibility policy before Claude-to-Gemini validation.
- Drop only cache-control protocol metadata, including top-level, system, tools and nested tool-result content; preserve user payloads, JSON schemas, tool arguments and opaque signatures.
- Keep the original client request intact for auditing and fallback/native Claude requests.
- Do not claim equivalent TTL, breakpoints, cache pricing or fabricate cache usage.
- Keep generic cross-format conversion strict and same-format passthrough intact.
- Cover streaming and non-streaming shared conversion paths and relevant provider model checks.
- Verify behavior locally without sending captured content to an external service or replaying billed inference.

## Acceptance
- Antigravity conversion accepts a synthetic request matching the incident's protocol shape.
- Protocol cache markers do not reach the Antigravity envelope; sentinel business fields named cache_control remain unchanged.
- Explicit strict/disabled policy and other providers still reject unsupported cache metadata.
- Native Claude fallback retains every original marker and source request is unchanged.
- Existing signature and schema behavior remains unchanged.
- Update existing documentation, run relevant checks and smoke, commit only isolated task changes.
