# Requirements

Integrate upstream PRs #893, #897, #898 and #899 into current master as authorized. Preserve existing native passthrough, account-scoped model resolution, schema budget and signature recovery contracts. Reuse already implemented behavior instead of duplicating or replacing it. No migrations, live provider calls or production data changes.

Acceptance: replayed Gemini signatures survive reasoning text and conversions; Cloud Code schemas retain applicable constraints without wrong-type keywords; injected Claude Code cache TTL follows client 1h markers; Antigravity client functions take priority over incompatible built-ins; audit-only payload compaction respects the existing switch, MIME metadata and 4096-byte threshold. Focused tests, static checks and changed UI copy observation pass; commit and push current branch.
