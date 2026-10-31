import type {
  ProviderKeyStatusSnapshot,
  QuotaWindowSnapshot,
  UpstreamMetadata,
} from '@/api/endpoints/types'

/** 可参与 Antigravity 排序和家族汇总的原始模型额度条目。 */
export interface AntigravityQuotaSortableItem {
  model: string
  label: string
  remainingPercent: number
  resetSeconds: number | null
  /** 汇总后的百分比文字；单值与范围都已包含百分号。 */
  detail?: string
}

type AntigravityQuotaPeriodMessageKey =
  | 'providers.antigravity.quota.period.weekly'
  | 'providers.antigravity.quota.period.fiveHours'

type AntigravityQuotaTranslator = (key: AntigravityQuotaPeriodMessageKey) => string

const ANTIGRAVITY_MODEL_LABELS: Record<string, string> = {
  'gemini-pro-agent': 'Gemini 3.1 Pro (High)',
  'gemini-3.1-pro-high': 'Gemini 3.1 Pro (High)',
  'gemini-3.1-pro-low': 'Gemini 3.1 Pro (Low)',
  'gemini-3-flash-agent': 'Gemini 3.5 Flash (High)',
  'gemini-3.5-flash-low': 'Gemini 3.5 Flash (Medium)',
  'gemini-3.5-flash-extra-low': 'Gemini 3.5 Flash (Low)',
  'claude-opus-4-6-thinking': 'Claude Opus 4.6 (Thinking)',
  'claude-sonnet-4-6': 'Claude Sonnet 4.6 (Thinking)',
  'claude-sonnet-4-6-thinking': 'Claude Sonnet 4.6 (Thinking)',
  'gemini-3.1-flash-image': 'Gemini 3.1 Flash Image',
  'gemini-3.1-flash-lite': 'Gemini 3.1 Flash Lite',
  'gemini-3-flash': 'Gemini 3 Flash',
  'gemini-2.5-pro': 'Gemini 2.5 Pro',
  'gemini-2.5-flash-thinking': 'Gemini 3.1 Flash Lite',
  'gemini-2.5-flash': 'Gemini 3.1 Flash Lite',
  'gemini-2.5-flash-lite': 'Gemini 3.1 Flash Lite',
  'gpt-oss-120b-medium': 'GPT-OSS 120B (Medium)',
  'tab_flash_lite_preview': 'Tab Flash Lite Preview',
  'tab_jump_flash_lite_preview': 'Tab Jump Flash Lite Preview',
  'models/proactive-observer': 'Proactive Observer',
}

const ANTIGRAVITY_MODEL_PRIORITY: Record<string, number> = {
  'claude-opus-4-6-thinking': 10,
  'claude-sonnet-4-6': 20,
  'claude-sonnet-4-6-thinking': 25,
  'gemini-pro-agent': 30,
  'gemini-3.1-pro-high': 35,
  'gemini-3.1-pro-low': 40,
  'gemini-3-flash-agent': 50,
  'gemini-3.5-flash-low': 60,
  'gemini-3.5-flash-extra-low': 70,
  'gemini-3.1-flash-image': 80,
  'gemini-3.1-flash-lite': 90,
  'gemini-3-flash': 180,
  'gemini-2.5-pro': 300,
  'gemini-2.5-flash-thinking': 310,
  'gemini-2.5-flash': 320,
  'gemini-2.5-flash-lite': 330,
  'gpt-oss-120b-medium': 700,
  'models/proactive-observer': 780,
  'tab_flash_lite_preview': 800,
  'tab_jump_flash_lite_preview': 810,
}

export function isOpaqueAntigravityQuotaIdentifier(value: string): boolean {
  return value.trim().startsWith('RateLimitResetCredit_')
}

export function resolveAntigravityQuotaLabel(
  model: string,
  rawLabel: unknown,
  opaqueDisplayIndex: { value: number },
): string {
  const normalizedModel = model.trim()
  const canonical = ANTIGRAVITY_MODEL_LABELS[normalizedModel]
  if (canonical) return canonical

  const candidate = String(rawLabel || '').trim()
  if (candidate && !isOpaqueAntigravityQuotaIdentifier(candidate)) return candidate
  if (isOpaqueAntigravityQuotaIdentifier(normalizedModel) || (candidate && isOpaqueAntigravityQuotaIdentifier(candidate))) {
    const label = `Key-${opaqueDisplayIndex.value}`
    opaqueDisplayIndex.value += 1
    return label
  }
  return candidate || normalizedModel
}

function getAntigravityModelPriority(model: string): number {
  const normalizedModel = model.trim()
  const explicit = ANTIGRAVITY_MODEL_PRIORITY[normalizedModel]
  if (explicit !== undefined) return explicit
  if (normalizedModel.startsWith('claude-')) return 30
  if (normalizedModel.startsWith('gemini-3.')) return 200
  if (normalizedModel.startsWith('gemini-2.')) return 390
  if (normalizedModel.startsWith('gemini-')) return 490
  if (normalizedModel.startsWith('gpt-oss-')) return 700
  if (normalizedModel.startsWith('models/')) return 780
  if (normalizedModel.startsWith('tab_')) return 800
  if (normalizedModel.startsWith('chat_')) return 900
  if (isOpaqueAntigravityQuotaIdentifier(normalizedModel)) return 950
  return 850
}

/** 按模型重要性、重置时间、剩余额度与稳定名称排序原始条目。 */
export function compareAntigravityQuotaItems<T extends AntigravityQuotaSortableItem>(
  a: T,
  b: T,
): number {
  return (getAntigravityModelPriority(a.model) - getAntigravityModelPriority(b.model))
    || ((a.resetSeconds ?? Number.POSITIVE_INFINITY) - (b.resetSeconds ?? Number.POSITIVE_INFINITY))
    || (a.remainingPercent - b.remainingPercent)
    || a.label.localeCompare(b.label)
    || a.model.localeCompare(b.model)
}

/** 按可见标签去重原始条目，并保留排序更优、重置更近或额度更低的代表窗口。 */
export function dedupeAntigravityQuotaItemsByLabel<T extends AntigravityQuotaSortableItem>(
  items: T[],
): T[] {
  const selectedByLabel = new Map<string, T>()
  for (const item of items) {
    const label = item.label.trim()
    if (!label) continue
    const selected = selectedByLabel.get(label)
    if (!selected || compareAntigravityQuotaItems(item, selected) < 0) {
      selectedByLabel.set(label, item)
    }
  }
  return Array.from(selectedByLabel.values()).sort(compareAntigravityQuotaItems)
}


function canonicalizeAntigravityQuotaGroupLabel(rawLabel: string): string {
  const normalized = rawLabel
    .trim()
    .toLowerCase()
    .replace(/[._-]+/g, ' ')
    .replace(/\s+/g, ' ')

  if (normalized === 'gemini额度' || /^gemini(?: models?)?$/.test(normalized)) {
    return 'Gemini Models'
  }
  if (
    normalized === 'claude & chatgpt'
    || /^claude\s*(?:&|and)\s*(?:gpt|chatgpt)(?: models?)?$/.test(normalized)
  ) {
    return 'Claude and GPT models'
  }
  return rawLabel.trim()
}

function getAntigravityQuotaGroupName(window: QuotaWindowSnapshot): string {
  const explicitGroupLabel = String(window.quota_group_label || '').trim()
  if (explicitGroupLabel) {
    return canonicalizeAntigravityQuotaGroupLabel(explicitGroupLabel)
  }

  const rawLabel = String(window.label || '').trim()
  const groupLabel = rawLabel.split(/\s*·\s*/, 1)[0]
    ?.replace(/\s+(?:weekly limit remaining|weekly|5\s*hours?|5h)$/i, '')
    .trim()
  return canonicalizeAntigravityQuotaGroupLabel(groupLabel || rawLabel)
}

function getAntigravityQuotaPeriodLabel(
  window: QuotaWindowSnapshot,
  translate: AntigravityQuotaTranslator,
): string {
  const periodCandidates = [window.window, window.bucket_id, window.code, window.label]
    .map(value => String(value || '').trim())
    .filter(Boolean)
  const normalized = periodCandidates.join(' ').toLowerCase()

  if (/(?:^|[^a-z])weekly(?:[^a-z]|$)/.test(normalized)) {
    return translate('providers.antigravity.quota.period.weekly')
  }
  if (/(?:^|[^a-z0-9])5\s*(?:h|hours?)(?:[^a-z0-9]|$)/.test(normalized)) {
    return translate('providers.antigravity.quota.period.fiveHours')
  }

  return String(window.window || '').trim()
}

export function resolveAntigravityQuotaGroupLabel(
  window: QuotaWindowSnapshot,
  translate: AntigravityQuotaTranslator,
): string {
  const groupName = getAntigravityQuotaGroupName(window)
  const periodLabel = getAntigravityQuotaPeriodLabel(window, translate)
  if (groupName && periodLabel) return `${groupName} · ${periodLabel}`
  return groupName || String(window.label || window.code || '').trim()
}

export type AntigravityAccountTier = 'free' | 'pro' | 'ultra'

/**
 * 校验并规范化 Antigravity 订阅等级。
 * 仅严格接受 free / pro / ultra，未识别或空值统一返回 null，绝不推断为 free。
 */
export function normalizeAntigravityAccountTier(tier?: string | null): AntigravityAccountTier | null {
  if (typeof tier !== 'string') return null
  const normalized = tier.trim().toLowerCase()
  if (normalized === 'free' || normalized === 'pro' || normalized === 'ultra') {
    return normalized
  }
  return null
}

/**
 * 从上游元数据中解析 Antigravity 账号等级。
 * 若无元数据、未配置或无法识别，显式返回 'unknown'，绝不推断为 'free'。
 */
export function resolveAntigravityAccountTier(
  metadata?: UpstreamMetadata | null,
): AntigravityAccountTier | 'unknown' {
  const tier = normalizeAntigravityAccountTier(metadata?.antigravity?.subscription_tier)
  return tier ?? 'unknown'
}

/**
 * 解析 Antigravity 账号等级徽章显示文案。
 * 已知等级返回 Free / Pro / Ultra；未知显式展示 Unknown / 未知。
 */
export function resolveAntigravityAccountTierLabel(
  metadata?: UpstreamMetadata | null,
  translate?: ((text: string) => string) | null,
): string {
  const tier = resolveAntigravityAccountTier(metadata)
  if (tier === 'free') return 'Free'
  if (tier === 'pro') return 'Pro'
  if (tier === 'ultra') return 'Ultra'
  return translate ? translate('未知') : 'Unknown'
}

export interface AntigravityBindingCarrier {
  provider_type?: string | null
  antigravity_quota_bindings?: Record<string, string> | null
  antigravity_quota_models?: string[] | null
  status_snapshot?: ProviderKeyStatusSnapshot | null
  upstream_metadata?: UpstreamMetadata | null
}

export interface AntigravityQuotaModelOption {
  /** 精确上游模型 ID，提交时保持原值。 */
  model: string
  /** 账号元数据中的原始可读名称（与 model 不同时提供辅助识别）。 */
  displayName: string | null
}

export interface AntigravityQuotaGroupOption {
  /** 服务端生成的稳定配额组标识（id:<ID> 或 buckets:[...]），绝不根据展示名合成。 */
  key: string
  /** 配额组展示名称。 */
  label: string
  /** 组内周期摘要（如“周 · 5小时”）。 */
  periodsText: string | null
  /** 可选配额组说明。 */
  description: string | null
}

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value)
}

function isStableAntigravityGroupKey(value: string): boolean {
  const trimmed = value.trim()
  if (!trimmed) return false
  return !/^group:\d+$/i.test(trimmed)
}

/** 判断当前编辑或展示的 Key 是否属于 Antigravity 提供商。 */
export function isAntigravityProviderKey(
  input?: AntigravityBindingCarrier | null,
  fallbackProviderType?: string | null,
): boolean {
  const candidates = [
    fallbackProviderType,
    input?.provider_type,
    input?.status_snapshot?.quota?.provider_type,
  ]
  for (const candidate of candidates) {
    if (typeof candidate === 'string' && candidate.trim().toLowerCase() === 'antigravity') {
      return true
    }
  }
  if (input?.antigravity_quota_bindings !== undefined && input.antigravity_quota_bindings !== null) {
    return true
  }
  if (input?.antigravity_quota_models !== undefined && input.antigravity_quota_models !== null) {
    return true
  }
  return isPlainRecord(input?.upstream_metadata?.antigravity)
}

/** 读取账号当前已保存的模型配额组绑定映射。 */
export function readAntigravityQuotaBindings(
  input?: AntigravityBindingCarrier | null,
): Record<string, string> {
  const raw = isPlainRecord(input?.antigravity_quota_bindings)
    ? input?.antigravity_quota_bindings
    : isPlainRecord(input?.upstream_metadata?.antigravity_quota_bindings)
      ? input?.upstream_metadata?.antigravity_quota_bindings
      : null
  if (!raw) return {}

  const result: Record<string, string> = {}
  for (const [rawModel, rawGroupKey] of Object.entries(raw)) {
    const model = rawModel.trim()
    const groupKey = typeof rawGroupKey === 'string' ? rawGroupKey.trim() : ''
    if (!model || !groupKey) continue
    result[model] = groupKey
  }
  return result
}

function findAuthoritativeAntigravityModelMap(
  metadata?: UpstreamMetadata | null,
): Record<string, unknown> | null {
  const antigravity = metadata?.antigravity
  if (!isPlainRecord(antigravity)) return null
  for (const field of ['quota_by_model', 'models', 'model_quotas'] as const) {
    const candidate = antigravity[field]
    if (isPlainRecord(candidate)) {
      return candidate
    }
  }
  return null
}

function collectRawModelDisplayNames(
  input?: AntigravityBindingCarrier | null,
): Map<string, string> {
  const byLowerModel = new Map<string, string>()
  const authoritativeMap = findAuthoritativeAntigravityModelMap(input?.upstream_metadata)
  if (authoritativeMap) {
    for (const [rawModel, rawInfo] of Object.entries(authoritativeMap)) {
      const model = rawModel.trim()
      if (!model || !isPlainRecord(rawInfo)) continue
      const displayName = typeof rawInfo.display_name === 'string'
        ? rawInfo.display_name.trim()
        : ''
      if (displayName && displayName !== model && !isOpaqueAntigravityQuotaIdentifier(displayName)) {
        byLowerModel.set(model.toLowerCase(), displayName)
      }
    }
  }

  const windows = input?.status_snapshot?.quota?.windows
  if (Array.isArray(windows)) {
    for (const window of windows) {
      if (String(window?.scope || '').trim().toLowerCase() !== 'model') continue
      const model = String(window?.model || '').trim()
      const label = String(window?.label || '').trim()
      if (!model || !label || label === model || isOpaqueAntigravityQuotaIdentifier(label)) continue
      const lower = model.toLowerCase()
      if (!byLowerModel.has(lower)) {
        byLowerModel.set(lower, label)
      }
    }
  }

  return byLowerModel
}

/**
 * 提取当前账号可绑定的精确上游模型候选列表，绝不使用内置硬编码模型表。
 * 优先级：1. antigravity_quota_models 安全字段 -> 2. upstream_metadata 权威模型映射 -> 3. quota.windows 模型窗口。
 */
export function resolveAntigravityBindableModels(
  input?: AntigravityBindingCarrier | null,
): AntigravityQuotaModelOption[] {
  if (!input) return []

  const displayNames = collectRawModelDisplayNames(input)
  const seenLower = new Set<string>()
  const models: string[] = []
  const pushUniqueModel = (rawModel: unknown) => {
    if (typeof rawModel !== 'string') return
    const model = rawModel.trim()
    if (!model) return
    const lower = model.toLowerCase()
    if (seenLower.has(lower)) return
    seenLower.add(lower)
    models.push(model)
  }

  if (Array.isArray(input.antigravity_quota_models)) {
    for (const model of input.antigravity_quota_models) {
      pushUniqueModel(model)
    }
  } else {
    const authoritativeMap = findAuthoritativeAntigravityModelMap(input.upstream_metadata)
    if (authoritativeMap !== null) {
      for (const [rawModel, rawInfo] of Object.entries(authoritativeMap)) {
        if (isPlainRecord(rawInfo) && rawInfo.disabled === true) continue
        pushUniqueModel(rawModel)
      }
    } else {
      const windows = input.status_snapshot?.quota?.windows
      if (Array.isArray(windows)) {
        for (const window of windows) {
          if (String(window?.scope || '').trim().toLowerCase() !== 'model') continue
          if (window?.is_included === false) continue
          pushUniqueModel(window?.model)
        }
      }
    }
  }

  const options = models.map((model): AntigravityQuotaModelOption => ({
    model,
    displayName: displayNames.get(model.toLowerCase()) ?? null,
  }))

  options.sort((a, b) => (
    (getAntigravityModelPriority(a.model) - getAntigravityModelPriority(b.model))
    || a.model.localeCompare(b.model)
  ))
  return options
}

/**
 * 从账号配额快照中提取已保存的配额组候选（按服务端稳定 quota_group 键去重）。
 */
export function resolveAntigravityQuotaGroupOptions(
  input: AntigravityBindingCarrier | null | undefined,
  translate: AntigravityQuotaTranslator,
): AntigravityQuotaGroupOption[] {
  if (!input) return []

  const byKey = new Map<string, {
    key: string
    label: string
    periods: string[]
    description: string | null
  }>()

  const windows = input.status_snapshot?.quota?.windows
  if (Array.isArray(windows)) {
    for (const window of windows) {
      if (String(window?.scope || '').trim().toLowerCase() !== 'quota_group') continue
      const rawGroupKey = typeof window?.quota_group === 'string' ? window.quota_group.trim() : ''
      if (!isStableAntigravityGroupKey(rawGroupKey)) continue

      const groupLabel = getAntigravityQuotaGroupName(window) || rawGroupKey
      const periodLabel = getAntigravityQuotaPeriodLabel(window, translate)
      const description = typeof window?.description === 'string' && window.description.trim()
        ? window.description.trim()
        : null

      const existing = byKey.get(rawGroupKey)
      if (existing) {
        if (periodLabel && !existing.periods.includes(periodLabel)) {
          existing.periods.push(periodLabel)
        }
        if (!existing.description && description) {
          existing.description = description
        }
      } else {
        byKey.set(rawGroupKey, {
          key: rawGroupKey,
          label: groupLabel,
          periods: periodLabel ? [periodLabel] : [],
          description,
        })
      }
    }
  }

  const rawGroups = input.upstream_metadata?.antigravity?.quota_groups
  if (Array.isArray(rawGroups)) {
    for (const group of rawGroups) {
      if (!isPlainRecord(group)) continue
      const rawGroupKey = typeof group.quota_group === 'string' ? group.quota_group.trim() : ''
      if (!isStableAntigravityGroupKey(rawGroupKey) || byKey.has(rawGroupKey)) continue
      const rawLabel = typeof group.display_name === 'string' ? group.display_name.trim() : ''
      const description = typeof group.description === 'string' && group.description.trim()
        ? group.description.trim()
        : null
      byKey.set(rawGroupKey, {
        key: rawGroupKey,
        label: rawLabel ? canonicalizeAntigravityQuotaGroupLabel(rawLabel) : rawGroupKey,
        periods: [],
        description,
      })
    }
  }

  return Array.from(byKey.values()).map(item => ({
    key: item.key,
    label: item.label,
    periodsText: item.periods.length > 0 ? item.periods.join(' · ') : null,
    description: item.description,
  }))
}
