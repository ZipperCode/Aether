<template>
  <dl
    class="grid grid-cols-1 gap-x-4 gap-y-1.5 text-xs sm:grid-cols-3"
    data-testid="service-tier-facts"
  >
    <div class="flex min-w-0 items-baseline justify-between gap-3 sm:block">
      <dt class="text-muted-foreground">
        请求层级
      </dt>
      <dd
        class="truncate font-mono font-medium text-foreground sm:mt-0.5"
        :title="requested || '-'"
      >
        {{ requested || '-' }}
      </dd>
    </div>
    <div class="flex min-w-0 items-baseline justify-between gap-3 sm:block">
      <dt class="text-muted-foreground">
        实际层级
      </dt>
      <dd
        class="truncate font-mono font-medium text-foreground sm:mt-0.5"
        :title="actual || '-'"
      >
        {{ actual || '-' }}
      </dd>
    </div>
    <div class="flex min-w-0 items-baseline justify-between gap-3 sm:block">
      <dt class="text-muted-foreground">
        计费层级
      </dt>
      <dd
        class="truncate font-mono font-medium text-foreground sm:mt-0.5"
        :title="billing || '-'"
      >
        {{ billing || '-' }}
      </dd>
    </div>
  </dl>
</template>

<script setup lang="ts">
defineProps<{
  requested: string | null
  actual: string | null
  billing: string | null
}>()
</script>
