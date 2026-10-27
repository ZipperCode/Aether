# Design

Keep public provider-pool signatures and existing persistence/API shape. Antigravity uses trimmed case-insensitive exact window identities and the first object model map in `quota_by_model`, `models`, `model_quotas` order. Retain explicit generic windows, adapter account fallback, multiple exact-window `all` semantics, and source timestamp precedence. Other providers retain family matching and merged legacy maps.

Reset deadlines use existing timestamp parsing in reset_at, next_reset_at, reset_time, next_reset_time, relative-seconds order; invalid aliases fall through without fabricated deadlines.

Antigravity adapter derives quota_hard_blocked from quota_exhausted. This is provider-resettable evidence, not independent persistent scheduling.quota_exhausted. Preserve probe eligibility and established skip reasons.

Gateway candidate snapshot becomes BTreeMap<key_id, BTreeMap<selected_provider_model_name, bool>> with entry deduplication and borrowed lookup. PoolGroup representative exemption remains.

Successful Antigravity model refresh persistence invalidates existing candidate caches via invalidate_provider_quota_candidate_caches; keep affinity and all independent guards. No new timers, structures, models, or configuration.

Test boundaries: inline provider tests; gateway candidate and pool cursor behavior; localhost full HTTP native Gemini fixture serving mapped claude-sonnet-4-5 against two explicit account catalogs, ordinary and Pool/cache-affinity modes.
