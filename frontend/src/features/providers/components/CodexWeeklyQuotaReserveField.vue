<template>
  <div class="space-y-2 rounded-md border border-border/60 bg-muted/30 px-3 py-2">
    <div class="flex items-center justify-between gap-3">
      <div class="space-y-0.5">
        <Label
          :for="inputId"
          class="text-sm font-medium"
        >
          {{ t('ui.weeklyQuotaReservePercent') }}
        </Label>
        <p class="text-xs text-muted-foreground">
          {{ t('ui.weeklyQuotaReserveDescription') }}
        </p>
      </div>
      <div class="flex shrink-0 items-center gap-1.5">
        <Input
          :id="inputId"
          :model-value="modelValue"
          type="number"
          min="0"
          max="99"
          placeholder="0"
          class="h-8 w-20 text-right tabular-nums"
          @update:model-value="handleInput"
        />
        <span class="text-xs text-muted-foreground">%</span>
      </div>
    </div>
    <div class="flex flex-wrap items-center gap-1.5">
      <Button
        v-for="preset in WEEKLY_QUOTA_RESERVE_PRESETS"
        :key="preset"
        type="button"
        size="sm"
        :variant="modelValue === preset ? 'default' : 'outline'"
        class="h-6 px-2 text-xs tabular-nums"
        @click="emit('update:modelValue', preset)"
      >
        {{ preset }}%
      </Button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from '@/i18n'
import { Button, Input, Label } from '@/components/ui'
import { parseNumberInput } from '@/utils/form'

const WEEKLY_QUOTA_RESERVE_PRESETS = [0, 5, 10, 20] as const

defineProps<{
  inputId: string
  modelValue: number
}>()

const emit = defineEmits<{
  'update:modelValue': [value: number]
}>()

const { t } = useI18n()

function handleInput(value: string | number) {
  emit('update:modelValue', parseNumberInput(value, { min: 0, max: 99 }) ?? 0)
}
</script>
