<template>
  <PageContainer>
    <PageHeader
      title="Tavily 账号池"
      description="管理 Tavily 账号与令牌"
    >
      <template #actions>
        <Button
          variant="outline"
          @click="$router.push({ name: 'TavilyHealthHistory' })"
        >
          健康记录
        </Button>
        <Button
          variant="outline"
          @click="$router.push({ name: 'TavilyMaintenanceHistory' })"
        >
          维护记录
        </Button>
        <Button @click="openCreateDialog = true">
          新增账号
        </Button>
      </template>
    </PageHeader>

    <div class="mt-6 space-y-4">
      <div class="flex gap-2">
        <Button
          variant="outline"
          :disabled="runningHealth"
          @click="handleRunHealthCheck"
        >
          {{ runningHealth ? '执行中...' : '执行健康检查' }}
        </Button>
        <Button
          variant="outline"
          :disabled="runningMaintenance"
          @click="handleRunMaintenance"
        >
          {{ runningMaintenance ? '执行中...' : '执行维护任务' }}
        </Button>
      </div>

      <div class="rounded-lg border">
        <table class="w-full text-sm">
          <thead class="bg-muted/50">
            <tr class="text-left">
              <th class="px-4 py-2">
                邮箱
              </th>
              <th class="px-4 py-2">
                状态
              </th>
              <th class="px-4 py-2">
                来源
              </th>
              <th class="px-4 py-2">
                操作
              </th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="item in accounts"
              :key="item.id"
              class="border-t"
            >
              <td class="px-4 py-2">
                {{ item.email }}
              </td>
              <td class="px-4 py-2">
                {{ item.status }}
              </td>
              <td class="px-4 py-2">
                {{ item.source }}
              </td>
              <td class="px-4 py-2">
                <Button
                  variant="ghost"
                  @click="$router.push({ name: 'TavilyPoolDetail', params: { accountId: item.id } })"
                >
                  查看详情
                </Button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div
      v-if="openCreateDialog"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/30"
    >
      <div class="w-full max-w-md rounded-lg bg-background border p-4 space-y-3">
        <h3 class="text-base font-medium">
          新增账号
        </h3>
        <input
          v-model="form.email"
          class="w-full rounded border px-3 py-2"
          placeholder="邮箱"
        >
        <input
          v-model="form.password"
          class="w-full rounded border px-3 py-2"
          placeholder="密码"
        >
        <input
          v-model="form.notes"
          class="w-full rounded border px-3 py-2"
          placeholder="备注（可选）"
        >
        <div class="flex justify-end gap-2">
          <Button
            variant="outline"
            @click="openCreateDialog = false"
          >
            取消
          </Button>
          <Button @click="handleCreateAccount">
            创建
          </Button>
        </div>
      </div>
    </div>
  </PageContainer>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { Button } from '@/components/ui'
import { PageContainer, PageHeader } from '@/components/layout'
import { tavilyPoolApi } from '@/features/tavily-pool/api'
import type { TavilyAccount } from '@/features/tavily-pool/types'
import { useToast } from '@/composables/useToast'
import { parseApiError } from '@/utils/errorParser'

const { success, error } = useToast()

const accounts = ref<TavilyAccount[]>([])
const runningHealth = ref(false)
const runningMaintenance = ref(false)
const openCreateDialog = ref(false)
const form = reactive({
  email: '',
  password: '',
  notes: ''
})

async function loadAccounts() {
  try {
    accounts.value = await tavilyPoolApi.listAccounts()
  } catch (err) {
    error(parseApiError(err, '加载 Tavily 账号失败'))
  }
}

async function handleCreateAccount() {
  try {
    await tavilyPoolApi.createAccount({
      email: form.email,
      password: form.password,
      notes: form.notes || undefined,
      source: 'manual'
    })
    success('账号创建成功')
    openCreateDialog.value = false
    form.email = ''
    form.password = ''
    form.notes = ''
    await loadAccounts()
  } catch (err) {
    error(parseApiError(err, '创建账号失败'))
  }
}

async function handleRunHealthCheck() {
  runningHealth.value = true
  try {
    const result = await tavilyPoolApi.runHealthCheck()
    success(`健康检查完成：成功 ${result.success}，失败 ${result.failed}`)
  } catch (err) {
    error(parseApiError(err, '健康检查执行失败'))
  } finally {
    runningHealth.value = false
  }
}

async function handleRunMaintenance() {
  runningMaintenance.value = true
  try {
    const result = await tavilyPoolApi.runMaintenance()
    success(`维护任务完成：处理 ${result.total} 条`)
  } catch (err) {
    error(parseApiError(err, '维护任务执行失败'))
  } finally {
    runningMaintenance.value = false
  }
}

onMounted(() => {
  loadAccounts()
})
</script>
