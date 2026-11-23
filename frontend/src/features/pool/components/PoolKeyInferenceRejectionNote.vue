<template>
  <div
    v-if="rejection"
    :class="variant === 'mobile'
      ? 'rounded-xl border border-border/50 bg-muted/30 px-3 py-2 text-[11px] leading-4 text-muted-foreground tabular-nums space-y-0.5'
      : 'space-y-0.5 text-[10px] leading-tight text-muted-foreground tabular-nums'"
    :title="rejection.title"
    :data-testid="`pool-key-antigravity-rejection-${variant}-${keyId}`"
  >
    <div
      v-if="rejection.label"
      class="font-medium text-amber-600 dark:text-amber-400"
    >
      {{ rejection.label }}
    </div>
    <div v-if="rejection.lastOccurredText">
      {{ rejection.lastOccurredText }}
    </div>
    <div v-if="rejection.retryAtText || rejection.remainingText">
      {{ rejection.retryAtText }}
      <span v-if="rejection.remainingText">
        ({{ rejection.remainingText }})
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { AntigravityInferenceRejectionDisplay } from '@/features/pool/utils/poolKeyStatus'

withDefaults(defineProps<{
  keyId: string
  rejection?: AntigravityInferenceRejectionDisplay | null
  variant?: 'desktop' | 'mobile'
}>(), {
  rejection: null,
  variant: 'desktop',
})
</script>
