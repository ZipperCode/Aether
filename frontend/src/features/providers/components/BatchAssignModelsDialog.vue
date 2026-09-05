<template>
  <Dialog
    :model-value="open"
    :title="providerName ? `批量管理模型 - ${providerName}` : '批量管理模型'"
    description="选中的模型将被关联到提供商，取消选中将移除关联"
    :icon="Layers"
    size="2xl"
    @update:model-value="handleDialogUpdate"
  >
    <template #default>
      <div class="space-y-4">
        <!-- 搜索栏 -->
        <div class="flex items-center gap-2">
          <div class="flex-1 relative">
            <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
            <Input
              v-model="searchQuery"
              placeholder="搜索模型..."
              class="pl-8 h-9"
            />
          </div>
          <DropdownMenu :modal="false">
            <DropdownMenuTrigger as-child>
              <Button
                variant="ghost"
                size="icon"
                class="h-9 w-9 shrink-0"
                :disabled="loadingGlobalModels || loadingProviderKeys || fetchingAutoMatchedModels || providerKeys.length === 0"
                :title="autoMatchButtonTitle"
                aria-label="按密钥匹配"
              >
                <Loader2
                  v-if="loadingProviderKeys || fetchingAutoMatchedModels"
                  class="w-4 h-4 animate-spin"
                />
                <ListChecks
                  v-else
                  class="w-4 h-4"
                />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              align="end"
              class="w-72 max-h-80 overflow-y-auto"
            >
              <DropdownMenuItem
                v-for="key in providerKeys"
                :key="key.id"
                class="flex-col items-start gap-0.5"
                :disabled="fetchingAutoMatchedModels"
                @select="applyAutoMatchFromKey(key)"
              >
                <span class="w-full truncate font-medium">
                  {{ getAutoMatchKeyLabel(key) }}
                </span>
                <span
                  v-if="getAutoMatchKeyDetail(key)"
                  class="w-full truncate text-xs text-muted-foreground"
                >
                  {{ getAutoMatchKeyDetail(key) }}
                </span>
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>

        <!-- 模型列表 -->
        <div class="border rounded-lg overflow-hidden">
          <div class="max-h-96 overflow-y-auto">
            <div
              v-if="loadingGlobalModels"
              class="flex items-center justify-center py-12"
            >
              <Loader2 class="w-6 h-6 animate-spin text-primary" />
            </div>

            <template v-else>
              <!-- 全局模型列表 -->
              <div v-if="filteredGlobalModels.length > 0">
                <div
                  class="flex items-center justify-between px-3 py-2 bg-muted sticky top-0 z-10"
                >
                  <div class="flex items-center gap-2">
                    <span class="text-xs font-medium">全局模型</span>
                    <span class="text-xs text-muted-foreground">({{ filteredGlobalModels.length }})</span>
                  </div>
                  <button
                    v-if="filteredGlobalModels.length > 0"
                    type="button"
                    class="text-xs text-primary hover:underline shrink-0"
                    @click.stop="toggleAllGlobalModels"
                  >
                    {{ isAllGlobalModelsSelected ? '取消全选' : '全选' }}
                  </button>
                </div>
                <div class="space-y-1 p-2">
                  <div
                    v-for="model in filteredGlobalModels"
                    :key="model.id"
                    class="flex items-start gap-2 px-2 py-1.5 rounded hover:bg-muted cursor-pointer"
                    :data-global-model-id="model.id"
                    @click="toggleGlobalModelSelection(model.id)"
                  >
                    <div
                      class="w-4 h-4 mt-0.5 border rounded flex items-center justify-center shrink-0"
                      :class="isGlobalModelSelected(model.id) ? 'bg-primary border-primary' : ''"
                    >
                      <Check
                        v-if="isGlobalModelSelected(model.id)"
                        class="w-3 h-3 text-primary-foreground"
                      />
                    </div>
                    <div class="flex-1 min-w-0">
                      <p class="text-sm font-medium truncate">
                        {{ model.display_name }}
                      </p>
                      <p class="text-xs text-muted-foreground truncate font-mono">
                        {{ model.name }}
                      </p>
                      <div
                        v-if="globalModelsToAdd.includes(model.id) && upstreamModels.length > 0"
                        class="mt-2 space-y-1"
                        @click.stop
                      >
                        <label
                          :for="`upstream-model-${model.id}`"
                          class="block text-xs text-muted-foreground"
                        >
                          真实上游模型（可选）
                        </label>
                        <select
                          :id="`upstream-model-${model.id}`"
                          :value="selectedUpstreamModelIds.get(model.id) || ''"
                          :aria-label="`为 ${model.display_name} 选择上游模型`"
                          class="h-8 w-full rounded-md border border-input bg-background px-2 text-xs font-mono text-foreground"
                          @click.stop
                          @change="setUpstreamModelSelection(model.id, $event)"
                        >
                          <option value="">
                            未指定（使用全局模型名自动推断）
                          </option>
                          <option
                            v-for="upstreamModel in upstreamModels"
                            :key="upstreamModel.id"
                            :value="upstreamModel.id"
                          >
                            {{ upstreamModel.id }}
                          </option>
                        </select>
                      </div>
                    </div>
                  </div>
                </div>
              </div>

              <!-- 空状态 -->
              <div
                v-if="filteredGlobalModels.length === 0"
                class="flex flex-col items-center justify-center py-12 text-muted-foreground"
              >
                <Layers class="w-10 h-10 mb-2 opacity-30" />
                <p class="text-sm">
                  {{ searchQuery ? '无匹配结果' : '暂无可用全局模型' }}
                </p>
                <p class="text-xs mt-1">
                  请先前往"模型目录"页面创建全局模型
                </p>
              </div>
            </template>
          </div>
        </div>
      </div>
    </template>
    <template #footer>
      <div class="flex items-center justify-between w-full">
        <p class="text-xs text-muted-foreground">
          {{ hasChanges ? `${pendingChangesCount} 项更改待保存` : '' }}
        </p>
        <div class="flex items-center gap-2">
          <Button
            :disabled="!hasChanges || saving || fetchingAutoMatchedModels"
            @click="handleSave"
          >
            <Loader2
              v-if="saving"
              class="w-4 h-4 mr-1 animate-spin"
            />
            {{ saving ? '保存中...' : '保存' }}
          </Button>
          <Button
            variant="outline"
            @click="handleClose"
          >
            关闭
          </Button>
        </div>
      </div>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { Layers, Loader2, Search, Check, ListChecks } from 'lucide-vue-next'
import Dialog from '@/components/ui/dialog/Dialog.vue'
import Button from '@/components/ui/button.vue'
import Input from '@/components/ui/input.vue'
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from '@/components/ui'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'
import { parseApiError } from '@/utils/errorParser'
import { useUpstreamModelsCache } from '../composables/useUpstreamModelsCache'
import {
  getGlobalModels,
  type GlobalModelResponse
} from '@/api/endpoints/global-models'
import {
  getProviderModels,
  getProviderKeys,
  batchAssignModelsToProvider,
  createModel,
  deleteModel,
  type Model,
  type EndpointAPIKey,
  type UpstreamModel,
} from '@/api/endpoints'

type AutoMatchKey = Pick<EndpointAPIKey, 'id' | 'name' | 'api_key_masked'>

interface Props {
  open: boolean
  providerId: string
  providerName?: string
}

const props = defineProps<Props>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  'changed': []
}>()

interface AutoMatchKeyLike {
  id: string
  name?: string | null
  api_key_masked?: string | null
}

const { error: showError, success, warning: showWarning } = useToast()
const { confirmWarning } = useConfirm()
const { fetchModels: fetchCachedModels } = useUpstreamModelsCache()

// 状态
const loadingGlobalModels = ref(false)
const loadingProviderKeys = ref(false)
const saving = ref(false)
const fetchingAutoMatchedModels = ref(false)
// 弹窗每次打开、关闭或切换 Provider 都递增，用于丢弃上一会话的异步结果。
let dialogSession = 0

// 数据
const allGlobalModels = ref<GlobalModelResponse[]>([])
const existingModels = ref<Model[]>([])
const providerKeys = ref<AutoMatchKey[]>([])
// 当前所选 Key 返回的真实上游模型，保留其模型名和 Endpoint 链路供新关联使用。
const upstreamModels = ref<UpstreamModel[]>([])

// 选择状态（本地状态，保存时才提交）
const selectedGlobalModelIds = ref<Set<string>>(new Set())
// 本轮新增关联中，Global Model ID 到真实上游模型 ID 的显式对应关系。
const selectedUpstreamModelIds = ref<Map<string, string>>(new Map())

// 初始状态（用于计算变更）
const initialGlobalModelIds = ref<Set<string>>(new Set())

// 搜索状态
const searchQuery = ref('')

const autoMatchButtonTitle = computed(() => {
  if (loadingProviderKeys.value) return '正在加载密钥'
  if (providerKeys.value.length === 0) return '暂无可用于匹配的密钥'
  return '选择密钥，获取真实上游模型并自动勾选同名模型'
})

/**
 * 按忽略大小写的完整名称建立唯一上游模型索引；同名冲突不自动选择，避免错误关联。
 */
const uniqueUpstreamModelsByName = computed(() => {
  const uniqueModels = new Map<string, UpstreamModel>()
  const duplicateNames = new Set<string>()
  for (const model of upstreamModels.value) {
    const normalizedName = normalizeModelName(model.id)
    if (!normalizedName || duplicateNames.has(normalizedName)) continue
    if (uniqueModels.has(normalizedName)) {
      uniqueModels.delete(normalizedName)
      duplicateNames.add(normalizedName)
    } else {
      uniqueModels.set(normalizedName, model)
    }
  }
  return uniqueModels
})

// 已关联的全局模型 ID 集合（从已有数据计算）
const existingGlobalModelIds = computed(() => {
  return new Set(
    existingModels.value
      .map(m => m.global_model_id)
  )
})

// 过滤后的全局模型
const filteredGlobalModels = computed(() => {
  const query = searchQuery.value.toLowerCase().trim()
  return allGlobalModels.value.filter(m => {
    if (query && !m.name.toLowerCase().includes(query) && !m.display_name.toLowerCase().includes(query)) {
      return false
    }
    return true
  })
})

// 全局模型是否全选
const isAllGlobalModelsSelected = computed(() => {
  if (filteredGlobalModels.value.length === 0) return false
  return filteredGlobalModels.value.every(m => isGlobalModelSelected(m.id))
})

/** 检查 Global Model 是否已在本轮关联选择中。 */
function isGlobalModelSelected(globalModelId: string): boolean {
  return selectedGlobalModelIds.value.has(globalModelId)
}

// 计算待添加的全局模型
const globalModelsToAdd = computed(() => {
  const toAdd: string[] = []
  for (const id of selectedGlobalModelIds.value) {
    if (!initialGlobalModelIds.value.has(id)) {
      toAdd.push(id)
    }
  }
  return toAdd
})

// 计算待移除的全局模型
const globalModelsToRemove = computed(() => {
  const toRemove: string[] = []
  for (const id of initialGlobalModelIds.value) {
    if (!selectedGlobalModelIds.value.has(id)) {
      toRemove.push(id)
    }
  }
  return toRemove
})

// 是否有变更
const hasChanges = computed(() => {
  return globalModelsToAdd.value.length > 0 ||
    globalModelsToRemove.value.length > 0
})

// 待变更数量
const pendingChangesCount = computed(() => {
  return globalModelsToAdd.value.length +
    globalModelsToRemove.value.length
})

/**
 * 切换 Global Model 选择，并同步清理或补入唯一同名的真实上游模型。
 */
function toggleGlobalModelSelection(id: string) {
  const wasSelected = selectedGlobalModelIds.value.has(id)
  if (wasSelected) {
    selectedGlobalModelIds.value.delete(id)
  } else {
    selectedGlobalModelIds.value.add(id)
  }
  selectedGlobalModelIds.value = new Set(selectedGlobalModelIds.value)
  syncUpstreamModelSelections(wasSelected ? [] : [id])
}

/** 全选或取消当前筛选结果，并同步本轮真实上游模型选择。 */
function toggleAllGlobalModels() {
  const allIds = filteredGlobalModels.value.map(m => m.id)
  const wasAllSelected = isAllGlobalModelsSelected.value
  if (wasAllSelected) {
    for (const id of allIds) {
      selectedGlobalModelIds.value.delete(id)
    }
  } else {
    for (const id of allIds) {
      selectedGlobalModelIds.value.add(id)
    }
  }
  selectedGlobalModelIds.value = new Set(selectedGlobalModelIds.value)
  syncUpstreamModelSelections(wasAllSelected ? [] : allIds)
}

/** 将模型名规范为仅忽略首尾空白和大小写的精确匹配键，不做前缀或模糊匹配。 */
function normalizeModelName(name: string | null | undefined): string {
  return (name || '').trim().toLowerCase()
}

/** 返回密钥的首选展示名称，缺少名称时依次使用掩码和短 ID。 */
function getAutoMatchKeyLabel(key: AutoMatchKeyLike): string {
  return key.name || key.api_key_masked || key.id.slice(0, 8)
}

/** 返回密钥列表中的辅助信息，避免重复主展示内容。 */
function getAutoMatchKeyDetail(key: AutoMatchKeyLike): string {
  if (key.name && key.api_key_masked) return key.api_key_masked
  return key.name ? key.id.slice(0, 8) : ''
}

/**
 * 仅保留仍有效的新关联选择，并按调用方指定范围自动填入唯一同名上游模型。
 * 不在后续无关选择变化时恢复用户主动清空的上游模型。
 */
function syncUpstreamModelSelections(autoSelectGlobalModelIds: Iterable<string> = []) {
  const availableUpstreamIds = new Set(upstreamModels.value.map(model => model.id))
  const nextSelections = new Map<string, string>()

  for (const [globalModelId, upstreamModelId] of selectedUpstreamModelIds.value) {
    if (
      selectedGlobalModelIds.value.has(globalModelId)
      && !initialGlobalModelIds.value.has(globalModelId)
      && availableUpstreamIds.has(upstreamModelId)
    ) {
      nextSelections.set(globalModelId, upstreamModelId)
    }
  }

  for (const globalModelId of autoSelectGlobalModelIds) {
    const globalModel = allGlobalModels.value.find(model => model.id === globalModelId)
    if (!globalModel) continue
    if (
      !selectedGlobalModelIds.value.has(globalModel.id)
      || initialGlobalModelIds.value.has(globalModel.id)
      || nextSelections.has(globalModel.id)
    ) continue

    const sameNameUpstreamModel = uniqueUpstreamModelsByName.value.get(
      normalizeModelName(globalModel.name),
    )
    if (sameNameUpstreamModel) {
      nextSelections.set(globalModel.id, sameNameUpstreamModel.id)
    }
  }

  selectedUpstreamModelIds.value = nextSelections
}

/** 记录用户为某个新 Global Model 显式选择的真实上游模型。 */
function setUpstreamModelSelection(globalModelId: string, event: Event) {
  const upstreamModelId = (event.target as HTMLSelectElement).value
  const nextSelections = new Map(selectedUpstreamModelIds.value)
  if (upstreamModelId) {
    nextSelections.set(globalModelId, upstreamModelId)
  } else {
    nextSelections.delete(globalModelId)
  }
  selectedUpstreamModelIds.value = nextSelections
}

/**
 * 获取所选 Key 的上游模型；同名项自动关联，不同名项保留给用户自由选择。
 */
async function applyAutoMatchFromKey(key: AutoMatchKey) {
  if (!props.providerId || !key || fetchingAutoMatchedModels.value) return

  const requestedProviderId = props.providerId
  const requestedSession = dialogSession
  fetchingAutoMatchedModels.value = true
  try {
    const result = await fetchCachedModels(requestedProviderId, key.id, true)
    if (
      requestedSession !== dialogSession
      || !props.open
      || props.providerId !== requestedProviderId
    ) return

    if (result.warning) {
      showWarning(`部分格式获取失败: ${result.warning}`)
    }

    upstreamModels.value = result.models
    syncUpstreamModelSelections(selectedGlobalModelIds.value)

    if (result.models.length === 0) {
      if (result.error) {
        showError(result.error, '获取上游模型失败')
      } else {
        showWarning('此 Key 未返回可用模型')
      }
      return
    }

    const matchedGlobalModelIds = allGlobalModels.value
      .filter(model => uniqueUpstreamModelsByName.value.has(normalizeModelName(model.name)))
      .map(model => model.id)

    const nextSelected = new Set(selectedGlobalModelIds.value)
    let newlySelectedCount = 0
    for (const id of matchedGlobalModelIds) {
      if (!nextSelected.has(id)) {
        newlySelectedCount++
      }
      nextSelected.add(id)
    }
    selectedGlobalModelIds.value = nextSelected
    syncUpstreamModelSelections(matchedGlobalModelIds)
    searchQuery.value = ''

    if (matchedGlobalModelIds.length === 0) {
      showWarning(`已获取 ${result.models.length} 个上游模型，请为新关联的全局模型选择对应模型`)
    } else if (newlySelectedCount > 0) {
      success(`已按 ${getAutoMatchKeyLabel(key)} 勾选 ${matchedGlobalModelIds.length} 个同名模型`)
    } else {
      success(`${matchedGlobalModelIds.length} 个同名模型已在选中列表中`)
    }
  } catch (err: unknown) {
    if (
      requestedSession === dialogSession
      && props.open
      && props.providerId === requestedProviderId
    ) {
      showError(parseApiError(err, '自动匹配模型失败'), '错误')
    }
  } finally {
    if (
      requestedSession === dialogSession
      && props.open
      && props.providerId === requestedProviderId
    ) {
      fetchingAutoMatchedModels.value = false
    }
  }
}

/** 关闭弹窗；存在未保存变更时先请求用户确认。 */
async function handleClose() {
  if (hasChanges.value) {
    const confirmed = await confirmWarning('有未保存的更改，确定要关闭吗？', '放弃更改')
    if (!confirmed) return
  }
  emit('update:open', false)
}

/** 处理遮罩等外部关闭动作，并保持未保存变更确认语义。 */
async function handleDialogUpdate(value: boolean) {
  if (!value && hasChanges.value) {
    const confirmed = await confirmWarning('有未保存的更改，确定要关闭吗？', '放弃更改')
    if (!confirmed) return
  }
  emit('update:open', value)
}

/**
 * 保存关联变更：显式上游选择逐项精确创建，其余新增项继续使用原批量推断接口。
 */
async function handleSave() {
  if (!hasChanges.value || saving.value || fetchingAutoMatchedModels.value) return

  saving.value = true
  let hasAnyOperation = false
  try {
    let totalSuccess = 0
    const allErrors: string[] = []

    // 移除全局模型
    for (const globalModelId of globalModelsToRemove.value) {
      const existingModel = existingModels.value.find(m => m.global_model_id === globalModelId)
      if (existingModel) {
        hasAnyOperation = true
        try {
          await deleteModel(props.providerId, existingModel.id)
          totalSuccess++
        } catch (err: unknown) {
          allErrors.push(parseApiError(err, '移除失败'))
        }
      }
    }

    // 显式选择真实上游模型的新增项逐项创建，确保名称和 Endpoint 链路不丢失。
    const explicitlyHandledGlobalModelIds = new Set<string>()
    for (const globalModelId of globalModelsToAdd.value) {
      const upstreamModelId = selectedUpstreamModelIds.value.get(globalModelId)
      const upstreamModel = upstreamModels.value.find(model => model.id === upstreamModelId)
      if (!upstreamModel) continue

      hasAnyOperation = true
      try {
        const endpointIds = [...new Set((upstreamModel.endpoint_ids || []).filter(Boolean))]
        await createModel(props.providerId, {
          global_model_id: globalModelId,
          provider_model_name: upstreamModel.id,
          ...(endpointIds.length > 0 ? { endpoint_ids: endpointIds } : {}),
        })
        explicitlyHandledGlobalModelIds.add(globalModelId)
        totalSuccess++
      } catch (err: unknown) {
        explicitlyHandledGlobalModelIds.add(globalModelId)
        allErrors.push(parseApiError(err, `模型 ${upstreamModel.id} 关联失败`))
      }
    }

    // 未选择上游模型的新增项保持历史自动推断行为。
    const inferredGlobalModelIds = globalModelsToAdd.value.filter(
      id => !explicitlyHandledGlobalModelIds.has(id),
    )
    if (inferredGlobalModelIds.length > 0) {
      hasAnyOperation = true
      try {
        const result = await batchAssignModelsToProvider(props.providerId, inferredGlobalModelIds)
        totalSuccess += result.success.length
        if (result.errors.length > 0) {
          allErrors.push(...result.errors.map(e => e.error))
        }
      } catch (err: unknown) {
        allErrors.push(parseApiError(err, '批量添加全局模型失败'))
      }
    }

    if (totalSuccess > 0) {
      success(`成功处理 ${totalSuccess} 个模型`)
    }

    if (allErrors.length > 0) {
      showError(`部分操作失败: ${allErrors.slice(0, 3).join(', ')}${allErrors.length > 3 ? '...' : ''}`, '警告')
    }

    emit('changed')
    emit('update:open', false)
  } catch (err: unknown) {
    showError(parseApiError(err, '保存失败'), '错误')
    if (hasAnyOperation) {
      emit('changed')
    }
  } finally {
    saving.value = false
  }
}

/** 从已有关联模型同步初始选择，并清空仅属于本轮新增的上游对应关系。 */
function syncGlobalModelSelection() {
  const globalIds = [...existingGlobalModelIds.value].filter((id): id is string => id !== undefined)
  selectedGlobalModelIds.value = new Set(globalIds)
  initialGlobalModelIds.value = new Set(globalIds)
  selectedUpstreamModelIds.value = new Map()
}

// 监听打开状态
watch(
  () => [props.open, props.providerId] as const,
  async ([isOpen, providerId]) => {
    const session = ++dialogSession
    upstreamModels.value = []
    selectedUpstreamModelIds.value = new Map()
    fetchingAutoMatchedModels.value = false
    if (isOpen && providerId) {
      await loadData(providerId, session)
    } else {
      searchQuery.value = ''
      selectedGlobalModelIds.value = new Set()
      initialGlobalModelIds.value = new Set()
      providerKeys.value = []
      loadingGlobalModels.value = false
      loadingProviderKeys.value = false
    }
  },
  { immediate: true },
)

/**
 * 并行加载基础关联数据，再查询 Provider 全部 Key 聚合的真实上游模型。
 * 整个初始加载链路禁止保存；仅当前弹窗会话可写入结果，失败或空结果继续使用批量推断兜底。
 */
async function loadData(providerId: string, session: number) {
  if (session !== dialogSession || !props.open || props.providerId !== providerId) return

  fetchingAutoMatchedModels.value = true
  try {
    await Promise.all([
      loadGlobalModels(providerId, session),
      loadExistingModels(providerId, session),
      loadProviderKeys(providerId, session),
    ])
    if (session !== dialogSession || !props.open || props.providerId !== providerId) return

    syncGlobalModelSelection()
    const result = await fetchCachedModels(providerId)
    if (session === dialogSession && props.open && props.providerId === providerId) {
      upstreamModels.value = result.models
      syncUpstreamModelSelections(selectedGlobalModelIds.value)
    }
  } finally {
    if (session === dialogSession && props.open && props.providerId === providerId) {
      fetchingAutoMatchedModels.value = false
    }
  }
}

/** 加载可供关联的完整 Global Model 列表。 */
async function loadGlobalModels(providerId: string, session: number) {
  try {
    loadingGlobalModels.value = true
    const response = await getGlobalModels({ limit: 1000 })
    if (session === dialogSession && props.open && props.providerId === providerId) {
      allGlobalModels.value = response.models
    }
  } catch (err: unknown) {
    if (session === dialogSession && props.open && props.providerId === providerId) {
      showError(parseApiError(err, '加载全局模型失败'), '错误')
    }
  } finally {
    if (session === dialogSession && props.open && props.providerId === providerId) {
      loadingGlobalModels.value = false
    }
  }
}

/** 加载当前 Provider 已有关联，用于计算新增与移除差异。 */
async function loadExistingModels(providerId: string, session: number) {
  try {
    const models = await getProviderModels(providerId)
    if (session === dialogSession && props.open && props.providerId === providerId) {
      existingModels.value = models
    }
  } catch (err: unknown) {
    if (session === dialogSession && props.open && props.providerId === providerId) {
      showError(parseApiError(err, '加载已关联模型失败'), '错误')
    }
  }
}

/** 加载可用于获取真实上游模型的 Provider 密钥。 */
async function loadProviderKeys(providerId: string, session: number) {
  try {
    loadingProviderKeys.value = true
    const keys = await getProviderKeys(providerId)
    if (session === dialogSession && props.open && props.providerId === providerId) {
      providerKeys.value = keys
    }
  } catch (err: unknown) {
    if (session === dialogSession && props.open && props.providerId === providerId) {
      providerKeys.value = []
      showError(parseApiError(err, '加载密钥失败'), '错误')
    }
  } finally {
    if (session === dialogSession && props.open && props.providerId === providerId) {
      loadingProviderKeys.value = false
    }
  }
}
</script>
