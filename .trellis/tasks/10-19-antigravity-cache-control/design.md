# Design

## Boundary
Add `prepare_antigravity_claude_cache_control_body(body: &Value, client_api_format: &str, provider_type: &str, provider_api_format: &str, provider_config: Option<&Value>) -> Cow<Value>` in aether-ai-formats shared standard-matrix code, exported via its existing API surface. Borrow out-of-scope, disabled and unmarked requests; clone only marked bodies. Normalize format aliases and provider spelling. Only explicit Antigravity + Claude Messages source + Gemini GenerateContent destination qualifies.

## Policy
Existing provider.config JSON owns boolean `antigravity_cache_control_compat`. Missing defaults to enabled; explicit false retains strict refusal. No database schema change or frontend control. Universal convert_request remains unchanged. Do not infer identity from model names or URLs.

## Traversal
Visit only protocol positions: root cache_control, system blocks, tool declarations, message/content blocks and nested tool_result content. Do not traverse arbitrary tool input, schemas, embedded JSON/document data or text. Keep opaque signatures intact.

## Consumers
The gateway family candidate planner prepares the source before shared conversion, covering sync and stream. Preserve original request for audits/fallback and body-rule source placeholders; when preparation changes the source, run rules outside conversion using the original source. For Gemini destinations the shared builder's later Responses/Codex/xAI edits are no-ops. Failure diagnostics inspect the prepared source to avoid misattributing later errors to removed markers.

Admin Antigravity model tests were inspected and intentionally remain unchanged: their dedicated path constructs OpenAI Chat test payloads rather than accepting Claude Messages inputs. The initially suspected general admin builder branch handles embedding/rerank only. No irrelevant admin integration is added. Agent bridge stays independent.

## Risks
This discards Claude cache breakpoints and TTL semantics only for Antigravity; no cache pricing equivalence or invented usage. It removes a local blocker, not evidence of upstream model availability. Synthetic smoke exercises conversion through final private transport envelopes, tool history, thinking controls and source immutability, with no network inference.
