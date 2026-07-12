<template>
  <div
    v-if="score !== undefined && score !== null"
    class="inline-flex items-center gap-1"
    :title="`Key 可用率 ${percent.toFixed(0)}%`"
    data-testid="pool-key-health"
  >
    <div class="h-1.5 w-10 overflow-hidden rounded-full bg-border">
      <div
        class="h-full rounded-full transition-all duration-300"
        :class="barClass"
        :style="{ width: `${percent}%` }"
        data-testid="pool-key-health-bar"
      />
    </div>
    <span class="text-[10px] font-medium tabular-nums" :class="textClass">
      {{ percent.toFixed(0) }}%
    </span>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{ score?: number | null }>()

const percent = computed(() => {
  const value = Number(props.score ?? 0)
  if (!Number.isFinite(value)) return 0
  return Math.min(Math.max(value * 100, 0), 100)
})

const barClass = computed(() => percent.value >= 80
  ? 'bg-emerald-500'
  : percent.value >= 50 ? 'bg-amber-500' : 'bg-red-500')
const textClass = computed(() => percent.value >= 80
  ? 'text-emerald-600 dark:text-emerald-400'
  : percent.value >= 50 ? 'text-amber-600 dark:text-amber-400' : 'text-red-600 dark:text-red-400')
</script>
