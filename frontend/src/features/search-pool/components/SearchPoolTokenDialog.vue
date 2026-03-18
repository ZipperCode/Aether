<template>
  <Dialog :open="open" @update:open="emit('update:open', $event)">
    <DialogContent class="sm:max-w-lg">
      <DialogHeader>
        <DialogTitle>{{ mode === 'edit' ? '编辑 Token' : '创建 Token' }}</DialogTitle>
        <DialogDescription>
          配置当前服务工作台下的网关访问 Token 与小时、每日、每月限制。
        </DialogDescription>
      </DialogHeader>

      <div class="space-y-4 py-2">
        <div class="space-y-2">
          <Label for="token-name">名称</Label>
          <Input id="token-name" v-model="form.name" placeholder="例如：team-a" />
        </div>

        <div class="grid gap-4 sm:grid-cols-3">
          <div class="space-y-2">
            <Label for="token-hourly">小时限制</Label>
            <Input id="token-hourly" v-model.number="form.hourly_limit" type="number" min="0" />
          </div>
          <div class="space-y-2">
            <Label for="token-daily">每日限制</Label>
            <Input id="token-daily" v-model.number="form.daily_limit" type="number" min="0" />
          </div>
          <div class="space-y-2">
            <Label for="token-monthly">每月限制</Label>
            <Input id="token-monthly" v-model.number="form.monthly_limit" type="number" min="0" />
          </div>
        </div>
      </div>

      <DialogFooter>
        <Button variant="outline" @click="emit('update:open', false)">取消</Button>
        <Button :disabled="submitting" @click="submit">{{ mode === 'edit' ? '保存' : '创建' }}</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import { reactive, watch } from 'vue'
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
} from '@/components/ui'
import type { SearchPoolToken } from '../types'

const props = defineProps<{
  open: boolean
  mode: 'create' | 'edit'
  token?: SearchPoolToken | null
  submitting?: boolean
}>()

const emit = defineEmits<{
  'update:open': [boolean]
  submit: [{ name: string; hourly_limit: number; daily_limit: number; monthly_limit: number }]
}>()

const form = reactive({
  name: '',
  hourly_limit: 0,
  daily_limit: 0,
  monthly_limit: 0,
})

watch(
  () => [props.open, props.token, props.mode],
  () => {
    form.name = props.token?.name || ''
    form.hourly_limit = props.token?.hourly_limit || 0
    form.daily_limit = props.token?.daily_limit || 0
    form.monthly_limit = props.token?.monthly_limit || 0
  },
  { immediate: true }
)

function submit() {
  emit('submit', {
    name: form.name.trim(),
    hourly_limit: Number(form.hourly_limit) || 0,
    daily_limit: Number(form.daily_limit) || 0,
    monthly_limit: Number(form.monthly_limit) || 0,
  })
}
</script>
