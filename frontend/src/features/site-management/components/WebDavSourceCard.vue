<template>
  <Card
    class="cursor-pointer hover:border-primary/40 transition-colors"
    @click="$emit('click')"
  >
    <div class="p-4 space-y-3">
      <div class="flex items-start justify-between gap-2">
        <div class="flex-1 min-w-0">
          <h3 class="font-medium truncate">
            {{ source.name }}
          </h3>
          <p class="text-xs text-muted-foreground truncate mt-1">
            {{ source.url }}
          </p>
        </div>
        <Badge :variant="source.is_active ? 'default' : 'outline'">
          {{ source.is_active ? '启用' : '停用' }}
        </Badge>
      </div>

      <div class="flex items-center gap-3 text-xs text-muted-foreground">
        <span>{{ source.account_count }} 个账号</span>
        <span>{{ source.checkin_enabled ? `签到 ${source.checkin_time}` : '签到已关闭' }}</span>
        <span v-if="source.last_sync_at">
          上次同步: {{ formatDate(source.last_sync_at) }}
        </span>
        <Badge
          v-if="source.last_sync_status"
          :variant="syncStatusVariant(source.last_sync_status)"
          class="text-[10px]"
        >
          {{ syncStatusLabel(source.last_sync_status) }}
        </Badge>
      </div>

      <div class="flex items-center gap-2 pt-1">
        <Button
          size="sm"
          variant="outline"
          :disabled="syncLoading"
          @click.stop="$emit('sync')"
        >
          <Loader2
            v-if="syncLoading"
            class="w-3 h-3 mr-1 animate-spin"
          />
          同步
        </Button>
        <Button
          size="sm"
          variant="outline"
          @click.stop="$emit('edit')"
        >
          编辑
        </Button>
        <Button
          size="sm"
          variant="outline"
          class="text-destructive hover:text-destructive"
          @click.stop="$emit('delete')"
        >
          删除
        </Button>
      </div>
    </div>
  </Card>
</template>

<script setup lang="ts">
import { Loader2 } from 'lucide-vue-next'
import { Card, Badge, Button } from '@/components/ui'
import type { WebDavSource } from '../types'

defineProps<{
  source: WebDavSource
  syncLoading?: boolean
}>()

defineEmits<{
  (e: 'click'): void
  (e: 'sync'): void
  (e: 'edit'): void
  (e: 'delete'): void
}>()

function formatDate(value: string | null): string {
  if (!value) return '-'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}

function syncStatusVariant(status: string): 'default' | 'destructive' | 'outline' {
  if (status === 'success') return 'default'
  if (status === 'failed') return 'destructive'
  return 'outline'
}

function syncStatusLabel(status: string): string {
  if (status === 'success') return '成功'
  if (status === 'failed') return '失败'
  if (status === 'running') return '运行中'
  return status
}
</script>
