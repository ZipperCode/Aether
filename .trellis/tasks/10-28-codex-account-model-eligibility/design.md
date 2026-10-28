# Design

`fetch_standard_models` adds `codex_model_discovery: { model_ids: [...] }` only after successful remote Codex aggregation. Use `collect_codex_model_ids` over raw merged cards; serialize borrowed IDs before moving the Vec into fetched_model_ids. Shared namespace constant belongs to provider-transport capabilities and its existing exports.

Existing background success persistence and admin per-namespace persistence store the result and invalidate routing. Preset/native versioned-KV display paths never synthesize evidence. Failed refresh preserves the last successful namespace; successful writes replace it.

A shared borrowed-JSON helper in provider-transport performs exact, case-sensitive final-model membership for Codex OAuth Responses/Compact/Search. Reject the entire evidence array if any entry is not a nonblank, edge-trimmed string. No allocation, auth parsing, sets or I/O on the request path. Configuration permissions remain separate.

Consumer gates: common transport after current capability checks; ordinary runtime after existing denials and only for non-PoolGroup; actual strong-read pool members before fallback/window truncation; standard admin test after final request model resolution and before refresh/send. Preserve Antigravity policy and quota/auth/cooldown priority. Use the existing Gateway provider_transport seam.

Regression ownership: transport unit behavior; model-fetch raw provenance; Gateway persistence; dedicated loopback ai_execute/stream/codex_model_discovery fixture for real catalog fetch, HTTP/SSE/WebSocket/admin and temporary independent curl launcher. Update only affected Codex OAuth fixtures.

Rollout requires per-account successful admin force refresh before traffic; old permission metadata is not backfilled as discovery evidence. No TTL or new retry policy.
