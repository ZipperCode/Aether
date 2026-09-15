<template>
  <div data-testid="provider-quota-progress-row">
    <div class="flex items-center justify-between text-[10px] mb-0.5">
      <span
        class="text-muted-foreground truncate mr-2 min-w-0 flex-1"
        :title="title || label"
      >
        {{ label }}
      </span>
      <span
        :class="meterClass"
        data-testid="provider-quota-progress-meter"
      >
        {{ meterText || (displayRemainingPercent == null ? legacyT('未知') : `${displayRemainingPercent.toFixed(1)}%`) }}
      </span>
    </div>
    <div
      v-if="displayRemainingPercent != null"
      class="relative w-full h-1.5 bg-border rounded-full overflow-hidden"
    >
      <div
        class="absolute left-0 top-0 h-full transition-all duration-300"
        :class="barClass"
        :style="{ width: `${normalizedRemainingPercent}%` }"
        data-testid="provider-quota-progress-bar"
      />
    </div>
    <slot name="footer">
      <div
        v-if="resetText"
        class="text-[9px] text-muted-foreground/70 mt-0.5"
        :class="footerClass"
        data-testid="provider-quota-progress-reset"
      >
        {{ resetText }}
      </div>
    </slot>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from '@/i18n'
import { finiteNumber } from '@/utils/providerKeyQuota'

const props = withDefaults(defineProps<{
  label: string
  usedPercent?: number | null
  remainingPercent?: number | null
  title?: string | null
  meterClass?: string
  barClass?: string
  footerClass?: string
  resetText?: string | null
  /** 可选的显式表头数值，用于显示家族额度范围而非伪造单一百分比。 */
  meterText?: string | null
}>(), {
  usedPercent: null,
  remainingPercent: null,
  title: null,
  meterClass: '',
  barClass: '',
  footerClass: '',
  resetText: null,
  meterText: null,
})

const { legacyT } = useI18n()

// 加成允许展示超过 100%，仅进度条宽度做截断；缺失比例不能变成 0%。
const displayRemainingPercent = computed(() => {
  const remaining = finiteNumber(props.remainingPercent)
  if (remaining !== null) return remaining

  const used = finiteNumber(props.usedPercent)
  if (used !== null) return Math.max(100 - used, 0)

  return null
})
const normalizedRemainingPercent = computed(() => Math.min(Math.max(displayRemainingPercent.value ?? 0, 0), 100))
</script>
