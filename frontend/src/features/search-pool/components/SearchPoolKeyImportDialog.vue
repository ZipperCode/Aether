<template>
  <Dialog :open="open" @update:open="emit('update:open', $event)">
    <DialogContent class="sm:max-w-2xl">
      <DialogHeader>
        <DialogTitle>批量导入 Key</DialogTitle>
        <DialogDescription>
          支持一行一个 key，或使用 `email,key`、`email,password,key` 格式导入。
        </DialogDescription>
      </DialogHeader>

      <div class="space-y-2 py-2">
        <Label for="key-import-content">导入内容</Label>
        <Textarea id="key-import-content" v-model="content" class="min-h-[240px] font-mono text-xs" placeholder="alice@example.com,fc-key-001&#10;bob@example.com,password,fc-key-002&#10;fc-key-003" />
      </div>

      <DialogFooter>
        <Button variant="outline" @click="emit('update:open', false)">取消</Button>
        <Button :disabled="submitting || !content.trim()" @click="emit('submit', content)">导入</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Label,
  Textarea,
} from '@/components/ui'

const props = defineProps<{
  open: boolean
  submitting?: boolean
}>()

const emit = defineEmits<{
  'update:open': [boolean]
  submit: [string]
}>()

const content = ref('')

watch(() => props.open, (open) => {
  if (!open) {
    content.value = ''
  }
})
</script>
