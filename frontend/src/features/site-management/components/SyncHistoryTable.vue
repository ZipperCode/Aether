<template>
  <div>
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead class="w-[40px]" />
          <TableHead>时间</TableHead>
          <TableHead>来源</TableHead>
          <TableHead>状态</TableHead>
          <TableHead>总账号</TableHead>
          <TableHead>Dry-run</TableHead>
          <TableHead>耗时</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        <template
          v-for="run in runs"
          :key="run.id"
        >
          <TableRow
            class="cursor-pointer"
            @click="toggleExpand(run.id)"
          >
            <TableCell>
              <span class="text-xs text-muted-foreground">
                {{ expandedRunId === run.id ? '&#9660;' : '&#9654;' }}
              </span>
            </TableCell>
            <TableCell>{{ formatDate(run.created_at) }}</TableCell>
            <TableCell>{{ run.trigger_source }}</TableCell>
            <TableCell>
              <Badge :variant="run.status === 'success' ? 'default' : run.status === 'failed' ? 'destructive' : 'outline'">
                {{ statusLabel(run.status) }}
              </Badge>
            </TableCell>
            <TableCell>{{ run.total_accounts }}</TableCell>
            <TableCell>
              <Badge :variant="run.dry_run ? 'warning' : 'outline'">
                {{ run.dry_run ? '是' : '否' }}
              </Badge>
            </TableCell>
            <TableCell>{{ formatDuration(run.started_at, run.finished_at) }}</TableCell>
          </TableRow>
          <TableRow v-if="expandedRunId === run.id">
            <TableCell :colspan="7">
              <div class="py-2 px-4">
                <div
                  v-if="itemsLoading"
                  class="flex justify-center py-4"
                >
                  <Loader2 class="w-5 h-5 animate-spin text-muted-foreground" />
                </div>
                <template v-else-if="items.length > 0">
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>域名</TableHead>
                        <TableHead>站点</TableHead>
                        <TableHead>状态</TableHead>
                        <TableHead>消息</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      <TableRow
                        v-for="item in items"
                        :key="item.id"
                      >
                        <TableCell>{{ item.domain }}</TableCell>
                        <TableCell>{{ item.site_url || '-' }}</TableCell>
                        <TableCell>
                          <Badge :variant="item.status === 'success' ? 'default' : item.status === 'skipped' ? 'outline' : 'destructive'">
                            {{ item.status }}
                          </Badge>
                        </TableCell>
                        <TableCell class="max-w-[300px] truncate">
                          {{ item.message || '-' }}
                        </TableCell>
                      </TableRow>
                    </TableBody>
                  </Table>
                  <Pagination
                    v-if="itemsTotal > itemsPageSize"
                    :current="itemsPage"
                    :total="itemsTotal"
                    :page-size="itemsPageSize"
                    :show-page-size-selector="false"
                    class="mt-2"
                    @update:current="itemsPage = $event; loadItems(run.id)"
                  />
                </template>
                <div
                  v-else
                  class="text-sm text-muted-foreground text-center py-4"
                >
                  没有同步明细数据
                </div>
              </div>
            </TableCell>
          </TableRow>
        </template>
      </TableBody>
    </Table>

    <div
      v-if="runs.length === 0 && !loading"
      class="text-center py-8 text-sm text-muted-foreground"
    >
      暂无同步记录
    </div>

    <Pagination
      v-if="total > pageSize"
      :current="page"
      :total="total"
      :page-size="pageSize"
      @update:current="page = $event; loadRuns()"
      @update:page-size="pageSize = $event; page = 1; loadRuns()"
    />
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { Loader2 } from 'lucide-vue-next'
import {
  Table, TableHeader, TableBody, TableRow, TableHead, TableCell,
  Badge, Pagination,
} from '@/components/ui'
import { siteManagementApi } from '../api'
import { useToast } from '@/composables/useToast'
import { parseApiError } from '@/utils/errorParser'
import type { SyncRun, SyncItem } from '../types'

const { error } = useToast()

const loading = ref(false)
const runs = ref<SyncRun[]>([])
const total = ref(0)
const page = ref(1)
const pageSize = ref(20)

const expandedRunId = ref<string | null>(null)
const itemsLoading = ref(false)
const items = ref<SyncItem[]>([])
const itemsTotal = ref(0)
const itemsPage = ref(1)
const itemsPageSize = 50

async function loadRuns() {
  loading.value = true
  try {
    const result = await siteManagementApi.listSyncRuns({
      page: page.value,
      page_size: pageSize.value,
    })
    runs.value = result.items
    total.value = result.total
  } catch (err) {
    error(parseApiError(err, '加载同步记录失败'))
  } finally {
    loading.value = false
  }
}

async function loadItems(runId: string) {
  itemsLoading.value = true
  try {
    const result = await siteManagementApi.getSyncRunItems(runId, {
      page: itemsPage.value,
      page_size: itemsPageSize,
    })
    items.value = result.items
    itemsTotal.value = result.total
  } catch (err) {
    error(parseApiError(err, '加载同步明细失败'))
  } finally {
    itemsLoading.value = false
  }
}

function toggleExpand(runId: string) {
  if (expandedRunId.value === runId) {
    expandedRunId.value = null
    return
  }
  expandedRunId.value = runId
  itemsPage.value = 1
  items.value = []
  loadItems(runId)
}

function formatDate(value: string | null): string {
  if (!value) return '-'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}

function formatDuration(start: string | null, end: string | null): string {
  if (!start || !end) return '-'
  const ms = new Date(end).getTime() - new Date(start).getTime()
  if (ms < 1000) return `${ms}ms`
  return `${(ms / 1000).toFixed(1)}s`
}

function statusLabel(status: string): string {
  const map: Record<string, string> = {
    success: '成功',
    failed: '失败',
    running: '运行中',
    pending: '等待中',
  }
  return map[status] || status
}

onMounted(() => {
  loadRuns()
})
</script>
