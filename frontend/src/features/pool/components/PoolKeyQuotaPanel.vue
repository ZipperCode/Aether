<template>
  <div
    v-if="variant === 'mobile'"
    class="rounded-xl border border-border/50 bg-muted/30 px-3 py-2 text-xs"
  >
    <div class="text-muted-foreground mb-1">
      {{ legacyT('配额') }}
    </div>
    <div
      v-if="balanceSummaries.length || effectiveItems.length"
      class="space-y-2"
    >
      <div v-for="balance in balanceSummaries" :key="balance.unit" class="rounded-md border border-border/50 bg-background/50 px-2 py-1.5" data-testid="pool-quota-balance">
        <div class="flex items-center justify-between gap-2 text-[10px]">
          <span class="text-muted-foreground">{{ legacyT('可用余额') }}</span>
          <span data-testid="pool-quota-available" class="min-w-0 text-right text-sm font-semibold tabular-nums" :class="[balance.isNegative ? 'text-red-600 dark:text-red-400' : 'text-foreground', balance.wrapClass]">{{ balance.available }}</span>
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
    </div>
    <div
      v-else-if="accountQuotaText || fallbackText"
      :class="textClass"
    >
      {{ accountQuotaText || fallbackText }}
    </div>
    <div
      v-else
      class="text-muted-foreground"
    >
      {{ supportsStructuredQuota ? legacyT('暂无额度数据') : '-' }}
    </div>
  </div>

  <template v-else>
    <div
      v-if="balanceSummaries.length || effectiveItems.length"
      class="max-w-[208px] space-y-2"
    >
      <div v-for="balance in balanceSummaries" :key="balance.unit" class="rounded-md border border-border/50 bg-muted/20 px-2 py-1.5" data-testid="pool-quota-balance">
        <div class="flex items-center justify-between gap-2 text-[10px] leading-none">
          <span class="text-muted-foreground">{{ legacyT('余额') }}</span>
          <span data-testid="pool-quota-available" class="min-w-0 text-right text-sm font-semibold tabular-nums" :class="[balance.isNegative ? 'text-red-600 dark:text-red-400' : 'text-foreground', balance.wrapClass]">{{ balance.available }}</span>
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
    </div>
    <span
      v-else-if="accountQuotaText || fallbackText"
      :class="textClass"
    >
      {{ accountQuotaText || fallbackText }}
    </span>
    <span
      v-else
      class="text-xs text-muted-foreground"
    >{{ supportsStructuredQuota ? legacyT('待刷新') : '-' }}</span>
  </template>
</template>

<script setup lang="ts">
import { computed, defineComponent, h, type PropType } from 'vue'
import { useI18n } from '@/i18n'
import type { QuotaStatusSnapshot } from '@/api/endpoints/types'
import { formatDecimalDisplay } from '@/utils/providerKeyQuota'

export interface PoolQuotaProgressDisplayItem {
  label: string
  remainingPercent: number
  resetText: string
  meterText: string
  barClass: string
  meterClass: string
  wrapMeter?: boolean
}

const props = withDefaults(defineProps<{
  items: PoolQuotaProgressDisplayItem[]
  quota?: QuotaStatusSnapshot | null
  providerType?: string | null
  accountQuotaText?: string | null
  fallbackText?: string | null
  textClass?: string
  variant?: 'desktop' | 'mobile'
}>(), {
  accountQuotaText: null,
  quota: null,
  providerType: null,
  fallbackText: null,
  textClass: '',
  variant: 'desktop',
})

const { legacyT } = useI18n()
const supportsStructuredQuota = computed(() => [
  'deepseek', 'openrouter', 'moonshot', 'kimi_coding', 'siliconflow', 'zhipu', 'zai',
].includes(props.providerType?.trim().toLowerCase() || ''))

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
      isNegative: availableNumber != null && availableNumber < 0,
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
  setup(props) {
    return () => props.items.map((item, idx) => h('div', {
      key: `${item.label}-${idx}`,
      class: props.mobile
        ? 'flex flex-col gap-1 min-w-0'
        : 'flex flex-col gap-1 min-w-[140px] max-w-[208px]',
    }, [
      h('div', { class: 'flex items-center justify-between text-[10px] leading-none' }, [
        h('span', { class: 'text-muted-foreground font-medium shrink-0' }, item.label),
        item.resetText
          ? h('span', {
            'data-testid': 'pool-quota-reset-text',
            class: 'text-muted-foreground/80 tabular-nums truncate',
            title: item.resetText,
          }, item.resetText)
          : null,
      ]),
      h('div', { class: 'flex items-center gap-1.5' }, [
        h('div', { class: 'relative flex-1 h-1.5 rounded-full bg-border overflow-hidden' }, [
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
            item.meterClass,
          ],
        }, item.meterText),
      ]),
    ]))
  },
})
</script>
