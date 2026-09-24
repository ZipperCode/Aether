import type {
  ProviderKeyStatusSnapshot,
  QuotaBalanceSnapshot,
  QuotaSourceSnapshot,
  QuotaStatusSnapshot,
  QuotaWindowSnapshot,
} from '@/api/endpoints/types/statusSnapshot'
import type { UpstreamMetadata } from '@/api/endpoints/types/provider'
import { getCodexQuotaWindowPresentation } from '@/utils/codexQuotaWindow'
import { isOfficialQuotaProviderType } from '@/features/providers/utils/providerTypeUtils'

export interface ProviderKeyQuotaCarrier {
  account_quota?: string | null
  status_snapshot?: ProviderKeyStatusSnapshot | null
  upstream_metadata?: UpstreamMetadata | null
}

function normalizeText(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const text = value.trim()
  return text || null
}

function formatPercent(value: number): string {
  return `${Math.max(value, 0).toFixed(1)}%`
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

/** 查询权限与模型调用权限不同；此标签只描述查询结果，不推断 Key 失效。 */
export function getQuotaQueryStatusLabel(
  quota: QuotaStatusSnapshot | null | undefined,
  fallbackProviderType?: string | null,
): string | null {
  const providerType = getQuotaProviderType(quota, fallbackProviderType)
  if (!isOfficialQuotaProviderType(providerType)) return null
  const sources = quota?.sources ?? []
  if (sources.length > 0) {
    // 不适用和不支持是已完成的能力结论，不应让成功的套餐显示为部分失败。
    const applicable = sources.filter(source => source.query_status !== 'not_applicable' && source.query_status !== 'unsupported')
    const successful = applicable.filter(source => source.query_status === 'ok')
    if (applicable.length > 0 && successful.length === applicable.length) return null
    if (successful.length > 0) return '额度部分更新'
    const unresolved = applicable.length ? applicable : sources
    const statuses = new Set(unresolved.map(source => source.query_status))
    if (statuses.size === 1) return quotaQueryStatusText(unresolved[0].query_status)
    if (applicable.length === 0) return '暂无可查询额度'
    return '额度查询未完成'
  }
  if (quota?.refresh_state?.error) return '额度查询失败'
  if (!quota || quota.code === 'unknown') return '额度未查询'
  return null
}

export interface QuotaStatusBadgeInfo {
  label: string
  tone: 'destructive' | 'warning' | 'neutral'
  variant: 'destructive' | 'outline'
  class: string
}

/** 根据额度查询与调度状态计算徽章展示语义，避免将能力结论（如不支持/未查询）渲染为销毁性红色。 */
export function getQuotaStatusBadgeInfo(label?: string | null): QuotaStatusBadgeInfo | null {
  const normalized = normalizeText(label)
  if (!normalized) return null

  // 真实调度阻断或查询失败 -> 红色 destructive
  if (normalized === '额度耗尽·需人工恢复' || normalized === '额度查询失败' || normalized === '查询失败') {
    return {
      label: normalized,
      tone: 'destructive',
      variant: 'destructive',
      class: '',
    }
  }

  // 警告类（疑似不足、部分更新、无查询权限） -> 琥珀色 outline
  if (
    normalized === '疑似额度不足'
    || normalized === '额度部分更新'
    || normalized === '无查询权限'
  ) {
    return {
      label: normalized,
      tone: 'warning',
      variant: 'outline',
      class: 'border-amber-500/40 text-amber-600 dark:text-amber-400 bg-amber-500/5',
    }
  }

  // 流程或能力结论（不支持查询、不适用、未查询、暂无可查询额度、额度查询未完成等） -> 中性 outline，绝不渲染成红色
  return {
    label: normalized,
    tone: 'neutral',
    variant: 'outline',
    class: 'border-border/60 text-muted-foreground bg-muted/20',
  }
}
/** 生成额度徽章的悬浮解释文案，明确查询结论与实际模型调用的独立性。 */
export function getGenericQuotaStatusTitle(params: {
  quota?: QuotaStatusSnapshot | null
  scheduling?: { code?: string | null; blocked?: boolean | null; reason?: string | null; last_observed_at?: number | null } | null
  providerType?: string | null
  locale?: string
}): string {
  const { quota, scheduling, providerType, locale } = params
  if (scheduling?.code === 'quota_exhausted' && scheduling.blocked === true) {
    const parts = ['额度耗尽·需人工恢复']
    if (scheduling.reason) parts.push(scheduling.reason)
    if (typeof scheduling.last_observed_at === 'number') {
      parts.push(new Date(scheduling.last_observed_at * 1000).toLocaleString(locale))
    }
    return parts.join(' · ')
  }
  if (scheduling?.code === 'quota_suspected') {
    const parts = ['疑似额度不足']
    if (scheduling.reason) parts.push(scheduling.reason)
    if (typeof scheduling.last_observed_at === 'number') {
      parts.push(new Date(scheduling.last_observed_at * 1000).toLocaleString(locale))
    }
    return parts.join(' · ')
  }

  const label = getQuotaQueryStatusLabel(quota, providerType)
  if (!label) return ''

  if (label === '不支持查询') {
    return '官方接口不支持该额度查询，不影响实际模型调用'
  }
  if (label === '不适用') {
    return '当前 Key 绑定的套餐不适用该额度查询，不影响实际模型调用'
  }
  if (label === '无查询权限') {
    return '无该额度接口的查询权限，不影响实际模型调用'
  }
  if (label === '未查询' || label === '额度未查询') {
    return '尚未执行额度查询，可手动刷新获取最新额度'
  }
  if (label === '暂无可查询额度') {
    return '当前端点下暂无可查询的官方额度接口'
  }
  if (label === '额度查询失败' || label === '查询失败') {
    const error = quota?.refresh_state?.error || quota?.token_plan_error
    return error ? `额度查询失败：${error}` : '额度查询失败，请检查网络或官方端点设置'
  }
  return label
}

function getQuotaWindows(
  quota: QuotaStatusSnapshot | null | undefined,
): QuotaWindowSnapshot[] {
  return Array.isArray(quota?.windows) ? quota.windows : []
}

export function getQuotaWindowRemainingPercent(
  window: QuotaWindowSnapshot | null | undefined,
): number | null {
  if (!window || window.is_included === false || window.unlimited === true) return null
  const remainingRatio = finiteNumber(window.remaining_ratio)
  if (remainingRatio != null) return Math.max(remainingRatio * 100, 0)
  const usedRatio = finiteNumber(window.used_ratio)
  if (usedRatio != null) return Math.max((1 - usedRatio) * 100, 0)
  const limit = finiteNumber(window.limit_value)
  if (limit == null || limit <= 0) return null
  const remaining = finiteNumber(window.remaining_value)
  if (remaining != null) return Math.max(remaining / limit * 100, 0)
  const used = finiteNumber(window.used_value)
  return used == null ? null : Math.max((1 - used / limit) * 100, 0)
}

function getQuotaWindow(
  quota: QuotaStatusSnapshot | null | undefined,
  code: string,
): QuotaWindowSnapshot | null {
  const normalizedCode = code.trim().toLowerCase()
  return getQuotaWindows(quota).find(window => normalizeText(window.code)?.toLowerCase() === normalizedCode) ?? null
}

/** 仅为比例和阈值计算解析数字；精确金额文字不能经过 Number 转换。 */
export function finiteNumber(value: unknown): number | null {
  if (typeof value === 'number') return Number.isFinite(value) ? value : null
  if (typeof value === 'string') {
    const text = value.trim()
    if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(text)) return null
    const parsed = Number(text)
    return Number.isFinite(parsed) ? parsed : null
  }
  return null
}

export function formatDecimalDisplay(value: unknown): string | null {
  if (typeof value === 'number') {
    return Number.isFinite(value)
      ? value.toLocaleString(undefined, { maximumFractionDigits: 20 })
      : null
  }
  if (typeof value !== 'string') return null
  const text = value.trim()
  const match = /^([+-]?)(\d*)(?:\.(\d*))?$/.exec(text)
  if (!match) return null
  const [, sign, integer, fraction] = match
  if (!integer && !fraction) return null
  const grouped = (integer || '0').replace(/\B(?=(\d{3})+(?!\d))/g, ',')
  return `${sign}${grouped}${fraction ? `.${fraction}` : ''}`
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
  return formatDecimalDisplay(value) ?? '未知'
}

function formatBalanceValue(value: unknown, unit: string): string | null {
  const amount = formatDecimalDisplay(value)
  if (amount == null) return null
  const normalizedUnit = unit.trim().toUpperCase()
  return normalizedUnit === 'USD' ? `$${amount}` : `${amount} ${normalizedUnit || '币种未注明'}`
}

export interface GenericQuotaBalanceDisplay {
  key: string
  label: string
  available: string
  parts: string[]
  remainingPercent: number | null
  insufficient: boolean
  summary: string
}

export interface GenericQuotaWindowDisplay {
  key: string
  label: string
  remainingPercent: number | null
  meterText: string
  detail: string | null
  resetText: string | null
  summary: string
}

export interface GenericQuotaSourceDisplay {
  id: string
  label: string
  metadata: string[]
  status: string | null
  error: string | null
  updatedText: string | null
  stale: boolean
  notes: string[]
  balances: GenericQuotaBalanceDisplay[]
  windows: GenericQuotaWindowDisplay[]
}

function quotaQueryStatusText(status: QuotaSourceSnapshot['query_status']): string {
  const labels: Record<QuotaSourceSnapshot['query_status'], string> = {
    not_queried: '未查询', ok: '查询成功', unsupported: '不支持查询',
    permission_denied: '无查询权限', not_applicable: '不适用', error: '查询失败',
  }
  return labels[status] || '查询状态未知'
}

/** 只展示已知计量单位；未知额度既不冒充 Token，也不换算成货币。 */
export function getQuotaUnitLabel(unit?: string | null): string {
  const value = normalizeText(unit)?.toLowerCase()
  const labels: Record<string, string> = {
    percent: '%', count: '次', requests: '次', tokens: 'Token', token: 'Token',
    credits: '积分', credit: '积分', points: '积分', point: '积分',
    usd: 'USD', cny: 'CNY', rmb: 'CNY',
    quota: '额度单位', opaque: '额度单位', units: '额度单位',
  }
  return value ? labels[value] || (unit || '').trim() : '单位未知'
}

export function getGenericQuotaTypeLabel(quota?: QuotaStatusSnapshot | null): string | null {
  const products = new Set((quota?.sources ?? [])
    .filter(source => source.query_status !== 'not_applicable' && source.query_status !== 'unsupported')
    .map(source => source.product))
  if (products.size > 1) return '多项额度'
  if (products.has('key_spending_limit')) return 'Key 消费限额'
  if (products.has('coding_plan')) return 'Coding Plan'
  if (products.has('token_plan')) return 'Token Plan'
  if (products.has('extra_usage')) return '额外用量'
  if ((quota?.windows?.length ?? 0) > 0) return '套餐额度'
  return quota?.balances?.[0]?.unit?.trim().toUpperCase() || null
}

export function formatQuotaDateTime(value: unknown): string | null {
  if (value == null || value === '') return null
  const date = typeof value === 'number' ? new Date(value * 1000) : new Date(String(value))
  return Number.isNaN(date.getTime()) ? null : date.toLocaleString()
}

function getBalanceDisplay(
  balance: QuotaBalanceSnapshot,
  index: number,
  spendingLimit: boolean,
  unlimited: boolean,
  informational: boolean,
): GenericQuotaBalanceDisplay {
  const available = formatBalanceValue(balance.available, balance.unit)
  const availableNumber = finiteNumber(balance.available)
  const totalNumber = finiteNumber(balance.total)
  const label = spendingLimit ? 'Key 剩余消费额度' : informational ? '标准余额（参考）' : '可用余额'
  const parts = [
    [spendingLimit ? '消费上限' : '总额', balance.total],
    ['赠送', balance.granted], ['充值', balance.topped_up], ['累计已用', balance.used],
  ].flatMap(([partLabel, value]) => {
    const formatted = formatBalanceValue(value, balance.unit)
    return formatted == null ? [] : [`${partLabel} ${formatted}`]
  })
  const valueText = unlimited && spendingLimit ? '未设置消费上限' : available ?? '未知'
  return {
    key: `${balance.source_id || 'legacy'}-${balance.unit}-${index}`,
    label,
    available: valueText,
    parts,
    remainingPercent: spendingLimit && !unlimited && availableNumber != null && totalNumber != null && totalNumber > 0
      ? Math.max(availableNumber / totalNumber * 100, 0) : null,
    insufficient: !unlimited && availableNumber != null && availableNumber <= 0,
    summary: [`${label} ${valueText}`, ...parts].join(' · '),
  }
}

function getWindowDisplay(window: QuotaWindowSnapshot, index: number): GenericQuotaWindowDisplay {
  const isFreeModelDaily = window.code === 'free_model_daily_requests'
  const defaultLabel = isFreeModelDaily ? '免费模型每日请求' : '套餐额度'
  const label = normalizeText(window.label) || (isFreeModelDaily ? '免费模型每日请求' : normalizeText(window.code)) || defaultLabel
  const percent = getQuotaWindowRemainingPercent(window)
  const remaining = formatDecimalDisplay(window.remaining_value)
  const used = formatDecimalDisplay(window.used_value)
  const limit = formatDecimalDisplay(window.limit_value)
  const unit = getQuotaUnitLabel(window.unit)
  const valueDetail = window.unit === 'percent' ? null : remaining != null
    ? `${remaining}${limit != null ? ` / ${limit}` : ''} ${unit}`
    : used != null ? `已用 ${used}${limit != null ? ` / ${limit}` : ''} ${unit}`
      : limit != null ? `上限 ${limit} ${unit}` : null
  const isCountUnit = window.unit === 'requests' || window.unit === 'count' || isFreeModelDaily
  const meterText = window.is_included === false ? '套餐不包含'
    : window.unlimited === true ? '不限额'
      : isCountUnit && valueDetail && percent != null
        ? `${valueDetail} · 剩余 ${formatPercent(percent)}`
        : percent != null
          ? `剩余 ${formatPercent(percent)}`
          : valueDetail ?? '额度未知'
  const resetAt = formatQuotaDateTime(window.reset_at) || normalizeText(window.reset_at_text)
  const resetSeconds = finiteNumber(window.reset_seconds)
  const resetText = resetAt ? `重置 ${resetAt}`
    : resetSeconds != null ? `${Math.ceil(Math.max(resetSeconds, 0) / 60)} 分钟后重置` : null
  const detail = window.is_included === false || window.unlimited === true ? null
    : isCountUnit
      ? null
      : percent != null ? valueDetail : null
  return {
    key: `${window.source_id || 'legacy'}-${window.code}-${index}`,
    label,
    remainingPercent: percent,
    meterText,
    detail,
    resetText,
    summary: [`${label} ${meterText}`, detail, resetText].filter(Boolean).join(' · '),
  }
}

/** 从同一份快照投影来源分组，不复制额度数据或重算旧套餐金额。 */
export function getGenericQuotaGroups(
  quota: QuotaStatusSnapshot | null | undefined,
  fallbackProviderType?: string | null,
): GenericQuotaSourceDisplay[] {
  if (!quota) return []
  const providerType = getQuotaProviderType(quota, fallbackProviderType)
  const sources = quota.sources ?? []
  const groups: Array<QuotaSourceSnapshot | null> = sources.length ? [...sources] : [null]
  const sourceIds = new Set(sources.map(source => source.id))
  // 旧快照或未知来源字段仍可阅读，但不推断其属于哪个新套餐。
  const hasUnassigned = [...(quota.balances ?? []), ...(quota.windows ?? [])]
    .some(item => !item.source_id || !sourceIds.has(item.source_id))
  if (sources.length && hasUnassigned) groups.push(null)

  return groups.map((source) => {
    const belongs = (item: { source_id?: string | null }) => source
      ? item.source_id === source.id
      : !item.source_id || !sourceIds.has(item.source_id)
    const spendingLimit = source?.product === 'key_spending_limit' || (!source && providerType === 'openrouter')
    const spendingUnlimited = spendingLimit && (quota.key_limit_unlimited ?? quota.unlimited) === true
    const stale = (source?.freshness ?? quota.freshness) === 'stale'
    const refreshState = source?.refresh_state ?? quota.refresh_state
    const error = normalizeText(refreshState?.error)
      || (!source ? normalizeText(quota.token_plan_error) : null)
    const status = source ? quotaQueryStatusText(source.query_status)
      : error ? '查询失败' : quota.code === 'unknown' ? '未查询' : null
    const scopeLabels: Record<string, string> = {
      personal: '个人',
      team: '团队',
      account: '账户',
      key: '当前 Key',
      model: '免费模型',
      workspace: '工作区',
    }
    const regionLabels: Record<string, string> = { cn: '国内', global: '国际', international: '国际' }
    const metadata = source ? [
      source.plan_name, source.plan_tier, source.plan_id ? `套餐 ID ${source.plan_id}` : null,
      source.scope ? scopeLabels[source.scope] || source.scope : null,
      source.region ? regionLabels[source.region] || source.region : null,
    ] : [quota.membership_level || quota.plan_type,
      quota.token_plan_scope ? scopeLabels[quota.token_plan_scope] || quota.token_plan_scope : null]
    const notes: string[] = []
    if (providerType === 'siliconflow' && source?.region === 'cn' && source.query_status === 'unsupported') {
      notes.push('官方国内余额接口已停用，暂无替代接口')
    }
    if (source && (source.product === 'coding_plan' || source.product === 'token_plan') && !normalizeText(source.plan_tier)) {
      metadata.push('套餐档位未知')
    }
    if (source?.currency_source === 'region_mapping') notes.push('币种依据站点区域，接口未返回币种')
    if (source?.currency_source === 'official_client_default') notes.push('币种使用官方客户端默认值 USD，接口未返回币种')
    if (spendingLimit) {
      if (spendingUnlimited) notes.push('未设置消费上限')
      notes.push('账户余额未知')
      if (quota.limit_reset != null) notes.push(`限额重置 ${formatQuotaDateTime(quota.limit_reset) || quota.limit_reset}`)
      if (quota.expires_at != null) notes.push(`到期 ${formatQuotaDateTime(quota.expires_at) || quota.expires_at}`)
      if (quota.include_byok_in_limit != null) notes.push(quota.include_byok_in_limit ? '消费上限包含 BYOK' : '消费上限不包含 BYOK')
      for (const [label, value] of [
        ['日消费', quota.usage_daily], ['周消费', quota.usage_weekly], ['月消费', quota.usage_monthly],
        ['BYOK 累计消费', quota.byok_usage], ['BYOK 日消费', quota.byok_usage_daily],
        ['BYOK 周消费', quota.byok_usage_weekly], ['BYOK 月消费', quota.byok_usage_monthly],
      ]) {
        const amount = formatBalanceValue(value, 'USD')
        if (amount != null) notes.push(`${label} ${amount}`)
      }
    }
    if (!source && quota.parallel_limit != null) metadata.push(`并发 ${formatDecimalDisplay(quota.parallel_limit) || '未知'}`)
    if (!source && !spendingLimit && quota.expires_at != null) metadata.push(`到期 ${formatQuotaDateTime(quota.expires_at) || quota.expires_at}`)
    let defaultLabel = '账户额度'
    if (source?.product === 'account_balance') {
      defaultLabel = '账户余额'
    } else if (source?.product === 'key_spending_limit' || (!source && providerType === 'openrouter')) {
      defaultLabel = 'Key 消费限额'
    } else if (source?.product === 'coding_plan' || source?.product === 'token_plan') {
      defaultLabel = '套餐'
    } else if (source?.product === 'model_requests' || source?.id === 'free_requests') {
      defaultLabel = '免费模型每日请求'
    } else if (sources.length) {
      defaultLabel = '原有额度'
    }
    const label = source?.label || defaultLabel
    return {
      id: source?.id || 'legacy',
      label,
      metadata: [...new Set(metadata.filter((value): value is string => Boolean(value)))],
      status,
      error,
      updatedText: formatQuotaDateTime(refreshState?.last_success_at
        ?? (!source ? quota.observed_at ?? quota.updated_at : null)),
      stale,
      notes,
      balances: (quota.balances ?? []).filter(belongs).map((balance, index) => getBalanceDisplay(
        balance, index, spendingLimit, spendingUnlimited,
        !source && isZhipuInformationalBalanceFallback(quota, fallbackProviderType),
      )),
      windows: getQuotaWindows(quota).filter(belongs).map(getWindowDisplay),
    }
  })
}

/** 列表摘要与卡片使用相同的来源、精确数值和单位。 */
export function getGenericQuotaSections(
  quota: QuotaStatusSnapshot | null | undefined,
  fallbackProviderType?: string | null,
): { balances: string[]; windows: string[]; rateLimits: string[]; status: string[] } {
  const groups = getGenericQuotaGroups(quota, fallbackProviderType)
  const grouped = (quota?.sources?.length ?? 0) > 0
  const prefix = (group: GenericQuotaSourceDisplay) => grouped ? `${group.label}：` : ''
  return {
    balances: groups.flatMap(group => group.balances.map(balance => `${prefix(group)}${balance.summary}`)),
    windows: groups.flatMap(group => group.windows.map(window => `${prefix(group)}${window.summary}`)),
    rateLimits: Object.entries(quota?.rate_limits ?? {}).flatMap(([key, value]) => {
      const formatted = key === 'kind' ? null : formatDecimalDisplay(value)
      return formatted == null ? [] : [`${key.toUpperCase()} ${formatted}`]
    }),
    status: groups.flatMap(group => [
      group.status && group.status !== '查询成功' ? `${prefix(group)}${group.status}` : null,
      ...group.notes.map(note => `${prefix(group)}${note}`),
    ].filter((value): value is string => value != null)),
  }
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

function getXaiQuotaText(quota: QuotaStatusSnapshot): string | null {
  const parts: string[] = []
  const usageText = getKiroQuotaText(quota)
  if (usageText) parts.push(usageText)

  const prepaid = getQuotaWindow(quota, 'prepaid')
  if (typeof prepaid?.remaining_value === 'number') {
    parts.push(`预付剩余 ${formatQuotaValue(prepaid.remaining_value)}`)
  }

  const onDemand = getQuotaWindow(quota, 'on_demand')
  const onDemandRemaining = getQuotaWindowRemainingPercent(onDemand)
  if (onDemandRemaining != null) {
    const valueText = getQuotaWindowValueText(onDemand)
    parts.push(`按需剩余 ${formatPercent(onDemandRemaining)}${valueText ? ` (${valueText})` : ''}`)
  } else if (typeof onDemand?.remaining_value === 'number') {
    parts.push(`按需剩余 ${formatQuotaValue(onDemand.remaining_value)}`)
  }

  if (parts.length > 0) return parts.join(' | ')
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
    case 'xai':
      return getXaiQuotaText(quota)
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
  if (isOfficialQuotaProviderType(getQuotaProviderType(quota, fallbackProviderType))) {
    const sections = getGenericQuotaSections(quota, fallbackProviderType)
    const values = [...sections.balances, ...sections.windows, ...sections.rateLimits, ...sections.status]
      .filter((value): value is string => Boolean(value))
    return values.length ? values.join(' | ') : getQuotaQueryStatusLabel(quota, fallbackProviderType) || '额度未知'
  }
  if (quota && (quota.kind || (quota.balances?.length ?? 0) > 0)) {
    const sections = getGenericQuotaSections(quota, fallbackProviderType)
    const structured = [...sections.balances, ...sections.windows, ...sections.rateLimits]
    if (structured.length > 0) return structured.join(' | ')
  }
  return getQuotaSnapshotFallbackText(input, fallbackProviderType) || getLegacyAccountQuotaText(input)
}
