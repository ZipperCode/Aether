import type {
  ModelProbeStatusSnapshot,
  ProviderKeyStatusSnapshot,
  QuotaStatusSnapshot,
  QuotaWindowSnapshot,
} from '@/api/endpoints/types/statusSnapshot'
import type { UpstreamMetadata } from '@/api/endpoints/types/provider'
import { getCodexQuotaWindowPresentation } from '@/utils/codexQuotaWindow'

export interface ProviderKeyQuotaCarrier {
  account_quota?: string | null
  status_snapshot?: ProviderKeyStatusSnapshot | null
  upstream_metadata?: UpstreamMetadata | null
}

export interface ProviderModelAvailabilityDisplay {
  status: 'ok' | 'failed' | 'unknown'
  title: string
  detail: string | null
  text: string
  model: string | null
  testedAt: number | null
}

function normalizeText(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const text = value.trim()
  return text || null
}

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) return 0
  if (value < 0) return 0
  if (value > 100) return 100
  return value
}

function formatPercent(value: number): string {
  return `${clampPercent(value).toFixed(1)}%`
}

function getQuotaSnapshot(
  input: ProviderKeyQuotaCarrier,
): QuotaStatusSnapshot | null {
  return input.status_snapshot?.quota ?? null
}

function getQuotaProviderType(
  quota: QuotaStatusSnapshot | null | undefined,
  fallbackProviderType?: string | null,
): string {
  const snapshotProviderType = normalizeText(quota?.provider_type)?.toLowerCase()
  if (snapshotProviderType) return snapshotProviderType
  return normalizeText(fallbackProviderType)?.toLowerCase() || ''
}

/** 智谱套餐查询失败后的标准余额仅供参考，不能据此判定模型不可调用。 */
export function isZhipuInformationalBalanceFallback(
  quota: QuotaStatusSnapshot | null | undefined,
  fallbackProviderType?: string | null,
): boolean {
  if (getQuotaProviderType(quota, fallbackProviderType) !== 'zhipu') return false
  if (normalizeText(quota?.kind)?.toLowerCase() !== 'balance') return false
  if (quota?.token_plan_scheduling_blocked !== false) return false
  const status = normalizeText(quota.token_plan_status)?.toLowerCase()
  return status === 'query_failed' || status === 'business_error'
}

/** DeepSeek 额度刷新失败时，保留的旧余额不能继续作为当前可用余额展示。 */
export function isDeepSeekQuotaUnavailable(
  quota: QuotaStatusSnapshot | null | undefined,
  fallbackProviderType?: string | null,
): boolean {
  if (getQuotaProviderType(quota, fallbackProviderType) !== 'deepseek') return false
  const freshness = normalizeText(quota?.freshness)?.toLowerCase()
  if (freshness === 'stale' || freshness === 'error') return true
  const code = normalizeText(quota?.code)?.toLowerCase()
  return code !== 'ok' && normalizeText(quota?.refresh_state?.error) != null
}

/** 官方额度接口明确拒绝鉴权时，Key 的额度快照与保留余额都不再可信。 */
export function isQuotaAuthenticationExpired(
  quota: QuotaStatusSnapshot | null | undefined,
): boolean {
  const code = normalizeText(quota?.code)?.toLowerCase()
  if (code === 'http_unauthorized') return true

  const error = normalizeText(quota?.refresh_state?.error)?.toLowerCase()
  return error?.startsWith('http_unauthorized:') === true
    || error === 'http_unauthorized'
    || error?.includes('quota upstream rejected authentication') === true
}

/** 通用额度区域应隐藏旧数据并展示账号级失效标记的场景。 */
export function isGenericQuotaUnavailable(
  quota: QuotaStatusSnapshot | null | undefined,
  fallbackProviderType?: string | null,
): boolean {
  return isQuotaAuthenticationExpired(quota)
    || isDeepSeekQuotaUnavailable(quota, fallbackProviderType)
}

function getQuotaWindows(
  quota: QuotaStatusSnapshot | null | undefined,
): QuotaWindowSnapshot[] {
  return Array.isArray(quota?.windows) ? quota.windows : []
}

function getQuotaWindowRemainingPercent(
  window: QuotaWindowSnapshot | null | undefined,
): number | null {
  if (!window) return null
  if (typeof window.remaining_ratio === 'number') {
    return clampPercent(window.remaining_ratio * 100)
  }
  if (typeof window.used_ratio === 'number') {
    return clampPercent((1 - window.used_ratio) * 100)
  }
  if (typeof window.limit_value === 'number' && window.limit_value > 0) {
    if (typeof window.remaining_value === 'number') {
      return clampPercent((window.remaining_value / window.limit_value) * 100)
    }
    if (typeof window.used_value === 'number') {
      return clampPercent((1 - (window.used_value / window.limit_value)) * 100)
    }
  }
  return null
}

function getQuotaWindow(
  quota: QuotaStatusSnapshot | null | undefined,
  code: string,
): QuotaWindowSnapshot | null {
  const normalizedCode = code.trim().toLowerCase()
  return getQuotaWindows(quota).find(window => normalizeText(window.code)?.toLowerCase() === normalizedCode) ?? null
}

function finiteNumber(value: unknown): number | null {
  if (typeof value === 'number') return Number.isFinite(value) ? value : null
  if (typeof value === 'string') {
    const text = value.trim()
    if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(text)) return null
    const parsed = Number(text)
    return Number.isFinite(parsed) ? parsed : null
  }
  return null
}

/** Plan 查询失败且没有正标准余额时，额度接口无法判断智谱 Key 是否可调用。 */
export function isZhipuAmbiguousQuotaFallback(
  quota: QuotaStatusSnapshot | null | undefined,
  fallbackProviderType?: string | null,
): boolean {
  if (!isZhipuInformationalBalanceFallback(quota, fallbackProviderType)) return false
  if (quota?.balance_insufficient === true) return true
  if (normalizeText(quota?.balance_status)?.toLowerCase() === 'insufficient') return true
  const availableBalances = (quota?.balances ?? [])
    .map(balance => finiteNumber(balance.available))
    .filter((value): value is number => value != null)
  return !availableBalances.some(value => value > 0)
}

export function getZhipuModelAvailabilityDisplay(
  quota: QuotaStatusSnapshot | null | undefined,
  modelProbe?: ModelProbeStatusSnapshot | null,
  fallbackProviderType?: string | null,
): ProviderModelAvailabilityDisplay | null {
  if (!isZhipuAmbiguousQuotaFallback(quota, fallbackProviderType)) return null

  const probeStatus = normalizeText(modelProbe?.status)?.toLowerCase()
  const model = normalizeText(modelProbe?.model)
  const testedAt = finiteNumber(modelProbe?.tested_at)
  if (probeStatus === 'ok') {
    const title = '模型调用已验证可用'
    const detail = '额度查询失败，额度未知'
    return { status: 'ok', title, detail, text: `${title} · ${detail}`, model, testedAt }
  }
  if (probeStatus === 'failed') {
    const title = '模型调用验证失败'
    const statusCode = finiteNumber(modelProbe?.status_code)
    const detail = normalizeText(modelProbe?.error)
      || (statusCode != null ? `上游 HTTP ${statusCode}` : null)
    return {
      status: 'failed',
      title,
      detail,
      text: detail ? `${title}：${detail}` : title,
      model,
      testedAt,
    }
  }

  const title = '额度未知，继续参与模型调度'
  return { status: 'unknown', title, detail: null, text: title, model: null, testedAt: null }
}

export function formatDecimalDisplay(value: unknown): string | null {
  if (typeof value === 'number') {
    return Number.isFinite(value)
      ? value.toLocaleString(undefined, { maximumFractionDigits: 20 })
      : null
  }
  if (typeof value !== 'string') return null
  const text = value.trim()
  const match = /^([+-]?)(\d+)(?:\.(\d*))?$/.exec(text)
  if (!match) return null
  const [, sign, integer, fraction] = match
  const grouped = integer.replace(/\B(?=(\d{3})+(?!\d))/g, ',')
  return `${sign}${grouped}${fraction === undefined ? '' : `.${fraction}`}`
}

function firstFiniteNumber(...values: unknown[]): number | null {
  for (const value of values) {
    const parsed = finiteNumber(value)
    if (parsed != null) return parsed
  }
  return null
}

function objectValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
}

function positiveNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 ? value : null
}

function windsurfCooldownHasPositiveReset(quota: QuotaStatusSnapshot): boolean {
  const rateLimit = quota.rate_limit
  if (rateLimit && typeof rateLimit === 'object') {
    const retryAfterMs = positiveNumber(rateLimit.retry_after_ms) ?? positiveNumber(rateLimit.retryAfterMs)
    if (retryAfterMs != null) return true
  }

  const rateLimitWindow = getQuotaWindow(quota, 'rate_limit')
  return (
    positiveNumber(rateLimitWindow?.reset_seconds) != null
    || positiveNumber(rateLimitWindow?.reset_at) != null
  )
}

function getQuotaWindowsByScope(
  quota: QuotaStatusSnapshot | null | undefined,
  scope: string,
): QuotaWindowSnapshot[] {
  const normalizedScope = scope.trim().toLowerCase()
  return getQuotaWindows(quota).filter(window => normalizeText(window.scope)?.toLowerCase() === normalizedScope)
}

function formatQuotaValue(value: number | null | undefined): string {
  const normalized = Number(value)
  if (!Number.isFinite(normalized)) return '0'
  const rounded = Math.round(normalized)
  if (Math.abs(normalized - rounded) < 1e-6) {
    return String(rounded)
  }
  return normalized.toFixed(1)
}

function formatBalanceValue(value: unknown, unit: string): string | null {
  const amount = formatDecimalDisplay(value)
  if (amount == null) return null
  const normalizedUnit = unit.trim().toUpperCase()
  return normalizedUnit === 'USD' ? `$${amount}` : `${amount} ${normalizedUnit || '额度'}`
}

/** Provider-neutral quota text. Structured snapshots take precedence over legacy labels. */
export function getGenericQuotaSections(
  quota: QuotaStatusSnapshot | null | undefined,
  fallbackProviderType?: string | null,
): {
  balances: string[]
  windows: string[]
  rateLimits: string[]
  status: string[]
} {
  if (!quota) return { balances: [], windows: [], rateLimits: [], status: [] }

  if (isGenericQuotaUnavailable(quota, fallbackProviderType)) {
    return { balances: [], windows: [], rateLimits: [], status: ['不可用'] }
  }

  const informationalZhipuBalance = isZhipuInformationalBalanceFallback(
    quota,
    fallbackProviderType,
  )
  const ambiguousZhipuQuota = isZhipuAmbiguousQuotaFallback(quota, fallbackProviderType)

  const balances = (Array.isArray(quota.balances) ? quota.balances : []).flatMap((balance) => {
    if (ambiguousZhipuQuota) return []
    const available = formatBalanceValue(balance.available, balance.unit)
    if (!available) return []
    const total = formatBalanceValue(balance.total ?? balance.granted, balance.unit)
    const availableNumber = finiteNumber(balance.available)
    const insufficient = quota.balance_insufficient === true
      || (availableNumber != null && availableNumber <= 0)
    const label = insufficient
      ? informationalZhipuBalance ? '标准余额不足' : '余额不足'
      : informationalZhipuBalance ? '标准余额可用' : '可用'
    return [`${label} ${available}${total ? ` / 总额 ${total}` : ''}`]
  })
  if (quota.unlimited === true && balances.length === 0) balances.push('无限制')
  const windows = getQuotaWindows(quota).flatMap((window) => {
    const label = normalizeText(window.label) || normalizeText(window.code) || '订阅窗口'
    const remaining = getQuotaWindowRemainingPercent(window)
    const value = getQuotaWindowValueText(window)
    if (value) return [`${label} 剩余 ${value}`]
    if (remaining != null) return [`${label} 剩余 ${formatPercent(remaining)}`]
    return []
  })
  const rateLimits = Object.entries(quota.rate_limits ?? {}).flatMap(([key, value]) => {
    if (key === 'kind') return []
    const limit = finiteNumber(value)
    return limit == null ? [] : [`${key.toUpperCase()} ${formatQuotaValue(limit)}`]
  })
  const status: string[] = []
  const appendStatus = (value: string | null) => {
    if (value && !status.includes(value)) status.push(value)
  }
  if (ambiguousZhipuQuota) {
    appendStatus('额度查询失败，额度未知')
    return { balances, windows, rateLimits, status }
  }
  if (quota.freshness === 'stale') appendStatus('数据已过期')
  else if (quota.freshness === 'error') appendStatus('刷新失败')
  else if (quota.freshness === 'unknown') appendStatus('更新时间未知')

  const error = normalizeText(quota.refresh_state?.error)
  const tokenPlanError = normalizeText(quota.token_plan_error) || error
  const tokenPlanStatus = normalizeText(quota.token_plan_status)?.toLowerCase()
  if (getQuotaProviderType(quota) === 'zhipu') {
    if (tokenPlanStatus === 'expired') appendStatus('Coding Plan 已过期')
    else if (tokenPlanStatus === 'not_permitted') appendStatus('无 Coding Plan 权限')
    else if (tokenPlanStatus === 'product_mismatch') appendStatus('Coding Plan 类型不匹配')
    else if (tokenPlanStatus === 'balance_insufficient') appendStatus('余额不足')
    else if (tokenPlanStatus === 'query_failed' || tokenPlanStatus === 'business_error') {
      appendStatus(tokenPlanError?.includes('business code 500')
        ? 'Coding Plan 查询失败（上游 500）'
        : 'Coding Plan 查询失败')
    }
    if (quota.balance_insufficient === true
      || quota.balances?.some(balance => {
        const available = finiteNumber(balance.available)
        return available != null && available <= 0
      })) {
      appendStatus(informationalZhipuBalance
        ? '标准余额不足（不阻断模型调用）'
        : '余额不足')
    }
  }
  if (error?.includes('business code 1113')) {
    appendStatus(informationalZhipuBalance
      ? '标准余额不足（不阻断模型调用）'
      : '余额不足')
  }
  else if (getQuotaProviderType(quota) === 'zhipu' && error?.includes('business code 500')) {
    appendStatus('Coding Plan 查询失败（上游 500）')
  } else if (error && error !== tokenPlanError) appendStatus(error)
  else if (error && !tokenPlanStatus) appendStatus(error)
  return { balances, windows, rateLimits, status }
}

function getQuotaWindowValueText(window: QuotaWindowSnapshot | null | undefined): string | null {
  const limit = finiteNumber(window?.limit_value)
  if (!window || limit == null || limit <= 0) return null
  const remaining = finiteNumber(window.remaining_value)
  if (remaining != null) return `${formatQuotaValue(remaining)}/${formatQuotaValue(limit)}`
  const used = finiteNumber(window.used_value)
  if (used != null) return `${formatQuotaValue(Math.max(limit - used, 0))}/${formatQuotaValue(limit)}`
  return null
}

function getGeminiCliCreditsTextFromQuota(quota: QuotaStatusSnapshot | null | undefined): string | null {
  const credits = quota?.credits
  if (!credits) return null
  const remaining = firstFiniteNumber(credits.remaining, credits.balance)
  if (remaining != null) return `AI Credits 剩余 ${formatQuotaValue(remaining)}`
  if (credits.unlimited === true) return 'AI Credits 不限量'
  if (credits.has_credits === true) return 'AI Credits 可用'
  if (credits.has_credits === false) return 'AI Credits 已用尽'
  return null
}

export function getGeminiCliAccountCreditsText(
  input: ProviderKeyQuotaCarrier,
  fallbackProviderType?: string | null,
): string | null {
  const quota = getQuotaSnapshot(input)
  if (getQuotaProviderType(quota, fallbackProviderType) !== 'gemini_cli') return null

  const quotaText = getGeminiCliCreditsTextFromQuota(quota)
  if (quotaText) return quotaText

  const metadata = input.upstream_metadata?.gemini_cli
  const credits = objectValue(metadata?.credits)
  const paidTier = objectValue(metadata?.paidTier)
  const currentTier = objectValue(metadata?.currentTier)
  const remaining = firstFiniteNumber(
    credits?.remaining,
    credits?.remainingCredits,
    credits?.available,
    credits?.availableCredits,
    credits?.balance,
    paidTier?.availableCredits,
    paidTier?.remainingCredits,
    currentTier?.availableCredits,
    currentTier?.remainingCredits,
  )
  if (remaining != null) return `AI Credits 剩余 ${formatQuotaValue(remaining)}`

  const hasCredits = typeof credits?.has_credits === 'boolean'
    ? credits.has_credits
    : typeof paidTier?.hasCredits === 'boolean'
      ? paidTier.hasCredits
      : typeof currentTier?.hasCredits === 'boolean'
        ? currentTier.hasCredits
        : null
  if (hasCredits === true) return 'AI Credits 可用'
  if (hasCredits === false) return 'AI Credits 已用尽'

  const unlimited = credits?.unlimited === true || paidTier?.unlimited === true || currentTier?.unlimited === true
  return unlimited ? 'AI Credits 不限量' : null
}

const GROK_QUOTA_MODE_LABELS: Record<string, string> = {
  quota_auto: 'Auto',
  auto: 'Auto',
  quota_fast: 'Fast',
  fast: 'Fast',
  quota_expert: 'Expert',
  expert: 'Expert',
  quota_heavy: 'Heavy',
  heavy: 'Heavy',
  quota_grok_4_3: 'Grok 4.3',
  'grok-420-computer-use-sa': 'Grok 4.3',
}

function getGrokQuotaWindowLabel(window: QuotaWindowSnapshot): string {
  const rawCode = normalizeText(window.code)?.replace(/^model:/i, '') || ''
  const rawLabel = normalizeText(window.label) || normalizeText(window.model) || rawCode
  const normalized = (rawLabel || rawCode).trim().toLowerCase()
  return GROK_QUOTA_MODE_LABELS[normalized] || GROK_QUOTA_MODE_LABELS[rawCode.toLowerCase()] || rawLabel || rawCode || '模式'
}

function getCodexQuotaText(quota: QuotaStatusSnapshot): string | null {
  const parts: string[] = []
  for (const window of getQuotaWindows(quota)) {
    const presentation = getCodexQuotaWindowPresentation(window)
    const remainingPercent = getQuotaWindowRemainingPercent(window)
    if (!presentation || remainingPercent == null) continue
    parts.push(`${presentation.label}剩余 ${formatPercent(remainingPercent)}`)
  }
  if (parts.length > 0) return parts.join(' | ')

  if (quota.credits?.has_credits === true && typeof quota.credits.balance === 'number') {
    return `积分 ${quota.credits.balance.toFixed(2)}`
  }
  if (quota.credits?.has_credits === true) return '有积分'
  if (quota.credits?.has_credits === false) return '无可用积分'

  return normalizeText(quota.label)
}

function getKiroQuotaText(quota: QuotaStatusSnapshot): string | null {
  const code = normalizeText(quota.code)?.toLowerCase()
  if (code === 'banned') {
    return normalizeText(quota.label) || '账号已封禁'
  }

  const window = getQuotaWindow(quota, 'usage') ?? getQuotaWindowsByScope(quota, 'account')[0] ?? null
  const remainingPercent = getQuotaWindowRemainingPercent(window)
  if (typeof window?.remaining_value === 'number' && typeof window.limit_value === 'number' && window.limit_value > 0 && window.remaining_value <= 0) {
    return `剩余 ${formatQuotaValue(window.remaining_value)}/${formatQuotaValue(window.limit_value)}`
  }
  if (remainingPercent != null) {
    if (typeof window?.used_value === 'number' && typeof window.limit_value === 'number' && window.limit_value > 0) {
      return `剩余 ${formatPercent(remainingPercent)} (${formatQuotaValue(window.used_value)}/${formatQuotaValue(window.limit_value)})`
    }
    return `剩余 ${formatPercent(remainingPercent)}`
  }

  if (typeof window?.remaining_value === 'number' && typeof window.limit_value === 'number' && window.limit_value > 0) {
    return `剩余 ${formatQuotaValue(window.remaining_value)}/${formatQuotaValue(window.limit_value)}`
  }

  return normalizeText(quota.label)
}

function getGrokQuotaText(quota: QuotaStatusSnapshot): string | null {
  const code = normalizeText(quota.code)?.toLowerCase()
  if (code === 'banned') {
    return normalizeText(quota.label) || '账号已封禁'
  }
  if (code === 'forbidden') {
    return normalizeText(quota.label) || '访问受限'
  }

  const modelWindows = getQuotaWindowsByScope(quota, 'model')
  const modelParts = modelWindows
    .map((window) => {
      const remainingPercent = getQuotaWindowRemainingPercent(window)
      if (remainingPercent == null) return null
      const valueText = getQuotaWindowValueText(window)
      return `${getGrokQuotaWindowLabel(window)}剩余 ${formatPercent(remainingPercent)}${valueText ? ` (${valueText})` : ''}`
    })
    .filter((value): value is string => value != null)

  if (modelParts.length > 0) return modelParts.join(' | ')

  const window = getQuotaWindow(quota, 'usage') ?? getQuotaWindowsByScope(quota, 'account')[0] ?? null
  const remainingPercent = getQuotaWindowRemainingPercent(window)
  if (typeof window?.remaining_value === 'number' && typeof window.limit_value === 'number' && window.limit_value > 0 && window.remaining_value <= 0) {
    return `剩余 ${formatQuotaValue(window.remaining_value)}/${formatQuotaValue(window.limit_value)}`
  }
  if (remainingPercent != null) {
    const valueText = getQuotaWindowValueText(window)
    if (valueText) {
      return `剩余 ${formatPercent(remainingPercent)} (${valueText})`
    }
    return `剩余 ${formatPercent(remainingPercent)}`
  }

  if (typeof window?.remaining_value === 'number' && typeof window.limit_value === 'number' && window.limit_value > 0) {
    return `剩余 ${formatQuotaValue(window.remaining_value)}/${formatQuotaValue(window.limit_value)}`
  }

  return normalizeText(quota.label)
}

function getWindsurfQuotaText(quota: QuotaStatusSnapshot): string | null {
  const code = normalizeText(quota.code)?.toLowerCase()
  if (code === 'banned' || code === 'forbidden' || code === 'quarantined') {
    return normalizeText(quota.label) || '账号不可用'
  }
  if (code === 'cooldown' && windsurfCooldownHasPositiveReset(quota)) {
    return normalizeText(quota.label) || '冷却中'
  }
  if (code === 'rate_limited' || code === 'rate_limit') {
    return normalizeText(quota.label) || '速率受限'
  }
  if (code === 'exhausted') {
    return normalizeText(quota.label) || '额度已耗尽'
  }

  const parts: string[] = []
  const dailyRemaining = getQuotaWindowRemainingPercent(getQuotaWindow(quota, 'daily'))
  const weeklyRemaining = getQuotaWindowRemainingPercent(getQuotaWindow(quota, 'weekly'))
  if (dailyRemaining != null) parts.push(`日剩余 ${formatPercent(dailyRemaining)}`)
  if (weeklyRemaining != null) parts.push(`周剩余 ${formatPercent(weeklyRemaining)}`)

  for (const [label, code] of [
    ['Prompt', 'prompt'],
    ['Flex', 'flex'],
  ] as const) {
    const window = getQuotaWindow(quota, code)
    if (!window) continue
    if (typeof window.remaining_value === 'number' && typeof window.limit_value === 'number' && window.limit_value > 0) {
      parts.push(`${label} 剩余 ${formatQuotaValue(window.remaining_value)}/${formatQuotaValue(window.limit_value)}`)
      continue
    }
    if (typeof window.used_value === 'number' && typeof window.limit_value === 'number' && window.limit_value > 0) {
      parts.push(`${label} 剩余 ${formatQuotaValue(Math.max(window.limit_value - window.used_value, 0))}/${formatQuotaValue(window.limit_value)}`)
      continue
    }
    const remainingPercent = getQuotaWindowRemainingPercent(window)
    if (remainingPercent != null) {
      parts.push(`${label} 剩余 ${formatPercent(remainingPercent)}`)
    }
  }

  if (typeof quota.allowed_models_count === 'number') {
    parts.push(`可用模型 ${quota.allowed_models_count} 个`)
  }

  if (parts.length > 0) return parts.join(' | ')

  if (code === 'cooldown') {
    return normalizeText(quota.label) || '冷却中'
  }

  return normalizeText(quota.label)
}

function getAntigravityQuotaText(quota: QuotaStatusSnapshot): string | null {
  const code = normalizeText(quota.code)?.toLowerCase()
  if (code === 'forbidden') {
    return normalizeText(quota.label) || '访问受限'
  }

  const remainingList = getQuotaWindowsByScope(quota, 'model')
    .map(getQuotaWindowRemainingPercent)
    .filter((value): value is number => value != null)

  if (remainingList.length === 0) return normalizeText(quota.label)

  const minimumRemaining = Math.min(...remainingList)
  if (remainingList.length === 1) {
    return `剩余 ${formatPercent(minimumRemaining)}`
  }
  return `最低剩余 ${formatPercent(minimumRemaining)} (${remainingList.length} 模型)`
}

function getGeminiCliQuotaText(quota: QuotaStatusSnapshot): string | null {
  const creditsText = getGeminiCliCreditsTextFromQuota(quota)
  const modelWindows = getQuotaWindowsByScope(quota, 'model')
  const activeCoolingModels = modelWindows
    .filter((window) => {
      if (window.is_exhausted === true) return true
      if (typeof window.used_ratio === 'number') return window.used_ratio >= 1.0 - 1e-6
      return false
    })
    .filter((window) => {
      if (typeof window.reset_at !== 'number') return true
      return window.reset_at > Math.floor(Date.now() / 1000)
    })
    .map((window) => normalizeText(window.label) || normalizeText(window.model) || '模型')

  if (activeCoolingModels.length === 1) {
    return `${activeCoolingModels[0]} 冷却中`
  }
  if (activeCoolingModels.length > 1) {
    return `${activeCoolingModels.length} 个模型冷却中`
  }

  const remainingList = modelWindows
    .map(getQuotaWindowRemainingPercent)
    .filter((value): value is number => value != null)
  if (remainingList.length === 0) return creditsText || normalizeText(quota.label)

  const minimumRemaining = Math.min(...remainingList)
  if (creditsText) return creditsText
  if (remainingList.length === 1) {
    return `剩余 ${formatPercent(minimumRemaining)}`
  }
  return `最低剩余 ${formatPercent(minimumRemaining)} (${remainingList.length} 模型)`
}

function getChatGPTWebQuotaText(quota: QuotaStatusSnapshot): string | null {
  const window = getQuotaWindow(quota, 'image_gen') ?? getQuotaWindowsByScope(quota, 'account')[0] ?? null
  if (!window) return normalizeText(quota.label)

  const remainingPercent = getQuotaWindowRemainingPercent(window)
  if (typeof window.remaining_value === 'number' && typeof window.limit_value === 'number' && window.limit_value > 0) {
    return `生图剩余 ${formatQuotaValue(window.remaining_value)}/${formatQuotaValue(window.limit_value)}`
  }
  if (remainingPercent != null) {
    return `生图剩余 ${formatPercent(remainingPercent)}`
  }

  if (typeof window.remaining_value === 'number') {
    return `生图剩余 ${formatQuotaValue(window.remaining_value)}`
  }

  return normalizeText(quota.label)
}

export function getLegacyAccountQuotaText(
  input: ProviderKeyQuotaCarrier,
): string | null {
  return normalizeText(input.account_quota)
}

export function getQuotaSnapshotFallbackText(
  input: ProviderKeyQuotaCarrier,
  fallbackProviderType?: string | null,
): string | null {
  const quota = getQuotaSnapshot(input)
  if (!quota) return null

  const providerType = getQuotaProviderType(quota, fallbackProviderType)
  switch (providerType) {
    case 'codex':
      return getCodexQuotaText(quota)
    case 'kiro':
      return getKiroQuotaText(quota)
    case 'grok':
      return getGrokQuotaText(quota)
    case 'windsurf':
      return getWindsurfQuotaText(quota)
    case 'antigravity':
      return getAntigravityQuotaText(quota)
    case 'gemini_cli':
      return getGeminiCliQuotaText(quota)
    case 'chatgpt_web':
      return getChatGPTWebQuotaText(quota)
    default:
      return normalizeText(quota.label)
  }
}

export function getQuotaDisplayText(
  input: ProviderKeyQuotaCarrier,
  fallbackProviderType?: string | null,
): string | null {
  const quota = getQuotaSnapshot(input)
  if (isGenericQuotaUnavailable(quota, fallbackProviderType)) return '不可用'
  const availability = getZhipuModelAvailabilityDisplay(
    quota,
    input.status_snapshot?.model_probe,
    fallbackProviderType,
  )
  if (availability) return availability.text
  if (quota && (quota.kind || (quota.balances?.length ?? 0) > 0)) {
    const sections = getGenericQuotaSections(quota, fallbackProviderType)
    const structured = [...sections.balances, ...sections.windows, ...sections.rateLimits]
    if (structured.length > 0) return structured.join(' | ')
  }
  return getQuotaSnapshotFallbackText(input, fallbackProviderType) || getLegacyAccountQuotaText(input)
}
