<template>
  <Dialog
    :open="open"
    title="批量导入 Tavily 账号"
    description="支持 JSON / CSV，内置示例与字段说明，避免格式错误"
    size="3xl"
    @update:open="emit('update:open', $event)"
  >
    <div class="space-y-4">
      <div class="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          :variant="fileType === 'json' ? 'default' : 'outline'"
          @click="setFileType('json')"
        >
          JSON
        </Button>
        <Button
          size="sm"
          :variant="fileType === 'csv' ? 'default' : 'outline'"
          @click="setFileType('csv')"
        >
          CSV
        </Button>
      </div>

      <div class="rounded-md border p-3 space-y-2">
        <div class="text-sm font-medium">
          导入说明
        </div>
        <ul class="text-xs text-muted-foreground space-y-1 list-disc pl-4">
          <li><code>email</code> 必填，且必须是邮箱格式。</li>
          <li><code>password</code> 新账号必填；旧账号在覆盖模式可选更新。</li>
          <li><code>api_key</code> 可选，单值字符串。</li>
          <li>冲突策略：<code>skip</code> 跳过账号字段但可补 API Key；<code>overwrite</code> 覆盖；<code>error</code> 遇冲突中止。</li>
        </ul>
      </div>

      <div class="rounded-md border p-3 space-y-2">
        <div class="flex items-center justify-between gap-2">
          <div class="text-sm font-medium">
            导入示例（{{ fileType.toUpperCase() }}）
          </div>
          <Button
            size="sm"
            variant="outline"
            @click="copyExample"
          >
            复制示例
          </Button>
        </div>
        <pre class="max-h-56 overflow-auto rounded bg-muted/40 p-3 text-xs leading-5">{{ currentExample }}</pre>
      </div>

      <div class="grid gap-3 md:grid-cols-2">
        <div>
          <Label class="mb-2 block">冲突处理</Label>
          <Select
            :model-value="mergeMode"
            @update:model-value="value => emit('update:mergeMode', value as TavilyImportMergeMode)"
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="skip">
                skip - 保留已有账号
              </SelectItem>
              <SelectItem value="overwrite">
                overwrite - 覆盖已有账号
              </SelectItem>
              <SelectItem value="error">
                error - 冲突即中止
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div>
          <Label class="mb-2 block">选择文件</Label>
          <input
            ref="fileInput"
            class="hidden"
            type="file"
            accept=".json,.csv"
            @change="handleFileChange"
          >
          <div class="flex items-center gap-2">
            <Button
              variant="outline"
              @click="fileInput?.click()"
            >
              选择文件
            </Button>
            <span class="text-xs text-muted-foreground truncate">{{ fileName || '未选择文件' }}</span>
          </div>
        </div>
      </div>

      <div
        v-if="localError"
        class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive"
      >
        {{ localError }}
      </div>
    </div>

    <template #footer>
      <Button
        variant="outline"
        @click="emit('update:open', false)"
      >
        取消
      </Button>
      <Button
        :disabled="submitting"
        @click="submitImport"
      >
        {{ submitting ? '导入中...' : '开始导入' }}
      </Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { Button, Dialog, Label, Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui'
import type { TavilyImportFileType, TavilyImportMergeMode } from '@/features/tavily-pool/types'

const props = defineProps<{
  open: boolean
  submitting: boolean
  fileType: TavilyImportFileType
  mergeMode: TavilyImportMergeMode
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  'update:fileType': [value: TavilyImportFileType]
  'update:mergeMode': [value: TavilyImportMergeMode]
  submit: [payload: { file_type: TavilyImportFileType; merge_mode: TavilyImportMergeMode; content: string }]
}>()

const JSON_EXAMPLE = `[
  {
    "email": "user1@example.com",
    "password": "plain_password",
    "api_key": "tvly-aaa",
    "notes": "可选备注",
    "source": "import"
  }
]`

const CSV_EXAMPLE = `email,password,api_key,notes,source
user1@example.com,plain_password,tvly-aaa,可选备注,import`

const fileInput = ref<HTMLInputElement | null>(null)
const fileName = ref('')
const fileContent = ref('')
const localError = ref('')

const currentExample = computed(() => (props.fileType === 'json' ? JSON_EXAMPLE : CSV_EXAMPLE))

function setFileType(nextType: TavilyImportFileType) {
  emit('update:fileType', nextType)
}

async function copyExample() {
  try {
    await navigator.clipboard.writeText(currentExample.value)
  } catch {
    // ignore clipboard errors in unsupported contexts
  }
}

function inferFileType(fileNameValue: string): TavilyImportFileType | null {
  const lower = fileNameValue.toLowerCase()
  if (lower.endsWith('.json')) return 'json'
  if (lower.endsWith('.csv')) return 'csv'
  return null
}

function handleFileChange(event: Event) {
  localError.value = ''
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  if (!file) {
    return
  }

  const inferred = inferFileType(file.name)
  if (!inferred) {
    localError.value = '仅支持 .json 或 .csv 文件'
    return
  }

  fileName.value = file.name
  emit('update:fileType', inferred)

  const reader = new FileReader()
  reader.onload = () => {
    fileContent.value = String(reader.result || '')
  }
  reader.onerror = () => {
    localError.value = '读取文件失败'
  }
  reader.readAsText(file)
}

function submitImport() {
  localError.value = ''
  if (!fileContent.value.trim()) {
    localError.value = '请先选择有效导入文件'
    return
  }

  if (props.fileType === 'json') {
    try {
      JSON.parse(fileContent.value)
    } catch {
      localError.value = 'JSON 格式错误，请检查示例'
      return
    }
  }

  emit('submit', {
    file_type: props.fileType,
    merge_mode: props.mergeMode,
    content: fileContent.value,
  })
}
</script>
