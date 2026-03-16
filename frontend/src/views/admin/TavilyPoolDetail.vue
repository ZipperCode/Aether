<template>
  <PageContainer>
    <PageHeader
      title="Tavily 账号详情"
      description="查看并管理账号令牌"
    >
      <template #actions>
        <Button
          variant="outline"
          @click="$router.push({ name: 'TavilyPool' })"
        >
          返回列表
        </Button>
      </template>
    </PageHeader>

    <div class="mt-6 space-y-3">
      <div class="flex gap-2">
        <input
          v-model="newToken"
          class="w-full max-w-md rounded border px-3 py-2"
          placeholder="输入新 API Key"
        >
        <Button @click="handleCreateToken">
          新增 API Key
        </Button>
      </div>

      <div class="rounded-lg border">
        <table class="w-full text-sm">
          <thead class="bg-muted/50">
            <tr class="text-left">
              <th class="px-4 py-2">
                API Key
              </th>
              <th class="px-4 py-2">
                状态
              </th>
              <th class="px-4 py-2">
                连续失败
              </th>
              <th class="px-4 py-2">
                最近检查
              </th>
              <th class="px-4 py-2">
                创建时间
              </th>
              <th class="px-4 py-2">
                操作
              </th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="item in tokens"
              :key="item.id"
              class="border-t"
            >
              <td class="px-4 py-2">
                {{ item.token_masked }}
              </td>
              <td class="px-4 py-2">
                {{ item.is_active ? 'active' : 'inactive' }}
              </td>
              <td class="px-4 py-2">
                {{ item.consecutive_fail_count }}
              </td>
              <td class="px-4 py-2">
                {{ item.last_checked_at || '-' }}
              </td>
              <td class="px-4 py-2">
                {{ item.created_at }}
              </td>
              <td class="px-4 py-2">
                <Button
                  variant="ghost"
                  :disabled="item.is_active"
                  @click="handleActivate(item.id)"
                >
                  激活
                </Button>
                <Button
                  variant="ghost"
                  class="ml-2"
                  @click="handleDelete(item.id)"
                >
                  删除
                </Button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </PageContainer>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { Button } from '@/components/ui'
import { PageContainer, PageHeader } from '@/components/layout'
import { tavilyPoolApi } from '@/features/tavily-pool/api'
import type { TavilyToken } from '@/features/tavily-pool/types'
import { useToast } from '@/composables/useToast'
import { parseApiError } from '@/utils/errorParser'

const route = useRoute()
const { success, error } = useToast()
const accountId = String(route.params.accountId || '')

const tokens = ref<TavilyToken[]>([])
const newToken = ref('')

async function loadTokens() {
  try {
    tokens.value = await tavilyPoolApi.listTokens(accountId)
  } catch (err) {
    error(parseApiError(err, '加载 API Key 失败'))
  }
}

async function handleCreateToken() {
  if (!newToken.value) return
  try {
    await tavilyPoolApi.createToken(accountId, newToken.value)
    success('API Key 创建成功')
    newToken.value = ''
    await loadTokens()
  } catch (err) {
    error(parseApiError(err, '创建 API Key 失败'))
  }
}

async function handleActivate(tokenId: string) {
  try {
    await tavilyPoolApi.activateToken(tokenId)
    success('API Key 已激活')
    await loadTokens()
  } catch (err) {
    error(parseApiError(err, '激活失败'))
  }
}

async function handleDelete(tokenId: string) {
  try {
    await tavilyPoolApi.deleteToken(tokenId)
    success('API Key 已删除')
    await loadTokens()
  } catch (err) {
    error(parseApiError(err, '删除失败'))
  }
}

onMounted(() => {
  loadTokens()
})
</script>
