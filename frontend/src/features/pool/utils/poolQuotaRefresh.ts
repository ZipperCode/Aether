import type { PoolKeyDetail } from '@/api/endpoints/pool'
import type { RefreshQuotaResult } from '@/api/endpoints/keys'
import { isQuotaSchedulingBlocked } from './poolKeyStatus'
import { isRecoverableRuntimeQuotaHoldProvider } from '@/features/providers/utils/providerKeyQuota'

/**
 * 判断号池行在刷新前是否处于 Codex / Antigravity 运行时 quota_exhausted 调度阻断。
 * 仅依据运行时调度状态判断，不根据配额百分比猜测可调度性。
 */
export function isRuntimeQuotaExhaustedPoolKey(
  key: PoolKeyDetail,
  fallbackProviderType?: string | null,
): boolean {
  const providerType = String(key.provider_type || fallbackProviderType || '').trim().toLowerCase()
  if (!isRecoverableRuntimeQuotaHoldProvider(providerType)) return false
  if (isQuotaSchedulingBlocked(key)) return true
  const reason = String(key.scheduling_reason || '').trim().toLowerCase()
  if (reason === 'key_quota_exhausted' || reason === 'quota_exhausted') return true
  return (key.scheduling_reasons ?? []).some((item) => {
    if (item.blocking === false) return false
    const code = String(item.code || '').trim().toLowerCase()
    return code === 'key_quota_exhausted' || code === 'quota_exhausted'
  })
}

/**
 * 判断额度刷新结果是否命中了刷新前处于运行时 quota_exhausted 的 Codex / Antigravity 行。
 * 命中时需通过非破坏性列表加载器重载当前页权威调度状态；普通未阻断行保持就地合并、不重载。
 */
export function shouldReloadPoolPageAfterQuotaRefresh(
  keys: PoolKeyDetail[],
  results: RefreshQuotaResult['results'],
  fallbackProviderType?: string | null,
  refreshedKeyIds?: readonly string[],
): boolean {
  const keyById = new Map<string, PoolKeyDetail>()
  for (const key of keys) {
    keyById.set(key.key_id, key)
  }

  if (results.length > 0) {
    return results.some((result) => {
      if (result.status === 'error' || result.status === 'backoff' || result.status === 'no_metadata') {
        return false
      }
      const row = keyById.get(result.key_id)
      return row ? isRuntimeQuotaExhaustedPoolKey(row, fallbackProviderType) : false
    })
  }

  if (!refreshedKeyIds || refreshedKeyIds.length === 0) return false
  return refreshedKeyIds.some((keyId) => {
    const row = keyById.get(keyId)
    return row ? isRuntimeQuotaExhaustedPoolKey(row, fallbackProviderType) : false
  })
}
/**
 * 将刷新结果合并到当前 Pool 页快照。
 * 额度与 Provider 原始元数据分别增量覆盖，未返回的本地状态保持不变。
 */
export function mergePoolKeyQuotaSnapshots(
  keys: PoolKeyDetail[],
  results: RefreshQuotaResult['results'],
): PoolKeyDetail[] {
  const resultByKeyId = new Map<string, RefreshQuotaResult['results'][number]>()
  for (const result of results) {
    if (result.quota_snapshot || result.metadata) {
      resultByKeyId.set(result.key_id, result)
    }
  }
  if (resultByKeyId.size === 0) return keys

  return keys.map((key) => {
    const result = resultByKeyId.get(key.key_id)
    if (!result) return key
    const quotaSnapshot = result.quota_snapshot
    const providerType = String(quotaSnapshot?.provider_type || key.provider_type || '').trim().toLowerCase()
    return {
      ...key,
      ...(quotaSnapshot ? {
        quota_updated_at: quotaSnapshot.updated_at ?? quotaSnapshot.observed_at ?? key.quota_updated_at ?? null,
        status_snapshot: {
          ...(key.status_snapshot ?? {}),
          oauth: key.status_snapshot?.oauth ?? { code: 'none' },
          account: key.status_snapshot?.account ?? { code: 'unknown', blocked: false },
          quota: quotaSnapshot,
        },
      } : {}),
      ...(result.metadata && providerType ? {
        upstream_metadata: {
          ...(key.upstream_metadata ?? {}),
          [providerType]: result.metadata,
        },
      } : {}),
    }
  })
}
