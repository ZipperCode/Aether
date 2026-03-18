<template>
  <Card class="border-border/60 p-6">
    <div class="flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
      <div>
        <h3 class="text-lg font-semibold text-foreground">Token 池</h3>
        <p class="mt-2 text-sm text-muted-foreground">管理当前服务的访问 Token、额度限制与基础调用统计。</p>
      </div>
      <Button @click="$emit('create')">
        <Plus class="mr-1.5 h-4 w-4" />创建 Token
      </Button>
    </div>

    <div class="mt-5 overflow-hidden rounded-2xl border border-border/60">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>名称</TableHead>
            <TableHead>Token</TableHead>
            <TableHead>配额</TableHead>
            <TableHead>用量统计</TableHead>
            <TableHead class="text-right">操作</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow v-if="tokens.length === 0">
            <TableCell colspan="5" class="py-10 text-center text-muted-foreground">暂无 Token</TableCell>
          </TableRow>
          <TableRow v-for="token in tokens" :key="token.id">
            <TableCell>
              <div class="font-medium text-foreground">{{ token.name || 'default' }}</div>
              <div class="mt-1 text-xs text-muted-foreground">{{ token.updated_at || '未更新' }}</div>
            </TableCell>
            <TableCell class="max-w-[320px] font-mono text-xs break-all">{{ token.token }}</TableCell>
            <TableCell>
              <div class="text-sm">时 {{ token.hourly_limit }} / 日 {{ token.daily_limit }} / 月 {{ token.monthly_limit }}</div>
            </TableCell>
            <TableCell>
              <div class="text-sm text-foreground">成功 {{ token.usage_success }} / 失败 {{ token.usage_failed }}</div>
              <div class="mt-1 text-xs text-muted-foreground">本月 {{ token.usage_this_month }}</div>
            </TableCell>
            <TableCell class="text-right">
              <div class="inline-flex items-center gap-2">
                <Button variant="outline" size="sm" @click="$emit('edit', token)">
                  <Pencil class="mr-1 h-3.5 w-3.5" />编辑
                </Button>
                <Button variant="outline" size="sm" class="text-destructive" @click="$emit('delete', token)">
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
import { Pencil, Plus, Trash2 } from 'lucide-vue-next'
import {
  Button,
  Card,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui'
import type { SearchPoolToken } from '../types'

defineProps<{
  tokens: SearchPoolToken[]
}>()

defineEmits<{
  create: []
  edit: [SearchPoolToken]
  delete: [SearchPoolToken]
}>()
</script>
