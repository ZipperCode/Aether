<template>
  <div
    v-if="hasContent"
    class="mt-2 rounded-md bg-muted/30 p-2"
    data-testid="provider-generic-quota"
  >
      <ProviderQuotaSectionHeader
        :title="legacyT('账号配额')"
        :loading="loading"
        :updated-text="updatedText"
        :refreshable="refreshable"
        :refresh-disabled="refreshDisabled"
        :refresh-title="legacyT('刷新额度')"
        @refresh="$emit('refresh')"
      />
      <div
        v-if="modelAvailability"
        class="mt-1.5 rounded border px-2 py-1.5 text-[10px]"
        :class="modelAvailability.status === 'ok'
          ? 'border-emerald-500/30 bg-emerald-500/5 text-emerald-700 dark:text-emerald-300'
          : modelAvailability.status === 'failed'
            ? 'border-red-500/30 bg-red-500/5 text-red-700 dark:text-red-300'
            : 'border-amber-500/30 bg-amber-500/5 text-amber-700 dark:text-amber-300'"
        data-testid="provider-model-availability"
      >
        <div class="font-medium">{{ legacyT(modelAvailability.title) }}</div>
        <div v-if="modelAvailability.detail" class="mt-0.5 opacity-80">
          {{ legacyT(modelAvailability.detail) }}
        </div>
      </div>
      <div
        v-if="balanceItems.length"
        class="mt-1.5 space-y-1.5"
        data-testid="provider-balance-panel"
      >
        <div
          v-for="balance in balanceItems"
          :key="`${balance.unit}-${balance.available}`"
          class="rounded border border-border/50 bg-background/45 px-2 py-1.5"
        >
          <template v-if="balance.showProgress">
            <ProviderQuotaProgressRow
              :label="balance.insufficient ? legacyT(balance.informational ? '标准余额不足' : '余额不足') : balance.unlimited ? legacyT('累计已用') : legacyT(balance.informational ? '标准余额' : '剩余余额')"
              :remaining-percent="balance.remainingPercent"
              :meter-class="balance.meterClass"
              :bar-class="balance.barClass"
            >
              <template #footer>
                <div class="mt-0.5 flex items-center justify-between gap-2 text-[9px] text-muted-foreground/70">
                  <span>{{ balance.usageDetail }}</span>
                  <span v-if="balance.available">{{ legacyT('剩余') }} {{ balance.available }}</span>
                </div>
              </template>
            </ProviderQuotaProgressRow>
          </template>
          <div v-else class="flex items-center justify-between gap-3">
            <div class="flex min-w-0 items-center gap-1.5">
              <WalletCards class="h-3.5 w-3.5 shrink-0" :class="balance.insufficient ? balance.informational ? 'text-amber-500' : 'text-red-500' : 'text-emerald-500'" />
              <div class="min-w-0">
                <div class="text-[9px] text-muted-foreground">{{ legacyT(balance.insufficient ? balance.informational ? '标准余额不足' : '余额不足' : balance.informational ? '标准余额' : '可用余额') }}</div>
                <div
                  class="break-all text-sm font-semibold leading-4"
                  :class="balance.insufficient ? balance.informational ? 'text-amber-600 dark:text-amber-400' : 'text-red-600 dark:text-red-400' : 'text-foreground'"
                  data-testid="provider-quota-available"
                >
                  {{ balance.available || legacyT('无限制') }}
                </div>
              </div>
            </div>
            <div v-if="balance.parts.length" class="flex shrink-0 flex-wrap justify-end gap-x-2 gap-y-0.5 text-[9px] text-muted-foreground">
              <span v-for="part in balance.parts" :key="part.label" class="inline-flex items-center gap-0.5">
                <Gift v-if="part.kind === 'granted'" class="h-2.5 w-2.5" />
                <CreditCard v-else class="h-2.5 w-2.5" />
                {{ part.label }} {{ part.value }}
              </span>
            </div>
          </div>
        </div>
      </div>

      <div v-if="quotaWindows.length" class="mt-1.5 space-y-2" data-testid="provider-subscription-panel">
        <ProviderQuotaProgressRow
          v-for="window in quotaWindows"
          :key="window.code"
          :label="window.label"
          :remaining-percent="window.remainingPercent"
          :meter-class="window.meterClass"
          :bar-class="window.barClass"
          :reset-text="window.footer"
        />
      </div>

      <div
        v-if="metadataItems.length"
        class="mt-1.5 flex flex-wrap gap-x-3 gap-y-1 text-[9px] text-muted-foreground"
      >
        <span v-for="item in metadataItems" :key="item.label">{{ item.label }} {{ item.value }}</span>
      </div>

      <div
        v-if="sections.rateLimits.length"
        class="mt-2 flex flex-wrap gap-1"
      >
        <Badge
          v-for="item in sections.rateLimits"
          :key="item"
          variant="secondary"
          class="h-4 px-1.5 py-0 text-[9px] font-normal"
        >
          {{ item }}
        </Badge>
      </div>

      <div
        v-if="sections.status.length && !quotaUnavailable"
        class="mt-1.5 text-[10px]"
        :class="informationalBalanceFallback ? 'text-amber-600 dark:text-amber-400' : 'text-red-600 dark:text-red-400'"
        data-testid="provider-generic-quota-status"
      >
        {{ sections.status.join(' · ') }}
      </div>
      <div
        v-else-if="!hasStructuredContent"
        class="mt-1.5 rounded border border-dashed border-border/60 px-2 py-2 text-center text-[10px] text-muted-foreground"
        data-testid="provider-quota-empty"
      >
        {{ loading ? legacyT('正在查询额度') : legacyT('暂无额度数据，点击刷新后重试') }}
      </div>
      </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { CreditCard, Gift, WalletCards } from 'lucide-vue-next'
import type {
  ModelProbeStatusSnapshot,
  QuotaStatusSnapshot,
  QuotaWindowSnapshot,
} from '@/api/endpoints/types'
import Badge from '@/components/ui/badge.vue'
import { useI18n } from '@/i18n'
import {
  formatDecimalDisplay,
  getGenericQuotaSections,
  getZhipuModelAvailabilityDisplay,
  isGenericQuotaUnavailable,
  isZhipuAmbiguousQuotaFallback,
  isZhipuInformationalBalanceFallback,
} from '@/utils/providerKeyQuota'
import ProviderQuotaProgressRow from './ProviderQuotaProgressRow.vue'
import ProviderQuotaSectionHeader from './ProviderQuotaSectionHeader.vue'

const props = withDefaults(defineProps<{
  quota?: QuotaStatusSnapshot | null
  modelProbe?: ModelProbeStatusSnapshot | null
  loading?: boolean
  providerType?: string | null
  refreshable?: boolean
  refreshDisabled?: boolean
}>(), {
  quota: null,
  modelProbe: null,
  loading: false,
  providerType: null,
  refreshable: false,
  refreshDisabled: false,
})

defineEmits<{
  (e: 'refresh'): void
}>()

const { legacyT } = useI18n()
const sections = computed(() => getGenericQuotaSections(props.quota, props.providerType))
const normalizedProviderType = computed(() => props.providerType?.trim().toLowerCase() || '')
const quotaUnavailable = computed(() => (
  isGenericQuotaUnavailable(props.quota, props.providerType)
))
const informationalBalanceFallback = computed(() => (
  isZhipuInformationalBalanceFallback(props.quota, props.providerType)
))
const ambiguousQuotaFallback = computed(() => (
  isZhipuAmbiguousQuotaFallback(props.quota, props.providerType)
))
const modelAvailability = computed(() => getZhipuModelAvailabilityDisplay(
  props.quota,
  props.modelProbe,
  props.providerType,
))

function number(value: unknown): number | null {
  if ((typeof value !== 'number' && typeof value !== 'string') || String(value).trim() === '') return null
  const text = String(value).trim()
  if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(text)) return null
  const parsed = Number(text)
  return Number.isFinite(parsed) ? parsed : null
}

function amount(value: unknown, unit: string): string | null {
  const text = formatDecimalDisplay(value)
  if (text == null) return null
  return unit.toUpperCase() === 'USD' ? `$${text}` : `${text} ${unit.toUpperCase()}`
}

const balanceItems = computed(() => (props.quota?.balances ?? []).flatMap((balance) => {
  if (quotaUnavailable.value) return []
  if (ambiguousQuotaFallback.value) return []
  const available = amount(balance.available, balance.unit)
  const total = amount(balance.total, balance.unit)
  const used = amount(balance.used, balance.unit)
  const totalNumber = number(balance.total)
  const availableNumber = number(balance.available)
  const insufficient = props.quota?.balance_insufficient === true
    || (availableNumber != null && availableNumber <= 0)
  const unlimited = props.quota?.unlimited === true
  if (!available && !used && !unlimited) return []
  const parts = [
    { label: legacyT('赠送'), value: amount(balance.granted, balance.unit), kind: 'granted' },
    { label: legacyT('充值'), value: amount(balance.topped_up, balance.unit), kind: 'topped_up' },
    { label: legacyT('累计已用'), value: unlimited ? used : null, kind: 'used' },
  ].filter((part): part is { label: string; value: string; kind: string } => part.value != null)
  const supportsBalanceProgress = normalizedProviderType.value === 'openrouter'
  const remainingPercent = unlimited
    ? 100
    : supportsBalanceProgress && totalNumber != null && totalNumber > 0 && availableNumber != null
      ? Math.min(100, Math.max(0, availableNumber / totalNumber * 100))
      : null
  return [{
    unit: balance.unit,
    available,
    parts,
    unlimited,
    insufficient,
    informational: informationalBalanceFallback.value,
    showProgress: !unlimited && remainingPercent != null,
    remainingPercent,
    usageDetail: `${used || amount(0, balance.unit)} / ${unlimited ? legacyT('无限制') : total || '-'}`,
    meterClass: !unlimited && (remainingPercent ?? 100) <= 10 ? 'text-red-600' : !unlimited && (remainingPercent ?? 100) <= 30 ? 'text-amber-600' : 'text-emerald-600',
    barClass: !unlimited && (remainingPercent ?? 100) <= 10 ? 'bg-red-500' : !unlimited && (remainingPercent ?? 100) <= 30 ? 'bg-amber-500' : 'bg-emerald-500',
  }]
}))

function dateTime(value: unknown): string | null {
  if (value == null || value === '') return null
  const date = typeof value === 'number' ? new Date(value * 1000) : new Date(String(value))
  return Number.isNaN(date.getTime()) ? null : date.toLocaleString()
}

const metadataItems = computed(() => [
  { label: legacyT('方案'), value: (props.quota?.membership_level || props.quota?.plan_type)?.replace(/^LEVEL_/, '').replaceAll('_', ' ') || null },
  { label: legacyT('套餐类型'), value: props.quota?.token_plan_scope === 'team' ? legacyT('团队版') : props.quota?.token_plan_scope === 'personal' ? legacyT('个人版') : null },
  { label: legacyT('并发'), value: props.quota?.parallel_limit != null ? String(props.quota.parallel_limit) : null },
  { label: legacyT('过期'), value: dateTime(props.quota?.expires_at) },
  { label: legacyT('限额重置'), value: dateTime(props.quota?.limit_reset) },
].filter((item): item is { label: string; value: string } => item.value != null))

function remainingPercent(window: QuotaWindowSnapshot): number | null {
  const remainingRatio = number(window.remaining_ratio)
  if (remainingRatio != null) return Math.min(100, Math.max(0, remainingRatio * 100))
  const usedRatio = number(window.used_ratio)
  if (usedRatio != null) return Math.min(100, Math.max(0, (1 - usedRatio) * 100))
  const remaining = number(window.remaining_value)
  const limit = number(window.limit_value)
  if (remaining != null && limit != null && limit > 0) return Math.min(100, Math.max(0, remaining / limit * 100))
  return null
}

function compactValue(value: unknown): string | null {
  return formatDecimalDisplay(value)
}

const quotaWindows = computed(() => (props.quota?.windows ?? []).flatMap((window) => {
  if (quotaUnavailable.value) return []
  const remaining = remainingPercent(window)
  const remainingValue = compactValue(window.remaining_value)
  const limitValue = compactValue(window.limit_value)
  const reset = window.reset_at_text || (window.reset_at ? new Date(window.reset_at * 1000).toLocaleString() : null)
  const detail = remainingValue && limitValue ? `${remainingValue} / ${limitValue}` : null
  if (remaining == null && detail == null && reset == null) return []
  return [{
    code: window.code,
    label: window.label || window.code || legacyT('订阅配额'),
    remainingPercent: remaining,
    meterClass: (remaining ?? 100) <= 10 ? 'text-red-600' : (remaining ?? 100) <= 30 ? 'text-amber-600' : 'text-emerald-600',
    barClass: (remaining ?? 100) <= 10 ? 'bg-red-500' : (remaining ?? 100) <= 30 ? 'bg-amber-500' : 'bg-emerald-500',
    footer: [detail, reset ? `${legacyT('重置')} ${reset}` : null].filter(Boolean).join(' · ') || null,
  }]
}))

const hasStructuredContent = computed(() => quotaUnavailable.value || Boolean(modelAvailability.value) || balanceItems.value.length > 0 || quotaWindows.value.length > 0 || sections.value.rateLimits.length > 0)
const hasContent = computed(() => Boolean(props.providerType || props.quota || props.loading))
const updatedText = computed(() => {
  const timestamp = props.quota?.refresh_state?.last_success_at ?? props.quota?.observed_at ?? props.quota?.updated_at
  if (typeof timestamp !== 'number') return null
  return new Date(timestamp * 1000).toLocaleString()
})
</script>
