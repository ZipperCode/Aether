<template>
  <div class="flex items-center justify-between mb-1">
    <span class="text-[10px] text-muted-foreground">
      {{ title }}
    </span>
    <div class="flex items-center gap-1">
      <span
        v-if="updatedText"
        class="text-[9px] text-muted-foreground/70"
        data-testid="provider-quota-header-updated"
      >
        {{ updatedText }}
      </span>
      <button
        v-if="refreshable"
        type="button"
        class="inline-flex h-5 w-5 items-center justify-center rounded text-muted-foreground/70 transition-colors hover:bg-muted hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
        :disabled="loading || refreshDisabled"
        :title="refreshTitle"
        :aria-label="refreshTitle"
        data-testid="provider-quota-header-refresh"
        @click.stop="$emit('refresh')"
      >
        <RefreshCw
          class="h-3 w-3"
          :class="{ 'animate-spin': loading }"
          :data-testid="loading ? 'provider-quota-header-loading' : 'provider-quota-header-icon'"
        />
      </button>
      <RefreshCw
        v-else-if="loading"
        class="h-3 w-3 animate-spin text-muted-foreground/70"
        data-testid="provider-quota-header-loading"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { RefreshCw } from 'lucide-vue-next'

withDefaults(defineProps<{
  title: string
  loading?: boolean
  updatedText?: string | null
  refreshable?: boolean
  refreshDisabled?: boolean
  refreshTitle?: string
}>(), {
  loading: false,
  updatedText: null,
  refreshable: false,
  refreshDisabled: false,
  refreshTitle: '刷新额度',
})

defineEmits<{
  (e: 'refresh'): void
}>()
</script>
