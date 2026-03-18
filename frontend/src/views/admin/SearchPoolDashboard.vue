<template>
  <PageContainer>
    <PageHeader title="搜索池网关" description="统一管理 Tavily 与 Firecrawl 的真实 Key、代理 Token 与工作台状态。" />

    <Card class="mt-6 overflow-hidden border-border/60 bg-gradient-to-br from-slate-950 via-slate-900 to-slate-800 p-8 text-slate-50 shadow-xl">
      <div class="max-w-4xl">
        <div class="flex flex-wrap gap-2 text-xs uppercase tracking-[0.18em] text-slate-300">
          <span class="rounded-full border border-white/10 bg-white/5 px-3 py-1">统一搜索入口</span>
          <span class="rounded-full border border-white/10 bg-white/5 px-3 py-1">Key + Token + Sync</span>
          <span class="rounded-full border border-white/10 bg-white/5 px-3 py-1">Search Workspace</span>
        </div>
        <h2 class="mt-6 text-4xl font-semibold tracking-tight text-white sm:text-5xl">
          把搜索池运维收敛到一个真正可用的工作台
        </h2>
        <p class="mt-5 max-w-3xl text-base leading-7 text-slate-300">
          先看服务分类面板，再进入单服务工作台处理真实额度、调用示例、Token 池和 Key 池。交互方式对齐参考项目，但视觉系统保持当前 Aether 管理端风格。
        </p>
      </div>
    </Card>

    <div class="mt-8 grid gap-6 xl:grid-cols-2">
      <SearchPoolServiceCard
        v-for="summary in services"
        :key="summary.service"
        :summary="summary"
        @select="goToWorkspace(summary.service)"
      />
    </div>
  </PageContainer>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { PageContainer, PageHeader } from '@/components/layout'
import { Card } from '@/components/ui'
import { useToast } from '@/composables/useToast'
import { searchPoolApi } from '@/features/search-pool/api'
import SearchPoolServiceCard from '@/features/search-pool/components/SearchPoolServiceCard.vue'
import type { SearchPoolServiceSummary, SearchService } from '@/features/search-pool/types'
import { parseApiError } from '@/utils/errorParser'

const router = useRouter()
const services = ref<SearchPoolServiceSummary[]>([])
const { error } = useToast()

async function loadServices() {
  try {
    services.value = await searchPoolApi.listServiceSummaries()
  } catch (err) {
    error(parseApiError(err, '加载服务工作台失败'))
  }
}

function goToWorkspace(service: SearchService) {
  void router.push({ name: 'SearchPoolServiceWorkspace', params: { service } })
}

onMounted(() => {
  void loadServices()
})
</script>
