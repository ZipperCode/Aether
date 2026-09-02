<template>
  <div
    v-if="variant === 'mobile'"
    class="rounded-xl border border-border/50 bg-muted/30 px-3 py-2 text-xs"
  >
    <div class="text-muted-foreground mb-1">
      {{ legacyT('配额') }}
    </div>
    <div
      v-if="modelAvailability"
      class="mb-2 rounded-md border px-2 py-1.5 text-[10px] leading-4"
      :class="modelAvailabilityClass"
      data-testid="pool-model-availability"
    >
      <div class="font-medium">{{ legacyT(modelAvailability.title) }}</div>
      <div v-if="modelAvailability.detail" class="mt-0.5 opacity-80">
        {{ legacyT(modelAvailability.detail) }}
      </div>
    </div>
    <div
      v-if="quotaUnavailable"
      class="rounded-md border border-red-500/30 bg-red-500/5 px-2 py-1.5 text-[10px] font-medium text-red-700 dark:text-red-300"
      data-testid="pool-quota-unavailable"
    >
      {{ legacyT('不可用') }}
    </div>
    <div
      v-else-if="balanceSummaries.length || effectiveItems.length"
      :class="hasNumericOnlyItems ? '' : 'space-y-2'"
    >
      <div v-for="balance in balanceSummaries" :key="balance.unit" class="rounded-md border border-border/50 bg-background/50 px-2 py-1.5" data-testid="pool-quota-balance">
        <div class="flex items-center justify-between gap-2 text-[10px]">
          <span class="text-muted-foreground">{{ legacyT(balance.insufficient ? balance.informational ? '标准余额不足' : '余额不足' : balance.informational ? '标准余额' : '可用余额') }}</span>
          <span data-testid="pool-quota-available" class="min-w-0 text-right text-sm font-semibold tabular-nums" :class="[balance.insufficient ? balance.informational ? 'text-amber-600 dark:text-amber-400' : 'text-red-600 dark:text-red-400' : 'text-foreground', balance.wrapClass]">{{ balance.available }}</span>
        </div>
        <div v-if="balance.remainingPercent != null" class="mt-1 flex items-center gap-1.5">
          <div class="relative h-1.5 flex-1 overflow-hidden rounded-full bg-border">
            <div class="absolute inset-y-0 left-0 rounded-full" :class="balance.barClass" :style="{ width: `${balance.remainingPercent}%` }" />
          </div>
          <span class="text-[10px] font-medium tabular-nums" :class="balance.meterClass">{{ balance.remainingPercent.toFixed(1) }}%</span>
        </div>
        <div v-if="balance.detail" class="mt-0.5 text-[9px] text-muted-foreground">{{ balance.detail }}</div>
      </div>
      <QuotaProgressRows
        v-if="effectiveItems.length"
        :items="effectiveItems"
        mobile
      />
      <div
        v-if="accountQuotaText"
        class="text-[10px] leading-none text-muted-foreground tabular-nums"
      >
        {{ accountQuotaText }}
      </div>
      <ResetCredits />
    </div>
    <div
      v-else-if="!modelAvailability && (accountQuotaText || fallbackText)"
      :class="textClass"
    >
      {{ accountQuotaText || fallbackText }}
    </div>
    <div
      v-else-if="!modelAvailability"
      class="text-muted-foreground"
    >
      {{ supportsStructuredQuota ? legacyT('暂无额度数据') : '-' }}
    </div>
  </div>

  <template v-else>
    <div
      v-if="modelAvailability"
      class="max-w-[208px] rounded-md border px-2 py-1.5 text-[10px] leading-4"
      :class="modelAvailabilityClass"
      data-testid="pool-model-availability"
    >
      <div class="font-medium">{{ legacyT(modelAvailability.title) }}</div>
      <div v-if="modelAvailability.detail" class="mt-0.5 opacity-80">
        {{ legacyT(modelAvailability.detail) }}
      </div>
    </div>
    <span
      v-if="quotaUnavailable"
      class="inline-flex rounded-md border border-red-500/30 bg-red-500/5 px-2 py-1 text-[10px] font-medium text-red-700 dark:text-red-300"
      data-testid="pool-quota-unavailable"
    >{{ legacyT('不可用') }}</span>
    <div
      v-else-if="balanceSummaries.length || effectiveItems.length"
      class="w-full max-w-[208px]"
      :class="[modelAvailability ? 'mt-2' : '', hasNumericOnlyItems ? '' : 'space-y-2']"
    >
      <div v-for="balance in balanceSummaries" :key="balance.unit" class="rounded-md border border-border/50 bg-muted/20 px-2 py-1.5" data-testid="pool-quota-balance">
        <div class="flex items-center justify-between gap-2 text-[10px] leading-none">
          <span class="text-muted-foreground">{{ legacyT(balance.insufficient ? balance.informational ? '标准余额不足' : '余额不足' : balance.informational ? '标准余额' : '余额') }}</span>
          <span data-testid="pool-quota-available" class="min-w-0 text-right text-sm font-semibold tabular-nums" :class="[balance.insufficient ? balance.informational ? 'text-amber-600 dark:text-amber-400' : 'text-red-600 dark:text-red-400' : 'text-foreground', balance.wrapClass]">{{ balance.available }}</span>
        </div>
        <div v-if="balance.remainingPercent != null" class="mt-1.5 flex items-center gap-1.5">
          <div class="relative h-1.5 flex-1 overflow-hidden rounded-full bg-border">
            <div class="absolute inset-y-0 left-0 rounded-full" :class="balance.barClass" :style="{ width: `${balance.remainingPercent}%` }" />
          </div>
          <span class="text-[10px] font-medium tabular-nums" :class="balance.meterClass">{{ balance.remainingPercent.toFixed(1) }}%</span>
        </div>
        <div v-if="balance.detail" class="mt-1 text-[9px] leading-none text-muted-foreground">{{ balance.detail }}</div>
      </div>
      <QuotaProgressRows v-if="effectiveItems.length" :items="effectiveItems" />
      <div
        v-if="accountQuotaText"
        class="text-[10px] leading-none text-muted-foreground tabular-nums"
      >
        {{ accountQuotaText }}
      </div>
      <ResetCredits />
    </div>
    <span
      v-else-if="!modelAvailability && (accountQuotaText || fallbackText)"
      :class="textClass"
    >
      {{ accountQuotaText || fallbackText }}
    </span>
    <span
      v-else-if="!modelAvailability"
      class="text-xs text-muted-foreground"
    >{{ supportsStructuredQuota ? legacyT('待刷新') : '-' }}</span>
  </template>
</template>

<script setup lang="ts">
import { computed, defineComponent, h, type PropType } from 'vue'
import { useI18n } from '@/i18n'
import type { ModelProbeStatusSnapshot, QuotaStatusSnapshot } from '@/api/endpoints/types'
import {
  formatDecimalDisplay,
  getZhipuModelAvailabilityDisplay,
  isGenericQuotaUnavailable,
  isZhipuAmbiguousQuotaFallback,
  isZhipuInformationalBalanceFallback,
} from '@/utils/providerKeyQuota'

/** Pool 额度行的展示模型；百分比控制进度宽度，文字可保留精确数值或范围。 */
export interface PoolQuotaProgressDisplayItem {
  label: string
  remainingPercent: number
  resetText: string
  meterText: string
  barClass: string
  meterClass: string
  wrapMeter?: boolean
  /** 为真时只显示紧凑数字，不绘制进度条；最终 Antigravity 摘要不使用该模式。 */
  numericOnly?: boolean
}

const props = withDefaults(defineProps<{
  items: PoolQuotaProgressDisplayItem[]
  quota?: QuotaStatusSnapshot | null
  modelProbe?: ModelProbeStatusSnapshot | null
  providerType?: string | null
  accountQuotaText?: string | null
  fallbackText?: string | null
  textClass?: string
  variant?: 'desktop' | 'mobile'
  /** Codex 可用重置机会的汇总文字；为空时不渲染重置区。 */
  resetCreditText?: string | null
  /** 最多若干条 Codex 重置机会的到期说明。 */
  resetCreditItems?: string[]
  /** 当前凭据代际与额度状态是否允许发起一次幂等重置。 */
  canConsumeResetCredit?: boolean
  /** 当前 Key 是否正在消耗重置机会，用于禁止重复提交。 */
  consumingResetCredit?: boolean
}>(), {
  accountQuotaText: null,
  quota: null,
  modelProbe: null,
  providerType: null,
  fallbackText: null,
  textClass: '',
  variant: 'desktop',
  resetCreditText: null,
  resetCreditItems: () => [],
  canConsumeResetCredit: false,
  consumingResetCredit: false,
})

/** 将重置动作交给页面级幂等流程处理，展示组件不直接访问接口。 */
const emit = defineEmits<{
  'consume-reset-credit': []
}>()

const { legacyT } = useI18n()
const supportsStructuredQuota = computed(() => [
  'deepseek', 'openrouter', 'moonshot', 'kimi_coding', 'siliconflow', 'zhipu', 'zai',
].includes(props.providerType?.trim().toLowerCase() || ''))
const informationalBalanceFallback = computed(() => (
  isZhipuInformationalBalanceFallback(props.quota, props.providerType)
))
const quotaUnavailable = computed(() => (
  isGenericQuotaUnavailable(props.quota, props.providerType)
))
const ambiguousQuotaFallback = computed(() => (
  isZhipuAmbiguousQuotaFallback(props.quota, props.providerType)
))
const modelAvailability = computed(() => getZhipuModelAvailabilityDisplay(
  props.quota,
  props.modelProbe,
  props.providerType,
))
const modelAvailabilityClass = computed(() => {
  if (modelAvailability.value?.status === 'ok') {
    return 'border-emerald-500/30 bg-emerald-500/5 text-emerald-700 dark:text-emerald-300'
  }
  if (modelAvailability.value?.status === 'failed') {
    return 'border-red-500/30 bg-red-500/5 text-red-700 dark:text-red-300'
  }
  return 'border-amber-500/30 bg-amber-500/5 text-amber-700 dark:text-amber-300'
})

function number(value: unknown): number | null {
  if ((typeof value !== 'number' && typeof value !== 'string') || String(value).trim() === '') return null
  const text = String(value).trim()
  if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(text)) return null
  const parsed = Number(text)
  return Number.isFinite(parsed) ? parsed : null
}

function amount(value: unknown, unit: string): string | null {
  const formatted = formatDecimalDisplay(value)
  if (formatted == null) return null
  return unit.toUpperCase() === 'USD' ? `$${formatted}` : `${formatted} ${unit.toUpperCase()}`
}

function remainingTone(remaining: number): { barClass: string; meterClass: string } {
  if (remaining <= 10) return { barClass: 'bg-red-500', meterClass: 'text-red-600' }
  if (remaining <= 30) return { barClass: 'bg-amber-500', meterClass: 'text-amber-600' }
  return { barClass: 'bg-emerald-500', meterClass: 'text-emerald-600' }
}

const balanceSummaries = computed(() => {
  if (!supportsStructuredQuota.value) return []
  if (quotaUnavailable.value) return []
  if (ambiguousQuotaFallback.value) return []
  return (props.quota?.balances ?? []).flatMap((balance) => {
    const available = amount(balance.available, balance.unit)
    const used = amount(balance.used, balance.unit)
    const total = amount(balance.total, balance.unit)
    const unlimited = props.quota?.unlimited === true
    if (!available && !used && !unlimited) return []
    const availableNumber = number(balance.available)
    const totalNumber = number(balance.total)
    const supportsBalanceProgress = props.providerType?.trim().toLowerCase() === 'openrouter'
    const remainingPercent = unlimited
      ? null
      : supportsBalanceProgress && availableNumber != null && totalNumber != null && totalNumber > 0
        ? Math.min(100, Math.max(0, availableNumber / totalNumber * 100))
        : null
    const tone = remainingTone(remainingPercent ?? 100)
    return [{
      unit: balance.unit,
      available: available || legacyT('无限制'),
      insufficient: props.quota?.balance_insufficient === true
        || (availableNumber != null && availableNumber <= 0),
      informational: informationalBalanceFallback.value,
      wrapClass: (available || legacyT('无限制')).length <= 24
        ? 'whitespace-nowrap'
        : 'break-all',
      remainingPercent,
      detail: used || total ? `${used || amount(0, balance.unit)} / ${unlimited ? legacyT('无限制') : total || '-'}` : null,
      ...tone,
    }]
  })
})

const effectiveItems = computed<PoolQuotaProgressDisplayItem[]>(() => {
  if (quotaUnavailable.value) return []
  if (props.items.length) return props.items
  if (!supportsStructuredQuota.value) return []
  return (props.quota?.windows ?? []).flatMap((window) => {
    const remainingRatio = number(window.remaining_ratio)
    const usedRatio = number(window.used_ratio)
    const remainingValue = number(window.remaining_value)
    const limitValue = number(window.limit_value)
    const remainingDisplay = formatDecimalDisplay(window.remaining_value)
    const limitDisplay = formatDecimalDisplay(window.limit_value)
    const remainingPercent = remainingRatio != null
      ? remainingRatio * 100
      : usedRatio != null
        ? (1 - usedRatio) * 100
        : remainingValue != null && limitValue != null && limitValue > 0
          ? remainingValue / limitValue * 100
          : null
    if (remainingPercent == null) return []
    const normalized = Math.min(100, Math.max(0, remainingPercent))
    const tone = remainingTone(normalized)
    const detail = remainingDisplay != null && limitDisplay != null
      ? `${remainingDisplay} / ${limitDisplay}`
      : `${normalized.toFixed(1)}%`
    return [{
      label: window.label || window.code || legacyT('配额'),
      remainingPercent: normalized,
      resetText: window.reset_at_text ? `${legacyT('重置')} ${window.reset_at_text}` : '',
      meterText: detail,
      wrapMeter: remainingDisplay != null && limitDisplay != null,
      ...tone,
    }]
  })
})
/** 判断当前有效额度行是否全部使用紧凑数字布局。 */
const hasNumericOnlyItems = computed(() => (
  effectiveItems.value.length > 0 && effectiveItems.value.every(item => item.numericOnly)
))

/** 渲染 Codex 重置机会摘要与单一消费入口。 */
const ResetCredits = defineComponent({
  name: 'PoolQuotaResetCredits',
  /** 返回依赖当前 props 的轻量渲染函数；无重置摘要时不占布局。 */
  setup() {
    return () => props.resetCreditText ? h('div', {
      'data-testid': 'pool-quota-reset-credits',
      class: 'mt-2 border-t border-border/50 pt-1.5 text-[10px] leading-4 text-muted-foreground',
    }, [
      h('div', { class: 'flex flex-wrap items-center gap-x-1' }, [
        props.canConsumeResetCredit
          ? h('button', {
            type: 'button',
            disabled: props.consumingResetCredit,
            class: 'font-medium text-primary hover:underline disabled:pointer-events-none disabled:opacity-60',
            onClick: () => emit('consume-reset-credit'),
          }, props.consumingResetCredit ? legacyT('重置中...') : legacyT('点击以进行重置'))
          : null,
        h('span', props.resetCreditText),
      ]),
      props.resetCreditItems.length
        ? h('div', { class: 'truncate tabular-nums', title: props.resetCreditItems.join(' · ') }, props.resetCreditItems.join(' · '))
        : null,
    ]) : null
  },
})

/** 统一渲染普通进度条与可选的紧凑数字额度行。 */
const QuotaProgressRows = defineComponent({
  name: 'QuotaProgressRows',
  props: {
    items: {
      type: Array as PropType<PoolQuotaProgressDisplayItem[]>,
      required: true,
    },
    mobile: {
      type: Boolean,
      default: false,
    },
  },
  /** 根据行级模式选择网格或进度条布局，并保留精确数值换行能力。 */
  setup(props) {
    return () => h('div', {
      'data-testid': 'pool-quota-rows',
      class: props.items.every(item => item.numericOnly)
        ? 'grid grid-cols-2 gap-x-3 gap-y-1.5 min-w-0'
        : 'space-y-2',
    }, props.items.map((item, idx) => h('div', {
      key: `${item.label}-${idx}`,
      class: item.numericOnly
        ? 'flex min-w-0 items-baseline justify-between gap-2 text-[10px] leading-4'
        : props.mobile
          ? 'flex flex-col gap-1 min-w-0'
          : 'flex flex-col gap-1 min-w-[140px] max-w-[208px]',
    }, [
      h('div', { class: item.numericOnly ? 'contents' : 'flex items-center justify-between text-[10px] leading-none' }, [
        h('span', {
          'data-testid': 'pool-quota-period-label',
          class: item.numericOnly
            ? 'min-w-0 truncate text-muted-foreground'
            : 'text-muted-foreground font-medium shrink-0',
          title: item.numericOnly ? item.label : undefined,
        }, item.label),
        item.resetText && !item.numericOnly
          ? h('span', {
            'data-testid': 'pool-quota-reset-text',
            class: 'text-muted-foreground/80 tabular-nums truncate',
            title: item.resetText,
          }, item.resetText)
          : null,
      ]),
      h('div', { class: item.numericOnly ? 'contents' : 'flex items-center gap-1.5' }, [
        item.numericOnly
          ? null
          : h('div', {
            'data-testid': 'pool-quota-progress-track',
            class: 'relative flex-1 h-1.5 rounded-full bg-border overflow-hidden',
          }, [
            h('div', {
              class: ['absolute left-0 top-0 h-full rounded-full transition-all duration-300', item.barClass],
              style: { width: `${item.remainingPercent}%` },
            }),
          ]),
        h('span', {
          'data-testid': 'pool-quota-meter-text',
          class: [
            'text-[10px] font-medium tabular-nums leading-tight',
            item.wrapMeter ? 'min-w-0 break-all text-right' : 'shrink-0',
            item.numericOnly ? 'text-right' : '',
            item.meterClass,
          ],
        }, item.meterText),
      ]),
    ])))
  },
})
</script>
