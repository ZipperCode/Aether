<template>
  <Dialog
    :model-value="isOpen"
    title="编辑账号"
    description="修改 OAuth 账号配置"
    :icon="SquarePen"
    size="xl"
    @update:model-value="handleDialogUpdate"
  >
    <form
      class="space-y-3"
      autocomplete="off"
      @submit.prevent="handleSave"
    >
      <!-- 基本信息：账号名称 + 备注 -->
      <div class="grid grid-cols-2 gap-3">
        <div>
          <Label for="name">账号名称 *</Label>
          <Input
            id="name"
            v-model="form.name"
            required
            placeholder="例如：主账号、备用账号"
            maxlength="100"
            autocomplete="off"
          />
        </div>
        <div>
          <Label for="note">备注</Label>
          <Input
            id="note"
            v-model="form.note"
            placeholder="可选的备注信息"
          />
        </div>
      </div>

      <!-- 配置项 -->
      <div class="grid grid-cols-4 gap-3">
        <div>
          <Label
            for="internal_priority"
            class="text-xs"
          >优先级</Label>
          <Input
            id="internal_priority"
            v-model.number="form.internal_priority"
            type="number"
            min="0"
            placeholder="10"
            class="h-8"
          />
          <p class="text-xs text-muted-foreground mt-0.5">
            越小越优先
          </p>
        </div>
        <div>
          <Label
            for="rpm_limit"
            class="text-xs"
          >RPM 限制</Label>
          <Input
            id="rpm_limit"
            :model-value="form.rpm_limit ?? ''"
            type="number"
            min="1"
            max="10000"
            placeholder="自适应"
            class="h-8"
            @update:model-value="(v) => form.rpm_limit = parseNullableNumberInput(v, { min: 1, max: 10000 })"
          />
          <p class="text-xs text-muted-foreground mt-0.5">
            留空自适应
          </p>
        </div>
        <div>
          <Label
            for="concurrent_limit"
            class="text-xs"
          >并发请求上限</Label>
          <Input
            id="concurrent_limit"
            :model-value="form.concurrent_limit ?? ''"
            type="number"
            min="0"
            placeholder="不限制"
            class="h-8"
            @update:model-value="(v) => form.concurrent_limit = parseNullableNumberInput(v, { min: 0 })"
          />
          <p class="text-xs text-muted-foreground mt-0.5">
            留空或 0 表示不限制
          </p>
        </div>
        <div>
          <Label
            for="cache_ttl_minutes"
            class="text-xs"
          >缓存 TTL</Label>
          <Input
            id="cache_ttl_minutes"
            :model-value="form.cache_ttl_minutes ?? ''"
            type="number"
            min="0"
            max="60"
            class="h-8"
            @update:model-value="(v) => form.cache_ttl_minutes = parseNumberInput(v, { min: 0, max: 60 }) ?? 5"
          />
          <p class="text-xs text-muted-foreground mt-0.5">
            分钟，0禁用
          </p>
        </div>
        <div>
          <Label
            for="max_probe_interval_minutes"
            class="text-xs"
          >熔断探测</Label>
          <Input
            id="max_probe_interval_minutes"
            :model-value="form.max_probe_interval_minutes ?? ''"
            type="number"
            min="0"
            max="32"
            placeholder="32"
            class="h-8"
            @update:model-value="(v) => form.max_probe_interval_minutes = parseNumberInput(v, { min: 0, max: 32 }) ?? 32"
          />
          <p class="text-xs text-muted-foreground mt-0.5">
            0-32分钟
          </p>
        </div>
      </div>

      <!-- 自动获取模型 -->
      <div class="space-y-3 py-2 px-3 rounded-md border border-border/60 bg-muted/30">
        <div class="flex items-center justify-between">
          <div class="space-y-0.5">
            <Label class="text-sm font-medium">自动获取上游可用模型</Label>
            <p class="text-xs text-muted-foreground">
              定时更新上游模型, 配合模型映射使用
            </p>
            <p
              v-if="showAutoFetchWarning"
              class="text-xs text-amber-600 dark:text-amber-400"
            >
              {{ autoFetchWarningMessage }}
            </p>
          </div>
          <Switch v-model="form.auto_fetch_models" />
        </div>

        <!-- 模型过滤规则（仅当开启自动获取时显示） -->
        <div
          v-if="form.auto_fetch_models"
          class="space-y-2 pt-2 border-t border-border/40"
        >
          <div class="grid grid-cols-2 gap-3">
            <div>
              <Label class="text-xs">包含规则</Label>
              <Input
                v-model="form.model_include_patterns_text"
                placeholder="gpt-*, claude-*, 留空包含全部"
                class="h-8 text-sm"
              />
            </div>
            <div>
              <Label class="text-xs">排除规则</Label>
              <Input
                v-model="form.model_exclude_patterns_text"
                placeholder="*-preview, *-beta"
                class="h-8 text-sm"
              />
            </div>
          </div>
          <p class="text-xs text-muted-foreground">
            逗号分隔，支持 * ? 通配符，不区分大小写
          </p>
        </div>
      </div>

      <!-- Antigravity 精确模型到配额组显式绑定 -->
      <div
        v-if="isAntigravityKey"
        class="space-y-2.5 rounded-md border border-border/60 bg-muted/30 px-3 py-2.5"
        data-testid="antigravity-quota-bindings-section"
      >
        <div class="flex flex-wrap items-start justify-between gap-2">
          <div class="min-w-0 flex-1 space-y-0.5">
            <div class="flex flex-wrap items-center gap-2">
              <Label class="text-sm font-medium">Antigravity 模型配额组绑定</Label>
              <span
                class="inline-flex h-5 items-center rounded-md px-2 text-[11px] font-medium"
                :class="savedBindingCount > 0 ? 'bg-primary/10 text-primary' : 'bg-muted text-muted-foreground'"
              >
                {{ savedBindingCount > 0 ? `已保存 ${savedBindingCount} 项` : '未绑定' }}
              </span>
            </div>
            <p class="text-xs text-muted-foreground">
              显式关联该账号下精确上游模型与配额组；未绑定模型仅按自身模型配额判定，点击右侧按钮独立保存生效。
            </p>
          </div>
          <div class="flex flex-wrap items-center gap-1.5 shrink-0">
            <Button
              type="button"
              variant="outline"
              class="h-7 px-2.5 text-xs"
              :disabled="!canAddBindingRow"
              data-testid="antigravity-binding-add"
              @click="addBindingRow"
            >
              添加绑定
            </Button>
            <Button
              type="button"
              variant="outline"
              class="h-7 px-2.5 text-xs"
              :disabled="!canClearBindings"
              data-testid="antigravity-binding-clear"
              @click="handleClearBindings"
            >
              {{ savingBindings && savingBindingAction === 'clear' ? '清空中...' : '清空绑定' }}
            </Button>
            <Button
              type="button"
              class="h-7 px-2.5 text-xs"
              :disabled="!canSaveBindings"
              data-testid="antigravity-binding-save"
              @click="handleSaveBindings"
            >
              {{ savingBindings && savingBindingAction === 'save' ? '保存中...' : '保存绑定' }}
            </Button>
          </div>
        </div>

        <div
          v-if="hasStaleBindingRows"
          class="rounded-md border border-amber-500/30 bg-amber-500/10 px-2.5 py-1.5 text-xs text-amber-700 dark:text-amber-400"
          data-testid="antigravity-binding-stale-warning"
        >
          部分已保存绑定对应的模型或配额组在最新快照中不存在，请重新选择当前候选项、移除失效行，或点击“清空绑定”。
        </div>
        <div
          v-else-if="bindingValidationMessage"
          class="text-xs text-amber-600 dark:text-amber-400"
          data-testid="antigravity-binding-validation"
        >
          {{ bindingValidationMessage }}
        </div>

        <div
          v-if="bindingError"
          role="alert"
          class="rounded-md border border-destructive/30 bg-destructive/10 px-2.5 py-1.5 text-xs text-destructive"
          data-testid="antigravity-binding-error"
        >
          {{ bindingError }}
        </div>
        <div
          v-else-if="bindingSuccess"
          role="status"
          class="rounded-md border border-emerald-500/30 bg-emerald-500/10 px-2.5 py-1.5 text-xs text-emerald-700 dark:text-emerald-400"
          data-testid="antigravity-binding-success"
        >
          {{ bindingSuccess }}
        </div>

        <div
          v-if="bindingRows.length > 0"
          class="space-y-2 border-t border-border/40 pt-2"
          data-testid="antigravity-binding-rows"
        >
          <div
            v-for="(row, index) in bindingRows"
            :key="row.uid"
            class="grid grid-cols-1 gap-2 rounded-md border p-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] sm:items-center"
            :class="(isRowStaleModel(row) || isRowStaleGroup(row)) ? 'border-amber-500/50 bg-amber-500/5' : 'border-border/50 bg-background/70'"
            :data-testid="`antigravity-binding-row-${index}`"
          >
            <div class="min-w-0 space-y-1">
              <label
                :for="`antigravity-binding-model-${row.uid}`"
                class="block text-[11px] text-muted-foreground"
              >
                精确上游模型
              </label>
              <select
                :id="`antigravity-binding-model-${row.uid}`"
                :value="row.model"
                :disabled="savingBindings || saving"
                :aria-label="`绑定行 ${index + 1} 精确上游模型`"
                class="h-8 w-full min-w-0 rounded-md border border-input bg-background px-2 text-xs font-mono text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
                data-testid="antigravity-binding-model-select"
                @change="handleRowModelChange(row.uid, $event)"
              >
                <option
                  value=""
                  disabled
                >
                  请选择上游模型...
                </option>
                <option
                  v-if="isRowStaleModel(row)"
                  :value="row.model"
                >
                  {{ row.model }}（不在当前发现列表）
                </option>
                <option
                  v-for="option in modelOptions"
                  :key="option.model"
                  :value="option.model"
                  :disabled="isModelTakenByOtherRow(option.model, index)"
                >
                  {{ formatModelOptionText(option) }}
                </option>
              </select>
            </div>

            <div class="min-w-0 space-y-1">
              <label
                :for="`antigravity-binding-group-${row.uid}`"
                class="block text-[11px] text-muted-foreground"
              >
                关联配额组
              </label>
              <select
                :id="`antigravity-binding-group-${row.uid}`"
                :value="row.groupKey"
                :disabled="savingBindings || saving"
                :aria-label="`绑定行 ${index + 1} 关联配额组`"
                class="h-8 w-full min-w-0 rounded-md border border-input bg-background px-2 text-xs text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
                data-testid="antigravity-binding-group-select"
                @change="handleRowGroupChange(row.uid, $event)"
              >
                <option
                  value=""
                  disabled
                >
                  请选择配额组...
                </option>
                <option
                  v-if="isRowStaleGroup(row)"
                  :value="row.groupKey"
                >
                  失效配额组（{{ row.groupKey }}）
                </option>
                <option
                  v-for="group in groupOptions"
                  :key="group.key"
                  :value="group.key"
                >
                  {{ formatGroupOptionText(group) }}
                </option>
              </select>
            </div>

            <div class="flex justify-end sm:pt-4">
              <Button
                type="button"
                variant="ghost"
                class="h-8 px-2 text-xs text-muted-foreground hover:text-destructive"
                :disabled="savingBindings || saving"
                :aria-label="`移除绑定行 ${index + 1}`"
                data-testid="antigravity-binding-remove"
                @click="removeBindingRow(row.uid)"
              >
                移除
              </Button>
            </div>
          </div>
        </div>
        <p
          v-else-if="modelOptions.length === 0 || groupOptions.length === 0"
          class="border-t border-border/40 pt-2 text-xs text-muted-foreground"
          data-testid="antigravity-binding-empty"
        >
          {{ emptyBindingPrerequisitesText }}
        </p>
        <p
          v-else
          class="border-t border-border/40 pt-2 text-xs text-muted-foreground"
          data-testid="antigravity-binding-unbound-hint"
        >
          当前未配置配额组绑定；点击“添加绑定”可为指定上游模型显式关联配额组。
        </p>
      </div>
    </form>

    <template #footer>
      <Button
        variant="outline"
        @click="handleCancel"
      >
        取消
      </Button>
      <Button
        :disabled="saving || !canSave"
        @click="handleSave"
      >
        {{ saving ? '保存中...' : '保存' }}
      </Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { Dialog, Button, Input, Label, Switch } from '@/components/ui'
import { SquarePen } from 'lucide-vue-next'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'
import { useFormDialog } from '@/composables/useFormDialog'
import { useI18n } from '@/i18n'
import { parseApiError } from '@/utils/errorParser'
import { parseNumberInput, parseNullableNumberInput, parsePatternText } from '@/utils/form'
import {
  updateProviderKey,
  type EndpointAPIKey,
  type EndpointAPIKeyUpdate,
} from '@/api/endpoints'
import {
  isAntigravityProviderKey,
  readAntigravityQuotaBindings,
  resolveAntigravityBindableModels,
  resolveAntigravityQuotaGroupOptions,
  type AntigravityQuotaGroupOption,
  type AntigravityQuotaModelOption,
} from '@/features/providers/utils/antigravityQuota'

const props = withDefaults(defineProps<{
  open: boolean
  editingKey: EndpointAPIKey | null
  providerType?: string | null
}>(), {
  providerType: null,
})

const emit = defineEmits<{
  close: []
  /** 保存成功时携带服务端返回的 OAuth Key 快照，避免父组件继续展示旧值。 */
  saved: [key: EndpointAPIKey]
  /** 独立保存或清空配额组绑定成功时同步最新 Key 快照，保持编辑弹窗打开。 */
  bindingsSaved: [key: EndpointAPIKey]
}>()

const { t } = useI18n()
const { success, error: showError } = useToast()
const { confirmWarning } = useConfirm()
// 显示自动获取模型警告：编辑模式下，原本未启用但现在启用，且已有 allowed_models
const showAutoFetchWarning = computed(() => {
  if (!props.editingKey) return false
  // 原本已启用，不需要警告
  if (props.editingKey.auto_fetch_models) return false
  // 现在未启用，不需要警告
  if (!form.value.auto_fetch_models) return false
  // 检查是否有已配置的模型权限
  const allowedModels = props.editingKey.allowed_models
  if (!allowedModels) return false
  if (Array.isArray(allowedModels) && allowedModels.length === 0) return false
  if (typeof allowedModels === 'object' && Object.keys(allowedModels).length === 0) return false
  return true
})

const autoFetchWarningMessage = computed(() => {
  if (!showAutoFetchWarning.value || !props.editingKey?.allowed_models) return ''
  const models = Array.isArray(props.editingKey.allowed_models)
    ? props.editingKey.allowed_models
    : []
  if (models.length === 0) return ''
  return `当前 Key 模型权限存在以下模型：${models.map(model => `“${model}”`).join('、')}，开启自动获取后将被覆盖`
})

// 表单是否可以保存
const canSave = computed(() => {
  // 必须填写名称
  if (!form.value.name.trim()) return false
  return true
})

const isOpen = computed(() => props.open)
const saving = ref(false)

const form = ref({
  name: '',
  internal_priority: 10,
  rpm_limit: undefined as number | null | undefined,
  concurrent_limit: undefined as number | null | undefined,
  cache_ttl_minutes: 5,
  max_probe_interval_minutes: 32,
  note: '',
  auto_fetch_models: false,
  model_include_patterns_text: '',
  model_exclude_patterns_text: ''
})

// ---------------------------------------------------------------------------
// Dirty 状态跟踪：通过快照比较判断表单是否被修改
// ---------------------------------------------------------------------------
const formSnapshot = ref('')
const loadedKeyId = ref<string | null>(null)

interface BindingDraftRow {
  uid: number
  model: string
  groupKey: string
}

let nextBindingRowUid = 1
const savedBindings = ref<Record<string, string>>({})
const bindingRows = ref<BindingDraftRow[]>([])
const savingBindings = ref(false)
const savingBindingAction = ref<'save' | 'clear' | null>(null)
const bindingError = ref<string | null>(null)
const bindingSuccess = ref<string | null>(null)

const isAntigravityKey = computed(() => isAntigravityProviderKey(props.editingKey, props.providerType))
const modelOptions = computed(() => resolveAntigravityBindableModels(props.editingKey))
const groupOptions = computed(() => resolveAntigravityQuotaGroupOptions(props.editingKey, t))
const savedBindingCount = computed(() => Object.keys(savedBindings.value).length)

const knownModelLowerSet = computed(() => new Set(modelOptions.value.map(item => item.model.toLowerCase())))
const knownGroupKeySet = computed(() => new Set(groupOptions.value.map(item => item.key)))

const duplicateGroupDisplayLabels = computed(() => {
  const counts = new Map<string, number>()
  for (const item of groupOptions.value) {
    const display = item.periodsText ? `${item.label} (${item.periodsText})` : item.label
    counts.set(display, (counts.get(display) ?? 0) + 1)
  }
  return new Set(
    Array.from(counts.entries())
      .filter(([, count]) => count > 1)
      .map(([display]) => display),
  )
})

function formatModelOptionText(option: AntigravityQuotaModelOption): string {
  if (option.displayName && option.displayName !== option.model) {
    return `${option.model} (${option.displayName})`
  }
  return option.model
}

function formatGroupOptionText(group: AntigravityQuotaGroupOption): string {
  const base = group.periodsText ? `${group.label} (${group.periodsText})` : group.label
  if (duplicateGroupDisplayLabels.value.has(base)) {
    return `${base} [${group.key}]`
  }
  return base
}

function isRowStaleModel(row: BindingDraftRow): boolean {
  const model = row.model.trim()
  if (!model) return false
  return !knownModelLowerSet.value.has(model.toLowerCase())
}

function isRowStaleGroup(row: BindingDraftRow): boolean {
  const groupKey = row.groupKey.trim()
  if (!groupKey) return false
  return !knownGroupKeySet.value.has(groupKey)
}

function isModelTakenByOtherRow(model: string, currentIndex: number): boolean {
  const targetLower = model.trim().toLowerCase()
  if (!targetLower) return false
  return bindingRows.value.some((row, index) => (
    index !== currentIndex && row.model.trim().toLowerCase() === targetLower
  ))
}

const hasStaleBindingRows = computed(() => (
  bindingRows.value.some(row => isRowStaleModel(row) || isRowStaleGroup(row))
))

const emptyBindingPrerequisitesText = computed(() => {
  if (modelOptions.value.length === 0 && groupOptions.value.length === 0) {
    return '暂无可用模型与配额组快照，请先刷新该账号配额后再配置绑定。'
  }
  if (modelOptions.value.length === 0) {
    return '当前账号暂无已发现的可用模型，请先刷新配额或同步模型后再绑定。'
  }
  return '当前账号暂无已保存的配额组快照，请先刷新该账号配额后再绑定。'
})

function hydrateBindingRows(bindings: Record<string, string>) {
  savedBindings.value = { ...bindings }
  bindingRows.value = Object.entries(bindings).map(([model, groupKey]) => ({
    uid: nextBindingRowUid++,
    model,
    groupKey,
  }))
}

const isBindingsDirty = computed(() => {
  if (!isAntigravityKey.value) return false
  const savedEntries = Object.entries(savedBindings.value)
  if (bindingRows.value.length !== savedEntries.length) return true
  for (const row of bindingRows.value) {
    const model = row.model.trim()
    const groupKey = row.groupKey.trim()
    if (!model || !groupKey) return true
    if (savedBindings.value[model] !== groupKey) return true
  }
  return false
})

const bindingValidationMessage = computed<string | null>(() => {
  if (!isBindingsDirty.value) return null
  const seenModels = new Set<string>()
  for (const row of bindingRows.value) {
    const model = row.model.trim()
    const groupKey = row.groupKey.trim()
    if (!model || !groupKey) {
      return '请为每条绑定完整选择精确上游模型与关联配额组。'
    }
    const lower = model.toLowerCase()
    if (seenModels.has(lower)) {
      return '同一上游模型不能重复配置多条配额组绑定。'
    }
    seenModels.add(lower)
    if (isRowStaleModel(row)) {
      return '存在不在当前账号可用列表中的模型，请移除或改选后再保存。'
    }
    if (isRowStaleGroup(row)) {
      return '存在失效的配额组标识，请重新选择当前配额组或移除后再保存。'
    }
  }
  return null
})

const canAddBindingRow = computed(() => (
  isAntigravityKey.value
  && modelOptions.value.length > 0
  && groupOptions.value.length > 0
  && bindingRows.value.length < modelOptions.value.length
  && !savingBindings.value
  && !saving.value
))

const canSaveBindings = computed(() => (
  isAntigravityKey.value
  && isBindingsDirty.value
  && !bindingValidationMessage.value
  && !savingBindings.value
  && !saving.value
))

const canClearBindings = computed(() => (
  isAntigravityKey.value
  && (savedBindingCount.value > 0 || bindingRows.value.length > 0)
  && !savingBindings.value
  && !saving.value
))

function addBindingRow() {
  if (!canAddBindingRow.value) return
  bindingError.value = null
  bindingSuccess.value = null
  bindingRows.value.push({
    uid: nextBindingRowUid++,
    model: '',
    groupKey: '',
  })
}

function removeBindingRow(uid: number) {
  if (savingBindings.value || saving.value) return
  bindingError.value = null
  bindingSuccess.value = null
  bindingRows.value = bindingRows.value.filter(row => row.uid !== uid)
}

function handleRowModelChange(uid: number, event: Event) {
  const value = (event.target as HTMLSelectElement).value
  const target = bindingRows.value.find(row => row.uid === uid)
  if (!target) return
  target.model = value
  bindingError.value = null
  bindingSuccess.value = null
}

function handleRowGroupChange(uid: number, event: Event) {
  const value = (event.target as HTMLSelectElement).value
  const target = bindingRows.value.find(row => row.uid === uid)
  if (!target) return
  target.groupKey = value
  bindingError.value = null
  bindingSuccess.value = null
}

function takeSnapshot() {
  formSnapshot.value = JSON.stringify(form.value)
}

const isDirty = computed(() => {
  if (!formSnapshot.value) return false
  return JSON.stringify(form.value) !== formSnapshot.value
})

// 对话框关闭时清除快照（快照在 loadKeyData 中拍摄）
watch(isOpen, (val) => {
  if (!val) {
    formSnapshot.value = ''
    loadedKeyId.value = null
  }
})

// 重置表单
function resetForm() {
  form.value = {
    name: '',
    internal_priority: 10,
    rpm_limit: undefined,
    concurrent_limit: undefined,
    cache_ttl_minutes: 5,
    max_probe_interval_minutes: 32,
    note: '',
    auto_fetch_models: false,
    model_include_patterns_text: '',
    model_exclude_patterns_text: ''
  }
  formSnapshot.value = ''
  loadedKeyId.value = null
  savedBindings.value = {}
  bindingRows.value = []
  bindingError.value = null
  bindingSuccess.value = null
}

// 加载密钥数据
function loadKeyData() {
  if (!props.editingKey) return
  const incomingId = props.editingKey.id
  const preserveUnsavedForm = incomingId === loadedKeyId.value && isDirty.value
  if (!preserveUnsavedForm) {
    form.value = {
      name: props.editingKey.name,
      internal_priority: props.editingKey.internal_priority ?? 10,
      rpm_limit: props.editingKey.rpm_limit ?? undefined,
      concurrent_limit: props.editingKey.concurrent_limit ?? undefined,
      cache_ttl_minutes: props.editingKey.cache_ttl_minutes ?? 5,
      max_probe_interval_minutes: props.editingKey.max_probe_interval_minutes ?? 32,
      note: props.editingKey.note || '',
      auto_fetch_models: props.editingKey.auto_fetch_models ?? false,
      model_include_patterns_text: (props.editingKey.model_include_patterns || []).join(', '),
      model_exclude_patterns_text: (props.editingKey.model_exclude_patterns || []).join(', ')
    }
    // 数据加载完成后拍快照，作为 dirty 判断的基准
    takeSnapshot()
  }

  const preserveUnsavedBindings = incomingId === loadedKeyId.value && isBindingsDirty.value
  if (!preserveUnsavedBindings) {
    hydrateBindingRows(readAntigravityQuotaBindings(props.editingKey))
    if (incomingId !== loadedKeyId.value) {
      bindingError.value = null
      bindingSuccess.value = null
    }
  }
  loadedKeyId.value = incomingId
}

// 使用 useFormDialog 统一处理对话框逻辑
const {
  handleDialogUpdate: _baseHandleDialogUpdate,
  handleCancel: _baseHandleCancel,
} = useFormDialog({
  isOpen: () => props.open,
  entity: () => props.editingKey,
  isLoading: saving,
  extraLoadingStates: [savingBindings],
  onClose: () => emit('close'),
  loadData: loadKeyData,
  resetForm,
})

// 包装关闭逻辑：有未保存更改时弹出确认
async function handleDialogUpdate(value: boolean) {
  if (!value && (isDirty.value || isBindingsDirty.value)) {
    const confirmed = await confirmWarning('有未保存的更改，确定要关闭吗？', '放弃更改')
    if (!confirmed) return
  }
  _baseHandleDialogUpdate(value)
}

async function handleCancel() {
  if (isDirty.value || isBindingsDirty.value) {
    const confirmed = await confirmWarning('有未保存的更改，确定要关闭吗？', '放弃更改')
    if (!confirmed) return
  }
  _baseHandleCancel()
}

/** 独立保存 Antigravity 模型配额组绑定（专用 binding-only PUT，不与常规表单字段混传）。 */
async function handleSaveBindings() {
  if (!props.editingKey || !canSaveBindings.value) return

  const targetKeyId = props.editingKey.id
  const payload: Record<string, string> = {}
  for (const row of bindingRows.value) {
    const model = row.model.trim()
    const groupKey = row.groupKey.trim()
    if (model && groupKey) {
      payload[model] = groupKey
    }
  }

  savingBindings.value = true
  savingBindingAction.value = 'save'
  bindingError.value = null
  bindingSuccess.value = null
  try {
    const updatedKey = await updateProviderKey(targetKeyId, {
      antigravity_quota_bindings: payload,
    })
    if (props.editingKey?.id !== targetKeyId) return
    const hasServerBindingsField = (
      updatedKey?.antigravity_quota_bindings !== undefined
      && updatedKey?.antigravity_quota_bindings !== null
    ) || (
      updatedKey?.upstream_metadata?.antigravity_quota_bindings !== undefined
      && updatedKey?.upstream_metadata?.antigravity_quota_bindings !== null
    )
    const nextSaved = hasServerBindingsField
      ? readAntigravityQuotaBindings(updatedKey)
      : payload
    hydrateBindingRows(nextSaved)
    const count = Object.keys(nextSaved).length
    const message = count > 0 ? `已保存 ${count} 项配额组绑定` : '配额组绑定已清空'
    bindingSuccess.value = message
    success(message, '成功')
    emit('bindingsSaved', updatedKey)
  } catch (err: unknown) {
    if (props.editingKey?.id !== targetKeyId) return
    const errorMessage = parseApiError(err, '保存配额组绑定失败')
    bindingError.value = errorMessage
    showError(errorMessage, '错误')
  } finally {
    if (props.editingKey?.id === targetKeyId) {
      savingBindings.value = false
      savingBindingAction.value = null
    }
  }
}

/** 独立清空全部 Antigravity 模型配额组绑定（已保存绑定通过空对象 `{}` PUT 清除）。 */
async function handleClearBindings() {
  if (!props.editingKey || !canClearBindings.value) return

  if (savedBindingCount.value === 0) {
    hydrateBindingRows({})
    bindingError.value = null
    bindingSuccess.value = null
    return
  }

  const targetKeyId = props.editingKey.id
  savingBindings.value = true
  savingBindingAction.value = 'clear'
  bindingError.value = null
  bindingSuccess.value = null
  try {
    const updatedKey = await updateProviderKey(targetKeyId, {
      antigravity_quota_bindings: {},
    })
    if (props.editingKey?.id !== targetKeyId) return
    const hasServerBindingsField = (
      updatedKey?.antigravity_quota_bindings !== undefined
      && updatedKey?.antigravity_quota_bindings !== null
    ) || (
      updatedKey?.upstream_metadata?.antigravity_quota_bindings !== undefined
      && updatedKey?.upstream_metadata?.antigravity_quota_bindings !== null
    )
    const nextSaved = hasServerBindingsField
      ? readAntigravityQuotaBindings(updatedKey)
      : {}
    hydrateBindingRows(nextSaved)
    const message = '已清空全部配额组绑定'
    bindingSuccess.value = message
    success(message, '成功')
    emit('bindingsSaved', updatedKey)
  } catch (err: unknown) {
    if (props.editingKey?.id !== targetKeyId) return
    const errorMessage = parseApiError(err, '清空配额组绑定失败')
    bindingError.value = errorMessage
    showError(errorMessage, '错误')
  } finally {
    if (props.editingKey?.id === targetKeyId) {
      savingBindings.value = false
      savingBindingAction.value = null
    }
  }
}

/** 保存 OAuth Key，并向父组件回传服务端最终快照。 */
async function handleSave() {
  if (!props.editingKey) {
    showError('无法保存：缺少账号信息', '错误')
    return
  }
  if (isBindingsDirty.value) {
    const confirmed = await confirmWarning(
      '配额组绑定有未保存的更改（需点击“保存绑定”独立保存），确定放弃绑定更改并仅保存基础配置吗？',
      '放弃绑定更改',
    )
    if (!confirmed) return
  }

  saving.value = true
  try {
    const shouldClearAllowedModels = !!props.editingKey.auto_fetch_models && !form.value.auto_fetch_models
    const updateData: EndpointAPIKeyUpdate = {
      name: form.value.name,
      internal_priority: form.value.internal_priority,
      rpm_limit: form.value.rpm_limit,
      concurrent_limit: form.value.concurrent_limit,
      cache_ttl_minutes: form.value.cache_ttl_minutes,
      max_probe_interval_minutes: form.value.max_probe_interval_minutes,
      note: form.value.note,
      allowed_models: shouldClearAllowedModels ? null : undefined,
      auto_fetch_models: form.value.auto_fetch_models,
      model_include_patterns: parsePatternText(form.value.model_include_patterns_text),
      model_exclude_patterns: parsePatternText(form.value.model_exclude_patterns_text)
    }

    const updatedKey = await updateProviderKey(props.editingKey.id, updateData)
    success('账号已更新', '成功')
    emit('saved', updatedKey)
    emit('close')
  } catch (err: unknown) {
    const errorMessage = parseApiError(err, '保存失败')
    showError(errorMessage, '错误')
  } finally {
    saving.value = false
  }
}
</script>
