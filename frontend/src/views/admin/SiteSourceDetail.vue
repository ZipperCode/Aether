<template>
  <PageContainer>
    <div class="mb-4">
      <Button
        variant="ghost"
        size="sm"
        @click="$router.push({ name: 'SiteManagement' })"
      >
        &larr; 返回源列表
      </Button>
    </div>

    <div
      v-if="sourceLoading"
      class="flex justify-center py-12"
    >
      <Loader2 class="w-6 h-6 animate-spin text-muted-foreground" />
    </div>

    <template v-else-if="source">
      <PageHeader
        :title="source.name"
        :description="source.url"
      >
        <template #actions>
          <Badge :variant="source.is_active ? 'default' : 'outline'">
            {{ source.is_active ? '启用' : '停用' }}
          </Badge>
          <Badge
            v-if="source.last_sync_status"
            :variant="source.last_sync_status === 'success' ? 'default' : 'destructive'"
          >
            {{ source.last_sync_status === 'success' ? '同步成功' : '同步失败' }}
          </Badge>
          <Button
            variant="outline"
            :disabled="syncLoading"
            @click="handleSync"
          >
            <Loader2
              v-if="syncLoading"
              class="w-4 h-4 mr-1 animate-spin"
            />
            同步
          </Button>
        </template>
      </PageHeader>

      <Card class="mt-6 overflow-hidden">
        <div class="p-4">
          <div
            v-if="accountsLoading && accounts.length === 0"
            class="flex justify-center py-8"
          >
            <Loader2 class="w-6 h-6 animate-spin text-muted-foreground" />
          </div>
          <AccountTable
            v-else
            :source-id="sourceId"
            :accounts="accounts"
            :total="accountTotal"
            :page="page"
            :page-size="pageSize"
            :loading="accountsLoading"
            @update:page="page = $event; loadAccounts()"
            @update:page-size="pageSize = $event; page = 1; loadAccounts()"
            @refresh="loadAccounts"
            @view-detail="openDetail"
          />
        </div>
      </Card>
    </template>

    <div
      v-else
      class="text-center py-16 text-muted-foreground"
    >
      <p>未找到 WebDav 源</p>
    </div>

    <AccountDetailDrawer
      :open="detailOpen"
      :account="selectedAccount"
      @update:open="detailOpen = $event"
    />
  </PageContainer>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { Loader2 } from 'lucide-vue-next'
import { PageContainer, PageHeader } from '@/components/layout'
import { Button, Badge, Card } from '@/components/ui'
import { siteManagementApi } from '@/features/site-management/api'
import type { WebDavSource, SiteAccount } from '@/features/site-management/types'
import AccountTable from '@/features/site-management/components/AccountTable.vue'
import AccountDetailDrawer from '@/features/site-management/components/AccountDetailDrawer.vue'
import { useToast } from '@/composables/useToast'
import { parseApiError } from '@/utils/errorParser'

const route = useRoute()
const sourceId = route.params.sourceId as string

const { success, error } = useToast()

const sourceLoading = ref(false)
const source = ref<WebDavSource | null>(null)
const syncLoading = ref(false)

const accountsLoading = ref(false)
const accounts = ref<SiteAccount[]>([])
const accountTotal = ref(0)
const page = ref(1)
const pageSize = ref(20)

const detailOpen = ref(false)
const selectedAccount = ref<SiteAccount | null>(null)

async function loadSource() {
  sourceLoading.value = true
  try {
    const sources = await siteManagementApi.listSources()
    source.value = sources.find(s => s.id === sourceId) || null
  } catch (err) {
    error(parseApiError(err, '加载源信息失败'))
  } finally {
    sourceLoading.value = false
  }
}

async function loadAccounts() {
  accountsLoading.value = true
  try {
    const result = await siteManagementApi.listAccounts(sourceId, {
      page: page.value,
      page_size: pageSize.value,
    })
    accounts.value = result.items
    accountTotal.value = result.total
  } catch (err) {
    error(parseApiError(err, '加载账号列表失败'))
  } finally {
    accountsLoading.value = false
  }
}

async function handleSync() {
  syncLoading.value = true
  try {
    await siteManagementApi.syncSource(sourceId)
    success('同步已触发')
    await Promise.all([loadSource(), loadAccounts()])
  } catch (err) {
    error(parseApiError(err, '同步失败'))
  } finally {
    syncLoading.value = false
  }
}

function openDetail(account: SiteAccount) {
  selectedAccount.value = account
  detailOpen.value = true
}

onMounted(() => {
  loadSource()
  loadAccounts()
})
</script>
