<template>
  <Card class="border-border/60 p-6">
    <div class="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
      <div>
        <h3 class="text-lg font-semibold text-foreground">Key 池</h3>
        <p class="mt-2 text-sm text-muted-foreground">新增、导入、启停和删除当前服务的真实上游 Key。</p>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <Input v-model="email" class="w-[220px]" placeholder="邮箱，可选" />
        <Input v-model="key" class="w-[280px]" placeholder="输入真实 key" />
        <Button :disabled="!key.trim()" @click="submitCreate">
          <Plus class="mr-1.5 h-4 w-4" />新增
        </Button>
        <Button variant="outline" @click="$emit('import')">
          <Upload class="mr-1.5 h-4 w-4" />批量导入
        </Button>
      </div>
    </div>

    <div class="mt-5 overflow-hidden rounded-2xl border border-border/60">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Key</TableHead>
            <TableHead>邮箱</TableHead>
            <TableHead>真实额度</TableHead>
            <TableHead>代理统计</TableHead>
            <TableHead>状态</TableHead>
            <TableHead class="text-right">操作</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow v-if="keys.length === 0">
            <TableCell colspan="6" class="py-10 text-center text-muted-foreground">暂无 Key</TableCell>
          </TableRow>
          <TableRow v-for="row in keys" :key="row.id">
            <TableCell>
              <div class="font-mono text-xs text-foreground">{{ row.key_masked }}</div>
              <div class="mt-1 text-xs text-muted-foreground">最近同步 {{ row.usage_synced_at || '未同步' }}</div>
            </TableCell>
            <TableCell>
              <div class="text-sm text-foreground">{{ row.email || '-' }}</div>
              <div class="mt-1 text-xs text-muted-foreground">计划 {{ row.usage_account_plan || '-' }}</div>
            </TableCell>
            <TableCell>
              <div class="text-sm text-foreground">剩余 {{ row.usage_account_remaining ?? row.usage_key_remaining ?? 0 }}</div>
              <div class="mt-1 text-xs text-muted-foreground">
                已用 {{ row.usage_account_used ?? row.usage_key_used ?? 0 }} / {{ row.usage_account_limit ?? row.usage_key_limit ?? 0 }}
              </div>
            </TableCell>
            <TableCell>
              <div class="text-sm text-foreground">成功 {{ row.total_used }} / 失败 {{ row.total_failed }}</div>
              <div class="mt-1 text-xs text-muted-foreground">连续失败 {{ row.consecutive_fails }}</div>
            </TableCell>
            <TableCell>
              <Badge :variant="row.active ? 'success' : 'secondary'">{{ row.active ? '启用' : '停用' }}</Badge>
            </TableCell>
            <TableCell class="text-right">
              <div class="inline-flex items-center gap-2">
                <Button variant="outline" size="sm" @click="$emit('toggle', row)">
                  <Power class="mr-1 h-3.5 w-3.5" />{{ row.active ? '停用' : '启用' }}
                </Button>
                <Button variant="outline" size="sm" class="text-destructive" @click="$emit('delete', row)">
                  <Trash2 class="mr-1 h-3.5 w-3.5" />删除
                </Button>
              </div>
            </TableCell>
          </TableRow>
        </TableBody>
      </Table>
    </div>
  </Card>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { Plus, Power, Trash2, Upload } from 'lucide-vue-next'
import {
  Badge,
  Button,
  Card,
  Input,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui'
import type { SearchPoolKey } from '../types'

defineProps<{
  keys: SearchPoolKey[]
}>()

const emit = defineEmits<{
  create: [{ key: string; email?: string }]
  import: []
  toggle: [SearchPoolKey]
  delete: [SearchPoolKey]
}>()

const key = ref('')
const email = ref('')

function submitCreate() {
  const normalizedKey = key.value.trim()
  if (!normalizedKey) return
  emit('create', { key: normalizedKey, email: email.value.trim() || undefined })
  key.value = ''
  email.value = ''
}
</script>
