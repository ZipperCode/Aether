<template>
  <Card
    class="group relative overflow-hidden border-border/60 p-0 transition-all duration-200 hover:-translate-y-0.5 hover:border-primary/40 hover:shadow-lg"
    :class="toneClass"
  >
    <button
      type="button"
      class="w-full text-left"
      @click="$emit('select')"
    >
      <div class="relative p-6">
        <div class="absolute inset-x-0 top-0 h-1" :class="barClass" />
        <div class="flex items-start justify-between gap-4">
          <div>
            <Badge variant="outline" class="border-current/15 bg-background/70 text-xs">
              {{ summary.service_badge }}
            </Badge>
            <h3 class="mt-4 text-2xl font-semibold text-foreground">
              {{ summary.title }}
            </h3>
            <p class="mt-2 text-sm leading-6 text-muted-foreground">
              {{ summary.description }}
            </p>
          </div>
          <div class="rounded-full border border-border/60 bg-background/80 p-2 text-muted-foreground transition-colors group-hover:text-foreground">
            <ArrowRight class="h-4 w-4" />
          </div>
        </div>

        <div class="mt-6 grid gap-3 sm:grid-cols-2">
          <div class="rounded-2xl border border-border/60 bg-background/80 p-4">
            <div class="text-xs uppercase tracking-[0.2em] text-muted-foreground">活跃 Key</div>
            <div class="mt-2 text-3xl font-semibold">{{ summary.keys_active }}</div>
            <div class="mt-1 text-xs text-muted-foreground">总计 {{ summary.keys_total }}</div>
          </div>
          <div class="rounded-2xl border border-border/60 bg-background/80 p-4">
            <div class="text-xs uppercase tracking-[0.2em] text-muted-foreground">Token</div>
            <div class="mt-2 text-3xl font-semibold">{{ summary.tokens_total }}</div>
            <div class="mt-1 text-xs text-muted-foreground">独立网关访问池</div>
          </div>
          <div class="rounded-2xl border border-border/60 bg-background/80 p-4">
            <div class="text-xs uppercase tracking-[0.2em] text-muted-foreground">今日调用</div>
            <div class="mt-2 text-3xl font-semibold">{{ summary.requests_today }}</div>
            <div class="mt-1 text-xs text-muted-foreground">按 usage log 聚合</div>
          </div>
          <div class="rounded-2xl border border-border/60 bg-background/80 p-4">
            <div class="text-xs uppercase tracking-[0.2em] text-muted-foreground">真实剩余</div>
            <div class="mt-2 text-3xl font-semibold">{{ summary.real_remaining }}</div>
            <div class="mt-1 truncate text-xs text-muted-foreground">{{ summary.route_label }}</div>
          </div>
        </div>

        <div class="mt-5 flex items-center justify-between gap-3 text-xs text-muted-foreground">
          <span class="truncate">{{ summary.route_path }}</span>
          <span>{{ summary.last_synced_at ? '已同步' : '未同步' }}</span>
        </div>
      </div>
    </button>
  </Card>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { ArrowRight } from 'lucide-vue-next'
import { Card, Badge } from '@/components/ui'
import type { SearchPoolServiceSummary } from '../types'

const props = defineProps<{
  summary: SearchPoolServiceSummary
}>()

defineEmits<{
  select: []
}>()

const toneClass = computed(() => {
  return props.summary.service === 'firecrawl'
    ? 'bg-gradient-to-br from-orange-50/70 via-background to-background dark:from-orange-950/10'
    : 'bg-gradient-to-br from-emerald-50/70 via-background to-background dark:from-emerald-950/10'
})

const barClass = computed(() => {
  return props.summary.service === 'firecrawl' ? 'bg-orange-400/80' : 'bg-emerald-400/80'
})
</script>
