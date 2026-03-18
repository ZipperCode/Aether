<template>
  <div class="search-pool-tokens">
    <h1>搜索池网关令牌</h1>
    <el-form :inline="true">
      <el-form-item label="服务">
        <el-select v-model="service" style="width: 180px">
          <el-option label="Tavily" value="tavily" />
          <el-option label="Firecrawl" value="firecrawl" />
        </el-select>
      </el-form-item>
      <el-form-item label="名称">
        <el-input v-model="name" placeholder="令牌名称" style="width: 220px" />
      </el-form-item>
      <el-form-item>
        <el-button type="primary" @click="createToken">新增</el-button>
        <el-button @click="loadTokens">刷新</el-button>
      </el-form-item>
    </el-form>

    <el-table :data="tokens" border>
      <el-table-column prop="service" label="服务" width="120" />
      <el-table-column prop="name" label="名称" width="180" />
      <el-table-column prop="token" label="Token" />
      <el-table-column prop="hourly_limit" label="时限" width="90" />
      <el-table-column prop="daily_limit" label="日限" width="90" />
      <el-table-column prop="monthly_limit" label="月限" width="90" />
      <el-table-column label="操作" width="100">
        <template #default="{ row }">
          <el-button link type="danger" @click="removeToken(row.id)">删除</el-button>
        </template>
      </el-table-column>
    </el-table>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { searchPoolApi } from '@/features/search-pool/api'
import type { SearchPoolToken, SearchService } from '@/features/search-pool/types'

const service = ref<SearchService>('tavily')
const name = ref('')
const tokens = ref<SearchPoolToken[]>([])

const loadTokens = async () => {
  tokens.value = await searchPoolApi.listTokens(service.value)
}

const createToken = async () => {
  await searchPoolApi.createToken({ service: service.value, name: name.value.trim() || 'default' })
  name.value = ''
  ElMessage.success('新增成功')
  await loadTokens()
}

const removeToken = async (tokenId: string) => {
  await searchPoolApi.deleteToken(tokenId)
  ElMessage.success('已删除')
  await loadTokens()
}

onMounted(loadTokens)
</script>

<style scoped>
.search-pool-tokens {
  display: grid;
  gap: 16px;
}
</style>
