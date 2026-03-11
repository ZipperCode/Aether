<template>
  <div>
    <div class="flex flex-wrap items-center gap-2 mb-3">
      <Input
        v-model="search"
        class="h-8 w-[260px]"
        placeholder="搜索域名..."
      />
      <div class="ml-auto flex items-center gap-2">
        <Checkbox
          :model-value="allSelected"
          @update:model-value="toggleSelectAll"
        />
        <span class="text-xs text-muted-foreground">
          全选 ({{ selectedIds.length }}/{{ accounts.length }})
        </span>
      </div>
      <Button
        size="sm"
        variant="outline"
        :disabled="selectedIds.length === 0 || batchCheckinLoading"
        @click="handleBatchCheckin"
      >
        <Loader2
          v-if="batchCheckinLoading"
          class="w-3 h-3 mr-1 animate-spin"
        />
        批量签到
      </Button>
      <Button
        size="sm"
        variant="outline"
        :disabled="selectedIds.length === 0 || batchBalanceLoading"
        @click="handleBatchBalance"
      >
        <Loader2
          v-if="batchBalanceLoading"
          class="w-3 h-3 mr-1 animate-spin"
        />
        批量余额
      </Button>
    </div>

    <Table>
      <TableHeader>
        <TableRow>
          <TableHead class="w-[40px]" />
          <TableHead>域名</TableHead>
          <TableHead>认证方式</TableHead>
          <TableHead>状态</TableHead>
          <TableHead>最近签到</TableHead>
          <TableHead>余额</TableHead>
          <TableHead>操作</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        <TableRow
          v-for="account in filteredAccounts"
          :key="account.id"
        >
          <TableCell>
            <Checkbox
              :model-value="selectedIds.includes(account.id)"
              @update:model-value="(v) => toggleSelect(account.id, v)"
            />
          </TableCell>
          <TableCell>
            <div>
              <div class="font-medium">
                {{ account.domain }}
              </div>
              <div
                v-if="account.site_url"
                class="text-xs text-muted-foreground truncate max-w-[200px]"
              >
                {{ account.site_url }}
              </div>
            </div>
          </TableCell>
          <TableCell>
            <Badge variant="outline">
              {{ account.auth_type }}
            </Badge>
          </TableCell>
          <TableCell>
            <Badge :variant="account.is_active ? 'default' : 'outline'">
              {{ account.is_active ? '启用' : '停用' }}
            </Badge>
          </TableCell>
          <TableCell>
            <div class="space-y-0.5">
              <Badge
                v-if="account.last_checkin_status"
                :variant="statusVariant(account.last_checkin_status)"
              >
                {{ statusLabel(account.last_checkin_status) }}
              </Badge>
              <span
                v-else
                class="text-xs text-muted-foreground"
              >-</span>
              <div
                v-if="account.last_checkin_at"
                class="text-xs text-muted-foreground"
              >
                {{ formatDate(account.last_checkin_at) }}
              </div>
            </div>
          </TableCell>
          <TableCell>
            <div class="space-y-0.5">
              <template v-if="account.last_balance_total != null">
                <span class="text-sm font-medium">{{ account.last_balance_total.toFixed(2) }}</span>
                <span
                  v-if="account.last_balance_currency"
                  class="text-xs text-muted-foreground ml-1"
                >{{ account.last_balance_currency }}</span>
              </template>
              <span
                v-else
                class="text-xs text-muted-foreground"
              >-</span>
              <div
                v-if="account.last_balance_at"
                class="text-xs text-muted-foreground"
              >
                {{ formatDate(account.last_balance_at) }}
              </div>
            </div>
          </TableCell>
          <TableCell>
            <div class="flex items-center gap-1">
              <Button
                size="sm"
                variant="outline"
                :disabled="checkinLoadingMap[account.id]"
                @click="handleCheckin(account)"
              >
                <Loader2
                  v-if="checkinLoadingMap[account.id]"
                  class="w-3 h-3 mr-1 animate-spin"
                />
                签到
              </Button>
              <Button
                size="sm"
                variant="outline"
                :disabled="balanceLoadingMap[account.id]"
                @click="handleBalance(account)"
              >
                <Loader2
                  v-if="balanceLoadingMap[account.id]"
                  class="w-3 h-3 mr-1 animate-spin"
                />
                余额
              </Button>
              <Button
                size="sm"
                variant="ghost"
                @click="$emit('viewDetail', account)"
              >
                详情
              </Button>
            </div>
          </TableCell>
        </TableRow>
      </TableBody>
    </Table>

    <div
      v-if="filteredAccounts.length === 0 && !loading"
      class="text-center py-8 text-sm text-muted-foreground"
    >
      {{ accounts.length === 0 ? '暂无账号数据' : '没有匹配的账号' }}
    </div>

    <Pagination
      v-if="total > pageSize"
      :current="page"
      :total="total"
      :page-size="pageSize"
      class="mt-2"
      @update:current="$emit('update:page', $event)"
      @update:page-size="$emit('update:pageSize', $event)"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { Loader2 } from 'lucide-vue-next'
import {
  Table, TableHeader, TableBody, TableRow, TableHead, TableCell,
  Badge, Button, Input, Checkbox, Pagination,
} from '@/components/ui'
import { siteManagementApi } from '../api'
import { useToast } from '@/composables/useToast'
import { parseApiError } from '@/utils/errorParser'
import type { SiteAccount } from '../types'

const props = defineProps<{
  sourceId: string
  accounts: SiteAccount[]
  total: number
  page: number
  pageSize: number
  loading?: boolean
}>()

const emit = defineEmits<{
  (e: 'update:page', value: number): void
  (e: 'update:pageSize', value: number): void
  (e: 'refresh'): void
  (e: 'viewDetail', account: SiteAccount): void
}>()

const { success, error } = useToast()

const search = ref('')
const selectedIds = ref<string[]>([])
const checkinLoadingMap = ref<Record<string, boolean>>({})
const balanceLoadingMap = ref<Record<string, boolean>>({})
const batchCheckinLoading = ref(false)
const batchBalanceLoading = ref(false)

const filteredAccounts = computed(() => {
  const keyword = search.value.trim().toLowerCase()
  if (!keyword) return props.accounts
  return props.accounts.filter(a =>
    a.domain.toLowerCase().includes(keyword) ||
    (a.site_url || '').toLowerCase().includes(keyword)
  )
})

const allSelected = computed(() => {
  return props.accounts.length > 0 && selectedIds.value.length === props.accounts.length
})

function toggleSelectAll(checked: boolean) {
  if (checked) {
    selectedIds.value = props.accounts.map(a => a.id)
  } else {
    selectedIds.value = []
  }
}

function toggleSelect(id: string, checked: boolean) {
  if (checked) {
    if (!selectedIds.value.includes(id)) {
      selectedIds.value = [...selectedIds.value, id]
    }
  } else {
    selectedIds.value = selectedIds.value.filter(i => i !== id)
  }
}

function formatDate(value: string | null): string {
  if (!value) return '-'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}

function statusVariant(status: string): 'default' | 'destructive' | 'outline' {
  if (status === 'success' || status === 'already_done') return 'default'
  if (status === 'failed' || status === 'auth_failed' || status === 'unknown_error') return 'destructive'
  return 'outline'
}

function statusLabel(status: string): string {
  const map: Record<string, string> = {
    success: '成功',
    already_done: '已签到',
    failed: '失败',
    auth_failed: '认证失败',
    auth_expired: '认证过期',
    not_configured: '未配置',
    not_supported: '不支持',
    skipped: '已跳过',
    unknown_error: '执行失败',
  }
  return map[status] || status
}

async function handleCheckin(account: SiteAccount) {
  checkinLoadingMap.value = { ...checkinLoadingMap.value, [account.id]: true }
  try {
    const result = await siteManagementApi.checkinAccount(props.sourceId, account.id)
    if (result.status === 'success' || result.status === 'already_done') {
      success(`[${account.domain}] 签到${result.status === 'already_done' ? '已完成' : '成功'}`)
    } else {
      error(`[${account.domain}] ${result.message || '签到失败'}`)
    }
    emit('refresh')
  } catch (err) {
    error(parseApiError(err, '签到失败'))
  } finally {
    checkinLoadingMap.value = { ...checkinLoadingMap.value, [account.id]: false }
  }
}

async function handleBalance(account: SiteAccount) {
  balanceLoadingMap.value = { ...balanceLoadingMap.value, [account.id]: true }
  try {
    const result = await siteManagementApi.balanceAccount(props.sourceId, account.id)
    success(`[${account.domain}] ${result.message || '余额查询成功'}`)
    emit('refresh')
  } catch (err) {
    error(parseApiError(err, '余额查询失败'))
  } finally {
    balanceLoadingMap.value = { ...balanceLoadingMap.value, [account.id]: false }
  }
}

async function handleBatchCheckin() {
  batchCheckinLoading.value = true
  try {
    await siteManagementApi.batchCheckin(props.sourceId, selectedIds.value)
    success('批量签到已触发')
    emit('refresh')
  } catch (err) {
    error(parseApiError(err, '批量签到失败'))
  } finally {
    batchCheckinLoading.value = false
  }
}

async function handleBatchBalance() {
  batchBalanceLoading.value = true
  try {
    await siteManagementApi.batchBalance(props.sourceId, selectedIds.value)
    success('批量余额查询已触发')
    emit('refresh')
  } catch (err) {
    error(parseApiError(err, '批量余额查询失败'))
  } finally {
    batchBalanceLoading.value = false
  }
}
</script>
