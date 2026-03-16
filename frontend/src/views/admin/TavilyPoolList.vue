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
        <Button
          variant="outline"
          @click="openImportDialog = true"
        >
          批量导入
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
        <Button
          variant="outline"
          :disabled="runningUsageSync"
          @click="handleSyncUsage"
        >
          {{ runningUsageSync ? '同步中...' : '同步额度' }}
        </Button>
        <Button
          variant="outline"
          :disabled="loadingPoolStats"
          @click="loadPoolStats"
        >
          {{ loadingPoolStats ? '刷新中...' : '刷新统计' }}
        </Button>
      </div>

      <div class="grid gap-3 md:grid-cols-5">
        <div class="rounded-lg border p-3">
          <div class="text-xs text-muted-foreground">
            总请求
          </div>
          <div class="text-lg font-semibold">
            {{ poolStats.total_requests }}
          </div>
        </div>
        <div class="rounded-lg border p-3">
          <div class="text-xs text-muted-foreground">
            成功
          </div>
          <div class="text-lg font-semibold">
            {{ poolStats.success_requests }}
          </div>
        </div>
        <div class="rounded-lg border p-3">
          <div class="text-xs text-muted-foreground">
            失败
          </div>
          <div class="text-lg font-semibold">
            {{ poolStats.failed_requests }}
          </div>
        </div>
        <div class="rounded-lg border p-3">
          <div class="text-xs text-muted-foreground">
            成功率
          </div>
          <div class="text-lg font-semibold">
            {{ Math.round(poolStats.success_rate * 10000) / 100 }}%
          </div>
        </div>
        <div class="rounded-lg border p-3">
          <div class="text-xs text-muted-foreground">
            平均延迟
          </div>
          <div class="text-lg font-semibold">
            {{ poolStats.avg_latency_ms }}ms
          </div>
        </div>
      </div>

      <div class="rounded-lg border p-4 space-y-3">
        <div class="text-sm font-medium">
          池调试操作
        </div>
        <div class="flex flex-wrap gap-2">
          <Button
            variant="outline"
            :disabled="leasingToken"
            @click="handleLeaseToken"
          >
            {{ leasingToken ? '租约中...' : '租约一个 API Key' }}
          </Button>
          <Button
            variant="outline"
            :disabled="!leasedTokenId || reportingResult"
            @click="handleReportResult(true)"
          >
            上报成功
          </Button>
          <Button
            variant="outline"
            :disabled="!leasedTokenId || reportingResult"
            @click="handleReportResult(false)"
          >
            上报失败
          </Button>
        </div>
        <div class="text-xs text-muted-foreground">
          当前租约: {{ leasedTokenMasked || '无' }}
        </div>
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
                健康
              </th>
              <th class="px-4 py-2">
                连续失败
              </th>
              <th class="px-4 py-2">
                额度
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
                {{ item.health_status }}
              </td>
              <td class="px-4 py-2">
                {{ item.fail_count }}
              </td>
              <td class="px-4 py-2">
                {{ renderUsage(item) }}
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
                <Button
                  variant="ghost"
                  class="ml-2"
                  @click="handleToggleStatus(item)"
                >
                  {{ item.status === 'active' ? '禁用' : '启用' }}
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
          v-model="form.apiKey"
          class="w-full rounded border px-3 py-2"
          placeholder="API Key（可选）"
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

    <TavilyAccountImportDialog
      :open="openImportDialog"
      :submitting="importingAccounts"
      :file-type="importFileType"
      :merge-mode="importMergeMode"
      @update:open="openImportDialog = $event"
      @update:file-type="importFileType = $event"
      @update:merge-mode="importMergeMode = $event"
      @submit="handleImportAccounts"
    />
  </PageContainer>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { Button } from '@/components/ui'
import { PageContainer, PageHeader } from '@/components/layout'
import { tavilyPoolApi } from '@/features/tavily-pool/api'
import type { TavilyAccount, TavilyImportFileType, TavilyImportMergeMode, TavilyPoolStats } from '@/features/tavily-pool/types'
import TavilyAccountImportDialog from '@/features/tavily-pool/components/TavilyAccountImportDialog.vue'
import { useToast } from '@/composables/useToast'
import { parseApiError } from '@/utils/errorParser'

const { success, error } = useToast()

const accounts = ref<TavilyAccount[]>([])
const runningHealth = ref(false)
const runningMaintenance = ref(false)
const runningUsageSync = ref(false)
const loadingPoolStats = ref(false)
const leasingToken = ref(false)
const reportingResult = ref(false)
const openCreateDialog = ref(false)
const openImportDialog = ref(false)
const importingAccounts = ref(false)
const importFileType = ref<TavilyImportFileType>('json')
const importMergeMode = ref<TavilyImportMergeMode>('skip')
const leasedTokenId = ref('')
const leasedTokenMasked = ref('')
const poolStats = ref<TavilyPoolStats>({
  total_requests: 0,
  success_requests: 0,
  failed_requests: 0,
  success_rate: 0,
  avg_latency_ms: 0
})
const form = reactive({
  email: '',
  password: '',
  apiKey: '',
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
      api_key: form.apiKey || undefined,
      notes: form.notes || undefined,
      source: 'manual'
    })
    success('账号创建成功')
    openCreateDialog.value = false
    form.email = ''
    form.password = ''
    form.apiKey = ''
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

function renderUsage(account: TavilyAccount): string {
  if (account.usage_account_limit == null || account.usage_account_used == null) {
    return account.usage_sync_error ? '同步失败' : '未同步'
  }
  return `${account.usage_account_used}/${account.usage_account_limit}`
}

async function handleSyncUsage() {
  runningUsageSync.value = true
  try {
    const result = await tavilyPoolApi.syncUsage()
    success(`额度同步完成：成功 ${result.synced_accounts}，失败 ${result.failed_accounts}`)
    await loadAccounts()
  } catch (err) {
    error(parseApiError(err, '额度同步失败'))
  } finally {
    runningUsageSync.value = false
  }
}

async function handleToggleStatus(item: TavilyAccount) {
  const nextStatus = item.status === 'active' ? 'disabled' : 'active'
  try {
    await tavilyPoolApi.updateAccountStatus(item.id, nextStatus)
    success(`账号已${nextStatus === 'active' ? '启用' : '禁用'}`)
    await loadAccounts()
  } catch (err) {
    error(parseApiError(err, '更新账号状态失败'))
  }
}

async function loadPoolStats() {
  loadingPoolStats.value = true
  try {
    poolStats.value = await tavilyPoolApi.getPoolStatsOverview()
  } catch (err) {
    error(parseApiError(err, '加载池统计失败'))
  } finally {
    loadingPoolStats.value = false
  }
}

async function handleLeaseToken() {
  leasingToken.value = true
  try {
    const lease = await tavilyPoolApi.leasePoolToken()
    leasedTokenId.value = lease.token_id
    leasedTokenMasked.value = lease.token_masked
    success(`已租约 API Key：${lease.token_masked}`)
  } catch (err) {
    error(parseApiError(err, '租约失败'))
  } finally {
    leasingToken.value = false
  }
}

async function handleReportResult(successFlag: boolean) {
  if (!leasedTokenId.value) {
    return
  }
  reportingResult.value = true
  try {
    const resp = await tavilyPoolApi.reportPoolResult({
      token_id: leasedTokenId.value,
      success: successFlag,
      endpoint: '/admin/manual',
      latency_ms: successFlag ? 80 : 400,
      error_message: successFlag ? undefined : 'manual failed report'
    })
    success(`上报完成：${resp.success ? '成功' : '失败'}，连续失败 ${resp.consecutive_fail_count}`)
    await loadPoolStats()
    await loadAccounts()
  } catch (err) {
    error(parseApiError(err, '上报结果失败'))
  } finally {
    reportingResult.value = false
  }
}

async function handleImportAccounts(payload: {
  file_type: TavilyImportFileType
  merge_mode: TavilyImportMergeMode
  content: string
}) {
  importingAccounts.value = true
  try {
    const result = await tavilyPoolApi.importAccounts(payload)
    success(
      `导入完成：创建 ${result.stats.created}，更新 ${result.stats.updated}，跳过 ${result.stats.skipped}，失败 ${result.stats.failed}`
    )
    if (result.errors.length > 0) {
      error(`存在 ${result.errors.length} 条错误，请检查导入内容`)
    }
    openImportDialog.value = false
    await loadAccounts()
  } catch (err) {
    error(parseApiError(err, '批量导入失败'))
  } finally {
    importingAccounts.value = false
  }
}

onMounted(() => {
  loadAccounts()
  loadPoolStats()
})
</script>
