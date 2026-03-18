<template>
  <div class="search-pool-dashboard">
    <h1>搜索池网关</h1>
    <el-space>
      <el-select v-model="service" style="width: 180px">
        <el-option label="Tavily" value="tavily" />
        <el-option label="Firecrawl" value="firecrawl" />
      </el-select>
      <el-button type="primary" @click="loadStats">刷新统计</el-button>
      <el-button @click="runSync">同步额度</el-button>
    </el-space>

    <el-card class="stats-card">
      <el-descriptions :column="2" border>
        <el-descriptions-item label="服务">{{ stats?.service || '-' }}</el-descriptions-item>
        <el-descriptions-item label="可用密钥">{{ stats?.keys_active ?? '-' }}</el-descriptions-item>
        <el-descriptions-item label="密钥总数">{{ stats?.keys_total ?? '-' }}</el-descriptions-item>
        <el-descriptions-item label="网关令牌">{{ stats?.tokens_total ?? '-' }}</el-descriptions-item>
        <el-descriptions-item label="请求总数">{{ stats?.requests_total ?? '-' }}</el-descriptions-item>
        <el-descriptions-item label="成功率">{{ successRate }}</el-descriptions-item>
      </el-descriptions>
    </el-card>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { searchPoolApi } from '@/features/search-pool/api'
import type { SearchPoolStatsOverview, SearchService } from '@/features/search-pool/types'

const service = ref<SearchService>('tavily')
const stats = ref<SearchPoolStatsOverview | null>(null)
const { success } = useToast()

const successRate = computed(() => {
  if (!stats.value) return '-'
  return `${(stats.value.success_rate * 100).toFixed(2)}%`
})

const loadStats = async () => {
  stats.value = await searchPoolApi.getStatsOverview(service.value)
}

const runSync = async () => {
  await searchPoolApi.syncUsage(service.value, true)
  success('同步完成')
  await loadStats()
}

onMounted(loadStats)
</script>

<style scoped>
.search-pool-dashboard {
  display: grid;
  gap: 16px;
}

.stats-card {
  max-width: 960px;
}
</style>
