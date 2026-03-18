<template>
  <div class="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
    <Card v-for="item in items" :key="item.label" class="overflow-hidden border-border/60 p-0">
      <div class="h-1 w-full" :class="item.tone || 'bg-primary/70'" />
      <div class="p-5">
        <div class="text-xs uppercase tracking-[0.18em] text-muted-foreground">{{ item.label }}</div>
        <div class="mt-3 text-3xl font-semibold text-foreground">{{ item.value }}</div>
        <div v-if="item.description" class="mt-2 text-xs text-muted-foreground">{{ item.description }}</div>
      </div>
    </Card>
  </div>
</template>

<script setup lang="ts">
export interface SearchPoolStatItem {
  label: string
  value: string | number
  description?: string
  tone?: string
}

defineProps<{
  items: SearchPoolStatItem[]
}>()
</script>
