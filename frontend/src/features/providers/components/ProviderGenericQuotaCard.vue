<template>
  <div
    v-if="providerType || quota || loading"
    class="rounded-md bg-muted/30 p-2"
    :class="compact ? 'min-w-0 text-xs' : 'mt-2'"
    data-testid="provider-generic-quota"
  >
    <ProviderQuotaSectionHeader
      v-if="!compact || refreshable"
      :title="legacyT('账户额度')"
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
      <div class="font-medium">
        {{ legacyT(modelAvailability.title) }}
      </div>
      <div
        v-if="modelAvailability.detail"
        class="mt-0.5"
      >
        {{ legacyT(modelAvailability.detail) }}
      </div>
    </div>
    <div
      class="space-y-2"
      :class="compact ? '' : 'mt-1.5'"
    >
      <section
        v-for="group in groups"
        :key="group.id"
        class="min-w-0 rounded border border-border/50 bg-background/45 p-2"
        :data-quota-source="group.id"
      >
        <div class="flex flex-wrap items-center justify-between gap-x-2 gap-y-1 text-[10px]">
          <span class="font-medium">{{ legacyT(group.label) }}</span>
          <span
            v-if="group.status"
            :class="group.status === '查询成功' && !group.stale ? 'text-muted-foreground' : 'text-amber-600 dark:text-amber-400'"
          >
            {{ legacyT(group.status) }}
          </span>
        </div>
        <div
          v-if="group.metadata.length"
          class="mt-1 break-words text-[9px] text-muted-foreground"
        >
          {{ group.metadata.join(' · ') }}
        </div>
        <div
          v-if="group.balances.length"
          class="mt-1.5 space-y-2"
          data-testid="provider-balance-panel"
        >
          <div
            v-for="balance in group.balances"
            :key="balance.key"
            class="min-w-0"
          >
            <div class="flex flex-wrap items-baseline justify-between gap-x-2 gap-y-1">
              <span class="text-[10px] text-muted-foreground">{{ legacyT(balance.label) }}</span>
              <span
                class="min-w-0 break-all text-right text-sm font-semibold tabular-nums"
                :class="balance.insufficient && !group.stale ? 'text-amber-600 dark:text-amber-400' : 'text-foreground'"
                data-testid="provider-quota-available"
              >{{ legacyT(balance.available) }}</span>
            </div>
            <ProviderQuotaProgressRow
              v-if="balance.remainingPercent != null"
              class="mt-1"
              :label="legacyT('剩余比例')"
              :remaining-percent="balance.remainingPercent"
              :meter-class="remainingTone(balance.remainingPercent).meterClass"
              :bar-class="remainingTone(balance.remainingPercent).barClass"
            />
            <div
              v-if="balance.parts.length"
              class="mt-1 break-words text-[9px] text-muted-foreground"
            >
              {{ balance.parts.join(' · ') }}
            </div>
          </div>
        </div>
        <div
          v-if="group.windows.length"
          class="mt-2 space-y-2"
          data-testid="provider-subscription-panel"
        >
          <ProviderQuotaProgressRow
            v-for="window in group.windows"
            :key="window.key"
            :label="legacyT(window.label)"
            :remaining-percent="window.remainingPercent"
            :meter-text="legacyT(window.meterText)"
            :meter-class="remainingTone(window.remainingPercent).meterClass"
            :bar-class="remainingTone(window.remainingPercent).barClass"
            :reset-text="[window.detail, window.resetText].filter(Boolean).join(' · ')"
            footer-class="break-words"
          />
        </div>
        <div
          v-if="group.notes.length"
          class="mt-1.5 text-[9px] text-muted-foreground"
        >
          <div
            v-for="note in group.notes"
            :key="note"
          >
            {{ legacyT(note) }}
          </div>
        </div>
        <div
          v-if="group.error"
          class="mt-1.5 break-words text-[10px] text-amber-600 dark:text-amber-400"
          data-testid="provider-generic-quota-status"
        >
          {{ legacyT(group.error) }}
        </div>
        <div
          v-if="group.updatedText"
          class="mt-1 text-[9px] text-muted-foreground/80"
        >
          {{ legacyT('上次成功') }} {{ group.updatedText }}
        </div>
        <div
          v-if="!group.balances.length && !group.windows.length"
          class="mt-1 text-[10px] text-muted-foreground"
          data-testid="provider-quota-empty"
        >
          {{ legacyT('额度未知') }}
        </div>
      </section>
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
      v-if="!groups.length"
      class="mt-1.5 text-[10px] text-muted-foreground"
      data-testid="provider-quota-empty"
    >
      {{ loading ? legacyT('正在查询额度') : legacyT('尚未查询额度') }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { ModelProbeStatusSnapshot, QuotaStatusSnapshot } from '@/api/endpoints/types'
import Badge from '@/components/ui/badge.vue'
import { useI18n } from '@/i18n'
import {
  formatQuotaDateTime,
  getGenericQuotaGroups,
  getGenericQuotaSections,
  getZhipuModelAvailabilityDisplay,
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
  /** 号池复用相同来源与数值展示，只压缩外层间距。 */
  compact?: boolean
}>(), {
  quota: null,
  modelProbe: null,
  loading: false,
  providerType: null,
  refreshable: false,
  refreshDisabled: false,
  compact: false,
})

defineEmits<{ (e: 'refresh'): void }>()

const { legacyT } = useI18n()
const groups = computed(() => getGenericQuotaGroups(props.quota, props.providerType))
const sections = computed(() => getGenericQuotaSections(props.quota, props.providerType))
const modelAvailability = computed(() => (props.quota?.sources?.length ?? 0) > 0 ? null : getZhipuModelAvailabilityDisplay(
  props.quota, props.modelProbe, props.providerType,
))
const updatedText = computed(() => formatQuotaDateTime(
  props.quota?.refresh_state?.last_success_at ?? props.quota?.observed_at ?? props.quota?.updated_at,
))

function remainingTone(remaining: number | null): { barClass: string; meterClass: string } {
  if (remaining == null) return { barClass: 'bg-muted', meterClass: 'text-muted-foreground' }
  if (remaining <= 10) return { barClass: 'bg-red-500', meterClass: 'text-red-600 dark:text-red-400' }
  if (remaining <= 30) return { barClass: 'bg-amber-500', meterClass: 'text-amber-600 dark:text-amber-400' }
  return { barClass: 'bg-emerald-500', meterClass: 'text-emerald-600 dark:text-emerald-400' }
}
</script>
