<template>
  <PageContainer>
    <PageHeader
      title="站点管理"
      description="查看 all-api-hub 同步差异、签到结果与历史记录"
    />

    <div class="mt-6 space-y-6">
      <Card class="p-4">
        <div class="flex flex-wrap items-center gap-2">
          <Button
            :disabled="syncTriggerLoading"
            @click="triggerSync(false)"
          >
            <Loader2
              v-if="syncTriggerLoading && !syncDryRun"
              class="w-4 h-4 mr-1 animate-spin"
            />
            立即同步
          </Button>
          <Button
            variant="outline"
            :disabled="syncTriggerLoading"
            @click="triggerSync(true)"
          >
            <Loader2
              v-if="syncTriggerLoading && syncDryRun"
              class="w-4 h-4 mr-1 animate-spin"
            />
            Dry-run 预览
          </Button>
          <Button
            variant="outline"
            :disabled="checkinTriggerLoading"
            @click="triggerCheckin"
          >
            <Loader2
              v-if="checkinTriggerLoading"
              class="w-4 h-4 mr-1 animate-spin"
            />
            手动签到
          </Button>
          <Button
            variant="ghost"
            :disabled="loading"
            @click="loadData"
          >
            刷新
          </Button>
        </div>
      </Card>

      <Card class="overflow-hidden">
        <div class="px-4 py-3 border-b border-border/60 flex items-center gap-2">
          <Button
            size="sm"
            :variant="activeTab === 'sync' ? 'default' : 'outline'"
            @click="activeTab = 'sync'"
          >
            同步记录
          </Button>
          <Button
            size="sm"
            :variant="activeTab === 'checkin' ? 'default' : 'outline'"
            @click="activeTab = 'checkin'"
          >
            签到记录
          </Button>
        </div>

        <div
          v-if="loading"
          class="py-12 flex justify-center"
        >
          <Loader2 class="w-6 h-6 animate-spin text-muted-foreground" />
        </div>

        <template v-else-if="activeTab === 'sync'">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>时间</TableHead>
                <TableHead>来源</TableHead>
                <TableHead>状态</TableHead>
                <TableHead>匹配/更新</TableHead>
                <TableHead>总账户</TableHead>
                <TableHead>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow
                v-for="run in syncRuns"
                :key="run.id"
              >
                <TableCell>{{ formatDate(run.created_at) }}</TableCell>
                <TableCell>{{ run.trigger_source }}</TableCell>
                <TableCell>
                  <Badge :variant="run.status === 'success' ? 'default' : 'destructive'">
                    {{ run.status }}
                  </Badge>
                </TableCell>
                <TableCell>{{ run.matched_providers }} / {{ run.updated_providers }}</TableCell>
                <TableCell>{{ run.total_accounts }}</TableCell>
                <TableCell>
                  <Button
                    size="sm"
                    variant="outline"
                    @click="showSyncItems(run.id)"
                  >
                    明细
                  </Button>
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>

          <div
            v-if="selectedSyncRunId"
            class="px-4 py-4 border-t border-border/60 space-y-3"
          >
            <div class="text-sm font-medium">
              同步差异明细（{{ selectedSyncRunId }}）
            </div>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>域名</TableHead>
                  <TableHead>Provider</TableHead>
                  <TableHead>状态</TableHead>
                  <TableHead>Cookie指纹</TableHead>
                  <TableHead>说明</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                <TableRow
                  v-for="item in syncItems"
                  :key="item.id"
                >
                  <TableCell>{{ item.domain }}</TableCell>
                  <TableCell>{{ item.provider_name || '-' }}</TableCell>
                  <TableCell>{{ item.status }}</TableCell>
                  <TableCell>
                    {{ item.before_fingerprint || '-' }} -> {{ item.after_fingerprint || '-' }}
                  </TableCell>
                  <TableCell>{{ item.message || '-' }}</TableCell>
                </TableRow>
              </TableBody>
            </Table>
            <div
              v-if="syncItems.length === 0"
              class="text-sm text-muted-foreground"
            >
              当前记录没有可展示的差异明细（可能未匹配到站点或未识别到访问凭据字段）
            </div>
          </div>
        </template>

        <template v-else>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>时间</TableHead>
                <TableHead>来源</TableHead>
                <TableHead>状态</TableHead>
                <TableHead>成功/失败/跳过</TableHead>
                <TableHead>总站点</TableHead>
                <TableHead>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow
                v-for="run in checkinRuns"
                :key="run.id"
              >
                <TableCell>{{ formatDate(run.created_at) }}</TableCell>
                <TableCell>{{ run.trigger_source }}</TableCell>
                <TableCell>
                  <Badge :variant="run.status === 'success' ? 'default' : 'destructive'">
                    {{ run.status }}
                  </Badge>
                </TableCell>
                <TableCell>{{ run.success_count }}/{{ run.failed_count }}/{{ run.skipped_count }}</TableCell>
                <TableCell>{{ run.total_providers }}</TableCell>
                <TableCell>
                  <Button
                    size="sm"
                    variant="outline"
                    @click="showCheckinItems(run.id)"
                  >
                    明细
                  </Button>
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>

          <div
            v-if="selectedCheckinRunId"
            class="px-4 py-4 border-t border-border/60 space-y-3"
          >
            <div class="text-sm font-medium">
              签到明细（{{ selectedCheckinRunId }}）
            </div>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Provider</TableHead>
                  <TableHead>域名</TableHead>
                  <TableHead>状态</TableHead>
                  <TableHead>余额</TableHead>
                  <TableHead>消息</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                <TableRow
                  v-for="item in checkinItems"
                  :key="item.id"
                >
                  <TableCell>{{ item.provider_name || '-' }}</TableCell>
                  <TableCell>{{ item.provider_domain || '-' }}</TableCell>
                  <TableCell>{{ item.status }}</TableCell>
                  <TableCell>{{ formatBalance(item.balance_total, item.balance_currency) }}</TableCell>
                  <TableCell>{{ item.message || '-' }}</TableCell>
                </TableRow>
              </TableBody>
            </Table>
            <div
              v-if="checkinItems.length === 0"
              class="text-sm text-muted-foreground"
            >
              当前记录没有签到明细
            </div>
          </div>
        </template>
      </Card>
    </div>
  </PageContainer>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { Loader2 } from 'lucide-vue-next'
import { PageContainer, PageHeader } from '@/components/layout'
import Card from '@/components/ui/card.vue'
import Button from '@/components/ui/button.vue'
import Badge from '@/components/ui/badge.vue'
import Table from '@/components/ui/table.vue'
import TableHeader from '@/components/ui/table-header.vue'
import TableBody from '@/components/ui/table-body.vue'
import TableRow from '@/components/ui/table-row.vue'
import TableHead from '@/components/ui/table-head.vue'
import TableCell from '@/components/ui/table-cell.vue'
import { adminApi, type SiteCheckinItem, type SiteCheckinRun, type SiteSyncItem, type SiteSyncRun } from '@/api/admin'
import { useToast } from '@/composables/useToast'
import { log } from '@/utils/logger'

const { success, error } = useToast()

const activeTab = ref<'sync' | 'checkin'>('sync')
const loading = ref(false)
const syncTriggerLoading = ref(false)
const syncDryRun = ref(false)
const checkinTriggerLoading = ref(false)

const syncRuns = ref<SiteSyncRun[]>([])
const syncItems = ref<SiteSyncItem[]>([])
const selectedSyncRunId = ref('')

const checkinRuns = ref<SiteCheckinRun[]>([])
const checkinItems = ref<SiteCheckinItem[]>([])
const selectedCheckinRunId = ref('')

function formatDate(value?: string | null): string {
  if (!value) return '-'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}

function formatBalance(total?: number | null, currency?: string | null): string {
  if (total == null) return '-'
  return `${total.toFixed(4)} ${currency || ''}`.trim()
}

async function loadData() {
  loading.value = true
  try {
    const [sync, checkin] = await Promise.all([
      adminApi.getSiteSyncRuns(20),
      adminApi.getSiteCheckinRuns(20),
    ])
    syncRuns.value = sync
    checkinRuns.value = checkin
  } catch (err) {
    error('加载站点管理数据失败')
    log.error('load site management data failed', err)
  } finally {
    loading.value = false
  }
}

async function showSyncItems(runId: string) {
  selectedSyncRunId.value = runId
  try {
    syncItems.value = await adminApi.getSiteSyncRunItems(runId, 500)
  } catch (err) {
    error('加载同步明细失败')
    log.error('load sync items failed', err)
  }
}

async function showCheckinItems(runId: string) {
  selectedCheckinRunId.value = runId
  try {
    checkinItems.value = await adminApi.getSiteCheckinRunItems(runId, 500)
  } catch (err) {
    error('加载签到明细失败')
    log.error('load checkin items failed', err)
  }
}

async function triggerSync(dryRun: boolean) {
  syncTriggerLoading.value = true
  syncDryRun.value = dryRun
  try {
    const result = await adminApi.triggerSiteSync({ dry_run: dryRun })
    success(dryRun ? 'Dry-run 已完成' : '同步已触发')
    await loadData()
    if (result.run_id) {
      await showSyncItems(result.run_id)
    }
  } catch (err) {
    error('触发同步失败')
    log.error('trigger site sync failed', err)
  } finally {
    syncTriggerLoading.value = false
  }
}

async function triggerCheckin() {
  checkinTriggerLoading.value = true
  try {
    const result = await adminApi.triggerSiteCheckin()
    success('签到任务已执行')
    await loadData()
    if (result.latest_run_id) {
      activeTab.value = 'checkin'
      await showCheckinItems(result.latest_run_id)
    }
  } catch (err) {
    error('触发签到失败')
    log.error('trigger site checkin failed', err)
  } finally {
    checkinTriggerLoading.value = false
  }
}

onMounted(() => {
  loadData()
})
</script>
