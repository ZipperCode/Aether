# Integration design

Use a normal non-fast-forward merge of upstream/main; no history rewriting. Upstream changes include referral refund flows and a wide frontend feature/i18n extraction. Preserve both local behavior and upstream structure/translations rather than selecting one whole side. Ownership: parent owns integration, i18n, pool, usage and remaining files; a frontend worker owns provider feature conflicts and ProviderManagement only. No overlapping edits; workers do not stage or commit.

Backend auto-merges require focused referral/wallet/module/provider-query and local model/stream regression checks. Frontend shared i18n and path changes justify the frontend suite and typechecked production build. No live financial actions or external model calls; HTTP and UI checks use local fixtures/demo. PR assessment is read-only after main integration.
