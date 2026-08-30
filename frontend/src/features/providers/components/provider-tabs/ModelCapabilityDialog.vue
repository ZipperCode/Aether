<template>
  <Dialog
    :open="open"
    size="3xl"
    @update:open="handleDialogUpdate"
  >
    <template #header>
      <div class="border-b border-border px-6 py-4">
        <div class="flex items-center gap-2">
          <BrainCircuit class="h-5 w-5 text-primary" />
          <div class="min-w-0 flex-1">
            <div class="text-lg font-semibold">
              模型能力检测
            </div>
            <p class="mt-1 text-xs text-muted-foreground">
              {{ model?.global_model_display_name || model?.provider_model_name || '-' }}
            </p>
          </div>
          <Button
            variant="ghost"
            size="icon"
            class="h-8 w-8 shrink-0"
            title="关闭"
            aria-label="关闭模型能力检测"
            @click="handleDialogUpdate(false)"
          >
            <X class="h-4 w-4" />
          </Button>
        </div>
      </div>
    </template>

    <div class="space-y-4">
      <div class="rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-xs text-amber-800 dark:text-amber-200">
        本检测只比较随机客观题上的能力表现，不是模型身份认证，也不能确认底层模型是否被替换或掺水。
      </div>

      <div
        v-if="phase === 'setup'"
        class="space-y-4"
      >
        <div class="grid gap-4 sm:grid-cols-2">
          <label class="space-y-1.5 text-sm">
            <span class="font-medium">检测模式</span>
            <select
              v-model="mode"
              class="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
            >
              <option value="quick">
                快筛 · 40 题
              </option>
              <option value="verify">
                复核 · 100 题
              </option>
            </select>
          </label>
          <label class="space-y-1.5 text-sm">
            <span class="font-medium">题面语言</span>
            <select
              v-model="language"
              class="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
            >
              <option value="bilingual">
                中英双语（默认）
              </option>
              <option value="zh">
                中文
              </option>
              <option value="en">
                English
              </option>
            </select>
          </label>
        </div>

        <div class="rounded-lg border border-border/60 p-4 space-y-3">
          <div class="font-medium text-sm">
            目标固定候选
          </div>
          <div class="grid gap-3 sm:grid-cols-2">
            <label class="space-y-1.5 text-xs">
              <span class="text-muted-foreground">Endpoint</span>
              <select
                v-model="targetEndpointId"
                class="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
                @change="selectDefaultTargetKey"
              >
                <option
                  v-for="endpoint in targetEndpointOptions"
                  :key="endpoint.id"
                  :value="endpoint.id"
                >
                  {{ formatApiFormat(endpoint.api_format) }} · {{ endpoint.base_url }}
                </option>
              </select>
            </label>
            <label class="space-y-1.5 text-xs">
              <span class="text-muted-foreground">Key</span>
              <select
                v-model="targetKeyId"
                class="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
              >
                <option
                  v-for="key in targetKeyOptions"
                  :key="key.id"
                  :value="key.id"
                >
                  {{ keyLabel(key) }}
                </option>
              </select>
            </label>
          </div>
        </div>

        <div class="rounded-lg border border-border/60 p-4 space-y-3">
          <label class="flex items-start gap-2 text-sm">
            <input
              v-model="useReference"
              type="checkbox"
              class="mt-0.5 h-4 w-4 rounded border-border"
            >
            <span>
              <span class="font-medium">与可信官方直连参考比较</span>
              <span class="mt-0.5 block text-xs text-muted-foreground">
                首次选择会保存到当前模型配置；后端每次都重新校验，不会自动换参考。
              </span>
            </span>
          </label>

          <div
            v-if="useReference"
            class="grid gap-3 sm:grid-cols-2"
          >
            <label class="space-y-1.5 text-xs">
              <span class="text-muted-foreground">参考 Provider</span>
              <select
                :value="referenceProviderId"
                class="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
                @change="handleReferenceProviderChange"
              >
                <option value="">
                  请选择
                </option>
                <option
                  v-for="item in referenceProviders"
                  :key="item.id"
                  :value="item.id"
                >
                  {{ item.name }}
                </option>
              </select>
            </label>
            <label class="space-y-1.5 text-xs">
              <span class="text-muted-foreground">参考模型</span>
              <select
                v-model="referenceModelId"
                class="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
                :disabled="referenceLoading"
              >
                <option value="">
                  请选择
                </option>
                <option
                  v-for="item in referenceModels"
                  :key="item.id"
                  :value="item.id"
                >
                  {{ item.global_model_display_name || item.provider_model_name }} · {{ item.provider_model_name }}
                </option>
              </select>
            </label>
            <label class="space-y-1.5 text-xs">
              <span class="text-muted-foreground">参考 Endpoint</span>
              <select
                v-model="referenceEndpointId"
                class="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
                :disabled="referenceLoading"
                @change="selectDefaultReferenceKey"
              >
                <option value="">
                  请选择
                </option>
                <option
                  v-for="endpoint in referenceEndpointOptions"
                  :key="endpoint.id"
                  :value="endpoint.id"
                >
                  {{ formatApiFormat(endpoint.api_format) }} · {{ endpoint.base_url }}
                </option>
              </select>
            </label>
            <label class="space-y-1.5 text-xs">
              <span class="text-muted-foreground">参考 Key</span>
              <select
                v-model="referenceKeyId"
                class="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
                :disabled="referenceLoading"
              >
                <option value="">
                  请选择
                </option>
                <option
                  v-for="key in referenceKeyOptions"
                  :key="key.id"
                  :value="key.id"
                >
                  {{ keyLabel(key) }}
                </option>
              </select>
            </label>
          </div>
        </div>

        <div class="rounded-md border border-border/60 bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
          本次 {{ questionCount }} 题；{{ useReference ? `${questionCount * 2} 次` : `${questionCount} 次` }}最大上游调用。所有调用共享并发上限 4，且每一方固定使用一个 Endpoint 与 Key。
        </div>
        <div
          v-if="message"
          class="rounded-md border px-3 py-2 text-xs"
          :class="messageIsError ? 'border-destructive/30 bg-destructive/10 text-destructive' : 'border-border/60 bg-muted/20 text-muted-foreground'"
        >
          {{ message }}
        </div>
        <div
          v-if="useReference && referenceEqualsTarget()"
          class="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive"
        >
          可信参考不能与目标使用完全相同的 Provider、模型、Endpoint 和 Key。
        </div>
        <Button
          class="w-full"
          :disabled="!canStart"
          @click="startRun(mode)"
        >
          <Loader2
            v-if="loading"
            class="mr-2 h-4 w-4 animate-spin"
          />
          开始能力检测
        </Button>
      </div>

      <div
        v-else-if="phase === 'running'"
        class="space-y-5 py-6 text-center"
      >
        <Loader2 class="mx-auto h-9 w-9 animate-spin text-primary" />
        <div>
          <div class="font-medium">
            正在运行 {{ mode === 'quick' ? '40 题快筛' : '100 题复核' }}
          </div>
          <p class="mt-1 text-xs text-muted-foreground">
            进度取决于上游响应；{{ mode === 'quick' ? '最长 10 分钟' : '最长 20 分钟' }}。
          </p>
        </div>
        <div class="h-2 overflow-hidden rounded-full bg-muted">
          <div class="h-full w-1/2 animate-pulse rounded-full bg-primary" />
        </div>
        <Button
          variant="outline"
          @click="cancelRun"
        >
          取消检测
        </Button>
      </div>

      <div
        v-else-if="result"
        class="space-y-4"
      >
        <div class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border/60 bg-muted/20 p-4">
          <div>
            <div class="text-xs text-muted-foreground">
              检测结论
            </div>
            <div class="mt-1 text-lg font-semibold">
              {{ verdictLabel(result.verdict) }}
            </div>
            <div
              v-if="result.inconclusive_reason"
              class="mt-1 text-xs text-muted-foreground"
            >
              {{ inconclusiveLabel(result.inconclusive_reason) }}
            </div>
          </div>
          <Badge :variant="verdictVariant(result.verdict)">
            {{ result.mode === 'quick' ? '40 题快筛' : '100 题复核' }}
          </Badge>
        </div>

        <div class="grid gap-3 md:grid-cols-2">
          <div
            v-for="card in resultMetricCards"
            :key="card.title"
            class="rounded-lg border border-border/60 p-4 space-y-3"
          >
            <div>
              <div class="text-sm font-medium">
                {{ card.title }}
              </div>
              <div class="mt-1 break-all text-xs text-muted-foreground">
                {{ card.modelName }}
              </div>
            </div>
            <div class="grid grid-cols-2 gap-3 text-xs">
              <div>
                <div class="text-muted-foreground">
                  总分
                </div>
                <div class="mt-1 text-lg font-semibold tabular-nums">
                  {{ formatPercent(card.metrics.score) }}
                </div>
              </div>
              <div>
                <div class="text-muted-foreground">
                  覆盖率
                </div>
                <div class="mt-1 text-lg font-semibold tabular-nums">
                  {{ formatPercent(card.metrics.coverage) }}
                </div>
              </div>
            </div>
            <div class="text-xs text-muted-foreground">
              95% Wilson：{{ formatPercent(card.metrics.wilson_low) }}–{{ formatPercent(card.metrics.wilson_high) }} · {{ card.metrics.correct }}/{{ card.metrics.scored }} 正确
            </div>
          </div>
        </div>

        <div class="overflow-hidden rounded-lg border border-border/60">
          <table class="w-full text-xs">
            <thead class="bg-muted/40 text-muted-foreground">
              <tr>
                <th class="px-3 py-2 text-left">
                  维度
                </th>
                <th class="px-3 py-2 text-right">
                  目标
                </th>
                <th
                  v-if="result.reference_metrics"
                  class="px-3 py-2 text-right"
                >
                  参考
                </th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="dimension in result.target_metrics.dimensions"
                :key="dimension.dimension"
                class="border-t border-border/50"
              >
                <td class="px-3 py-2">
                  {{ dimensionLabel(dimension.dimension) }}
                </td>
                <td class="px-3 py-2 text-right tabular-nums">
                  {{ formatPercent(dimension.score) }} · {{ dimension.scored }}/{{ dimension.planned }}
                </td>
                <td
                  v-if="result.reference_metrics"
                  class="px-3 py-2 text-right tabular-nums"
                >
                  {{ formatPercent(referenceDimensionScore(dimension.dimension)) }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <div
          v-if="result.comparison"
          class="grid gap-3 rounded-lg border border-border/60 p-4 text-xs sm:grid-cols-3"
        >
          <div>
            <div class="text-muted-foreground">
              配对覆盖
            </div>
            <div class="mt-1 font-medium tabular-nums">
              {{ formatPercent(result.comparison.paired_coverage) }}
            </div>
          </div>
          <div>
            <div class="text-muted-foreground">
              参考 − 目标
            </div>
            <div class="mt-1 font-medium tabular-nums">
              {{ formatSignedPercent(result.comparison.score_gap) }}
            </div>
          </div>
          <div>
            <div class="text-muted-foreground">
              McNemar p
            </div>
            <div class="mt-1 font-medium tabular-nums">
              {{ result.comparison.p_value.toFixed(4) }}
            </div>
          </div>
        </div>

        <div class="grid gap-3 text-xs sm:grid-cols-2">
          <div class="rounded-lg border border-border/60 p-3">
            <div class="font-medium">
              目标失败分类
            </div>
            <div class="mt-2 text-muted-foreground">
              {{ failureSummary(result.target_metrics.failures) }}
            </div>
          </div>
          <div
            v-if="result.reference_metrics"
            class="rounded-lg border border-border/60 p-3"
          >
            <div class="font-medium">
              参考失败分类
            </div>
            <div class="mt-2 text-muted-foreground">
              {{ failureSummary(result.reference_metrics.failures) }}
            </div>
          </div>
          <div class="rounded-lg border border-border/60 p-3">
            <div class="font-medium">
              目标用量与耗时
            </div>
            <div class="mt-2 text-muted-foreground">
              {{ usageSummary(result.target_metrics.usage) }} · 上游累计 {{ formatDuration(result.target_metrics.elapsed_ms) }}
            </div>
          </div>
          <div
            v-if="result.reference_metrics"
            class="rounded-lg border border-border/60 p-3"
          >
            <div class="font-medium">
              参考用量与耗时
            </div>
            <div class="mt-2 text-muted-foreground">
              {{ usageSummary(result.reference_metrics.usage) }} · 上游累计 {{ formatDuration(result.reference_metrics.elapsed_ms) }}
            </div>
          </div>
        </div>
        <div class="text-right text-xs text-muted-foreground">
          总墙钟耗时 {{ formatDuration(result.elapsed_ms) }} · Seed {{ result.seed }}
        </div>

        <div class="flex flex-wrap justify-end gap-2">
          <Button
            variant="outline"
            @click="backToSetup"
          >
            返回配置
          </Button>
          <Button
            v-if="result.verdict === 'needs_verification'"
            @click="startVerification"
          >
            开始 100 题复核
          </Button>
          <Button
            v-else
            @click="startRun(mode)"
          >
            使用新 Seed 重测
          </Button>
        </div>
      </div>
    </div>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { BrainCircuit, Loader2, X } from 'lucide-vue-next'
import { Dialog } from '@/components/ui'
import Badge from '@/components/ui/badge.vue'
import Button from '@/components/ui/button.vue'
import {
  getProviderEndpoints,
  getProviderModels,
  getProvidersSummary,
  testModelCapability,
  type CapabilityTestReferenceConfig,
  type EndpointAPIKey,
  type Model,
  type ModelCapabilityDimension,
  type ModelCapabilityFailureCounts,
  type ModelCapabilityInconclusiveReason,
  type ModelCapabilityLanguage,
  type ModelCapabilityMetrics,
  type ModelCapabilityMode,
  type ModelCapabilityUsage,
  type ModelCapabilityVerdict,
  type ProviderEndpoint,
  type ProviderWithEndpointsSummary,
  type TestModelCapabilityResponse,
} from '@/api/endpoints'
import { getProviderKeys } from '@/api/endpoints/keys'
import { updateModel } from '@/api/endpoints/models'
import { formatApiFormat } from '@/api/endpoints/types/api-format'
import { parseApiError } from '@/utils/errorParser'
import {
  isModelCapabilityApiFormat,
  modelSupportsCapabilityDetection,
  modelTestKeySupportsEndpoint,
} from './model-test-capabilities'

/** 对话框输入只接收当前模型与已有 provider 目录对象。 */
const props = defineProps<{
  /** 是否打开对话框。 */
  open: boolean
  /** 当前目标 Provider。 */
  provider: ProviderWithEndpointsSummary
  /** 当前目标 ProviderModel。 */
  model: Model | null
  /** 当前 Provider 已加载的 endpoints，可减少重复请求。 */
  endpoints?: ProviderEndpoint[]
}>()

/** 关闭和保存配置后通知模型列表刷新。 */
const emit = defineEmits<{
  /** 关闭对话框。 */
  close: []
  /** 模型 config 已合并保存。 */
  saved: []
}>()

/** 对话框只在配置、运行与结果三种互斥阶段间切换。 */
type DialogPhase = 'setup' | 'running' | 'result'

const phase = ref<DialogPhase>('setup')
const mode = ref<ModelCapabilityMode>('quick')
const language = ref<ModelCapabilityLanguage>('bilingual')
const useReference = ref(false)
const loading = ref(false)
const referenceLoading = ref(false)
const message = ref('')
const messageIsError = ref(false)
const result = ref<TestModelCapabilityResponse | null>(null)
const targetEndpointId = ref('')
const targetKeyId = ref('')
const targetKeys = ref<EndpointAPIKey[]>([])
const referenceProviders = ref<ProviderWithEndpointsSummary[]>([])
const referenceProviderId = ref('')
const referenceModelId = ref('')
const referenceEndpointId = ref('')
const referenceKeyId = ref('')
const referenceModels = ref<Model[]>([])
const referenceEndpoints = ref<ProviderEndpoint[]>([])
const referenceKeys = ref<EndpointAPIKey[]>([])
const savedReference = ref<CapabilityTestReferenceConfig | null>(null)
let abortController: AbortController | null = null
// 只允许最后一次 Provider 选择写回异步结果，避免旧请求拼出跨 Provider 四元组。
let referenceLoadVersion = 0

/** 当前模式的服务端计划题数。 */
const questionCount = computed(() => mode.value === 'verify' ? 100 : 40)

/** 当前目标可用于能力检测且至少有一个可用 Key 的 endpoints。 */
const targetEndpointOptions = computed(() => (props.endpoints ?? [])
  .filter(endpoint => isCapabilityEndpoint(endpoint, targetKeys.value, props.provider.provider_type)))

/** 选中目标 endpoint 下可固定使用的 Keys。 */
const targetKeyOptions = computed(() => keysForEndpoint(
  targetKeys.value,
  targetEndpointOptions.value.find(endpoint => endpoint.id === targetEndpointId.value),
  props.provider.provider_type,
))

/** 参考 Provider 下可用于文本能力检测的 endpoints。 */
const referenceEndpointOptions = computed(() => {
  const provider = referenceProviders.value.find(item => item.id === referenceProviderId.value)
  return referenceEndpoints.value.filter(endpoint => (
    isCapabilityEndpoint(endpoint, referenceKeys.value, provider?.provider_type)
  ))
})

/** 选中参考 endpoint 下可固定使用的 Keys。 */
const referenceKeyOptions = computed(() => {
  const provider = referenceProviders.value.find(item => item.id === referenceProviderId.value)
  return keysForEndpoint(
    referenceKeys.value,
    referenceEndpointOptions.value.find(endpoint => endpoint.id === referenceEndpointId.value),
    provider?.provider_type,
  )
})

/** 启动前确保目标候选完整，启用参考时四个参考 ID 也完整。 */
const canStart = computed(() => (
  !loading.value
  && Boolean(props.model && targetEndpointId.value && targetKeyId.value)
  && (!useReference.value || (referenceIsComplete() && !referenceEqualsTarget()))
))

/** 将目标与可选参考归一为同一组结果卡数据，避免维护单用途展示组件。 */
const resultMetricCards = computed<Array<{
  /** 卡片标题。 */
  title: string
  /** 实际执行模型名。 */
  modelName: string
  /** 后端评分与置信区间。 */
  metrics: ModelCapabilityMetrics
}>>(() => {
  if (!result.value) return []
  const cards = [{
    title: '目标',
    modelName: result.value.target.effective_model,
    metrics: result.value.target_metrics,
  }]
  if (result.value.reference && result.value.reference_metrics) {
    cards.push({
      title: '可信参考',
      modelName: result.value.reference.effective_model,
      metrics: result.value.reference_metrics,
    })
  }
  return cards
})

/** 打开时加载当前 Keys、Provider 列表，并尽量恢复已保存参考。 */
async function initializeDialog() {
  if (!props.model) return
  referenceLoadVersion += 1
  referenceLoading.value = false
  referenceProviderId.value = ''
  referenceModelId.value = ''
  referenceEndpointId.value = ''
  referenceKeyId.value = ''
  referenceModels.value = []
  referenceEndpoints.value = []
  referenceKeys.value = []
  phase.value = 'setup'
  result.value = null
  message.value = ''
  messageIsError.value = false
  loading.value = true
  const hasSavedReference = Object.prototype.hasOwnProperty.call(
    props.model.config ?? {},
    'capability_test_reference',
  )
  savedReference.value = readCapabilityReference(props.model)
  useReference.value = hasSavedReference
  if (hasSavedReference && !savedReference.value) {
    showMessage('已保存的参考配置格式无效，请重新选择后再运行。', true)
  }
  try {
    const [keys, providers] = await Promise.all([
      getProviderKeys(props.provider.id),
      getProvidersSummary({ page: 1, page_size: 9999 }),
    ])
    targetKeys.value = keys
    referenceProviders.value = providers.items.filter(item => item.is_active)
    const preferredEndpoint = targetEndpointOptions.value[0]
    targetEndpointId.value = preferredEndpoint?.id ?? ''
    selectDefaultTargetKey()
    if (savedReference.value) {
      if (referenceProviders.value.some(item => item.id === savedReference.value?.provider_id)) {
        await loadReferenceProvider(savedReference.value.provider_id, savedReference.value)
      } else {
        showMessage('已保存的参考 Provider 已停用或不存在，请重新选择。', true)
      }
    }
  } catch (error: unknown) {
    showMessage(parseApiError(error, '加载能力检测配置失败'), true)
  } finally {
    loading.value = false
  }
}

/** 从运行时 JSON config 安全读取完整参考四元组。 */
function readCapabilityReference(model: Model): CapabilityTestReferenceConfig | null {
  const value = model.config?.capability_test_reference
  if (!value || typeof value !== 'object') return null
  const reference = value as Partial<CapabilityTestReferenceConfig>
  if (![reference.provider_id, reference.model_id, reference.endpoint_id, reference.api_key_id]
    .every(item => typeof item === 'string' && item.trim())) return null
  return {
    provider_id: reference.provider_id!,
    model_id: reference.model_id!,
    endpoint_id: reference.endpoint_id!,
    api_key_id: reference.api_key_id!,
  }
}

/** 判断 endpoint 协议、启用状态与 Key 能力是否满足文本能力检测。 */
function isCapabilityEndpoint(
  endpoint: ProviderEndpoint,
  keys: EndpointAPIKey[],
  providerType?: string | null,
): boolean {
  return endpoint.is_active !== false
    && isModelCapabilityApiFormat(endpoint.api_format)
    && keys.some(key => modelTestKeySupportsEndpoint(key, endpoint, providerType))
}

/** 返回某 endpoint 下启用且协议权限匹配的 Keys。 */
function keysForEndpoint(
  keys: EndpointAPIKey[],
  endpoint: ProviderEndpoint | undefined,
  providerType?: string | null,
): EndpointAPIKey[] {
  if (!endpoint) return []
  return keys
    .filter(key => modelTestKeySupportsEndpoint(key, endpoint, providerType))
    .sort((left, right) => left.internal_priority - right.internal_priority)
}

/** Endpoint 改变后固定选择优先级最高的目标 Key。 */
function selectDefaultTargetKey() {
  if (!targetKeyOptions.value.some(key => key.id === targetKeyId.value)) {
    targetKeyId.value = targetKeyOptions.value[0]?.id ?? ''
  }
}

/** Endpoint 改变后固定选择优先级最高的参考 Key。 */
function selectDefaultReferenceKey() {
  if (!referenceKeyOptions.value.some(key => key.id === referenceKeyId.value)) {
    referenceKeyId.value = referenceKeyOptions.value[0]?.id ?? ''
  }
}

/** 用户切换参考 Provider 时加载其模型、endpoint 与 Keys，不跨 Provider 复用旧 ID。 */
async function handleReferenceProviderChange(event: Event) {
  const providerId = (event.target as HTMLSelectElement).value
  await loadReferenceProvider(providerId)
}

/** 加载指定参考 Provider，并仅在四个保存引用都仍存在时恢复它们。 */
async function loadReferenceProvider(
  providerId: string,
  preferred?: CapabilityTestReferenceConfig,
) {
  const loadVersion = ++referenceLoadVersion
  referenceProviderId.value = providerId
  referenceModelId.value = ''
  referenceEndpointId.value = ''
  referenceKeyId.value = ''
  referenceModels.value = []
  referenceEndpoints.value = []
  referenceKeys.value = []
  if (!providerId) {
    referenceLoading.value = false
    return
  }
  referenceLoading.value = true
  try {
    const [models, endpoints, keys] = await Promise.all([
      getProviderModels(providerId, { is_active: true, limit: 1000 }),
      getProviderEndpoints(providerId),
      getProviderKeys(providerId),
    ])
    if (loadVersion !== referenceLoadVersion || referenceProviderId.value !== providerId) return
    referenceModels.value = models.filter(item => (
      item.is_active && modelSupportsCapabilityDetection(item)
    ))
    referenceEndpoints.value = endpoints
    referenceKeys.value = keys
    referenceModelId.value = referenceModels.value.some(item => item.id === preferred?.model_id)
      ? preferred!.model_id
      : ''
    referenceEndpointId.value = preferred
      ? (referenceEndpointOptions.value.some(item => item.id === preferred.endpoint_id)
          ? preferred.endpoint_id
          : '')
      : referenceEndpointOptions.value[0]?.id ?? ''
    referenceKeyId.value = preferred
      ? (referenceKeyOptions.value.some(item => item.id === preferred.api_key_id)
          ? preferred.api_key_id
          : '')
      : referenceKeyOptions.value[0]?.id ?? ''
    if (preferred && !referenceIsComplete()) {
      showMessage('已保存的参考引用已失效，请重新选择后再运行。', true)
    }
  } catch (error: unknown) {
    if (loadVersion === referenceLoadVersion) {
      showMessage(parseApiError(error, '加载参考配置失败'), true)
    }
  } finally {
    if (loadVersion === referenceLoadVersion) referenceLoading.value = false
  }
}

/** 判断参考四元组是否完整且当前列表仍包含所有引用。 */
function referenceIsComplete(): boolean {
  return Boolean(
    referenceProviderId.value
    && referenceModels.value.some(item => item.id === referenceModelId.value)
    && referenceEndpointOptions.value.some(item => item.id === referenceEndpointId.value)
    && referenceKeyOptions.value.some(item => item.id === referenceKeyId.value),
  )
}

/** 阻止把目标自身作为可信参考，避免产生没有意义的自比较。 */
function referenceEqualsTarget(): boolean {
  return Boolean(
    props.model
    && referenceProviderId.value === props.provider.id
    && referenceModelId.value === props.model.id
    && referenceEndpointId.value === targetEndpointId.value
    && referenceKeyId.value === targetKeyId.value,
  )
}

/** 仅当参考发生变化时合并保存 config，保留所有其他模型配置键。 */
async function saveReferenceIfNeeded() {
  if (!useReference.value || !props.model || !referenceIsComplete()) return
  const next: CapabilityTestReferenceConfig = {
    provider_id: referenceProviderId.value,
    model_id: referenceModelId.value,
    endpoint_id: referenceEndpointId.value,
    api_key_id: referenceKeyId.value,
  }
  if (JSON.stringify(next) === JSON.stringify(savedReference.value)) return
  const updated = await updateModel(props.provider.id, props.model.id, {
    config: {
      ...(props.model.config ?? {}),
      capability_test_reference: next,
    },
  })
  savedReference.value = next
  if (updated.config) props.model.config = updated.config
  emit('saved')
}

/** 保存必要配置并调用同步能力检测；每次调用都让后端生成新 seed。 */
async function startRun(nextMode: ModelCapabilityMode) {
  if (!props.model || !canStart.value || abortController) return
  const controller = new AbortController()
  abortController = controller
  mode.value = nextMode
  loading.value = true
  message.value = ''
  messageIsError.value = false
  try {
    await saveReferenceIfNeeded()
    if (controller.signal.aborted || !props.open || !props.model) return
    phase.value = 'running'
    result.value = await testModelCapability({
      provider_id: props.provider.id,
      model_id: props.model.id,
      endpoint_id: targetEndpointId.value,
      api_key_id: targetKeyId.value,
      mode: nextMode,
      language: language.value,
      use_saved_reference: useReference.value,
      request_id: createCapabilityRequestId(),
    }, { signal: controller.signal })
    phase.value = 'result'
  } catch (error: unknown) {
    const cancelled = abortController?.signal.aborted === true
    phase.value = 'setup'
    showMessage(
      cancelled
        ? '检测已取消；未完成题目不会计为能力下降。'
        : parseApiError(error, '模型能力检测失败'),
      !cancelled,
    )
  } finally {
    abortController = null
    loading.value = false
  }
}

/** 取消当前 HTTP 请求；后端 future 随连接取消并停止未完成调用。 */
function cancelRun() {
  abortController?.abort()
}

/** 快筛建议复核时直接用相同配置发起新 seed 的 100 题运行。 */
function startVerification() {
  void startRun('verify')
}

/** 从结果返回配置页，不改变已保存参考。 */
function backToSetup() {
  result.value = null
  phase.value = 'setup'
}

/** 对话框关闭时先取消自身请求，避免后台继续消耗上游调用。 */
function handleDialogUpdate(value: boolean) {
  if (value) return
  cancelRun()
  emit('close')
}

/** 设置配置页提示，并明确区分普通状态与错误。 */
function showMessage(value: string, isError: boolean) {
  message.value = value
  messageIsError.value = isError
}

/** 优先使用浏览器原生 UUID；非安全 HTTP 环境缺少 randomUUID 时使用无敏感信息的时间随机串。 */
function createCapabilityRequestId(): string {
  const randomId = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`
  return `provider-capability-${randomId}`
}

/** 生成不含完整 Key 的可辨识标签。 */
function keyLabel(key: EndpointAPIKey): string {
  return key.name?.trim() || key.api_key_masked?.trim() || key.id
}

/** 将 0-1 比率显示为一位小数百分比。 */
function formatPercent(value: number | null | undefined): string {
  return value == null ? '-' : `${(value * 100).toFixed(1)}%`
}

/** 显示带正负号的参考分差。 */
function formatSignedPercent(value: number | null): string {
  if (value == null) return '-'
  const percent = value * 100
  return `${percent > 0 ? '+' : ''}${percent.toFixed(1)}pp`
}

/** 将毫秒墙钟耗时显示为秒或分钟。 */
function formatDuration(value: number): string {
  return value >= 60_000 ? `${(value / 60_000).toFixed(1)} 分钟` : `${(value / 1000).toFixed(1)} 秒`
}

/** 把 token 与可用费用压缩为一行；缺失值不推算。 */
function usageSummary(usage: ModelCapabilityUsage | null): string {
  if (!usage) return '上游未提供 usage/cost'
  const tokens = usage.total_tokens == null ? 'token -' : `${usage.total_tokens} tokens`
  const cost = usage.cost_usd == null ? '费用 -' : `$${usage.cost_usd.toFixed(6)}`
  return `${tokens} · ${cost}`
}

/** 仅展示非零失败桶，全部已评分时显示“无”。 */
function failureSummary(failures: ModelCapabilityFailureCounts): string {
  const labels: Record<keyof ModelCapabilityFailureCounts, string> = {
    network_failure: '网络',
    rate_limited: '限流',
    timeout: '超时',
    filtered: '过滤',
    refused: '拒答',
    truncated: '截断',
    unparseable: '无法解析',
    upstream_error: '上游错误',
    cancelled: '取消',
  }
  const values = (Object.entries(failures) as Array<[keyof ModelCapabilityFailureCounts, number]>)
    .filter(([, count]) => count > 0)
    .map(([key, count]) => `${labels[key]} ${count}`)
  return values.length > 0 ? values.join(' · ') : '无'
}

/** 返回参考对应维度分数。 */
function referenceDimensionScore(dimension: ModelCapabilityDimension): number | null {
  return result.value?.reference_metrics?.dimensions
    .find(item => item.dimension === dimension)?.score ?? null
}

/** 五维机器名转换为业务展示名。 */
function dimensionLabel(value: ModelCapabilityDimension): string {
  return {
    quantitative: '数量',
    logical: '逻辑',
    algorithmic: '算法',
    language: '语言关系',
    instruction: '指令遵循',
  }[value]
}

/** 固定 verdict 转换为不夸大身份结论的中文文案。 */
function verdictLabel(value: ModelCapabilityVerdict): string {
  return {
    profile_only: '能力画像',
    no_large_deviation: '未发现明显偏离',
    needs_verification: '建议复核',
    no_significant_deviation: '复核未发现显著偏离',
    significant_deviation: '与参考明显不一致',
    inconclusive: '无法判断',
  }[value]
}

/** 无法判断原因转换为可执行提示。 */
function inconclusiveLabel(value: ModelCapabilityInconclusiveReason): string {
  return {
    total_timeout: '检测达到总时限，未完成题目已停止。',
    target_coverage: '目标有效回答覆盖不足。',
    reference_coverage: '参考有效回答覆盖不足。',
    paired_coverage: '双方同题可配对覆盖不足。',
  }[value]
}

/** verdict 对应 Badge 视觉语义；显著偏离与无法判断不显示为成功。 */
function verdictVariant(value: ModelCapabilityVerdict): 'success' | 'warning' | 'destructive' | 'outline' {
  if (value === 'significant_deviation') return 'destructive'
  if (value === 'needs_verification' || value === 'inconclusive') return 'warning'
  if (value === 'profile_only') return 'outline'
  return 'success'
}

watch(() => props.open, (open) => {
  if (open) void initializeDialog()
  else cancelRun()
})
</script>
