import { describe, expect, it } from 'vitest'

import type { PoolKeyDetail } from '@/api/endpoints/pool'
import type { RefreshQuotaResult } from '@/api/endpoints/keys'
import { mergePoolKeyQuotaSnapshots } from '@/features/pool/utils/poolQuotaRefresh'

function createKey(keyId: string): PoolKeyDetail {
  return {
    key_id: keyId,
    key_name: keyId,
    is_active: true,
    auth_type: 'oauth',
    api_formats: ['openai:responses'],
    internal_priority: 0,
    account_quota: null,
    cooldown_reason: null,
    cooldown_ttl_seconds: null,
    cost_window_usage: 0,
    cost_limit: null,
    request_count: 0,
    total_tokens: 0,
    total_cost_usd: '0',
    sticky_sessions: 0,
    lru_score: null,
    created_at: null,
    last_used_at: null,
  }
}

describe('mergePoolKeyQuotaSnapshots', () => {
  /** 刷新成功只更新额度；管理员恢复前，调度阻断和已有模型探测证据必须保留。 */
  it('preserves runtime scheduling and model-probe state during quota refresh', () => {
    const key = createKey('blocked')
    key.status_snapshot = {
      oauth: { code: 'valid' },
      account: { code: 'ok', blocked: false },
      quota: { code: 'exhausted', exhausted: true },
      scheduling: { code: 'quota_exhausted', blocked: true, requires_manual_recovery: true },
      model_probe: { status: 'ok', model: 'model-a' },
    }

    const [updated] = mergePoolKeyQuotaSnapshots([key], [{
      key_id: key.key_id,
      key_name: key.key_name,
      status: 'success',
      quota_snapshot: { code: 'ok', exhausted: false },
    }])

    expect(updated.status_snapshot?.quota.exhausted).toBe(false)
    expect(updated.status_snapshot?.scheduling).toEqual(key.status_snapshot.scheduling)
    expect(updated.status_snapshot?.model_probe).toEqual(key.status_snapshot.model_probe)
    expect(updated.status_snapshot?.oauth).toEqual(key.status_snapshot.oauth)
  })

  it('merges snapshots from non-success quota results', () => {
    const keys = [createKey('exhausted'), createKey('invalid'), createKey('unchanged')]
    const results: RefreshQuotaResult['results'] = [
      {
        key_id: 'exhausted',
        key_name: 'exhausted',
        status: 'quota_exhausted',
        quota_snapshot: {
          code: 'exhausted',
          exhausted: true,
          updated_at: 123,
        },
      },
      {
        key_id: 'invalid',
        key_name: 'invalid',
        status: 'auth_invalid',
        quota_snapshot: {
          code: 'unknown',
          exhausted: false,
          observed_at: 456,
        },
      },
      {
        key_id: 'unchanged',
        key_name: 'unchanged',
        status: 'error',
      },
    ]

    const merged = mergePoolKeyQuotaSnapshots(keys, results)

    expect(merged[0].status_snapshot?.quota.code).toBe('exhausted')
    expect(merged[0].quota_updated_at).toBe(123)
    expect(merged[1].status_snapshot?.quota.code).toBe('unknown')
    expect(merged[1].quota_updated_at).toBe(456)
    expect(merged[2]).toBe(keys[2])
  })
})
