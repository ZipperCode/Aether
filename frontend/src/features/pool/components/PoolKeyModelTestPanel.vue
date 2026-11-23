<template>
  <div
    class="rounded-lg border border-border/70 bg-muted/20 p-3 space-y-3"
    :data-testid="`pool-model-test-panel-${poolKey.key_id}`"
  >
    <!-- 顶部栏：模型统计、端点切换、搜索与刷新 -->
    <div class="flex flex-col gap-2 sm:flex-row sm:flex-wrap sm:items-center sm:justify-between">
      <div class="flex items-center justify-between gap-2 min-w-0">
        <div class="flex flex-wrap items-center gap-2 min-w-0">
          <span class="text-xs font-semibold text-foreground">
            {{ t('pool.modelTest.title') }}
          </span>
          <span
            v-if="modelsState.loaded"
            class="text-xs text-muted-foreground tabular-nums"
          >
            {{ modelCountLabel }}
          </span>
          <Badge
            v-if="modelsState.loaded && modelsState.models.length > 0"
            variant="outline"
            class="h-5 px-1.5 text-[10px] font-normal"
          >
            {{ modelsState.fromCache ? t('pool.modelTest.sourceCache') : t('pool.modelTest.sourceLive') }}
          </Badge>
          <Badge
            v-if="compatibleEndpoints.length === 1"
            variant="secondary"
            class="h-5 px-1.5 text-[10px] font-normal"
            :title="compatibleEndpoints[0].base_url"
          >
            {{ formatApiFormat(compatibleEndpoints[0].api_format) }}
          </Badge>
        </div>

        <Button
          variant="ghost"
          size="icon"
          class="h-7 w-7 shrink-0 text-muted-foreground hover:text-foreground sm:hidden"
          :title="t('ui.close')"
          :aria-label="t('ui.close')"
          @click="emit('close')"
        >
          <X class="h-3.5 w-3.5" />
        </Button>
      </div>

      <div class="flex flex-wrap items-center gap-1.5">
        <div
          v-if="compatibleEndpoints.length > 1"
          class="w-40 sm:w-44"
        >
          <Select
            :model-value="selectedEndpointId"
            @update:model-value="handleSelectEndpoint"
          >
            <SelectTrigger
              class="h-7 min-h-7 border-border/60 bg-background px-2 text-xs"
              :aria-label="t('pool.modelTest.endpointLabel')"
              :data-testid="`pool-model-test-endpoint-${poolKey.key_id}`"
            >
              <SelectValue :placeholder="t('pool.modelTest.autoEndpoint')" />
            </SelectTrigger>
            <SelectContent
              align="end"
              :searchable="false"
            >
              <SelectItem
                v-for="endpoint in compatibleEndpoints"
                :key="endpoint.id"
                :value="endpoint.id"
                :text-value="endpoint.api_format"
              >
                <div class="flex items-center justify-between gap-2">
                  <span>{{ formatApiFormat(endpoint.api_format) }}</span>
                  <span
                    v-if="!endpoint.is_active"
                    class="text-[10px] text-muted-foreground"
                  >
                    {{ t('ui.disabledbc5a87a') }}
                  </span>
                </div>
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div class="relative min-w-[140px] flex-1 sm:w-52 sm:flex-none">
          <Search class="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            v-model="searchText"
            class="h-7 bg-background pl-7 pr-2 text-xs"
            :placeholder="t('pool.modelTest.searchPlaceholder')"
            :data-testid="`pool-model-test-search-${poolKey.key_id}`"
          />
        </div>

        <Button
          variant="outline"
          size="sm"
          class="h-7 shrink-0 px-2 text-xs"
          :disabled="modelsState.loading"
          :data-testid="`pool-model-test-refresh-${poolKey.key_id}`"
          @click="emit('refreshModels')"
        >
          <RefreshCw
            class="mr-1 h-3 w-3"
            :class="{ 'animate-spin': modelsState.loading }"
          />
          {{ t('pool.modelTest.refreshModels') }}
        </Button>

        <Button
          variant="ghost"
          size="icon"
          class="hidden h-7 w-7 shrink-0 text-muted-foreground hover:text-foreground sm:inline-flex"
          :title="t('ui.close')"
          :aria-label="t('ui.close')"
          @click="emit('close')"
        >
          <X class="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>

    <!-- 状态提示：停用账号或上游警告 -->
    <div
      v-if="!poolKey.is_active"
      class="rounded-md border border-amber-500/30 bg-amber-500/10 px-2.5 py-1.5 text-[11px] text-amber-700 dark:text-amber-300"
    >
      {{ t('pool.modelTest.inactiveAccountHint') }}
    </div>
    <div
      v-if="modelsState.warning"
      class="rounded-md border border-amber-500/30 bg-amber-500/10 px-2.5 py-1.5 text-[11px] text-amber-700 dark:text-amber-300 break-words"
    >
      {{ modelsState.warning }}
    </div>

    <!-- 自定义模型名直测栏 -->
    <div class="flex items-center gap-2 rounded-md border border-border/60 bg-background/80 px-2.5 py-2">
      <Input
        v-model="customModelInput"
        class="h-7 min-w-0 flex-1 bg-background text-xs font-mono"
        :placeholder="t('pool.modelTest.customModelPlaceholder')"
        :data-testid="`pool-model-test-custom-input-${poolKey.key_id}`"
        @keydown.enter.prevent="handleRunCustomModelTest"
      />
      <Button
        variant="default"
        size="sm"
        class="h-7 shrink-0 px-2.5 text-xs"
        :disabled="!trimmedCustomModel || activeCustomResult?.testing"
        :data-testid="`pool-model-test-custom-run-${poolKey.key_id}`"
        @click="handleRunCustomModelTest"
      >
        <Loader2
          v-if="activeCustomResult?.testing"
          class="mr-1 h-3 w-3 animate-spin"
        />
        <Play
          v-else
          class="mr-1 h-3 w-3"
        />
        {{ t('pool.modelTest.runCustomTest') }}
      </Button>
      <Button
        v-if="activeCustomResult?.testing"
        variant="ghost"
        size="sm"
        class="h-7 shrink-0 px-2 text-xs text-muted-foreground"
        @click="handleCancelTest(trimmedCustomModel)"
      >
        {{ t('pool.modelTest.cancelTest') }}
      </Button>
      <Button
        v-else-if="trimmedCustomModel"
        variant="ghost"
        size="sm"
        class="h-7 shrink-0 px-2 text-xs text-muted-foreground"
        :data-testid="`pool-model-test-custom-detail-${poolKey.key_id}`"
        @click="handleInspectModel(trimmedCustomModel, null)"
      >
        <SlidersHorizontal class="mr-1 h-3 w-3" />
        {{ t('pool.modelTest.detailButton') }}
      </Button>
    </div>

    <!-- 已测自定义模型结果列表 -->
    <div
      v-if="customTestedEntries.length > 0"
      class="space-y-1"
    >
      <div class="text-[11px] font-medium text-muted-foreground">
        {{ t('pool.modelTest.customResultTitle') }}
      </div>
      <div class="divide-y divide-border/50 rounded-md border border-border/60 bg-background">
        <div
          v-for="entry in customTestedEntries"
          :key="`custom-${entry.modelName}`"
          class="flex flex-wrap items-center justify-between gap-2 px-2.5 py-1.5 text-xs"
        >
          <div class="flex min-w-0 flex-1 items-center gap-1.5">
            <span
              class="truncate font-mono text-foreground"
              :title="entry.modelName"
            >
              {{ entry.modelName }}
            </span>
            <button
              type="button"
              class="rounded p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground shrink-0"
              :title="t('ui.copyModelId')"
              @click="copyModelId(entry.modelName)"
            >
              <Copy class="h-3 w-3" />
            </button>
          </div>
          <div class="flex flex-wrap items-center gap-2 shrink-0">
            <div
              v-if="entry.result"
              class="flex items-center gap-1.5"
            >
              <Badge
                v-if="entry.result.testing"
                variant="secondary"
                class="h-5 px-1.5 text-[10px]"
              >
                <Loader2 class="mr-1 h-2.5 w-2.5 animate-spin" />
                {{ t('pool.modelTest.statusTesting') }}
              </Badge>
              <template v-else-if="entry.result.status === 'success'">
                <Badge
                  variant="success"
                  class="h-5 px-1.5 text-[10px] tabular-nums"
                  :title="entry.result.responsePreview || undefined"
                >
                  {{ formatPassLabel(entry.result) }}
                </Badge>
                <span
                  v-if="entry.result.latencyMs != null"
                  class="text-[11px] text-muted-foreground tabular-nums"
                >
                  {{ t('pool.modelTest.latencyMs', { ms: entry.result.latencyMs }) }}
                </span>
              </template>
              <template v-else-if="entry.result.status === 'failed'">
                <Badge
                  variant="destructive"
                  class="h-5 px-1.5 text-[10px] tabular-nums"
                  :title="entry.result.errorMessage || undefined"
                >
                  {{ formatFailLabel(entry.result) }}
                </Badge>
                <span
                  v-if="entry.result.errorMessage"
                  class="max-w-[220px] truncate text-[11px] text-destructive"
                  :title="entry.result.errorMessage"
                >
                  {{ entry.result.errorMessage }}
                </span>
              </template>
            </div>
            <Button
              v-if="entry.result?.testing"
              variant="ghost"
              size="sm"
              class="h-6 px-2 text-[11px] text-muted-foreground"
              @click="handleCancelTest(entry.modelName)"
            >
              {{ t('pool.modelTest.cancelTest') }}
            </Button>
            <Button
              v-else
              variant="outline"
              size="sm"
              class="h-6 px-2 text-[11px]"
              @click="handleTestModel(entry.modelName, null)"
            >
              <Play class="mr-1 h-2.5 w-2.5" />
              {{ entry.result?.status && entry.result.status !== 'idle' ? t('pool.modelTest.retestButton') : t('pool.modelTest.testButton') }}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              class="h-6 px-2 text-[11px] text-muted-foreground"
              @click="handleInspectModel(entry.modelName, null)"
            >
              {{ t('pool.modelTest.detailButton') }}
            </Button>
          </div>
        </div>
      </div>
    </div>

    <!-- 获取模型列表主内容区 -->
    <div
      v-if="modelsState.loading && !modelsState.loaded"
      class="flex items-center justify-center gap-2 rounded-md border border-border/50 bg-background py-6 text-xs text-muted-foreground"
    >
      <Loader2 class="h-4 w-4 animate-spin text-primary" />
      <span>{{ t('pool.modelTest.loadingModels') }}</span>
    </div>

    <div
      v-else-if="modelsState.error && modelsState.models.length === 0"
      class="flex flex-wrap items-center justify-between gap-2 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2.5 text-xs text-destructive"
    >
      <div class="min-w-0 flex-1 space-y-0.5">
        <div class="font-medium">
          {{ t('pool.modelTest.fetchFailed') }}
        </div>
        <div class="break-words text-[11px] opacity-90">
          {{ modelsState.error }}
        </div>
      </div>
      <div class="flex items-center gap-1.5 shrink-0">
        <Button
          v-if="canRefreshToken"
          variant="outline"
          size="sm"
          class="h-7 border-destructive/30 bg-background px-2 text-xs text-foreground"
          :disabled="refreshingOauth"
          @click="emit('refreshOauth')"
        >
          <RefreshCw
            class="mr-1 h-3 w-3"
            :class="{ 'animate-spin': refreshingOauth }"
          />
          {{ t('pool.modelTest.refreshToken') }}
        </Button>
        <Button
          variant="outline"
          size="sm"
          class="h-7 border-destructive/30 bg-background px-2 text-xs text-foreground"
          :disabled="modelsState.loading"
          @click="emit('refreshModels')"
        >
          {{ t('pool.modelTest.retryFetch') }}
        </Button>
      </div>
    </div>

    <div
      v-else-if="modelsState.models.length === 0"
      class="rounded-md border border-dashed border-border/70 bg-background/60 px-3 py-5 text-center text-xs text-muted-foreground"
    >
      {{ t('pool.modelTest.emptyModels') }}
    </div>

    <div
      v-else-if="filteredModels.length === 0"
      class="rounded-md border border-border/50 bg-background/60 px-3 py-5 text-center text-xs text-muted-foreground"
    >
      {{ t('pool.modelTest.noMatchingModels') }}
    </div>

    <div
      v-else
      class="max-h-72 overflow-y-auto divide-y divide-border/50 rounded-md border border-border/60 bg-background"
    >
      <div
        v-for="model in filteredModels"
        :key="model.id"
        class="flex flex-wrap items-center justify-between gap-2 px-3 py-2 text-xs hover:bg-muted/25 transition-colors"
        :data-testid="`pool-model-test-row-${poolKey.key_id}-${model.id}`"
      >
        <div class="min-w-0 flex-1 space-y-0.5">
          <div class="flex flex-wrap items-center gap-1.5 min-w-0">
            <span
              class="truncate font-mono text-foreground"
              :title="model.id"
            >
              {{ model.id }}
            </span>
            <button
              type="button"
              class="rounded p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground shrink-0"
              :title="t('ui.copyModelId')"
              @click="copyModelId(model.id)"
            >
              <Copy class="h-3 w-3" />
            </button>
            <Badge
              v-if="!isModelAllowedByPoolKey(poolKey, model.id)"
              variant="outline"
              class="h-4 px-1 text-[10px] font-normal text-muted-foreground"
              :title="t('pool.modelTest.notAllowedTitle')"
            >
              {{ t('pool.modelTest.notAllowedBadge') }}
            </Badge>
          </div>
          <div
            v-if="getRowResult(model)?.status === 'failed' && getRowResult(model)?.errorMessage"
            class="truncate text-[11px] text-destructive"
            :title="getRowResult(model)?.errorMessage || undefined"
          >
            {{ getRowResult(model)?.errorMessage }}
          </div>
          <div
            v-else-if="getRowResult(model)?.status === 'success' && getRowResult(model)?.responsePreview"
            class="truncate text-[11px] text-muted-foreground"
            :title="getRowResult(model)?.responsePreview || undefined"
          >
            {{ getRowResult(model)?.responsePreview }}
          </div>
        </div>

        <div class="flex flex-wrap items-center gap-2 shrink-0">
          <!-- 测试状态与耗时 -->
          <div class="flex items-center gap-1.5">
            <Badge
              v-if="getRowResult(model)?.testing"
              variant="secondary"
              class="h-5 px-1.5 text-[10px]"
            >
              <Loader2 class="mr-1 h-2.5 w-2.5 animate-spin" />
              {{ t('pool.modelTest.statusTesting') }}
            </Badge>
            <template v-else-if="getRowResult(model)?.status === 'success'">
              <Badge
                variant="success"
                class="h-5 px-1.5 text-[10px] tabular-nums"
              >
                {{ formatPassLabel(getRowResult(model)!) }}
              </Badge>
              <span
                v-if="getRowResult(model)?.latencyMs != null"
                class="text-[11px] text-muted-foreground tabular-nums"
              >
                {{ t('pool.modelTest.latencyMs', { ms: getRowResult(model)!.latencyMs ?? 0 }) }}
              </span>
            </template>
            <Badge
              v-else-if="getRowResult(model)?.status === 'failed'"
              variant="destructive"
              class="h-5 px-1.5 text-[10px] tabular-nums"
            >
              {{ formatFailLabel(getRowResult(model)!) }}
            </Badge>
            <span
              v-else
              class="text-[11px] text-muted-foreground"
            >
              {{ t('pool.modelTest.statusIdle') }}
            </span>
          </div>

          <!-- 速测 / 取消 / 详情 -->
          <Button
            v-if="getRowResult(model)?.testing"
            variant="ghost"
            size="sm"
            class="h-6 px-2 text-[11px] text-muted-foreground"
            :data-testid="`pool-model-test-cancel-${poolKey.key_id}-${model.id}`"
            @click="handleCancelTest(model.id, model)"
          >
            {{ t('pool.modelTest.cancelTest') }}
          </Button>
          <Button
            v-else
            variant="outline"
            size="sm"
            class="h-6 px-2 text-[11px]"
            :data-testid="`pool-model-test-run-${poolKey.key_id}-${model.id}`"
            @click="handleTestModel(model.id, model)"
          >
            <Play class="mr-1 h-2.5 w-2.5" />
            {{ getRowResult(model)?.status && getRowResult(model)?.status !== 'idle' ? t('pool.modelTest.retestButton') : t('pool.modelTest.testButton') }}
          </Button>

          <Button
            variant="ghost"
            size="sm"
            class="h-6 px-2 text-[11px] text-muted-foreground hover:text-foreground"
            :data-testid="`pool-model-test-detail-${poolKey.key_id}-${model.id}`"
            @click="handleInspectModel(model.id, model)"
          >
            {{ t('pool.modelTest.detailButton') }}
          </Button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Copy, Loader2, Play, RefreshCw, Search, SlidersHorizontal, X } from 'lucide-vue-next'
import {
  Badge,
  Button,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui'
import { formatApiFormat } from '@/api/endpoints/types/api-format'
import type { PoolKeyDetail } from '@/api/endpoints/pool'
import type { ProviderEndpoint } from '@/api/endpoints/types'
import { useClipboard } from '@/composables/useClipboard'
import { useI18n } from '@/i18n'
import {
  getCompatiblePoolKeyEndpoints,
  isModelAllowedByPoolKey,
  selectDefaultPoolKeyEndpoint,
  type PoolAccountModelItem,
  type PoolAccountModelTestResult,
  type PoolAccountModelsState,
} from '../composables/usePoolKeyModelTest'

export interface PoolKeyModelInspectPayload {
  poolKey: PoolKeyDetail
  modelName: string
  endpoint: ProviderEndpoint | null
  modelMeta: PoolAccountModelItem | null
  result: PoolAccountModelTestResult | null
}

const props = withDefaults(defineProps<{
  poolKey: PoolKeyDetail
  providerType?: string | null
  endpoints: ProviderEndpoint[]
  modelsState: PoolAccountModelsState
  customTestedModels: string[]
  getModelResult: (keyId: string, modelName: string, endpointId?: string | null) => PoolAccountModelTestResult | null
  canRefreshToken?: boolean
  refreshingOauth?: boolean
}>(), {
  providerType: null,
  canRefreshToken: false,
  refreshingOauth: false,
})

const emit = defineEmits<{
  refreshModels: []
  refreshOauth: []
  testModel: [payload: { modelName: string; endpoint: ProviderEndpoint | null; modelMeta: PoolAccountModelItem | null; isCustom?: boolean }]
  cancelTest: [payload: { modelName: string; endpoint: ProviderEndpoint | null }]
  inspectModel: [payload: PoolKeyModelInspectPayload]
  close: []
}>()

const { t } = useI18n()
const { copyToClipboard } = useClipboard()

const searchText = ref('')
const customModelInput = ref('')
const selectedEndpointId = ref('')

const compatibleEndpoints = computed(() =>
  getCompatiblePoolKeyEndpoints(props.poolKey, props.endpoints, props.providerType),
)

watch(
  compatibleEndpoints,
  (endpoints) => {
    if (endpoints.length === 0) {
      selectedEndpointId.value = ''
      return
    }
    if (!endpoints.some(item => item.id === selectedEndpointId.value)) {
      selectedEndpointId.value = selectDefaultPoolKeyEndpoint(endpoints)?.id ?? endpoints[0].id
    }
  },
  { immediate: true },
)

const selectedEndpoint = computed<ProviderEndpoint | null>(() => {
  if (!selectedEndpointId.value) {
    return compatibleEndpoints.value[0] ?? null
  }
  return compatibleEndpoints.value.find(item => item.id === selectedEndpointId.value)
    ?? compatibleEndpoints.value[0]
    ?? null
})

const filteredModels = computed(() => {
  const query = searchText.value.trim().toLowerCase()
  if (!query) return props.modelsState.models
  return props.modelsState.models.filter((item) => {
    const idMatch = item.id.toLowerCase().includes(query)
    const displayMatch = item.display_name?.toLowerCase().includes(query) ?? false
    return idMatch || displayMatch
  })
})

const modelCountLabel = computed(() => {
  const total = props.modelsState.models.length
  const filtered = filteredModels.value.length
  if (searchText.value.trim() && filtered !== total) {
    return t('pool.modelTest.filteredCount', { filtered, total })
  }
  return t('pool.modelTest.modelCount', { count: total })
})

const trimmedCustomModel = computed(() => customModelInput.value.trim())

function resolveEndpointForModel(modelMeta: PoolAccountModelItem | null): ProviderEndpoint | null {
  if (compatibleEndpoints.value.length <= 1) {
    return compatibleEndpoints.value[0] ?? null
  }
  if (selectedEndpoint.value) {
    return selectedEndpoint.value
  }
  return selectDefaultPoolKeyEndpoint(compatibleEndpoints.value, modelMeta)
}

function getRowResult(model: PoolAccountModelItem): PoolAccountModelTestResult | null {
  const endpoint = resolveEndpointForModel(model)
  return props.getModelResult(props.poolKey.key_id, model.id, endpoint?.id ?? null)
}

const activeCustomResult = computed(() => {
  if (!trimmedCustomModel.value) return null
  return props.getModelResult(
    props.poolKey.key_id,
    trimmedCustomModel.value,
    selectedEndpoint.value?.id ?? null,
  )
})

const fetchedModelIdSet = computed(() => new Set(props.modelsState.models.map(item => item.id)))

const customTestedEntries = computed(() => {
  const endpointId = selectedEndpoint.value?.id ?? null
  return props.customTestedModels
    .filter(modelName => !fetchedModelIdSet.value.has(modelName))
    .map(modelName => ({
      modelName,
      result: props.getModelResult(props.poolKey.key_id, modelName, endpointId),
    }))
    .filter(item => item.result !== null)
})

function handleSelectEndpoint(value: unknown) {
  if (typeof value === 'string') {
    selectedEndpointId.value = value
  }
}

function handleTestModel(modelName: string, modelMeta: PoolAccountModelItem | null) {
  const trimmed = modelName.trim()
  if (!trimmed) return
  emit('testModel', {
    modelName: trimmed,
    endpoint: resolveEndpointForModel(modelMeta),
    modelMeta,
    isCustom: false,
  })
}

function handleRunCustomModelTest() {
  if (!trimmedCustomModel.value) return
  emit('testModel', {
    modelName: trimmedCustomModel.value,
    endpoint: selectedEndpoint.value,
    modelMeta: null,
    isCustom: true,
  })
}

function handleCancelTest(modelName: string, modelMeta: PoolAccountModelItem | null = null) {
  const trimmed = modelName.trim()
  if (!trimmed) return
  emit('cancelTest', {
    modelName: trimmed,
    endpoint: resolveEndpointForModel(modelMeta),
  })
}

function handleInspectModel(modelName: string, modelMeta: PoolAccountModelItem | null) {
  const trimmed = modelName.trim()
  if (!trimmed) return
  const endpoint = resolveEndpointForModel(modelMeta)
  const result = props.getModelResult(props.poolKey.key_id, trimmed, endpoint?.id ?? null)
  emit('inspectModel', {
    poolKey: props.poolKey,
    modelName: trimmed,
    endpoint,
    modelMeta,
    result,
  })
}

function formatPassLabel(result: PoolAccountModelTestResult): string {
  return result.statusCode
    ? `${t('pool.modelTest.statusPassed')} ${result.statusCode}`
    : t('pool.modelTest.statusPassed')
}

function formatFailLabel(result: PoolAccountModelTestResult): string {
  return result.statusCode
    ? `${t('pool.modelTest.statusFailed')} ${result.statusCode}`
    : t('pool.modelTest.statusFailed')
}

async function copyModelId(modelId: string) {
  await copyToClipboard(modelId)
}
</script>
