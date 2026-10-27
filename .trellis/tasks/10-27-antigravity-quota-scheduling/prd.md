# Antigravity resettable quota scheduling

Approved requirement: exclude an Antigravity account only for the requested exhausted upstream model, and automatically restore eligibility after a successful quota refresh or elapsed reset. Preserve per-account model resolution, explicit runtime quota bans, active/OAuth/health/cooldown restrictions, model catalog ownership, API shapes, and other providers' behavior.

Acceptance:
- Exact model identities cannot be masked by available siblings; authoritative `quota_by_model` wins over legacy maps, including an empty object.
- Unix/RFC3339 reset aliases restore only resettable quota eligibility; malformed deadlines retain exhaustion.
- Ordinary and Pool/sticky scheduling reject known exhausted Antigravity models despite the legacy ignore switch; exhausted accounts remain eligible for quota probes.
- Candidate snapshots preserve separate results per actual key/model pair.
- Only successfully persisted model quota refreshes invalidate candidate and resolved-page caches; failed fetch/CAS and account observations do not restore old exhausted models.
- Real localhost HTTP gateway/execution fixtures prove A→B→A serving, failed refresh preservation, independent manual restrictions, and all-exhausted 503 in ordinary and Pool modes, without production credentials or external dependencies.

User-approved full plan: `local://antigravity-quota-scheduling-plan.md`.
