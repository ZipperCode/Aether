import { getI18nLocale, translate, type Locale, type MessageKey } from '@/i18n'
import type { PoolKeyDetail } from '@/api/endpoints/pool'
import { buildPoolMobileTagItems } from '@/features/pool/utils/poolMobilePresentation'
import type { PoolMobileTagItem, PoolMobileTagTone } from '@/features/pool/utils/poolMobilePresentation'
import { getOAuthOrgBadge } from '@/features/providers/utils/oauthIdentity'
import { formatOAuthPlanType } from '@/features/providers/utils/oauthPlanType'
import { resolveAntigravityAccountTierLabel } from '@/features/providers/utils/antigravityQuota'
import { getProviderAuthLabel, isOAuthManagedCredential } from '@/features/providers/utils/providerKeyAuth'
import {
  getAccountStatusDisplay,
  getOAuthRefreshButtonTitle as resolveOAuthRefreshButtonTitle,
  getOAuthStatusDisplayWithFallback,
  getOAuthStatusTitle as resolveOAuthStatusTitle,
} from '@/features/providers/utils/providerKeyStatus'
import {
  getLegacyAccountQuotaText,
  getQuotaExhaustedSchedulingHint,
  getQuotaExhaustedSchedulingLabel,
  isRecoverableRuntimeQuotaHold,
} from '@/features/providers/utils/providerKeyQuota'
import { getQuotaSnapshot, resolvePoolKeyPlanType } from './poolKeyQuotaProgress'

export type PoolKeyScore = NonNullable<PoolKeyDetail['pool_score']>

export const poolScoreHardStateOptions = [
  { value: 'all', get label() { return translate('ui.allStatuses') } },
  { value: 'available', get label() { return translate('ui.available4d99c97') } },
  { value: 'unknown', get label() { return translate('chart.unknown') } },
  { value: 'cooldown', get label() { return translate('ui.cooldown') } },
  { value: 'quota_exhausted', get label() { return translate('ui.quotaExhausted') } },
  { value: 'auth_invalid', get label() { return translate('ui.invalidAuthorization') } },
  { value: 'banned', get label() { return translate('ui.banned') } },
  { value: 'inactive', get label() { return translate('ui.disable') } },
]

export const poolScoreProbeStatusOptions = [
  { value: 'all', get label() { return translate('ui.allProbes') } },
  { value: 'never', get label() { return translate('ui.notProbed') } },
  { value: 'ok', get label() { return translate('ui.normal') } },
  { value: 'failed', get label() { return translate('ui.failed28384d7') } },
  { value: 'stale', get label() { return translate('ui.expired97399bd') } },
  { value: 'in_progress', get label() { return translate('ui.probing') } },
]

export const COOLDOWN_REASON_MAP: Record<string, string> = {
  get rate_limited_429() { return translate('ui.429RateLimited') },
  get forbidden_403() { return translate('ui.403Forbidden') },
  get overloaded_529() { return translate('ui.529Overloaded') },
  get auth_failed_401() { return translate('ui.401Unauthorized') },
  get payment_required_402() { return translate('ui.402PaymentRequired') },
  get server_error_500() { return translate('ui.500ServerError') },
  get request_timeout_408() { return translate('ui.408RequestTimeout') },
  get conflict_409() { return translate('ui.409Conflict') },
  get locked_423() { return translate('ui.423Locked') },
  too_early_425: '425 Too Early',
  get bad_gateway_502() { return translate('ui.502BadGateway') },
  get service_unavailable_503() { return translate('ui.503ServiceUnavailable') },
  get gateway_timeout_504() { return translate('ui.504GatewayTimeout') },
}

export function formatCooldownReason(reason: string): string {
  return COOLDOWN_REASON_MAP[reason] || reason
}

export type PoolStatusVariant = 'default' | 'secondary' | 'destructive' | 'outline' | 'success' | 'warning' | 'dark'

export function isHealthDerivedSchedulingReason(reason: string | null | undefined): boolean {
  const normalized = String(reason || '').trim().toLowerCase()
  return normalized === 'health_low'
    || normalized === 'health_degraded'
    || normalized === 'health'
    || normalized === 'circuit_open'
    || normalized === 'circuit_breaker'
}

export function isHealthDerivedSchedulingLabel(label: string | null | undefined): boolean {
  const normalized = String(label || '').trim()
  return normalized === '健康低'
    || normalized === '健康度较低'
    || normalized === '降级'
    || normalized === '熔断'
    || normalized === '熔断中'
}

export function getVisibleSchedulingReason(key: PoolKeyDetail): string | null {
  const reason = String(key.scheduling_reason || '').trim()
  if (!reason || isHealthDerivedSchedulingReason(reason)) return null
  return reason
}

export function getVisibleSchedulingReasons(key: PoolKeyDetail) {
  return (key.scheduling_reasons ?? []).filter((item) => {
    const source = String(item.source || '').trim().toLowerCase()
    return source !== 'health'
      && !isHealthDerivedSchedulingReason(item.code)
      && !isHealthDerivedSchedulingLabel(item.label)
  })
}

export function getSchedulingStatus(key: PoolKeyDetail, tick: number): 'available' | 'degraded' | 'blocked' {
  if (getAccountAlertLabel(key)) return 'blocked'
  if (getBlockingOAuthStatusLabel(key, tick)) return 'blocked'

  const status = key.scheduling_status
  if (
    (status === 'available' || status === 'degraded' || status === 'blocked')
    && !isHealthDerivedSchedulingReason(key.scheduling_reason)
    && !isHealthDerivedSchedulingLabel(key.scheduling_label)
  ) {
    return status
  }

  if (!key.is_active) return 'blocked'
  if (key.cooldown_reason) return 'degraded'
  if (key.cost_limit != null && key.cost_limit > 0 && key.cost_window_usage >= key.cost_limit) return 'blocked'
  return 'available'
}

export function compactPoolStatusLabel(label: string | null | undefined): string | null {
  const rawLabel = String(label || '')
  const normalized = rawLabel.trim()
  if (!normalized) return null

  const knownLabels: Record<string, MessageKey> = {
    'Token 失效': 'ui.invalid',
    'Token 过期': 'ui.expired',
    Token失效: 'ui.invalid',
    Token过期: 'ui.expired',
    账号已封禁: 'ui.accountBanned',
    账号封禁: 'ui.accountBanned',
    工作区已停用: 'ui.workspaceDisabled',
    工作区停用: 'ui.workspaceDisabled',
    账号访问受限: 'ui.accessRestricted',
    访问受限: 'ui.accessRestricted',
    账号停用: 'ui.accountDisabled',
    需要验证: 'ui.verificationRequired',
    账号隔离: 'ui.accountIsolation',
    账号异常: 'ui.accountIssue',
    健康度较低: 'ui.lowHealth',
  }
  const key = knownLabels[normalized]
  // Unknown labels may be server or user content. Do not truncate them or
  // attempt to infer a translation from their text.
  return key ? translate(key) : rawLabel
}

export function getOAuthStatusBadgeLabel(status: ReturnType<typeof getVisibleOAuthState>): string | null {
  if (!status) return null
  if (status.requiresReauth) return translate('ui.renewalFailed')
  if (status.isInvalid) return translate('ui.invalid')
  if (status.isExpired) return translate('ui.expired')
  if (status.kind === 'missing_refresh_token') return translate('ui.notAdded')
  if (status.kind === 'unknown_expiration') return translate('chart.unknown')
  if (status.isExpiringSoon) return translate('ui.expiringSoon')
  return translate('ui.valid')
}

export function getBlockingOAuthStatusLabel(key: PoolKeyDetail, tick: number): string | null {
  const oauthState = getVisibleOAuthState(key, tick)
  if (!oauthState?.isInvalid && !oauthState?.isExpired) return null
  return getOAuthStatusBadgeLabel(oauthState)
}

export function isQuotaSchedulingBlocked(key: PoolKeyDetail): boolean {
  const scheduling = key.status_snapshot?.scheduling
  return scheduling?.code === 'quota_exhausted' && scheduling.blocked === true
}

export function isPoolKeyCostExhausted(key: PoolKeyDetail): boolean {
  return key.cost_limit != null
    && key.cost_limit > 0
    && key.cost_window_usage >= key.cost_limit
}

const schedulingLabelKeys: Partial<Record<string, MessageKey>> = {
  available: 'ui.available4d99c97',
  inactive: 'ui.disabledbc5a87a',
  manual_disabled: 'ui.disable',
  cooldown: 'ui.coolingDown',
  account_quota_exhausted: 'ui.quotaExhausted',
  account_quota_reserved: 'ui.quotaReserved',
  cost_exhausted: 'ui.limitExceeded',
  invalid: 'ui.invalid',
  expired: 'ui.expired',
  banned: 'ui.accountBanned',
  forbidden: 'ui.accessRestricted',
  quarantined: 'ui.accountIsolation',
  rate_limited: 'ui.rateLimited6d5ac6f',
  oauth_token_invalid: 'ui.invalid',
  oauth_token_expired: 'ui.expired',
  account_banned: 'ui.accountBanned',
  account_suspended: 'ui.accountBanned',
  account_disabled: 'ui.accountDisabled',
  workspace_deactivated: 'ui.workspaceDisabled',
  account_forbidden: 'ui.accessRestricted',
  account_verification: 'ui.verificationRequired',
  account_quarantined: 'ui.accountIsolation',
  account_blocked: 'ui.accountIssue',
  quota_exhausted: 'ui.quotaExhausted',
  quota_reserved: 'ui.quotaReserved',
  key_quota_exhausted: 'pool.status.quotaExhaustedManualRecovery',
  quota_suspected: 'pool.status.quotaSuspected',
}

function getKnownSchedulingLabel(code: string | null | undefined): string | null {
  const key = schedulingLabelKeys[String(code || '').trim().toLowerCase()]
  return key ? translate(key) : null
}

function resolvePoolKeyProviderType(key: PoolKeyDetail, providerType?: string | null): string {
  return String(providerType || key.provider_type || key.status_snapshot?.quota?.provider_type || '').trim().toLowerCase()
}

function isQuotaExhaustedSchedulingCode(code: string | null | undefined): boolean {
  const normalized = String(code || '').trim().toLowerCase()
  return normalized === 'key_quota_exhausted' || normalized === 'quota_exhausted'
}

function isAntigravityProvider(providerType?: string | null): boolean {
  return String(providerType || '').trim().toLowerCase() === 'antigravity'
}

function isCodexOAuthKey(key: PoolKeyDetail, providerType?: string | null): boolean {
  if (String(providerType || '').trim().toLowerCase() !== 'codex') return false
  const authType = String(key.auth_type || '').trim().toLowerCase()
  return authType === 'oauth' || isOAuthManagedCredential(key)
}

function isAntigravityInferenceRejectionCooldown(
  key: PoolKeyDetail,
  providerType?: string | null,
  tick = 0,
): boolean {
  if (!isAntigravityProvider(providerType)) return false
  if (key.cooldown_reason !== 'forbidden_403') return false
  if (!key.is_active) return false
  if (getAccountAlertLabel(key)) return false
  if (getBlockingOAuthStatusLabel(key, tick)) return false
  const visibleReason = getVisibleSchedulingReason(key)
  return visibleReason !== 'manual_disabled' && visibleReason !== 'inactive'
}


export interface PoolKeyModelFetchDisplay {
  modelFetchLabel: string
  modelFetchClass: string
  modelFetchTitle: string
  modelFetchHint: string
}

export function getPoolKeyModelFetchDisplay(
  key: PoolKeyDetail,
  providerType?: string | null,
): PoolKeyModelFetchDisplay {
  if (isCodexOAuthKey(key, providerType)) {
    const errorText = String(key.last_models_fetch_error || '').trim()
    const catalogReady = key.codex_model_catalog_ready === true
    const lastAttemptText = key.last_models_fetch_at
      ? formatUnixSecondsWithSeconds(key.last_models_fetch_at)
      : ''

    if (errorText) {
      const hint = catalogReady
        ? translate('pool.discovery.failedWithCatalogHint')
        : translate('pool.discovery.failedActionHint')
      const titleParts = [hint]
      if (lastAttemptText) {
        titleParts.push(translate('pool.discovery.lastAttemptAt', { time: lastAttemptText }))
      }
      return {
        modelFetchLabel: translate('pool.discovery.failed'),
        modelFetchClass: 'text-amber-600 dark:text-amber-400',
        modelFetchTitle: titleParts.join('\n'),
        modelFetchHint: hint,
      }
    }

    if (catalogReady) {
      return {
        modelFetchLabel: translate('pool.discovery.ready'),
        modelFetchClass: 'text-emerald-600 dark:text-emerald-400',
        modelFetchTitle: lastAttemptText
          ? translate('pool.discovery.lastDiscoveredAt', { time: lastAttemptText })
          : translate('pool.discovery.readyTitle'),
        modelFetchHint: '',
      }
    }

    const hint = key.auto_fetch_models === false
      ? translate('pool.discovery.pendingManualHint')
      : translate('pool.discovery.pendingAutoHint')
    const titleParts = [hint]
    if (lastAttemptText) {
      titleParts.push(translate('pool.discovery.lastAttemptAt', { time: lastAttemptText }))
    }
    return {
      modelFetchLabel: translate('pool.discovery.pending'),
      modelFetchClass: 'text-muted-foreground',
      modelFetchTitle: titleParts.join('\n'),
      modelFetchHint: hint,
    }
  }

  if (!key.auto_fetch_models) {
    return { modelFetchLabel: '', modelFetchClass: '', modelFetchTitle: '', modelFetchHint: '' }
  }
  if (key.last_models_fetch_error) {
    return {
      modelFetchLabel: translate('pool.discovery.syncFailed'),
      modelFetchClass: 'text-amber-600 dark:text-amber-400',
      modelFetchTitle: key.last_models_fetch_error,
      modelFetchHint: '',
    }
  }
  if (key.last_models_fetch_at) {
    return {
      modelFetchLabel: translate('pool.discovery.syncSuccess'),
      modelFetchClass: 'text-emerald-600 dark:text-emerald-400',
      modelFetchTitle: translate('pool.discovery.lastSyncedAt', {
        time: formatUnixSecondsWithSeconds(key.last_models_fetch_at),
      }),
      modelFetchHint: '',
    }
  }
  return {
    modelFetchLabel: translate('pool.discovery.syncPending'),
    modelFetchClass: 'text-muted-foreground',
    modelFetchTitle: translate('pool.discovery.syncPendingTitle'),
    modelFetchHint: '',
  }
}

export interface AntigravityInferenceRejectionDisplay {
  mode: 'cooldown' | 'history'
  label: string
  lastOccurredText: string
  retryAtText: string
  remainingText: string
  title: string
}

export function getAntigravityInferenceRejectionDisplay(
  key: PoolKeyDetail,
  providerType: string | null | undefined,
  tick: number,
): AntigravityInferenceRejectionDisplay | null {
  if (!isAntigravityProvider(providerType)) return null

  const rejection = key.last_inference_rejection
  const observedAt = Number(rejection?.observed_at ?? 0)
  const retryAt = Number(rejection?.retry_at ?? 0)
  const hasValidRejection = Number.isFinite(observedAt) && observedAt > 0
  const lastOccurredText = hasValidRejection
    ? translate('pool.status.inferenceRejectedLastOccurred', {
        time: formatUnixSecondsWithSeconds(observedAt),
      })
    : ''
  const retryAtText = Number.isFinite(retryAt) && retryAt > 0
    ? translate('pool.status.inferenceRejectedRetryAt', {
        time: formatUnixSecondsWithSeconds(retryAt),
      })
    : ''

  if (isAntigravityInferenceRejectionCooldown(key, providerType, tick)) {
    const remainingSeconds = key.cooldown_ttl_seconds != null && key.cooldown_ttl_seconds > 0
      ? key.cooldown_ttl_seconds
      : (Number.isFinite(retryAt) && retryAt > 0 && tick > 0
          ? Math.max(0, Math.ceil(retryAt - tick / 1000))
          : 0)
    const remainingText = remainingSeconds > 0 ? formatTTL(remainingSeconds) : ''
    const title = [
      remainingText
        ? `${translate('pool.status.inferenceRejected')} (${remainingText})`
        : translate('pool.status.inferenceRejected'),
      lastOccurredText,
      retryAtText,
    ].filter(Boolean).join('\n')
    return {
      mode: 'cooldown',
      label: '',
      lastOccurredText,
      retryAtText,
      remainingText,
      title,
    }
  }

  if (!hasValidRejection) return null

  const label = translate('pool.status.inferenceRejectedHistory')
  const title = [
    translate('pool.status.inferenceRejectedHistoryTitle'),
    lastOccurredText,
    retryAtText,
  ].filter(Boolean).join('\n')
  return {
    mode: 'history',
    label,
    lastOccurredText,
    retryAtText,
    remainingText: '',
    title,
  }
}

export function getSchedulingBadgeLabel(
  key: PoolKeyDetail,
  tick: number,
  providerType?: string | null,
): string {
  const accountAlert = getAccountAlertLabel(key)
  if (accountAlert) return compactPoolStatusLabel(accountAlert) || accountAlert
  const oauthAlert = getBlockingOAuthStatusLabel(key, tick)
  if (oauthAlert) return oauthAlert
  if (isAntigravityInferenceRejectionCooldown(key, providerType, tick)) {
    return translate('pool.status.inferenceRejected')
  }

  const effectiveProviderType = resolvePoolKeyProviderType(key, providerType)
  const visibleReason = getVisibleSchedulingReason(key)
  if (isQuotaExhaustedSchedulingCode(visibleReason)) {
    return getQuotaExhaustedSchedulingLabel(effectiveProviderType, key.is_active)
  }
  const knownLabel = getKnownSchedulingLabel(visibleReason)
  if (knownLabel) {
    if (key.scheduling_reason === 'available' && isPoolKeyCostExhausted(key)) return translate('ui.limitExceeded')
    return knownLabel
  }
  if (key.is_active !== false && isQuotaSchedulingBlocked(key)) {
    return getQuotaExhaustedSchedulingLabel(effectiveProviderType, key.is_active)
  }

  const rawLabel = String(key.scheduling_label || '')
  const normalizedLabel = rawLabel.trim()
  if (
    normalizedLabel
    && !isHealthDerivedSchedulingReason(key.scheduling_reason)
    && !isHealthDerivedSchedulingLabel(rawLabel)
  ) {
    if ((normalizedLabel === '可用' || key.scheduling_reason === 'available') && isPoolKeyCostExhausted(key)) {
      return translate('ui.limitExceeded')
    }
    if (normalizedLabel === '禁用' || normalizedLabel === '停用') return translate('ui.disable')
    return compactPoolStatusLabel(rawLabel) || rawLabel
  }

  if (!key.is_active) return translate('ui.disabledbc5a87a')
  if (key.cooldown_reason) return translate('ui.coolingDown')
  return translate('ui.available4d99c97')
}

export function getSchedulingBadgeVariant(
  key: PoolKeyDetail,
  tick: number,
  providerType?: string | null,
): PoolStatusVariant {
  if (getAccountAlertLabel(key)) return 'destructive'
  if (getBlockingOAuthStatusLabel(key, tick)) return 'destructive'
  if (isAntigravityInferenceRejectionCooldown(key, providerType, tick)) return 'warning'

  const reason = getVisibleSchedulingReason(key)
  if (reason === 'manual_disabled' || reason === 'inactive') return 'secondary'
  if (reason === 'account_blocked' || reason === 'account_quota_exhausted' || reason === 'key_quota_exhausted' || reason === 'quota_exhausted' || reason === 'cost_exhausted') return 'destructive'
  if (reason === 'account_quota_reserved' || reason === 'quota_reserved') return 'warning'
  if (reason === 'cooldown') return 'warning'
  if (reason === 'cost_soft' || reason === 'cost') return 'warning'
  if (isPoolKeyCostExhausted(key)) return 'destructive'
  if (reason === 'available') return 'default'
  if (!reason && !key.is_active) return 'secondary'

  const status = getSchedulingStatus(key, tick)
  if (status === 'blocked') return 'destructive'
  if (status === 'degraded') return 'warning'
  return 'default'
}

export function getSchedulingTitle(
  key: PoolKeyDetail,
  tick: number,
  providerType?: string | null,
): string {
  const effectiveProviderType = resolvePoolKeyProviderType(key, providerType)
  const accountAlertTitle = getAccountAlertTitle(key, effectiveProviderType)
  if (accountAlertTitle) return accountAlertTitle
  if (getBlockingOAuthStatusLabel(key, tick)) return getOAuthStatusTitle(key, tick)

  if (isAntigravityInferenceRejectionCooldown(key, effectiveProviderType, tick)) {
    const rejectionDisplay = getAntigravityInferenceRejectionDisplay(key, effectiveProviderType, tick)
    if (rejectionDisplay?.title) return rejectionDisplay.title
  }

  const reasons = getVisibleSchedulingReasons(key)
  if (reasons.length > 0) {
    let hasRecoverableQuotaHold = false
    const title = reasons.map((item) => {
      const ttl = item.ttl_seconds && item.ttl_seconds > 0 ? ` (${formatTTL(item.ttl_seconds)})` : ''
      const detail = item.detail ? ` - ${item.detail}` : ''
      if (isQuotaExhaustedSchedulingCode(item.code)) {
        if (item.blocking !== false && isRecoverableRuntimeQuotaHold(effectiveProviderType, key.is_active)) {
          hasRecoverableQuotaHold = true
        }
        return `${getQuotaExhaustedSchedulingLabel(effectiveProviderType, key.is_active)}${ttl}${detail}`
      }
      return `${getKnownSchedulingLabel(item.code) ?? item.label}${ttl}${detail}`
    }).join('\n')
    const lines = [title]
    const hint = hasRecoverableQuotaHold
      ? getQuotaExhaustedSchedulingHint(effectiveProviderType, key.is_active)
      : null
    if (hint) lines.push(hint)
    const observedAt = key.status_snapshot?.scheduling?.last_observed_at
    if (typeof observedAt === 'number') {
      lines.push(translate('pool.status.observedAt', { time: new Date(observedAt * 1000).toLocaleString(getI18nLocale()) }))
    }
    const baseTitle = lines.join('\n')
    if (isAntigravityProvider(effectiveProviderType)) {
      const rejectionDisplay = getAntigravityInferenceRejectionDisplay(key, effectiveProviderType, tick)
      if (rejectionDisplay?.title) return `${baseTitle}\n${rejectionDisplay.title}`
    }
    return baseTitle
  }

  if (key.cooldown_reason) {
    const ttl = key.cooldown_ttl_seconds ? ` (${formatTTL(key.cooldown_ttl_seconds)})` : ''
    return `${formatCooldownReason(key.cooldown_reason)}${ttl}`
  }
  if (isPoolKeyCostExhausted(key)) return translate('ui.limitExceeded')
  const baseLabel = getSchedulingBadgeLabel(key, tick, effectiveProviderType)
  const lines = [baseLabel]
  const isRecoverableQuotaHold = isRecoverableRuntimeQuotaHold(effectiveProviderType, key.is_active)
    && (isQuotaExhaustedSchedulingCode(getVisibleSchedulingReason(key)) || isQuotaSchedulingBlocked(key))
  if (isRecoverableQuotaHold) {
    const hint = getQuotaExhaustedSchedulingHint(effectiveProviderType, key.is_active)
    if (hint) lines.push(hint)
    const schedulingReason = String(key.status_snapshot?.scheduling?.reason || '').trim()
    if (schedulingReason) lines.push(schedulingReason)
    const observedAt = key.status_snapshot?.scheduling?.last_observed_at
    if (typeof observedAt === 'number') {
      lines.push(translate('pool.status.observedAt', { time: new Date(observedAt * 1000).toLocaleString(getI18nLocale()) }))
    }
  }
  if (isAntigravityProvider(effectiveProviderType)) {
    const rejectionDisplay = getAntigravityInferenceRejectionDisplay(key, effectiveProviderType, tick)
    if (rejectionDisplay?.title) lines.push(rejectionDisplay.title)
  }
  return lines.join('\n')
}

export function formatTTL(seconds: number): string {
  if (seconds <= 0) return ''
  const m = Math.floor(seconds / 60)
  const s = seconds % 60
  return m > 0 ? `${m}m ${s}s` : `${s}s`
}

export function getRowClass(key: PoolKeyDetail, tick: number): string {
  const status = getSchedulingStatus(key, tick)
  if (!key.is_active || status === 'blocked') return 'bg-muted/50 opacity-60'
  return ''
}

export function getAuthTypeChipLabel(key: PoolKeyDetail): string {
  return getProviderAuthLabel(key)
}

export function getMobileOAuthTone(key: PoolKeyDetail, tick: number): PoolMobileTagTone | null {
  const oauthState = getVisibleOAuthState(key, tick)
  if (!oauthState) return null
  if (oauthState.isInvalid || oauthState.isExpired) return 'danger'
  if (oauthState.isExpiringSoon) return 'warning'
  return 'muted'
}

export function getMobileTagItems(key: PoolKeyDetail, tick: number, fallbackProviderType = ''): PoolMobileTagItem[] {
  const accountAlert = getAccountAlertLabel(key)
  const oauthState = getVisibleOAuthState(key, tick)
  const orgBadge = getOAuthOrgBadge(key)
  const planType = resolvePoolKeyPlanType(key, fallbackProviderType)

  return buildPoolMobileTagItems({
    accountStatusLabel: compactPoolStatusLabel(accountAlert),
    accountStatusTone: accountAlert ? 'danger' : null,
    oauthStatusLabel: getOAuthStatusBadgeLabel(oauthState),
    oauthStatusTone: getMobileOAuthTone(key, tick),
    priorityLabel: `P${key.internal_priority ?? 50}`,
    authLabel: getAuthTypeChipLabel(key),
    planLabel: fallbackProviderType === 'antigravity'
      ? resolveAntigravityAccountTierLabel(key.upstream_metadata, translate)
      : planType ? formatOAuthPlanType(planType) : null,
    orgLabel: orgBadge?.label ?? null,
    proxyLabel: key.proxy?.node_id ? translate('ui.dedicatedProxy') : null,
  })
}

export function getMobileTagClass(item: PoolMobileTagItem): string {
  if (item.tone === 'danger') {
    return 'border-red-500/30 bg-red-500/10 text-red-700 dark:text-red-300'
  }
  if (item.tone === 'warning') {
    return 'border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300'
  }
  if (item.tone === 'accent') {
    return 'border-blue-500/30 bg-blue-500/10 text-blue-700 dark:text-blue-300'
  }
  if (item.tone === 'muted') {
    return 'border-border/60 bg-background/70 text-muted-foreground'
  }
  return 'border-border/60 bg-background/80 text-foreground/80'
}

export function getVisibleOAuthState(key: PoolKeyDetail, tick: number) {
  return getOAuthStatusDisplayWithFallback(key, tick)
}

export function getOAuthRefreshButtonTitle(key: PoolKeyDetail, tick: number): string {
  return resolveOAuthRefreshButtonTitle(key, tick)
}

export function getOAuthStatusTitle(key: PoolKeyDetail, tick: number): string {
  return resolveOAuthStatusTitle(key, tick)
}

export const _accountAlertCache = new WeakMap<PoolKeyDetail, Partial<Record<Locale, string | null>>>()

export function getQuotaAlertSnapshotState(key: PoolKeyDetail): { label: string, title: string } | null {
  const quota = getQuotaSnapshot(key)
  if (!quota) return null

  const code = String(quota.code || '').trim().toLowerCase()
  if (!['banned', 'forbidden', 'quarantined', 'rate_limited', 'exhausted'].includes(code)) return null

  const knownLabel = getKnownSchedulingLabel(code === 'exhausted' ? 'account_quota_exhausted' : code)
  const label = knownLabel ?? String(quota.label || '').trim()

  const reason = String(quota.reason || '').trim()
  return {
    label,
    title: reason ? `${label}: ${reason}` : label,
  }
}

export function getAccountAlertLabel(key: PoolKeyDetail): string | null {
  const locale = getI18nLocale()
  const cached = _accountAlertCache.get(key)?.[locale]
  if (cached !== undefined) return cached

  const account = getAccountStatusDisplay(key)
  let result: string | null = (account.blocked || account.label)
    ? getKnownSchedulingLabel(account.code) ?? account.label
    : null
  const quotaAlert = getQuotaAlertSnapshotState(key)
  if (!result && quotaAlert) result = quotaAlert.label
  if (!result && !getQuotaSnapshot(key)) {
    const quotaText = getLegacyAccountQuotaText(key)
    if (quotaText === '账号已封禁' || quotaText === '封禁') result = translate('ui.accountBanned')
    else if (quotaText === '访问受限') result = translate('ui.accessRestricted')
  }

  _accountAlertCache.set(key, { ..._accountAlertCache.get(key), [locale]: result })
  return result
}

export function getAccountAlertTitle(key: PoolKeyDetail, providerType?: string | null): string {
  const label = getAccountAlertLabel(key)
  if (!label) return ''

  const account = getAccountStatusDisplay(key)
  if (account.blocked && account.label) {
    return account.reason ? `${label}: ${account.reason}` : label
  }

  const quotaAlert = getQuotaAlertSnapshotState(key)
  if (quotaAlert?.title) {
    const quotaCode = String(getQuotaSnapshot(key)?.code || '').trim().toLowerCase()
    const effectiveProviderType = resolvePoolKeyProviderType(key, providerType)
    if (quotaCode === 'exhausted' && isRecoverableRuntimeQuotaHold(effectiveProviderType, key.is_active)) {
      const hint = getQuotaExhaustedSchedulingHint(effectiveProviderType, key.is_active)
      return hint ? `${quotaAlert.title}\n${hint}` : quotaAlert.title
    }
    return quotaAlert.title
  }

  const quotaText = getLegacyAccountQuotaText(key)
  if (quotaText) return `${label}: ${quotaText}`
  return label
}

export function getQuotaTextClass(quotaText: string): string {
  if (quotaText.includes('封禁') || quotaText.includes('受限') || quotaText.includes('不可用')) {
    return 'text-[11px] text-destructive leading-4'
  }
  return 'text-[11px] text-foreground/90 leading-4'
}

export function formatPoolScore(value: number | null | undefined): string {
  const n = Number(value)
  if (!Number.isFinite(n)) return '-'
  return n.toFixed(3)
}

export function formatPoolScoreReason(value: PoolKeyScore['score_reason'] | null | undefined): string {
  if (!value) return translate('ui.noCalculationResults')
  try {
    return JSON.stringify(value, null, 2)
  } catch {
    return String(value)
  }
}

export function getPoolScoreHardStateLabel(value: PoolKeyScore['hard_state'] | null | undefined): string {
  if (!value) return '-'
  return poolScoreHardStateOptions.find(item => item.value === value)?.label || value
}

export function getPoolScoreProbeStatusLabel(value: PoolKeyScore['probe_status'] | null | undefined): string {
  if (!value) return '-'
  return poolScoreProbeStatusOptions.find(item => item.value === value)?.label || value
}

export function formatUnixSeconds(seconds: number | null | undefined): string {
  const raw = Number(seconds ?? 0)
  if (!Number.isFinite(raw) || raw <= 0) return '-'
  return formatRelativeTime(new Date(raw * 1000).toISOString())
}
export function formatUnixSecondsWithSeconds(seconds: number | null | undefined): string {
  const raw = Number(seconds ?? 0)
  if (!Number.isFinite(raw) || raw <= 0) return '-'
  const date = new Date(raw * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  const M = pad(date.getMonth() + 1)
  const D = pad(date.getDate())
  const h = pad(date.getHours())
  const m = pad(date.getMinutes())
  const s = pad(date.getSeconds())
  return `${M}-${D} ${h}:${m}:${s}`
}

export function formatRelativeTime(isoStr: string): string {
  const date = new Date(isoStr)
  const pad = (n: number) => String(n).padStart(2, '0')
  const M = pad(date.getMonth() + 1)
  const D = pad(date.getDate())
  const h = pad(date.getHours())
  const m = pad(date.getMinutes())
  return `${M}-${D} ${h}:${m}`
}

export function formatPoolKeyImportedAt(key: PoolKeyDetail): string {
  const value = key.imported_at || key.created_at
  return value ? formatRelativeTime(value) : '-'
}
