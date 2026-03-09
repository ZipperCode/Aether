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
            @click="reloadAll"
          >
            刷新
          </Button>
        </div>
      </Card>

      <Card class="overflow-hidden">
        <div class="px-4 py-3 border-b border-border/60 flex items-center justify-between gap-3">
          <div>
            <div class="text-sm font-medium">
              账号管理（WebDAV）
            </div>
            <div class="text-xs text-muted-foreground mt-1">
              查看并编辑从 all-api-hub 备份解析的账号，然后应用到当前 Provider
            </div>
          </div>
          <div class="flex items-center gap-2">
            <Button
              size="sm"
              variant="outline"
              :disabled="accountsLoading"
              @click="loadAccounts(true)"
            >
              <Loader2
                v-if="accountsLoading"
                class="w-4 h-4 mr-1 animate-spin"
              />
              刷新账号
            </Button>
            <Button
              size="sm"
              variant="outline"
              :disabled="accountsSyncLoading || accounts.length === 0"
              @click="applyAccountsSync(true, false)"
            >
              Dry-run 应用
            </Button>
            <Button
              size="sm"
              :disabled="accountsSyncLoading || accounts.length === 0"
              @click="applyAccountsSync(false, false)"
            >
              <Loader2
                v-if="accountsSyncLoading"
                class="w-4 h-4 mr-1 animate-spin"
              />
              应用账号同步
            </Button>
          </div>
        </div>
        <div class="px-4 py-3 border-b border-border/60 flex flex-wrap items-center gap-2">
          <Input
            v-model="accountSearch"
            class="h-8 w-[260px]"
            placeholder="搜索域名、站点地址、用户ID"
          />
          <Button
            size="sm"
            :variant="authTypeFilter === 'all' ? 'default' : 'outline'"
            @click="authTypeFilter = 'all'"
          >
            全部
          </Button>
          <Button
            size="sm"
            :variant="authTypeFilter === 'cookie' ? 'default' : 'outline'"
            @click="authTypeFilter = 'cookie'"
          >
            Cookie
          </Button>
          <Button
            size="sm"
            :variant="authTypeFilter === 'access_token' ? 'default' : 'outline'"
            @click="authTypeFilter = 'access_token'"
          >
            Access Token
          </Button>
          <div class="text-xs text-muted-foreground ml-auto">
            已选 {{ selectedAccountIndices.length }} / 可见 {{ filteredAccounts.length }} / 总计 {{ accounts.length }}
          </div>
        </div>
        <div class="px-4 py-3 border-b border-border/60 flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            variant="outline"
            :disabled="filteredAccounts.length === 0"
            @click="selectVisibleAccounts"
          >
            选中可见
          </Button>
          <Button
            size="sm"
            variant="outline"
            :disabled="selectedAccountIndices.length === 0"
            @click="clearSelectedAccounts"
          >
            清空选择
          </Button>
          <Button
            size="sm"
            variant="outline"
            :disabled="selectedAccountIndices.length === 0"
            @click="openBatchEditDialog"
          >
            批量编辑
          </Button>
          <Button
            size="sm"
            :disabled="accountsSyncLoading || selectedAccountIndices.length === 0"
            @click="applyAccountsSync(false, true)"
          >
            仅同步选中
          </Button>
        </div>

        <div
          v-if="accountsLoading"
          class="py-10 flex justify-center"
        >
          <Loader2 class="w-6 h-6 animate-spin text-muted-foreground" />
        </div>
        <template v-else>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead class="w-[56px]">
                  选择
                </TableHead>
                <TableHead>站点</TableHead>
                <TableHead>认证方式</TableHead>
                <TableHead>签到开关</TableHead>
                <TableHead>最近签到</TableHead>
                <TableHead>余额状态</TableHead>
                <TableHead>用户ID</TableHead>
                <TableHead>Token</TableHead>
                <TableHead>Cookie</TableHead>
                <TableHead>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow
                v-for="item in filteredAccounts"
                :key="item.rowKey"
              >
                <TableCell>
                  <Checkbox
                    :model-value="isAccountSelected(item.index)"
                    @update:model-value="(checked) => toggleAccountSelection(item.index, checked)"
                  />
                </TableCell>
                <TableCell>
                  <div class="space-y-1">
                    <div class="font-medium">
                      {{ item.account.domain }}
                    </div>
                    <div class="text-xs text-muted-foreground">
                      {{ item.account.site_url || '-' }}
                    </div>
                  </div>
                </TableCell>
                <TableCell>{{ item.account.auth_type || 'cookie' }}</TableCell>
                <TableCell>
                  <Badge :variant="item.account.checkin_enabled === false ? 'outline' : 'default'">
                    {{ item.account.checkin_enabled === false ? '关闭' : '开启' }}
                  </Badge>
                </TableCell>
                <TableCell>
                  <div class="space-y-1">
                    <Badge
                      :variant="checkinStatusVariant(item.account.last_checkin_status)"
                    >
                      {{ checkinStatusLabel(item.account.last_checkin_status) }}
                    </Badge>
                    <div class="text-xs text-muted-foreground truncate max-w-[220px]">
                      {{ item.account.last_checkin_message || '-' }}
                    </div>
                    <div class="text-xs text-muted-foreground">
                      {{ formatDate(item.account.last_checkin_at) }}
                    </div>
                  </div>
                </TableCell>
                <TableCell>
                  <div class="space-y-1">
                    <Badge
                      :variant="balanceStatusVariant(item.account.last_balance_status)"
                    >
                      {{ balanceStatusLabel(item.account.last_balance_status) }}
                    </Badge>
                    <div class="text-xs text-muted-foreground truncate max-w-[220px]">
                      {{ item.account.last_balance_message || '-' }}
                    </div>
                    <div class="text-xs text-muted-foreground">
                      {{ formatBalance(item.account.last_balance_total, item.account.last_balance_currency) }}
                    </div>
                    <div class="text-xs text-muted-foreground">
                      {{ formatDate(item.account.last_balance_at) }}
                    </div>
                  </div>
                </TableCell>
                <TableCell>{{ item.account.user_id || '-' }}</TableCell>
                <TableCell>{{ item.account.access_token ? '已配置' : '-' }}</TableCell>
                <TableCell>{{ item.account.cookie ? '已配置' : '-' }}</TableCell>
                <TableCell>
                  <div class="flex items-center gap-2">
                    <Button
                      size="sm"
                      variant="outline"
                      :disabled="!item.account.id || manualCheckinLoading[item.account.id]"
                      @click="manualCheckin(item.index)"
                    >
                      <Loader2
                        v-if="item.account.id && manualCheckinLoading[item.account.id]"
                        class="w-3 h-3 mr-1 animate-spin"
                      />
                      手动签到
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      :disabled="!item.account.id || manualBalanceLoading[item.account.id]"
                      @click="manualBalance(item.index)"
                    >
                      <Loader2
                        v-if="item.account.id && manualBalanceLoading[item.account.id]"
                        class="w-3 h-3 mr-1 animate-spin"
                      />
                      刷新余额
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      @click="openAccountEdit(item.index)"
                    >
                      编辑
                    </Button>
                  </div>
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
          <div
            v-if="accounts.length === 0"
            class="px-4 py-4 text-sm text-muted-foreground"
          >
            未读取到账号数据，请检查 WebDAV 配置或备份内容
          </div>
        </template>
      </Card>

      <Card class="overflow-hidden">
        <div class="px-4 py-3 border-b border-border/60 flex items-center justify-between gap-3">
          <div class="text-sm font-medium">
            手动签到日志（当前会话）
          </div>
          <Button
            size="sm"
            variant="outline"
            :disabled="manualCheckinLogs.length === 0"
            @click="clearManualCheckinLogs"
          >
            清空日志
          </Button>
        </div>
        <Table v-if="manualCheckinLogs.length > 0">
          <TableHeader>
            <TableRow>
              <TableHead>时间</TableHead>
              <TableHead>站点</TableHead>
              <TableHead>状态</TableHead>
              <TableHead>消息</TableHead>
              <TableHead>耗时</TableHead>
              <TableHead>返回摘要</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableRow
              v-for="logItem in manualCheckinLogs"
              :key="logItem.id"
            >
              <TableCell>{{ formatDate(logItem.executedAt) }}</TableCell>
              <TableCell>{{ logItem.domain }}</TableCell>
              <TableCell>
                <Badge :variant="logItem.status === 'success' ? 'default' : 'destructive'">
                  {{ logItem.status }}
                </Badge>
              </TableCell>
              <TableCell>{{ logItem.message || '-' }}</TableCell>
              <TableCell>{{ logItem.responseTimeMs != null ? `${logItem.responseTimeMs}ms` : '-' }}</TableCell>
              <TableCell class="max-w-[520px] truncate">
                {{ logItem.dataSummary || '-' }}
              </TableCell>
            </TableRow>
          </TableBody>
        </Table>
        <div
          v-else
          class="px-4 py-4 text-sm text-muted-foreground"
        >
          暂无手动签到日志
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
            <div class="flex flex-wrap items-center justify-between gap-2">
              <div class="text-sm font-medium">
                签到明细（{{ selectedCheckinRunId }}）
              </div>
              <div class="flex items-center gap-2">
                <Button
                  size="sm"
                  :variant="checkinItemFilter === 'all' ? 'default' : 'outline'"
                  @click="checkinItemFilter = 'all'"
                >
                  全部
                </Button>
                <Button
                  size="sm"
                  :variant="checkinItemFilter === 'failed' ? 'default' : 'outline'"
                  @click="checkinItemFilter = 'failed'"
                >
                  仅失败
                </Button>
                <Button
                  size="sm"
                  :variant="checkinItemFilter === 'skipped' ? 'default' : 'outline'"
                  @click="checkinItemFilter = 'skipped'"
                >
                  仅排除
                </Button>
              </div>
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
                  v-for="item in filteredCheckinItems"
                  :key="item.id"
                >
                  <TableCell>{{ item.provider_name || '-' }}</TableCell>
                  <TableCell>{{ item.provider_domain || '-' }}</TableCell>
                  <TableCell>
                    <Badge :variant="checkinItemStatusVariant(item)">
                      {{ checkinItemStatusLabel(item) }}
                    </Badge>
                  </TableCell>
                  <TableCell>{{ formatBalance(item.balance_total, item.balance_currency) }}</TableCell>
                  <TableCell>{{ normalizeCheckinItemMessage(item.message, Boolean(item.manual_verification_required)) || '-' }}</TableCell>
                </TableRow>
              </TableBody>
            </Table>
            <div
              v-if="filteredCheckinItems.length === 0"
              class="text-sm text-muted-foreground"
            >
              当前筛选条件下没有签到明细
            </div>
          </div>
        </template>
      </Card>

      <Dialog v-model:open="accountEditDialogVisible">
        <DialogContent class="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>编辑账号</DialogTitle>
            <DialogDescription>
              保存后仅更新当前页面内的数据，点击“应用账号同步”后才会实际写入 Provider 配置。
            </DialogDescription>
          </DialogHeader>
          <div class="space-y-4">
            <div class="space-y-2">
              <label class="text-sm font-medium">站点地址</label>
              <Input v-model="editingAccount.site_url" />
            </div>
            <div class="space-y-2">
              <label class="text-sm font-medium">认证方式</label>
              <Input
                v-model="editingAccount.auth_type"
                placeholder="cookie 或 access_token"
              />
            </div>
            <div class="space-y-2">
              <label class="text-sm font-medium">用户 ID</label>
              <Input
                v-model="editingAccount.user_id"
                placeholder="可选"
              />
            </div>
            <div class="space-y-2">
              <label class="text-sm font-medium">签到开关</label>
              <div class="flex items-center gap-3">
                <Switch v-model="editingAccountCheckinEnabled" />
                <span class="text-sm text-muted-foreground">
                  {{ editingAccountCheckinEnabled ? '开启：参与批量签到任务' : '关闭：批量签到任务跳过该提供商' }}
                </span>
              </div>
            </div>
            <div class="space-y-2">
              <label class="text-sm font-medium">Access Token</label>
              <Textarea
                v-model="editingAccount.access_token"
                rows="4"
                placeholder="可选"
              />
            </div>
            <div class="space-y-2">
              <label class="text-sm font-medium">Cookie</label>
              <Textarea
                v-model="editingAccount.cookie"
                rows="4"
                placeholder="可选"
              />
            </div>
          </div>
          <DialogFooter>
            <Button
              variant="ghost"
              @click="accountEditDialogVisible = false"
            >
              取消
            </Button>
            <Button @click="saveAccountEdit">
              保存
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog v-model:open="batchEditDialogVisible">
        <DialogContent class="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>批量编辑账号</DialogTitle>
            <DialogDescription>
              仅对已选中的账号生效；留空的字段不会覆盖现有值。
            </DialogDescription>
          </DialogHeader>
          <div class="space-y-4">
            <div class="space-y-2">
              <label class="text-sm font-medium">认证方式（可选）</label>
              <Input
                v-model="batchEditForm.auth_type"
                placeholder="cookie 或 access_token"
              />
            </div>
            <div class="space-y-2">
              <label class="text-sm font-medium">用户 ID（可选）</label>
              <Input
                v-model="batchEditForm.user_id"
                placeholder="留空表示不修改"
              />
            </div>
            <div class="space-y-2">
              <label class="text-sm font-medium">Access Token（可选）</label>
              <Textarea
                v-model="batchEditForm.access_token"
                rows="4"
                placeholder="留空表示不修改"
              />
            </div>
            <div class="space-y-2">
              <label class="text-sm font-medium">Cookie（可选）</label>
              <Textarea
                v-model="batchEditForm.cookie"
                rows="4"
                placeholder="留空表示不修改"
              />
            </div>
          </div>
          <DialogFooter>
            <Button
              variant="ghost"
              @click="batchEditDialogVisible = false"
            >
              取消
            </Button>
            <Button @click="saveBatchEdit">
              应用到已选账号
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  </PageContainer>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Loader2 } from 'lucide-vue-next'
import { PageContainer, PageHeader } from '@/components/layout'
import Card from '@/components/ui/card.vue'
import Button from '@/components/ui/button.vue'
import Badge from '@/components/ui/badge.vue'
import Checkbox from '@/components/ui/checkbox.vue'
import Switch from '@/components/ui/switch.vue'
import Input from '@/components/ui/input.vue'
import Textarea from '@/components/ui/textarea.vue'
import Table from '@/components/ui/table.vue'
import TableHeader from '@/components/ui/table-header.vue'
import TableBody from '@/components/ui/table-body.vue'
import TableRow from '@/components/ui/table-row.vue'
import TableHead from '@/components/ui/table-head.vue'
import TableCell from '@/components/ui/table-cell.vue'
import Dialog from '@/components/ui/dialog/Dialog.vue'
import DialogContent from '@/components/ui/dialog/DialogContent.vue'
import DialogDescription from '@/components/ui/dialog/DialogDescription.vue'
import DialogFooter from '@/components/ui/dialog/DialogFooter.vue'
import DialogHeader from '@/components/ui/dialog/DialogHeader.vue'
import DialogTitle from '@/components/ui/dialog/DialogTitle.vue'
import {
  adminApi,
  type SiteCheckinItem,
  type SiteCheckinRun,
  type SiteManualCheckinResponse,
  type SiteManagementAccount,
  type SiteSyncItem,
  type SiteSyncRun,
} from '@/api/admin'
import { useToast } from '@/composables/useToast'
import { parseApiError } from '@/utils/errorParser'
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
const checkinItemFilter = ref<'all' | 'failed' | 'skipped'>('all')
const accountsLoading = ref(false)
const accountsSyncLoading = ref(false)
const manualCheckinLoading = ref<Record<string, boolean>>({})
const manualBalanceLoading = ref<Record<string, boolean>>({})
const manualCheckinLogs = ref<Array<{
  id: string
  executedAt: string
  domain: string
  targetId: string
  status: string
  message: string
  manualVerificationRequired: boolean
  responseTimeMs: number | null
  dataSummary: string
}>>([])
const accounts = ref<SiteManagementAccount[]>([])
const accountSearch = ref('')
const authTypeFilter = ref<'all' | 'cookie' | 'access_token'>('all')
const selectedAccountIndices = ref<number[]>([])

const accountEditDialogVisible = ref(false)
const editingAccountIndex = ref<number>(-1)
const editingAccountCheckinEnabled = ref(true)
const editingAccount = ref<SiteManagementAccount>({
  site_url: '',
  domain: '',
  provider_id: '',
  provider_name: '',
  checkin_enabled: true,
  auth_type: 'cookie',
  user_id: '',
  access_token: '',
  cookie: '',
})
const batchEditDialogVisible = ref(false)
const batchEditForm = ref({
  auth_type: '',
  user_id: '',
  access_token: '',
  cookie: '',
})

const filteredAccounts = computed(() => {
  const keyword = accountSearch.value.trim().toLowerCase()
  return accounts.value
    .map((account, index) => ({
      account,
      index,
      rowKey: `${index}:${account.domain || ''}:${account.site_url || ''}`,
    }))
    .filter(({ account }) => {
      const authType = (account.auth_type || 'cookie').toLowerCase()
      if (authTypeFilter.value !== 'all' && authType !== authTypeFilter.value) {
        return false
      }
      if (!keyword) {
        return true
      }
      const haystack = [
        account.domain || '',
        account.site_url || '',
        account.user_id || '',
      ]
        .join(' ')
        .toLowerCase()
      return haystack.includes(keyword)
    })
})

const filteredCheckinItems = computed(() => {
  if (checkinItemFilter.value === 'all') {
    return checkinItems.value
  }
  if (checkinItemFilter.value === 'failed') {
    return checkinItems.value.filter(item => item.status === 'failed' || isManualVerificationItem(item))
  }
  return checkinItems.value.filter(item => item.status === 'skipped')
})

function isManualVerificationMessage(message?: string | null): boolean {
  const text = (message || '').trim().toLowerCase()
  if (!text) return false
  return (
    text.startsWith('auth_failed_needs_manual_verification:') ||
    text.includes('turnstile') ||
    text.includes('captcha') ||
    text.includes('验证码') ||
    text.includes('需手动核验')
  )
}

function isManualVerificationItem(item: Pick<SiteCheckinItem, 'manual_verification_required' | 'message'>): boolean {
  return Boolean(item.manual_verification_required) || isManualVerificationMessage(item.message)
}

function extractManualVerificationFromData(data: unknown): boolean {
  if (!data || typeof data !== 'object') {
    return false
  }
  const candidate = (data as Record<string, unknown>).manual_verification_required
  return candidate === true
}

function normalizeCheckinItemMessage(
  message?: string | null,
  manualVerificationRequired = false
): string {
  const text = message || ''
  if (text.startsWith('auth_failed_needs_manual_verification:')) {
    return text.replace('auth_failed_needs_manual_verification:', '').trim()
  }
  if (manualVerificationRequired && !isManualVerificationMessage(text)) {
    return `${text || '签到认证失败'}（需手动核验）`
  }
  return text
}

function checkinItemStatusLabel(item: SiteCheckinItem): string {
  if (isManualVerificationItem(item)) {
    return '需手动核验'
  }
  if (item.status === 'success') return '成功'
  if (item.status === 'failed') return '失败'
  if (item.status === 'skipped') return '已跳过'
  return item.status
}

function checkinItemStatusVariant(
  item: SiteCheckinItem
): 'default' | 'destructive' | 'outline' {
  if (isManualVerificationItem(item)) return 'destructive'
  if (item.status === 'success') return 'default'
  if (item.status === 'failed') return 'destructive'
  return 'outline'
}

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

function checkinStatusVariant(status?: string): 'default' | 'destructive' | 'outline' {
  if (status === 'success') return 'default'
  if (status === 'already_done' || status === 'skipped') return 'outline'
  if (status === 'failed' || status === 'auth_failed' || status === 'unknown_error') return 'destructive'
  return 'outline'
}

function checkinStatusLabel(status?: string): string {
  if (status === 'success') return '成功'
  if (status === 'already_done') return '已签到'
  if (status === 'auth_failed') return '认证失败'
  if (status === 'auth_expired') return '认证过期'
  if (status === 'not_configured') return '未配置'
  if (status === 'not_supported') return '不支持'
  if (status === 'failed') return '失败'
  if (status === 'skipped') return '已跳过'
  if (status === 'unknown_error') return '执行失败'
  return status || '未执行'
}

function balanceStatusVariant(status?: string): 'default' | 'destructive' | 'outline' {
  if (status === 'success') return 'default'
  if (status === 'pending' || status === 'already_done' || status === 'skipped') return 'outline'
  if (status === 'auth_failed' || status === 'auth_expired' || status === 'unknown_error') {
    return 'destructive'
  }
  return 'outline'
}

function balanceStatusLabel(status?: string): string {
  if (status === 'success') return '成功'
  if (status === 'pending') return '处理中'
  if (status === 'auth_failed') return '认证失败'
  if (status === 'auth_expired') return '认证过期'
  if (status === 'not_configured') return '未配置'
  if (status === 'not_supported') return '不支持'
  if (status === 'unknown_error') return '查询失败'
  if (status === 'skipped') return '已跳过'
  return status || '未同步'
}

function summarizeManualCheckinData(data: SiteManualCheckinResponse['data']): string {
  if (!data || typeof data !== 'object') {
    return ''
  }
  const json = JSON.stringify(data)
  if (json.length <= 280) {
    return json
  }
  return `${json.slice(0, 280)}...`
}

function clearManualCheckinLogs() {
  manualCheckinLogs.value = []
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

async function loadAccounts(refresh = false) {
  accountsLoading.value = true
  try {
    const rows = await adminApi.getSiteAccounts(refresh)
    // 首次仅返回 WebDAV 原始列表时，自动触发一次 refresh 生成可执行账号记录（含 account id）。
    if (!refresh && rows.length > 0 && rows.every(item => !item.id)) {
      accounts.value = await adminApi.getSiteAccounts(true)
    } else {
      accounts.value = rows
    }
    selectedAccountIndices.value = []
  } catch (err) {
    error('加载账号列表失败')
    log.error('load site accounts failed', err)
  } finally {
    accountsLoading.value = false
  }
}

function isAccountSelected(index: number): boolean {
  return selectedAccountIndices.value.includes(index)
}

function toggleAccountSelection(index: number, checked: boolean) {
  if (checked) {
    if (!selectedAccountIndices.value.includes(index)) {
      selectedAccountIndices.value.push(index)
    }
    return
  }
  selectedAccountIndices.value = selectedAccountIndices.value.filter(i => i !== index)
}

function selectVisibleAccounts() {
  const visibleIndices = filteredAccounts.value.map(item => item.index)
  const merged = new Set([...selectedAccountIndices.value, ...visibleIndices])
  selectedAccountIndices.value = Array.from(merged)
}

function clearSelectedAccounts() {
  selectedAccountIndices.value = []
}

function openAccountEdit(index: number) {
  const target = accounts.value[index]
  if (!target) return
  editingAccountIndex.value = index
  editingAccount.value = {
    site_url: target.site_url || '',
    domain: target.domain || '',
    provider_id: target.provider_id || '',
    provider_name: target.provider_name || '',
    checkin_enabled: target.checkin_enabled !== false,
    auth_type: target.auth_type || 'cookie',
    user_id: target.user_id || '',
    access_token: target.access_token || '',
    cookie: target.cookie || '',
  }
  editingAccountCheckinEnabled.value = target.checkin_enabled !== false
  accountEditDialogVisible.value = true
}

function saveAccountEdit() {
  const idx = editingAccountIndex.value
  if (idx < 0 || !accounts.value[idx]) return
  accounts.value[idx] = {
    ...accounts.value[idx],
    site_url: (editingAccount.value.site_url || '').trim(),
    checkin_enabled: editingAccountCheckinEnabled.value,
    auth_type: (editingAccount.value.auth_type || 'cookie').trim().toLowerCase(),
    user_id: (editingAccount.value.user_id || '').trim() || null,
    access_token: (editingAccount.value.access_token || '').trim() || null,
    cookie: (editingAccount.value.cookie || '').trim() || null,
  }
  accountEditDialogVisible.value = false
  success('账号已更新，请点击“应用账号同步”生效')
}

function openBatchEditDialog() {
  batchEditForm.value = {
    auth_type: '',
    user_id: '',
    access_token: '',
    cookie: '',
  }
  batchEditDialogVisible.value = true
}

function saveBatchEdit() {
  const normalizedAuthType = batchEditForm.value.auth_type.trim().toLowerCase()
  const nextUserId = batchEditForm.value.user_id.trim()
  const nextToken = batchEditForm.value.access_token.trim()
  const nextCookie = batchEditForm.value.cookie.trim()

  if (!normalizedAuthType && !nextUserId && !nextToken && !nextCookie) {
    error('请至少填写一个需要批量更新的字段')
    return
  }

  for (const index of selectedAccountIndices.value) {
    const target = accounts.value[index]
    if (!target) continue
    accounts.value[index] = {
      ...target,
      auth_type: normalizedAuthType || target.auth_type || 'cookie',
      user_id: nextUserId || target.user_id || null,
      access_token: nextToken || target.access_token || null,
      cookie: nextCookie || target.cookie || null,
    }
  }

  batchEditDialogVisible.value = false
  success(`已批量更新 ${selectedAccountIndices.value.length} 个账号`)
}

async function applyAccountsSync(dryRun: boolean, selectedOnly: boolean) {
  accountsSyncLoading.value = true
  try {
    const targetAccounts = selectedOnly
      ? selectedAccountIndices.value
          .map(index => accounts.value[index])
          .filter((item): item is SiteManagementAccount => Boolean(item))
      : accounts.value
    if (targetAccounts.length === 0) {
      error(selectedOnly ? '请先选择要同步的账号' : '没有可同步的账号')
      return
    }
    const result = await adminApi.applySiteAccountsSync({
      accounts: targetAccounts,
      dry_run: dryRun,
    })
    if (selectedOnly) {
      success(dryRun ? '选中账号 Dry-run 已完成' : '选中账号同步已应用')
    } else {
      success(dryRun ? '账号 Dry-run 已完成' : '账号同步已应用')
    }
    if (!dryRun && (result.checkin_pref_updated || 0) > 0) {
      success(`签到开关已更新 ${result.checkin_pref_updated} 个提供商`)
    }
    activeTab.value = 'sync'
    await loadData()
    if (!dryRun) {
      await loadAccounts(false)
    }
    if (result.run_id) {
      await showSyncItems(result.run_id)
    }
  } catch (err) {
    error('应用账号同步失败')
    log.error('apply site accounts sync failed', err)
  } finally {
    accountsSyncLoading.value = false
  }
}

async function manualCheckin(index: number) {
  const target = accounts.value[index]
  const accountId = target?.id
  if (!accountId) {
    error('该账号未生成记录 ID，无法手动签到')
    return
  }
  manualCheckinLoading.value = { ...manualCheckinLoading.value, [accountId]: true }
  try {
    const result = await adminApi.checkinSiteAccount(accountId)
    const manualVerificationRequired = extractManualVerificationFromData(result.data)
    const message = normalizeCheckinItemMessage(
      result.message || (result.status === 'success' ? '签到成功' : '签到已执行'),
      manualVerificationRequired
    )
    manualCheckinLogs.value = [
      {
        id: `${Date.now()}-${accountId}`,
        executedAt: result.executed_at,
        domain: target.domain || '-',
        targetId: accountId,
        status: result.status,
        message,
        manualVerificationRequired,
        responseTimeMs: result.response_time_ms ?? null,
        dataSummary: summarizeManualCheckinData(result.data),
      },
      ...manualCheckinLogs.value,
    ].slice(0, 50)
    log.info('manual checkin result', {
      accountId,
      domain: target.domain,
      status: result.status,
      message: result.message,
      manualVerificationRequired,
      responseTimeMs: result.response_time_ms,
      data: result.data,
    })
    if (result.status === 'success' || result.status === 'already_done') {
      success(`[${target.domain}] ${message}`)
    } else {
      error(`[${target.domain}] ${message}`)
    }
    await loadAccounts(false)
    await loadData()
  } catch (err) {
    const errMsg = parseApiError(err)
    manualCheckinLogs.value = [
      {
        id: `${Date.now()}-${accountId}-error`,
        executedAt: new Date().toISOString(),
        domain: target.domain || '-',
        targetId: accountId,
        status: 'failed',
        message: errMsg,
        manualVerificationRequired: false,
        responseTimeMs: null,
        dataSummary: '',
      },
      ...manualCheckinLogs.value,
    ].slice(0, 50)
    error(`手动签到失败: ${errMsg}`)
    log.error('manual checkin failed', { accountId, domain: target.domain, error: err })
  } finally {
    manualCheckinLoading.value = { ...manualCheckinLoading.value, [accountId]: false }
  }
}

async function manualBalance(index: number) {
  const target = accounts.value[index]
  const accountId = target?.id
  if (!accountId) {
    error('该账号未生成记录 ID，无法刷新余额')
    return
  }
  manualBalanceLoading.value = { ...manualBalanceLoading.value, [accountId]: true }
  try {
    const result = await adminApi.balanceSiteAccount(accountId)
    const message = result.message || (result.status === 'success' ? '余额刷新成功' : '余额刷新已执行')
    success(`[${target.domain}] ${message}`)
    await loadAccounts(false)
    await loadData()
  } catch (err) {
    const errMsg = parseApiError(err)
    error(`刷新余额失败: ${errMsg}`)
    log.error('manual balance failed', { accountId, domain: target.domain, error: err })
  } finally {
    manualBalanceLoading.value = { ...manualBalanceLoading.value, [accountId]: false }
  }
}

async function reloadAll() {
  await Promise.all([loadData(), loadAccounts(false)])
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
  checkinItemFilter.value = 'all'
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
  reloadAll()
})
</script>
