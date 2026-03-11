<template>
  <PageContainer>
    <PageHeader
      title="WebDav 源管理"
      description="管理 WebDav 备份源，同步站点账号数据"
    >
      <template #actions>
        <Button
          variant="outline"
          @click="$router.push({ name: 'SiteSyncHistory' })"
        >
          同步记录
        </Button>
        <Button
          variant="outline"
          @click="$router.push({ name: 'SiteCheckinHistory' })"
        >
          签到记录
        </Button>
        <Button @click="openCreateDialog">
          添加源
        </Button>
      </template>
    </PageHeader>

    <div class="mt-6">
      <div
        v-if="loading"
        class="flex justify-center py-12"
      >
        <Loader2 class="w-6 h-6 animate-spin text-muted-foreground" />
      </div>

      <div
        v-else-if="sources.length === 0"
        class="text-center py-16 text-muted-foreground"
      >
        <p class="text-lg font-medium">
          暂无 WebDav 源
        </p>
        <p class="text-sm mt-2">
          点击「添加源」来添加第一个 WebDav 备份源
        </p>
        <Button
          class="mt-4"
          @click="openCreateDialog"
        >
          添加源
        </Button>
      </div>

      <div
        v-else
        class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3"
      >
        <WebDavSourceCard
          v-for="source in sources"
          :key="source.id"
          :source="source"
          :sync-loading="syncLoadingMap[source.id]"
          @click="$router.push({ name: 'SiteSourceDetail', params: { sourceId: source.id } })"
          @sync="handleSync(source)"
          @edit="openEditDialog(source)"
          @delete="handleDelete(source)"
        />
      </div>
    </div>

    <WebDavSourceFormDialog
      :open="formDialogOpen"
      :source="editingSource"
      @update:open="formDialogOpen = $event"
      @saved="handleSaved"
    />
  </PageContainer>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { Loader2 } from 'lucide-vue-next'
import { PageContainer, PageHeader } from '@/components/layout'
import { Button } from '@/components/ui'
import { siteManagementApi } from '@/features/site-management/api'
import type { WebDavSource } from '@/features/site-management/types'
import WebDavSourceCard from '@/features/site-management/components/WebDavSourceCard.vue'
import WebDavSourceFormDialog from '@/features/site-management/components/WebDavSourceFormDialog.vue'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'
import { parseApiError } from '@/utils/errorParser'

const { success, error } = useToast()
const { confirmDanger } = useConfirm()

const loading = ref(false)
const sources = ref<WebDavSource[]>([])
const syncLoadingMap = ref<Record<string, boolean>>({})
const formDialogOpen = ref(false)
const editingSource = ref<WebDavSource | null>(null)

async function loadSources() {
  loading.value = true
  try {
    sources.value = await siteManagementApi.listSources()
  } catch (err) {
    error(parseApiError(err, '加载 WebDav 源失败'))
  } finally {
    loading.value = false
  }
}

function openCreateDialog() {
  editingSource.value = null
  formDialogOpen.value = true
}

function openEditDialog(source: WebDavSource) {
  editingSource.value = source
  formDialogOpen.value = true
}

function handleSaved() {
  loadSources()
}

async function handleSync(source: WebDavSource) {
  syncLoadingMap.value = { ...syncLoadingMap.value, [source.id]: true }
  try {
    await siteManagementApi.syncSource(source.id)
    success(`${source.name} 同步已触发`)
    await loadSources()
  } catch (err) {
    error(parseApiError(err, '同步失败'))
  } finally {
    syncLoadingMap.value = { ...syncLoadingMap.value, [source.id]: false }
  }
}

async function handleDelete(source: WebDavSource) {
  const confirmed = await confirmDanger(
    `确定要删除 WebDav 源「${source.name}」吗？关联的账号数据也会被清除。`,
    '删除 WebDav 源',
  )
  if (!confirmed) return

  try {
    await siteManagementApi.deleteSource(source.id)
    success('WebDav 源已删除')
    await loadSources()
  } catch (err) {
    error(parseApiError(err, '删除失败'))
  }
}

onMounted(() => {
  loadSources()
})
</script>
