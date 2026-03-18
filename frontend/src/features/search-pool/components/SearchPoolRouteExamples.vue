<template>
  <Card class="border-border/60 p-6">
    <div class="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
      <div>
        <h3 class="text-lg font-semibold text-foreground">调用方式</h3>
        <p class="mt-2 text-sm leading-6 text-muted-foreground">
          当前服务入口与 curl 示例。兼容上游接口形态，团队只需要替换 base URL 与 Bearer Token。
        </p>
      </div>
      <div class="rounded-2xl border border-border/60 bg-muted/20 px-4 py-3 text-sm text-muted-foreground">
        <div class="text-xs uppercase tracking-[0.18em]">Base URL</div>
        <div class="mt-2 font-mono text-foreground">{{ baseUrl }}</div>
      </div>
    </div>

    <div class="mt-5 space-y-3">
      <div v-for="(example, index) in curlExamples" :key="index" class="overflow-hidden rounded-2xl border border-border/60 bg-slate-950 text-slate-100">
        <div class="flex items-center justify-between border-b border-white/10 px-4 py-2 text-xs uppercase tracking-[0.18em] text-slate-300">
          <span>示例 {{ index + 1 }}</span>
          <Button size="sm" variant="ghost" class="h-7 px-2 text-slate-200 hover:bg-white/10 hover:text-white" @click="copy(example)">
            <Copy class="mr-1 h-3.5 w-3.5" />复制
          </Button>
        </div>
        <pre class="overflow-x-auto px-4 py-4 text-xs leading-6"><code>{{ example }}</code></pre>
      </div>
    </div>
  </Card>
</template>

<script setup lang="ts">
import { Copy } from 'lucide-vue-next'
import { Button, Card } from '@/components/ui'
import { useToast } from '@/composables/useToast'

defineProps<{
  baseUrl: string
  curlExamples: string[]
}>()

const { success, error } = useToast()

async function copy(value: string) {
  try {
    await navigator.clipboard.writeText(value)
    success('已复制示例命令')
  } catch {
    error('复制失败，请手动复制')
  }
}
</script>
