<template>
  <div
    class="flex flex-col gap-2 border-t pt-3 sm:flex-row sm:items-center sm:justify-between"
    data-testid="claude-endpoint-operation-support"
  >
    <Label class="text-xs normal-case tracking-normal text-muted-foreground">
      支持的操作
    </Label>
    <div class="flex flex-wrap items-center gap-x-5 gap-y-2">
      <div class="flex items-center gap-2">
        <span class="text-xs text-foreground">消息</span>
        <Switch
          :model-value="true"
          disabled
          aria-label="消息操作"
          title="消息操作始终启用"
        />
      </div>
      <div class="flex items-center gap-2">
        <span class="text-xs text-foreground">Token 计数</span>
        <Switch
          :model-value="modelValue"
          :disabled="disabled || locked"
          aria-label="Token 计数操作"
          :title="locked ? '该提供商类型不支持 Token 计数' : (modelValue ? '已支持 Token 计数' : '不支持 Token 计数')"
          data-testid="claude-count-tokens-switch"
          @update:model-value="$emit('update:modelValue', $event)"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { Label, Switch } from '@/components/ui'

defineProps<{
  modelValue: boolean
  disabled?: boolean
  locked?: boolean
}>()

defineEmits<{
  'update:modelValue': [value: boolean]
}>()
</script>
