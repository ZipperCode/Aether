# Responses parity review fixes

## Goal
修复对提交 `308ed7ba4` 的只读审核确认的 Responses 跨协议回归，保持原生同格式透传不变，并补充可执行回归测试。

## Required fixes
- Preserve Responses `input` system/developer instructions in direct Claude/Gemini request conversion.
- Give every emitted reasoning block a distinct output-item ID; keep text and its Claude carrier signature in one block; reconcile done-only text once.
- Reject Claude-owned signed/redacted carriers for Gemini sync and stream responses; preserve foreign provider ciphertext; route checked carrier emission errors through a complete `response.failed` terminal.
- Mark unfinished hosted search calls unsuccessful in failed snapshots.
- Vertex partialArgs: reject ambiguous idless continuation, decode RFC 9535 quoted-key escapes, and validate only the candidate whose terminal event is closing.
- Include discovered flat tools in ambiguity counts; rewrite discovered custom `allowed_tools` to lowered function form; restore tool identities from the expanded previous-response history inventory.

## Acceptance
- Each listed defect has a focused regression asserting consumer-visible output or typed failure.
- Full `aether-ai-formats` tests pass, provider transport Responses tests pass, gateway checks pass, Clippy with `-D warnings`, rustfmt and coverage check pass.
- Executable smoke exercises direct system/developer conversion, two signed reasoning blocks, Claude carrier origin rejection, failed hosted search, Vertex parallel/escaped/idless cases, and tool discovery/history restoration.
- Update Responses protocol contract and commit only these repairs; do not push.

## Constraints
No public routes, schema migrations, retries, native same-format rewrites, live provider calls, or silent field dropping.