<template>
  <div
    v-if="providerType || quota || loading"
    class="rounded-md bg-muted/30 p-2"
    :class="compact ? 'min-w-0 text-xs' : 'mt-2'"
    data-testid="provider-generic-quota"
  >
    <ProviderQuotaSectionHeader
      v-if="!compact || refreshable"
      :title="t('providers.local.quotaTitle')"
      :loading="loading"
      :updated-text="updatedText"
      :refreshable="refreshable"
      :refresh-disabled="refreshDisabled"
      :refresh-title="t('ui.refreshQuota')"
      @refresh="$emit('refresh')"
    />
    <div
      class="space-y-2"
      :class="compact ? '' : 'mt-1.5'"
    >
      <section
        v-for="group in groups"
        :key="group.id"
        class="min-w-0 rounded bg-background/45 p-2"
        :data-quota-source="group.id"
      >
        <div class="flex flex-wrap items-center justify-between gap-x-2 gap-y-1 text-[10px]">
          <span class="font-medium">{{ group.label }}</span>
          <span
            v-if="group.status"
            :class="!group.stale && !group.error ? 'text-muted-foreground' : 'text-amber-600 dark:text-amber-400'"
          >
            {{ group.status }}
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
              <span class="text-[10px] text-muted-foreground">{{ balance.label }}</span>
              <span
                class="min-w-0 break-all text-right text-sm font-semibold tabular-nums"
                :class="balance.insufficient && !group.stale ? 'text-amber-600 dark:text-amber-400' : 'text-foreground'"
                data-testid="provider-quota-available"
              >{{ balance.available }}</span>
            </div>
            <ProviderQuotaProgressRow
              v-if="balance.remainingPercent != null"
              class="mt-1"
              :label="t('providers.local.remainingRatio')"
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
            :label="window.label"
            :remaining-percent="window.remainingPercent"
            :meter-text="window.meterText"
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
            {{ note }}
          </div>
        </div>
        <div
          v-if="group.error"
          class="mt-1.5 break-words text-[10px] text-amber-600 dark:text-amber-400"
          data-testid="provider-generic-quota-status"
        >
          {{ group.error }}
        </div>
        <div
          v-if="group.updatedText"
          class="mt-1 text-[9px] text-muted-foreground/80"
        >
          {{ t('providers.local.lastSucceeded') }} {{ group.updatedText }}
        </div>
        <div
          v-if="!group.balances.length && !group.windows.length"
          class="mt-1 text-[10px] text-muted-foreground"
          data-testid="provider-quota-empty"
        >
          {{ t('providers.local.quotaUnknown') }}
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
      {{ loading ? t('providers.local.queryingQuota') : t('providers.local.quotaNotQueriedYet') }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { QuotaStatusSnapshot } from '@/api/endpoints/types'
import Badge from '@/components/ui/badge.vue'
import { useI18n } from '@/i18n'
import {
  formatQuotaDateTime,
  getGenericQuotaGroups,
  getGenericQuotaSections,
} from '@/features/providers/utils/providerKeyQuota'
import ProviderQuotaProgressRow from './ProviderQuotaProgressRow.vue'
import ProviderQuotaSectionHeader from './ProviderQuotaSectionHeader.vue'

const props = withDefaults(defineProps<{
  quota?: QuotaStatusSnapshot | null
  loading?: boolean
  providerType?: string | null
  refreshable?: boolean
  refreshDisabled?: boolean
  /** 号池复用相同来源与数值展示，只压缩外层间距。 */
  compact?: boolean
}>(), {
  quota: null,
  loading: false,
  providerType: null,
  refreshable: false,
  refreshDisabled: false,
  compact: false,
})

defineEmits<{ (e: 'refresh'): void }>()

const { t } = useI18n()
const groups = computed(() => getGenericQuotaGroups(props.quota, props.providerType))
const sections = computed(() => getGenericQuotaSections(props.quota, props.providerType))
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
