<template>
  <div
    class="flex flex-col gap-2 border-t pt-3 sm:flex-row sm:items-center sm:justify-between"
    data-testid="claude-endpoint-operation-support"
  >
    <Label class="text-xs normal-case tracking-normal text-muted-foreground">
      {{ t('providers.local.supportedOperations') }}
    </Label>
    <div class="flex flex-wrap items-center gap-x-5 gap-y-2">
      <div class="flex items-center gap-2">
        <span class="text-xs text-foreground">{{ t('providers.local.messagesOp') }}</span>
        <Switch
          :model-value="true"
          disabled
          :aria-label="t('providers.local.messagesOperation')"
          :title="t('providers.local.messagesOperationAlwaysEnabled')"
        />
      </div>
      <div class="flex items-center gap-2">
        <span class="text-xs text-foreground">{{ t('providers.local.countTokensOp') }}</span>
        <Switch
          :model-value="modelValue"
          :disabled="disabled || locked"
          :aria-label="t('providers.local.countTokensOperation')"
          :title="locked ? t('providers.local.providerTypeDoesNotSupportCountTokens') : (modelValue ? t('providers.local.countTokensSupported') : t('providers.local.countTokensUnsupported'))"
          data-testid="claude-count-tokens-switch"
          @update:model-value="$emit('update:modelValue', $event)"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { Label, Switch } from '@/components/ui'
import { useI18n } from '@/i18n'

defineProps<{
  modelValue: boolean
  disabled?: boolean
  locked?: boolean
}>()

defineEmits<{
  'update:modelValue': [value: boolean]
}>()
const { t } = useI18n()

</script>
