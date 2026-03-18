<template>
  <PageContainer max-width="full">
    <PageHeader :title="workspace?.title || '搜索池工作台'" :description="workspace?.description || '管理当前服务的搜索池、Token 与调用方式'">
      <template #actions>
        <div class="flex flex-wrap items-center gap-2">
          <Button variant="outline" @click="goBack">
            <ArrowLeft class="mr-1.5 h-4 w-4" />返回总览
          </Button>
          <Button variant="outline" :disabled="syncing" @click="runSync">
            <RefreshCcw class="mr-1.5 h-4 w-4" :class="{ 'animate-spin': syncing }" />同步额度
          </Button>
          <Button :disabled="loading" @click="loadWorkspace">
            <RefreshCcw class="mr-1.5 h-4 w-4" :class="{ 'animate-spin': loading }" />刷新
          </Button>
        </div>
      </template>
    </PageHeader>

    <div class="mt-6 space-y-6">
      <SearchPoolStatsGrid v-if="workspace" :items="statItems" />

      <SearchPoolRouteExamples
        v-if="workspace"
        :base-url="workspace.usage_examples.base_url"
        :curl-examples="workspace.usage_examples.curl_examples"
      />

      <SearchPoolTokenTable
        v-if="workspace"
        :tokens="workspace.tokens"
        @create="openCreateToken"
        @edit="openEditToken"
        @delete="handleDeleteToken"
      />

      <SearchPoolKeyTable
        v-if="workspace"
        :keys="workspace.keys"
        @create="handleCreateKey"
        @import="importDialogOpen = true"
        @toggle="handleToggleKey"
        @delete="handleDeleteKey"
      />
    </div>

    <SearchPoolTokenDialog
      :open="tokenDialogOpen"
      :mode="tokenDialogMode"
      :token="editingToken"
      :submitting="tokenSubmitting"
      @update:open="tokenDialogOpen = $event"
      @submit="handleSubmitToken"
    />

    <SearchPoolKeyImportDialog
      :open="importDialogOpen"
      :submitting="importSubmitting"
      @update:open="importDialogOpen = $event"
      @submit="handleImportKeys"
    />
  </PageContainer>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, RefreshCcw } from 'lucide-vue-next'
import { PageContainer, PageHeader } from '@/components/layout'
import { Button } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { searchPoolApi } from '@/features/search-pool/api'
import SearchPoolKeyImportDialog from '@/features/search-pool/components/SearchPoolKeyImportDialog.vue'
import SearchPoolKeyTable from '@/features/search-pool/components/SearchPoolKeyTable.vue'
import SearchPoolRouteExamples from '@/features/search-pool/components/SearchPoolRouteExamples.vue'
import SearchPoolStatsGrid from '@/features/search-pool/components/SearchPoolStatsGrid.vue'
import SearchPoolTokenDialog from '@/features/search-pool/components/SearchPoolTokenDialog.vue'
import SearchPoolTokenTable from '@/features/search-pool/components/SearchPoolTokenTable.vue'
import type { SearchPoolKey, SearchPoolService, SearchPoolToken, SearchPoolWorkspace } from '@/features/search-pool/types'
import { parseApiError } from '@/utils/errorParser'

const route = useRoute()
const router = useRouter()
const { success, error } = useToast()
const { confirmDanger } = useConfirm()

const workspace = ref<SearchPoolWorkspace | null>(null)
const loading = ref(false)
const syncing = ref(false)
const tokenSubmitting = ref(false)
const importSubmitting = ref(false)

const tokenDialogOpen = ref(false)
const tokenDialogMode = ref<'create' | 'edit'>('create')
const editingToken = ref<SearchPoolToken | null>(null)
const importDialogOpen = ref(false)

const currentService = computed<SearchPoolService>(() => {
  const service = String(route.params.service || 'tavily').toLowerCase()
  return service === 'firecrawl' ? 'firecrawl' : 'tavily'
})

const statItems = computed(() => {
  if (!workspace.value) return []
  const stats = workspace.value.stats
  return [
    { label: '真实总额度', value: stats.real_limit, description: '来自同步后的额度字段', tone: 'bg-emerald-400/80' },
    { label: '真实已用', value: stats.real_used, description: '按 key 聚合', tone: 'bg-sky-400/80' },
    { label: '真实剩余', value: stats.real_remaining, description: '当前最关键的剩余额度', tone: 'bg-emerald-500/80' },
    { label: 'Key 池状态', value: `${stats.keys_active} / ${stats.keys_total}`, description: '活跃 / 总数', tone: 'bg-violet-400/80' },
    { label: 'Token 数', value: stats.tokens_total, description: '当前工作台独立 Token 池', tone: 'bg-amber-400/80' },
    { label: '今日代理调用', value: stats.requests_today, description: '按 usage log 聚合', tone: 'bg-orange-400/80' },
    { label: '本月代理调用', value: stats.requests_this_month, description: '按 usage log 聚合', tone: 'bg-slate-500/80' },
    { label: '成功率', value: `${(stats.success_rate * 100).toFixed(2)}%`, description: `成功 ${stats.requests_success} / 失败 ${stats.requests_failed}`, tone: 'bg-teal-400/80' },
  ]
})

async function loadWorkspace() {
  loading.value = true
  try {
    workspace.value = await searchPoolApi.getWorkspace(currentService.value)
  } catch (err) {
    error(parseApiError(err, '加载工作台失败'))
  } finally {
    loading.value = false
  }
}

function goBack() {
  void router.push({ name: 'SearchPoolDashboard' })
}

async function runSync() {
  syncing.value = true
  try {
    const result = await searchPoolApi.syncUsage(currentService.value, true)
    success(`同步完成，共处理 ${result.synced_keys} 个 Key`)
    await loadWorkspace()
  } catch (err) {
    error(parseApiError(err, '同步额度失败'))
  } finally {
    syncing.value = false
  }
}

function openCreateToken() {
  editingToken.value = null
  tokenDialogMode.value = 'create'
  tokenDialogOpen.value = true
}

function openEditToken(token: SearchPoolToken) {
  editingToken.value = token
  tokenDialogMode.value = 'edit'
  tokenDialogOpen.value = true
}

async function handleSubmitToken(payload: { name: string; hourly_limit: number; daily_limit: number; monthly_limit: number }) {
  tokenSubmitting.value = true
  try {
    if (tokenDialogMode.value === 'edit' && editingToken.value) {
      await searchPoolApi.updateToken(editingToken.value.id, payload)
      success('Token 已更新')
    } else {
      await searchPoolApi.createToken({ service: currentService.value, ...payload })
      success('Token 已创建')
    }
    tokenDialogOpen.value = false
    await loadWorkspace()
  } catch (err) {
    error(parseApiError(err, tokenDialogMode.value === 'edit' ? '更新 Token 失败' : '创建 Token 失败'))
  } finally {
    tokenSubmitting.value = false
  }
}

async function handleDeleteToken(token: SearchPoolToken) {
  const confirmed = await confirmDanger(`确认删除 Token “${token.name || token.token.slice(0, 12)}” 吗？`, '删除 Token')
  if (!confirmed) return

  try {
    await searchPoolApi.deleteToken(token.id)
    success('Token 已删除')
    await loadWorkspace()
  } catch (err) {
    error(parseApiError(err, '删除 Token 失败'))
  }
}

async function handleCreateKey(payload: { key: string; email?: string }) {
  try {
    await searchPoolApi.createKey({ service: currentService.value, key: payload.key, email: payload.email })
    success('Key 已新增')
    await loadWorkspace()
  } catch (err) {
    error(parseApiError(err, '新增 Key 失败'))
  }
}

async function handleImportKeys(content: string) {
  importSubmitting.value = true
  try {
    const result = await searchPoolApi.importKeys({ service: currentService.value, content })
    success(`成功导入 ${result.created} 个 Key`)
    importDialogOpen.value = false
    await loadWorkspace()
  } catch (err) {
    error(parseApiError(err, '导入 Key 失败'))
  } finally {
    importSubmitting.value = false
  }
}

async function handleToggleKey(row: SearchPoolKey) {
  try {
    await searchPoolApi.toggleKey(row.id, !row.active)
    success(row.active ? 'Key 已停用' : 'Key 已启用')
    await loadWorkspace()
  } catch (err) {
    error(parseApiError(err, '更新 Key 状态失败'))
  }
}

async function handleDeleteKey(row: SearchPoolKey) {
  const confirmed = await confirmDanger(`确认删除 Key “${row.key_masked}” 吗？`, '删除 Key')
  if (!confirmed) return

  try {
    await searchPoolApi.deleteKey(row.id)
    success('Key 已删除')
    await loadWorkspace()
  } catch (err) {
    error(parseApiError(err, '删除 Key 失败'))
  }
}

watch(currentService, () => {
  void loadWorkspace()
})

onMounted(() => {
  void loadWorkspace()
})
</script>
