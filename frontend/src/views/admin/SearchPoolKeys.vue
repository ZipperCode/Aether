<template>
  <div class="search-pool-keys">
    <h1>搜索池密钥管理</h1>
    <el-form :inline="true">
      <el-form-item label="服务">
        <el-select v-model="service" style="width: 180px">
          <el-option label="Tavily" value="tavily" />
          <el-option label="Firecrawl" value="firecrawl" />
        </el-select>
      </el-form-item>
      <el-form-item label="Key">
        <el-input v-model="newKey" placeholder="输入 key" style="width: 320px" />
      </el-form-item>
      <el-form-item>
        <el-button type="primary" @click="createKey">新增</el-button>
        <el-button @click="loadKeys">刷新</el-button>
      </el-form-item>
    </el-form>

    <el-table :data="keys" border>
      <el-table-column prop="service" label="服务" width="120" />
      <el-table-column prop="key_masked" label="密钥" />
      <el-table-column prop="email" label="邮箱" width="220" />
      <el-table-column label="状态" width="120">
        <template #default="{ row }">
          <el-tag :type="row.active ? 'success' : 'info'">{{ row.active ? '启用' : '停用' }}</el-tag>
        </template>
      </el-table-column>
      <el-table-column label="操作" width="180">
        <template #default="{ row }">
          <el-button link type="primary" @click="toggle(row)">{{ row.active ? '停用' : '启用' }}</el-button>
          <el-button link type="danger" @click="removeKey(row.id)">删除</el-button>
        </template>
      </el-table-column>
    </el-table>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { searchPoolApi } from '@/features/search-pool/api'
import type { SearchPoolKey, SearchService } from '@/features/search-pool/types'

const service = ref<SearchService>('tavily')
const newKey = ref('')
const keys = ref<SearchPoolKey[]>([])

const loadKeys = async () => {
  keys.value = await searchPoolApi.listKeys(service.value)
}

const createKey = async () => {
  if (!newKey.value.trim()) return
  await searchPoolApi.createKey({ service: service.value, key: newKey.value.trim() })
  newKey.value = ''
  ElMessage.success('新增成功')
  await loadKeys()
}

const toggle = async (row: SearchPoolKey) => {
  await searchPoolApi.toggleKey(row.id, !row.active)
  await loadKeys()
}

const removeKey = async (id: string) => {
  await searchPoolApi.deleteKey(id)
  ElMessage.success('已删除')
  await loadKeys()
}

onMounted(loadKeys)
</script>

<style scoped>
.search-pool-keys {
  display: grid;
  gap: 16px;
}
</style>
