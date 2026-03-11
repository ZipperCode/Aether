<template>
  <Dialog
    :open="open"
    @update:open="$emit('update:open', $event)"
  >
    <DialogContent class="sm:max-w-lg">
      <DialogHeader>
        <DialogTitle>账号详情</DialogTitle>
        <DialogDescription>
          {{ account?.domain || '-' }}
        </DialogDescription>
      </DialogHeader>

      <div
        v-if="account"
        class="space-y-3 text-sm"
      >
        <div class="grid grid-cols-2 gap-3">
          <div>
            <div class="text-muted-foreground">
              域名
            </div>
            <div class="font-medium">
              {{ account.domain }}
            </div>
          </div>
          <div>
            <div class="text-muted-foreground">
              站点 URL
            </div>
            <div class="font-medium truncate">
              {{ account.site_url || '-' }}
            </div>
          </div>
          <div>
            <div class="text-muted-foreground">
              认证方式
            </div>
            <div class="font-medium">
              {{ account.auth_type }}
            </div>
          </div>
          <div>
            <div class="text-muted-foreground">
              状态
            </div>
            <Badge :variant="account.is_active ? 'default' : 'outline'">
              {{ account.is_active ? '启用' : '停用' }}
            </Badge>
          </div>
          <div>
            <div class="text-muted-foreground">
              签到开关
            </div>
            <Badge :variant="account.checkin_enabled ? 'default' : 'outline'">
              {{ account.checkin_enabled ? '开启' : '关闭' }}
            </Badge>
          </div>
          <div>
            <div class="text-muted-foreground">
              余额同步
            </div>
            <Badge :variant="account.balance_sync_enabled ? 'default' : 'outline'">
              {{ account.balance_sync_enabled ? '开启' : '关闭' }}
            </Badge>
          </div>
        </div>

        <Separator />

        <div class="space-y-2">
          <div class="font-medium">
            签到状态
          </div>
          <div class="grid grid-cols-2 gap-2 text-xs">
            <div>
              <span class="text-muted-foreground">状态: </span>
              <Badge
                v-if="account.last_checkin_status"
                :variant="account.last_checkin_status === 'success' ? 'default' : 'destructive'"
              >
                {{ account.last_checkin_status }}
              </Badge>
              <span v-else>-</span>
            </div>
            <div>
              <span class="text-muted-foreground">时间: </span>{{ formatDate(account.last_checkin_at) }}
            </div>
            <div class="col-span-2">
              <span class="text-muted-foreground">消息: </span>{{ account.last_checkin_message || '-' }}
            </div>
          </div>
        </div>

        <div class="space-y-2">
          <div class="font-medium">
            余额状态
          </div>
          <div class="grid grid-cols-2 gap-2 text-xs">
            <div>
              <span class="text-muted-foreground">状态: </span>
              <Badge
                v-if="account.last_balance_status"
                :variant="account.last_balance_status === 'success' ? 'default' : 'destructive'"
              >
                {{ account.last_balance_status }}
              </Badge>
              <span v-else>-</span>
            </div>
            <div>
              <span class="text-muted-foreground">余额: </span>
              {{ account.last_balance_total != null ? `${account.last_balance_total.toFixed(2)} ${account.last_balance_currency || ''}` : '-' }}
            </div>
            <div>
              <span class="text-muted-foreground">时间: </span>{{ formatDate(account.last_balance_at) }}
            </div>
            <div>
              <span class="text-muted-foreground">消息: </span>{{ account.last_balance_message || '-' }}
            </div>
          </div>
        </div>

        <Separator />

        <div class="grid grid-cols-2 gap-2 text-xs text-muted-foreground">
          <div>创建时间: {{ formatDate(account.created_at) }}</div>
          <div>更新时间: {{ formatDate(account.updated_at) }}</div>
        </div>
      </div>

      <DialogFooter>
        <Button
          variant="ghost"
          @click="$emit('update:open', false)"
        >
          关闭
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter,
  Badge, Button, Separator,
} from '@/components/ui'
import type { SiteAccount } from '../types'

defineProps<{
  open: boolean
  account: SiteAccount | null
}>()

defineEmits<{
  (e: 'update:open', value: boolean): void
}>()

function formatDate(value: string | null): string {
  if (!value) return '-'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}
</script>
