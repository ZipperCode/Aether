# Subsequent Gemini Flash requests (read-only)

## Window and evidence

192.168.2.212, `2026-10-25 20:09:54+08 <= usage.created_at < 2026-10-25 21:07:12+08`. Read-only SQL on usage/request_candidates plus bounded aether-app logs. No provider generation calls, credential output, service changes, or configuration writes.

The final bounded query observed 271 gemini-3.8-flash requests: 225 completed200,46 failed503. All271 had542 Antigravity upstream404 attempts, exactly2 per request on the same unrestricted account. No other Antigravity account was attempted for this model. Candidate failures span20:09:54.236–21:03:06.110.

Pool logs in the analysis window contain319 Antigravity/Gemini pool_group_exhausted events with key_model_disabled155–157. Counts are filter observations, not distinct accounts or requests. Representative20:09:56.093: scanned_keys286,budget_scanned_keys158,max_scanned_keys512,skip_reason_counts={key_model_disabled:157}. Current inventory156 active accounts:1 auto_fetch=false/allowed_modelsNULL;155 auto_fetch=true with real suffixed Flash IDs. These facts and the reproduced pool hydration bug establish the same pool failure chain; an upstream404 body alone does not prove its complete upstream resource cause.

## Independent failures

Other provider candidates in Gemini chains included thought-signature400 (Google162,Google-free44,shubiaobiao63), invalid-API-key400 (Google-free156), auth_unavailable503 (54), TLS handshakeEOF(115), quota429 and first-byte timeout. These remain outside the account-model fix. Antigravity Claude tool_use.id required400 and VALIDATION_REQUIRED403 occurred separately; Claude also succeeded through multi-account fallback.

Representative failed request:79b6d6fc-ee8d-4008-b76f-88ff8fec686d. Representative fallback success:52d1e254-c0b6-4569-b32b-6e4abb766d9d.

## Query shape

```sql
BEGIN READ ONLY;
SET LOCAL statement_timeout='10s';
WITH reqs AS (
 SELECT request_id FROM usage
 WHERE created_at >= '2026-10-25 20:09:54+08'
   AND created_at < '2026-10-25 21:07:12+08'
   AND model='gemini-3.8-flash'
)
SELECT c.status,c.status_code,count(*) AS attempts,
 count(DISTINCT c.request_id) AS requests,count(DISTINCT c.key_id) AS keys
FROM request_candidates c JOIN reqs USING(request_id)
WHERE provider_id='4b872ec3-8fe8-4ea4-a348-c7ef5ef1b3b9'
GROUP BY c.status,c.status_code;
COMMIT;
```
