<template>
  <div class="mt-2 rounded-md bg-muted/30 p-2" data-testid="nous-quota-card">
    <ProviderQuotaSectionHeader
      title="Nous 账号额度"
      :loading="loading"
      :updated-text="updatedText"
    />

    <div v-if="quota.plan_type" class="mb-2 text-[10px] font-medium">
      套餐：{{ quota.plan_type }}
    </div>

    <div v-if="progressItems.length" class="grid grid-cols-1 gap-3 sm:grid-cols-2">
      <ProviderQuotaProgressRow
        v-for="item in progressItems"
        :key="item.code"
        :label="item.label"
        :used-percent="item.usedPercent"
        :remaining-percent="item.remainingPercent"
        :meter-class="meterClass(item.usedPercent)"
        :bar-class="barClass(item.usedPercent)"
        :reset-text="item.resetText"
      >
        <template #footer>
          <div class="mt-0.5 text-[9px] text-muted-foreground/70">{{ item.detail }}</div>
        </template>
      </ProviderQuotaProgressRow>
    </div>

    <div v-if="summaryItems.length" class="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[10px] text-muted-foreground">
      <span v-for="item in summaryItems" :key="item.label">{{ item.label }} {{ item.value }}</span>
    </div>

    <div v-if="rateLimitsText" class="mt-2 text-[10px] text-muted-foreground">
      配置速率上限：{{ rateLimitsText }}
    </div>
    <div v-if="quota.exhausted" class="mt-1 text-[10px] font-medium text-red-600">
      {{ quota.exhausted_reason === 'no_usable_credits' ? '无可用推理 Credits' : '账号额度不可用' }}
    </div>
    <div v-if="quota.billing_stale" class="mt-1 text-[10px] text-amber-600">账单数据已过期，当前显示上次成功结果</div>
    <div v-else-if="quota.billing_available === false" class="mt-1 text-[10px] text-muted-foreground">账单信息暂不可用</div>
    <div v-if="rateLimitRecovery" class="mt-1 text-[10px] text-amber-600">临时限流，{{ rateLimitRecovery }}</div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { QuotaStatusSnapshot, QuotaWindowSnapshot } from '@/api/endpoints/types'
import ProviderQuotaProgressRow from './ProviderQuotaProgressRow.vue'
import ProviderQuotaSectionHeader from './ProviderQuotaSectionHeader.vue'

const props = defineProps<{ quota: QuotaStatusSnapshot; loading?: boolean }>()

function decimal(value: unknown): number | null {
  if ((typeof value !== 'string' && typeof value !== 'number') || String(value).trim() === '') return null
  const parsed = Number(value)
  return Number.isFinite(parsed) ? parsed : null
}

function format(value: unknown, currency = false): string | null {
  const parsed = decimal(value)
  if (parsed == null) return null
  const text = typeof value === 'string'
    ? value.trim().replace(/(\.\d*?[1-9])0+$|\.0+$/, '$1')
    : (Number.isInteger(parsed) ? String(parsed) : String(parsed))
  return currency ? `$${text}` : text
}

function windowByCode(code: string): QuotaWindowSnapshot | null {
  return props.quota.windows?.find(item => item.code === code) ?? null
}

function percent(window: QuotaWindowSnapshot): { used: number; remaining: number } | null {
  if (window.is_exhausted === true) return { remaining: 0, used: 100 }
  const remainingRatio = decimal(window.remaining_ratio)
  const usedRatio = decimal(window.used_ratio)
  const limit = decimal(window.limit_value)
  const remaining = decimal(window.remaining_value)
  const used = decimal(window.used_value)
  let remainingPercent = remainingRatio != null ? remainingRatio * 100 : usedRatio != null ? (1 - usedRatio) * 100 : null
  if (remainingPercent == null && limit != null && limit > 0) {
    remainingPercent = remaining != null ? remaining / limit * 100 : used != null ? (1 - used / limit) * 100 : null
  }
  if (remainingPercent == null) return null
  const normalized = Math.max(0, Math.min(100, remainingPercent))
  return { remaining: normalized, used: 100 - normalized }
}

function resetText(window: QuotaWindowSnapshot): string | null {
  const resetAt = window.reset_at ?? props.quota.current_period_end
  if (!resetAt) return null
  return `周期结束：${new Date(resetAt * 1000).toLocaleString()}`
}

const progressItems = computed(() => [
  { code: 'subscription_credits', label: '订阅 Credits', currency: false },
  { code: 'monthly_spend', label: '本月消费', currency: true },
].flatMap(config => {
  const window = windowByCode(config.code)
  if (!window) return []
  const values = percent(window)
  if (!values) return []
  if (config.code === 'subscription_credits' && props.quota.exhausted === true) {
    values.remaining = 0
    values.used = 100
  }
  const left = config.currency ? window.used_value : window.remaining_value
  const detail = `${format(left, config.currency) ?? '-'} / ${format(window.limit_value, config.currency) ?? '-'}`
  return [{ ...config, usedPercent: values.used, remainingPercent: values.remaining, detail, resetText: resetText(window) }]
}))

const summaryItems = computed(() => [
  { label: '总可用 Credits', value: format(props.quota.total_usable_credits) },
  { label: '购买 Credits', value: format(props.quota.purchased_credits_remaining) },
  { label: '账户余额', value: format(props.quota.balance_usd, true) },
].filter((item): item is { label: string; value: string } => item.value != null))

const rateLimitsText = computed(() => {
  const limits = props.quota.rate_limits
  if (!limits) return null
  return ([['RPM', limits.rpm], ['TPM', limits.tpm], ['RPH', limits.rph], ['TPH', limits.tph]] as const)
    .filter(([, value]) => typeof value === 'number')
    .map(([label, value]) => `${label} ${value}`)
    .join(' · ') || null
})

const rateLimitRecovery = computed(() => {
  const window = windowByCode('rate_limit')
  if (!window?.is_exhausted) return null
  if (window.reset_seconds != null) return `${window.reset_seconds} 秒后恢复`
  if (window.reset_at != null) return `${new Date(window.reset_at * 1000).toLocaleString()} 恢复`
  return '等待上游恢复'
})

const updatedText = computed(() => {
  const timestamp = props.quota.updated_at ?? props.quota.observed_at
  return timestamp ? new Date(timestamp * 1000).toLocaleString() : null
})

function meterClass(used: number) { return used >= 90 ? 'text-red-600' : used >= 70 ? 'text-amber-600' : 'text-green-600' }
function barClass(used: number) { return used >= 90 ? 'bg-red-500' : used >= 70 ? 'bg-amber-500' : 'bg-green-500' }
</script>
