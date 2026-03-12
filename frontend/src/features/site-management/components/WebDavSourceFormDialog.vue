<template>
  <Dialog
    :open="open"
    @update:open="$emit('update:open', $event)"
  >
    <DialogContent class="sm:max-w-lg">
      <DialogHeader>
        <DialogTitle>{{ isEdit ? '编辑 WebDav 源' : '添加 WebDav 源' }}</DialogTitle>
        <DialogDescription>
          {{ isEdit ? '修改 WebDav 源的连接信息' : '添加一个新的 WebDav 备份源' }}
        </DialogDescription>
      </DialogHeader>

      <div class="space-y-4">
        <div class="space-y-2">
          <Label>名称</Label>
          <Input
            v-model="form.name"
            placeholder="例如：我的备份"
          />
        </div>
        <div class="space-y-2">
          <Label>WebDav URL</Label>
          <Input
            v-model="form.url"
            placeholder="https://dav.example.com/backup"
          />
        </div>
        <div class="space-y-2">
          <Label>用户名</Label>
          <Input
            v-model="form.username"
            placeholder="WebDav 用户名"
          />
        </div>
        <div class="space-y-2">
          <Label>密码</Label>
          <Input
            v-model="form.password"
            type="password"
            :placeholder="isEdit ? '留空表示不修改' : 'WebDav 密码'"
          />
        </div>
        <div class="rounded-lg border border-border/60 p-3 space-y-3">
          <div class="flex items-start justify-between gap-3">
            <div class="space-y-1">
              <Label>独立签到</Label>
              <p class="text-xs text-muted-foreground">
                为当前 WebDav 源单独启用每日签到调度
              </p>
            </div>
            <Switch v-model="form.checkin_enabled" />
          </div>
          <div class="space-y-2">
            <Label>签到时间</Label>
            <Input
              v-model="form.checkin_time"
              type="time"
              step="60"
              :disabled="!form.checkin_enabled"
            />
          </div>
        </div>
      </div>

      <DialogFooter class="flex-col sm:flex-row gap-2">
        <Button
          v-if="isEdit"
          variant="outline"
          :disabled="testLoading"
          @click="handleTest"
        >
          <Loader2
            v-if="testLoading"
            class="w-4 h-4 mr-1 animate-spin"
          />
          测试连接
        </Button>
        <div class="flex-1" />
        <Button
          variant="ghost"
          @click="$emit('update:open', false)"
        >
          取消
        </Button>
        <Button
          :disabled="saving"
          @click="handleSubmit"
        >
          <Loader2
            v-if="saving"
            class="w-4 h-4 mr-1 animate-spin"
          />
          {{ isEdit ? '保存' : '添加' }}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { Loader2 } from 'lucide-vue-next'
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter,
  Button, Input, Label, Switch,
} from '@/components/ui'
import { siteManagementApi } from '../api'
import { useToast } from '@/composables/useToast'
import { parseApiError } from '@/utils/errorParser'
import type { WebDavSource, CreateWebDavSourceRequest, UpdateWebDavSourceRequest } from '../types'

const props = defineProps<{
  open: boolean
  source?: WebDavSource | null
}>()

const emit = defineEmits<{
  (e: 'update:open', value: boolean): void
  (e: 'saved', source: WebDavSource): void
}>()

const { success, error } = useToast()
const isEdit = ref(false)
const saving = ref(false)
const testLoading = ref(false)

const form = ref({
  name: '',
  url: '',
  username: '',
  password: '',
  checkin_enabled: true,
  checkin_time: '04:00',
})

watch(() => props.open, (val) => {
  if (val) {
    if (props.source) {
      isEdit.value = true
      form.value = {
        name: props.source.name,
        url: props.source.url,
        username: props.source.username,
        password: '',
        checkin_enabled: props.source.checkin_enabled,
        checkin_time: props.source.checkin_time,
      }
    } else {
      isEdit.value = false
      form.value = {
        name: '',
        url: '',
        username: '',
        password: '',
        checkin_enabled: true,
        checkin_time: '04:00',
      }
    }
  }
})

async function handleTest() {
  if (!props.source) return
  testLoading.value = true
  try {
    const result = await siteManagementApi.testConnection(props.source.id)
    if (result.success) {
      success('连接测试成功')
    } else {
      error(`连接测试失败: ${result.message}`)
    }
  } catch (err) {
    error(parseApiError(err, '连接测试失败'))
  } finally {
    testLoading.value = false
  }
}

async function handleSubmit() {
  if (!form.value.name.trim()) {
    error('请输入名称')
    return
  }
  if (!form.value.url.trim()) {
    error('请输入 WebDav URL')
    return
  }
  if (!form.value.username.trim()) {
    error('请输入用户名')
    return
  }
  if (!isEdit.value && !form.value.password) {
    error('请输入密码')
    return
  }

  saving.value = true
  try {
    let result: WebDavSource
    if (isEdit.value && props.source) {
      const payload: UpdateWebDavSourceRequest = {
        name: form.value.name.trim(),
        url: form.value.url.trim(),
        username: form.value.username.trim(),
        checkin_enabled: form.value.checkin_enabled,
        checkin_time: form.value.checkin_time,
      }
      if (form.value.password) {
        payload.password = form.value.password
      }
      result = await siteManagementApi.updateSource(props.source.id, payload)
      success('WebDav 源已更新')
    } else {
      const payload: CreateWebDavSourceRequest = {
        name: form.value.name.trim(),
        url: form.value.url.trim(),
        username: form.value.username.trim(),
        password: form.value.password,
        checkin_enabled: form.value.checkin_enabled,
        checkin_time: form.value.checkin_time,
      }
      result = await siteManagementApi.createSource(payload)
      success('WebDav 源已添加')
    }
    emit('saved', result)
    emit('update:open', false)
  } catch (err) {
    error(parseApiError(err, isEdit.value ? '更新失败' : '添加失败'))
  } finally {
    saving.value = false
  }
}
</script>
