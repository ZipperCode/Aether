<template>
  <PageContainer>
    <PageHeader
      title="Tavily 维护记录"
      description="查看维护任务执行历史"
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
              任务
            </th>
            <th class="px-4 py-2">
              状态
            </th>
            <th class="px-4 py-2">
              总数
            </th>
            <th class="px-4 py-2">
              成功
            </th>
            <th class="px-4 py-2">
              失败
            </th>
            <th class="px-4 py-2">
              跳过
            </th>
            <th class="px-4 py-2">
              开始时间
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
              {{ item.job_name }}
            </td>
            <td class="px-4 py-2">
              {{ item.status }}
            </td>
            <td class="px-4 py-2">
              {{ item.total }}
            </td>
            <td class="px-4 py-2">
              {{ item.success }}
            </td>
            <td class="px-4 py-2">
              {{ item.failed }}
            </td>
            <td class="px-4 py-2">
              {{ item.skipped }}
            </td>
            <td class="px-4 py-2">
              {{ item.started_at }}
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
import type { TavilyMaintenanceRun } from '@/features/tavily-pool/types'
import { useToast } from '@/composables/useToast'
import { parseApiError } from '@/utils/errorParser'

const { error } = useToast()
const items = ref<TavilyMaintenanceRun[]>([])

async function loadData() {
  try {
    items.value = await tavilyPoolApi.listMaintenanceRuns()
  } catch (err) {
    error(parseApiError(err, '加载维护记录失败'))
  }
}

onMounted(() => {
  loadData()
})
</script>
