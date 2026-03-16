<template>
  <PageContainer>
    <PageHeader
      title="Tavily 健康检查记录"
      description="查看最近健康检查执行结果"
    >
      <template #actions>
        <Button
          variant="outline"
          @click="$router.push({ name: 'TavilyPool' })"
        >
          返回账号池
        </Button>
      </template>
    </PageHeader>

    <div class="mt-6 rounded-lg border">
      <table class="w-full text-sm">
        <thead class="bg-muted/50">
          <tr class="text-left">
            <th class="px-4 py-2">
              账号ID
            </th>
            <th class="px-4 py-2">
              类型
            </th>
            <th class="px-4 py-2">
              状态
            </th>
            <th class="px-4 py-2">
              错误信息
            </th>
            <th class="px-4 py-2">
              时间
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="item in items"
            :key="item.id"
            class="border-t"
          >
            <td class="px-4 py-2">
              {{ item.account_id || '-' }}
            </td>
            <td class="px-4 py-2">
              {{ item.check_type }}
            </td>
            <td class="px-4 py-2">
              {{ item.status }}
            </td>
            <td class="px-4 py-2">
              {{ item.error_message || '-' }}
            </td>
            <td class="px-4 py-2">
              {{ item.checked_at }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </PageContainer>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { Button } from '@/components/ui'
import { PageContainer, PageHeader } from '@/components/layout'
import { tavilyPoolApi } from '@/features/tavily-pool/api'
import type { TavilyHealthCheck } from '@/features/tavily-pool/types'
import { useToast } from '@/composables/useToast'
import { parseApiError } from '@/utils/errorParser'

const { error } = useToast()
const items = ref<TavilyHealthCheck[]>([])

async function loadData() {
  try {
    items.value = await tavilyPoolApi.listHealthChecks()
  } catch (err) {
    error(parseApiError(err, '加载健康检查记录失败'))
  }
}

onMounted(() => {
  loadData()
})
</script>
