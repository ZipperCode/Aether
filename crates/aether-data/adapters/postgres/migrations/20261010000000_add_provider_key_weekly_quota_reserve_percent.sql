-- Account-level Codex weekly quota reserve threshold (0 disables the guard).
ALTER TABLE public.provider_api_keys
    ADD COLUMN IF NOT EXISTS weekly_quota_reserve_percent integer NOT NULL DEFAULT 0;
