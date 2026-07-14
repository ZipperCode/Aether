<template>
  <div class="space-y-3">
    <div class="space-y-2 border-b border-border/60 pb-3">
      <div class="flex items-center justify-between gap-3">
        <div class="min-w-0">
          <p class="text-sm font-medium text-foreground">
            处理层级
          </p>
          <p class="truncate text-xs text-muted-foreground">
            {{ activeProcessingTierLabel }}
          </p>
        </div>
        <Button
          v-if="activeProcessingTierKey && isActiveProcessingTierConfigured"
          type="button"
          variant="ghost"
          size="sm"
          class="h-8 w-8 shrink-0 p-0 text-muted-foreground hover:text-destructive"
          data-testid="processing-tier-remove"
          :aria-label="`移除 ${activeProcessingTierLabel} 费率`"
          :title="`移除 ${activeProcessingTierLabel} 费率`"
          @click="removeActiveProcessingTier"
        >
          <Trash2 class="h-4 w-4" />
        </Button>
      </div>

      <div
        class="flex flex-wrap gap-1"
        role="group"
        aria-label="处理层级费率"
      >
        <Button
          v-for="option in processingTierOptions"
          :key="option.scope"
          type="button"
          :variant="activePricingScope === option.scope ? 'secondary' : 'ghost'"
          size="sm"
          class="h-8 max-w-full gap-1.5 px-2.5 font-medium"
          :aria-pressed="activePricingScope === option.scope"
          :aria-label="`${option.label}，${option.configured ? '已配置' : '未配置'}`"
          :title="option.label"
          :data-processing-tier="option.key"
          :data-configured="option.configured ? 'true' : 'false'"
          @click="selectPricingScope(option.scope)"
        >
          <span class="truncate">{{ option.label }}</span>
          <Plus
            v-if="!option.configured"
            class="h-3.5 w-3.5 shrink-0 text-muted-foreground"
            aria-hidden="true"
          />
        </Button>
      </div>
    </div>

    <div
      v-if="!isActivePricingScopeConfigured"
      class="flex flex-wrap items-center justify-between gap-3 py-4"
      data-testid="processing-tier-empty"
    >
      <p class="text-sm text-muted-foreground">
        未配置 {{ activeProcessingTierLabel }} 费率
      </p>
      <Button
        type="button"
        variant="outline"
        size="sm"
        data-testid="processing-tier-add"
        @click="addActiveProcessingTier"
      >
        <Plus class="mr-2 h-4 w-4" />
        添加费率
      </Button>
    </div>

    <template v-else>
      <template v-if="showTokenPricing !== false">
      <!-- 阶梯列表 -->
      <div
        v-for="(tier, index) in localTiers"
        :key="index"
        class="space-y-3 border-b border-border/60 pb-3 last:border-b-0"
      >
        <!-- 阶梯头部 -->
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2 text-sm">
            <span class="text-muted-foreground">{{ getTierStartLabel(index) }}</span>
            <span class="text-muted-foreground">-</span>
            <template v-if="isTierUpperBoundEditable(index)">
              <template v-if="customInputMode[index]">
                <Input
                  v-model="customInputValue[index]"
                  type="number"
                  min="1"
                  class="h-7 w-20 text-sm"
                  :aria-label="`${activeProcessingTierLabel} 阶梯 ${index + 1} 自定义上限（千 Token）`"
                  placeholder="K"
                  @keyup.enter="confirmCustomInput(index)"
                  @blur="confirmCustomInput(index)"
                />
                <span class="text-xs text-muted-foreground">K</span>
              </template>
              <select
                v-else
                :value="getSelectValue(index)"
                class="h-7 px-2 text-sm border rounded bg-background"
                :aria-label="`${activeProcessingTierLabel} 阶梯 ${index + 1} 上限`"
                @change="(e) => handleThresholdChange(index, parseInt((e.target as HTMLSelectElement).value))"
              >
                <option
                  v-for="opt in getAvailableThresholds(index)"
                  :key="opt.value"
                  :value="opt.value"
                >
                  {{ opt.label }}
                </option>
              </select>
            </template>
            <span
              v-else
              class="font-medium"
            >无上限</span>
          </div>
          <div class="flex items-center gap-1">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              class="h-7 px-2 text-xs text-muted-foreground"
              :aria-label="`${activeProcessingTierLabel} 阶梯 ${index + 1} 切换缓存价格输入方式`"
              @click="toggleCachePriceMode(index)"
            >
              <Repeat2 class="mr-1 h-3.5 w-3.5" />
              {{ getCachePriceMode(index) === 'multiplier' ? '价格' : '倍率' }}
            </Button>
            <Button
              v-if="localTiers.length > 1"
              variant="ghost"
              size="sm"
              class="h-7 w-7 p-0"
              :aria-label="`删除 ${activeProcessingTierLabel} 阶梯 ${index + 1}`"
              :title="`删除 ${activeProcessingTierLabel} 阶梯 ${index + 1}`"
              @click="removeTier(index)"
            >
              <X class="w-4 h-4 text-muted-foreground hover:text-destructive" />
            </Button>
          </div>
        </div>

        <!-- 价格输入 -->
        <div
          class="grid gap-3"
          :class="[showCache1h ? 'grid-cols-2 lg:grid-cols-5' : 'grid-cols-2 lg:grid-cols-4']"
        >
          <div class="space-y-1">
            <Label class="text-xs">输入 ($/M)</Label>
            <Input
              :model-value="tier.input_price_per_1m"
              data-testid="tier-input-price"
              :data-tier-index="index"
              type="number"
              step="0.01"
              min="0"
              class="h-8"
              :aria-label="`${activeProcessingTierLabel} 阶梯 ${index + 1} 输入价格（美元/百万 Token）`"
              placeholder="0"
              @update:model-value="(v) => updateInputPrice(index, parseFloatInput(v))"
            />
          </div>
          <div class="space-y-1">
            <Label class="text-xs">输出 ($/M)</Label>
            <Input
              :model-value="tier.output_price_per_1m"
              type="number"
              step="0.01"
              min="0"
              class="h-8"
              :aria-label="`${activeProcessingTierLabel} 阶梯 ${index + 1} 输出价格（美元/百万 Token）`"
              placeholder="0"
              @update:model-value="(v) => updateOutputPrice(index, parseFloatInput(v))"
            />
          </div>
          <div class="space-y-1">
            <Label class="text-xs text-muted-foreground">
              {{ getCachePriceMode(index) === 'multiplier' ? '创建（倍率）' : '创建 ($/M)' }}
            </Label>
            <div class="relative">
              <Input
                :model-value="getCacheCreationEditorValue(index)"
                type="number"
                step="0.01"
                min="0"
                class="h-8"
                :class="getCachePriceMode(index) === 'multiplier' ? 'pr-7' : ''"
                :aria-label="`${activeProcessingTierLabel} 阶梯 ${index + 1} 缓存创建${getCachePriceMode(index) === 'multiplier' ? '倍率' : '价格'}`"
                placeholder="0"
                @update:model-value="(v) => updateCacheCreation(index, v)"
              />
              <span
                v-if="getCachePriceMode(index) === 'multiplier'"
                class="absolute right-2 top-1/2 -translate-y-1/2 text-xs text-muted-foreground"
              >×</span>
            </div>
          </div>
          <div class="space-y-1">
            <Label class="text-xs text-muted-foreground">
              {{ getCachePriceMode(index) === 'multiplier' ? '读取（倍率）' : '读取 ($/M)' }}
            </Label>
            <div class="relative">
              <Input
                :model-value="getCacheReadEditorValue(index)"
                type="number"
                step="0.01"
                min="0"
                class="h-8"
                :class="getCachePriceMode(index) === 'multiplier' ? 'pr-7' : ''"
                :aria-label="`${activeProcessingTierLabel} 阶梯 ${index + 1} 缓存读取${getCachePriceMode(index) === 'multiplier' ? '倍率' : '价格'}`"
                placeholder="0"
                @update:model-value="(v) => updateCacheRead(index, v)"
              />
              <span
                v-if="getCachePriceMode(index) === 'multiplier'"
                class="absolute right-2 top-1/2 -translate-y-1/2 text-xs text-muted-foreground"
              >×</span>
            </div>
          </div>
          <div
            v-if="showCache1h"
            class="space-y-1"
          >
            <Label class="text-xs text-muted-foreground">1h 缓存</Label>
            <Input
              :model-value="getCache1hDisplay(index)"
              type="number"
              step="0.01"
              min="0"
              class="h-8"
              :aria-label="`${activeProcessingTierLabel} 阶梯 ${index + 1} 一小时缓存价格`"
              :placeholder="getCache1hPlaceholder(index)"
              @update:model-value="(v) => updateCache1h(index, v)"
            />
          </div>
        </div>
      </div>

      <!-- 添加阶梯按钮 -->
      <Button
        variant="outline"
        size="sm"
        class="w-full"
        @click="addTier"
      >
        <Plus class="w-4 h-4 mr-2" />
        添加价格阶梯
      </Button>
      </template>
    </template>

    <div
      v-if="showImagePricing && showImageEditor !== false && isActivePricingScopeConfigured"
      class="space-y-3 border-t border-border/60 pt-3"
    >
      <div class="flex flex-wrap items-end justify-between gap-3">
        <Label class="text-xs font-medium">图像输出计费 ($/张)</Label>
        <div class="flex items-center gap-2">
          <Label class="text-xs text-muted-foreground">默认价</Label>
          <Input
            :model-value="imageOutputPriceDefault"
            type="number"
            step="0.001"
            min="0"
            class="h-8 w-24"
            :aria-label="`${activeProcessingTierLabel} 图像输出默认价格`"
            placeholder="0"
            @update:model-value="updateImageOutputPriceDefault"
          />
        </div>
      </div>

      <div class="space-y-2">
        <div class="flex items-center justify-between gap-2">
          <Label class="text-xs text-muted-foreground">精确分辨率覆盖</Label>
          <span class="text-[11px] text-muted-foreground">优先匹配 size + quality</span>
        </div>
        <div class="overflow-x-auto pb-1">
          <div class="min-w-[32rem] space-y-2">
            <div class="grid grid-cols-[minmax(120px,1.1fr)_repeat(3,minmax(0,1fr))_32px] gap-2 text-xs text-muted-foreground">
              <span>分辨率</span>
              <span>low</span>
              <span>medium</span>
              <span>high</span>
              <span />
            </div>
            <div
              v-for="(row, rowIndex) in imageOutputPriceRows"
              :key="row.id"
              class="grid grid-cols-[minmax(120px,1.1fr)_repeat(3,minmax(0,1fr))_32px] items-center gap-2"
            >
              <Input
                :model-value="row.size"
                class="h-8 font-mono text-xs"
                :aria-label="`图像输出分辨率 ${rowIndex + 1}`"
                placeholder="1024x1024"
                @update:model-value="(v) => updateImageOutputSize(row.id, v)"
              />
              <Input
                v-for="quality in IMAGE_OUTPUT_QUALITIES"
                :key="`${row.id}-${quality}`"
                :model-value="getImageOutputPrice(row, quality)"
                type="number"
                step="0.001"
                min="0"
                class="h-8"
                :aria-label="`${row.size || `分辨率 ${rowIndex + 1}`} ${quality} 图像输出价格`"
                placeholder="0"
                @update:model-value="(v) => updateImageOutputPrice(row.id, quality, v)"
              />
              <Button
                type="button"
                variant="ghost"
                size="sm"
                class="h-8 w-8 p-0"
                :aria-label="`删除图像输出分辨率 ${row.size || rowIndex + 1}`"
                :title="`删除图像输出分辨率 ${row.size || rowIndex + 1}`"
                @click="removeImageOutputSizeRow(row.id)"
              >
                <X class="h-4 w-4 text-muted-foreground hover:text-destructive" />
              </Button>
            </div>
          </div>
        </div>
        <Button
          type="button"
          variant="outline"
          size="sm"
          class="w-full"
          @click="addImageOutputSizeRow"
        >
          <Plus class="w-4 h-4 mr-2" />
          添加分辨率
        </Button>
      </div>

      <div class="space-y-2 border-t pt-3">
        <div class="flex items-center justify-between gap-2">
          <Label class="text-xs text-muted-foreground">像素区间</Label>
          <span class="text-[11px] text-muted-foreground">矩阵未命中时按宽×高落档</span>
        </div>
        <div class="overflow-x-auto pb-1">
          <div class="min-w-[32rem] space-y-2">
            <div class="grid grid-cols-[minmax(120px,1.1fr)_repeat(3,minmax(0,1fr))_32px] gap-2 text-xs text-muted-foreground">
              <span>上限像素</span>
              <span>low</span>
              <span>medium</span>
              <span>high</span>
              <span />
            </div>
            <div
              v-for="(row, rowIndex) in imageOutputPriceRangeRows"
              :key="row.id"
              class="grid grid-cols-[minmax(120px,1.1fr)_repeat(3,minmax(0,1fr))_32px] items-center gap-2"
            >
              <Input
                :model-value="row.upToPixels"
                type="number"
                min="1"
                class="h-8 font-mono text-xs"
                :aria-label="`图像像素区间 ${rowIndex + 1} 上限`"
                placeholder="空=无上限"
                @update:model-value="(v) => updateImageOutputRangeLimit(row.id, v)"
              />
              <Input
                v-for="quality in IMAGE_OUTPUT_QUALITIES"
                :key="`${row.id}-${quality}`"
                :model-value="getImageOutputRangePrice(row, quality)"
                type="number"
                step="0.001"
                min="0"
                class="h-8"
                :aria-label="`图像像素区间 ${rowIndex + 1} ${quality} 价格`"
                placeholder="0"
                @update:model-value="(v) => updateImageOutputRangePrice(row.id, quality, v)"
              />
              <Button
                type="button"
                variant="ghost"
                size="sm"
                class="h-8 w-8 p-0"
                :aria-label="`删除图像像素区间 ${rowIndex + 1}`"
                :title="`删除图像像素区间 ${rowIndex + 1}`"
                @click="removeImageOutputRangeRow(row.id)"
              >
                <X class="h-4 w-4 text-muted-foreground hover:text-destructive" />
              </Button>
            </div>
          </div>
        </div>
        <Button
          type="button"
          variant="outline"
          size="sm"
          class="w-full"
          @click="addImageOutputRangeRow"
        >
          <Plus class="w-4 h-4 mr-2" />
          添加像素区间
        </Button>
      </div>
    </div>

    <!-- 验证提示 -->
    <p
      v-if="validationError"
      class="text-xs text-destructive"
      role="alert"
      aria-live="polite"
    >
      {{ validationError }}
    </p>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, reactive } from 'vue'
import { Plus, Repeat2, Trash2, X } from 'lucide-vue-next'
import { Button, Input, Label } from '@/components/ui'
import { formatTokens } from '@/utils/format'
import type {
  ImageOutputQualityPricing,
  ImageOutputPriceRange,
  PricingTier,
  ProcessingTierPricingConfig,
  TieredPricingConfig,
} from '@/api/endpoints/types'
import {
  cacheMultiplierFromPrice,
  cachePriceFromInputMultiplier,
} from '@/features/models/utils/tiered-pricing-multipliers'
import { comparePricingUpperBounds } from '@/features/models/utils/tiered-pricing'

type ImageOutputQuality = 'low' | 'medium' | 'high'
type ImageOutputPriceRow = {
  id: string
  size: string
  prices: Partial<Record<ImageOutputQuality, number>>
  rawPrices: ImageOutputQualityPricing
}
type ImageOutputPriceRangeRow = {
  id: string
  upToPixels: string
  prices: Partial<Record<ImageOutputQuality, number>>
  rawRange: Record<string, unknown>
}
type ImagePricingConfig = Pick<
  TieredPricingConfig,
  'image_output_prices' | 'image_output_price_default' | 'image_output_price_ranges'
>
type ImagePricingState = {
  rows: ImageOutputPriceRow[]
  rangeRows: ImageOutputPriceRangeRow[]
  defaultPrice: string
}
type CacheManualState = { creation: boolean; read: boolean; cache1h: boolean }
type CachePriceMode = 'multiplier' | 'price'
type CacheMultiplierDraft = { creation: string; read: string }
type ProcessingTierOption = {
  scope: string
  key: string
  label: string
  configured: boolean
}
type PricingScopePolicy = {
  allowEmptyTiers: boolean
  terminalUpperBound: 'require-unbounded' | 'finite-or-unbounded'
}

const props = withDefaults(defineProps<{
  modelValue?: TieredPricingConfig | null
  showTokenPricing?: boolean
  showCache1h?: boolean
  showImagePricing?: boolean
  showImageEditor?: boolean
}>(), {
  showTokenPricing: true,
  showImageEditor: true,
})
const emit = defineEmits<{
  'update:modelValue': [value: TieredPricingConfig | null]
}>()
const DEFAULT_IMAGE_OUTPUT_SIZES = ['1024x1024', '1536x1024', '1024x1536']
const DEFAULT_IMAGE_OUTPUT_PIXEL_LIMITS = [1_048_576, 1_572_864, 2_097_152]
const IMAGE_OUTPUT_QUALITIES: ImageOutputQuality[] = ['low', 'medium', 'high']
const STANDARD_PRICING_SCOPE = 'standard'
const PROCESSING_PRICING_SCOPE_PREFIX = 'processing:'
const UNBOUNDED_THRESHOLD_VALUE = -2
const KNOWN_PROCESSING_TIERS = [
  { key: 'priority', label: 'Priority' },
  { key: 'flex', label: 'Flex' },
  { key: 'batch', label: 'Batch' },
] as const

// 本地状态
const basePricingConfig = ref<Record<string, unknown>>({})
const standardTiers = ref<PricingTier[]>([])
const processingTierConfigs = ref<Record<string, ProcessingTierPricingConfig>>({})
const activePricingScope = ref(STANDARD_PRICING_SCOPE)
const processingTierKeysEdited = ref(false)
const originalEmptyProcessingTiers = ref<'absent' | 'null' | 'object'>('absent')
const lastEmittedPricingJson = ref<string>('')
let imageOutputPriceRowId = 0
let imageOutputPriceRangeRowId = 0

// 跟踪每个阶梯的缓存价格是否被手动设置
const cacheManualStateByScope = reactive<Record<string, Record<number, CacheManualState>>>({})
const cachePriceModesByScope = reactive<Record<string, Record<number, CachePriceMode>>>({})
const cacheMultiplierDraftsByScope = reactive<Record<string, Record<number, CacheMultiplierDraft>>>({})
const imagePricingStateByScope = reactive<Record<string, ImagePricingState>>({})

const activeProcessingTierKey = computed(() => processingTierKeyFromScope(activePricingScope.value))
const isActiveProcessingTierConfigured = computed(() => {
  const key = activeProcessingTierKey.value
  return key !== null && hasOwn(processingTierConfigs.value, key)
})
const isActivePricingScopeConfigured = computed(() => (
  activePricingScope.value === STANDARD_PRICING_SCOPE || isActiveProcessingTierConfigured.value
))
const activeProcessingTierLabel = computed(() => {
  const key = activeProcessingTierKey.value
  if (key === null) return 'Standard'
  return KNOWN_PROCESSING_TIERS.find(tier => tier.key === key)?.label ?? key
})
const processingTierOptions = computed<ProcessingTierOption[]>(() => {
  const knownKeys = new Set<string>(KNOWN_PROCESSING_TIERS.map(tier => tier.key))
  const options: ProcessingTierOption[] = [{
    scope: STANDARD_PRICING_SCOPE,
    key: 'standard',
    label: 'Standard',
    configured: true,
  }]
  for (const tier of KNOWN_PROCESSING_TIERS) {
    options.push({
      scope: processingTierScope(tier.key),
      key: tier.key,
      label: tier.label,
      configured: hasOwn(processingTierConfigs.value, tier.key),
    })
  }
  const discoveredKeys = Object.keys(processingTierConfigs.value)
    .filter(key => !knownKeys.has(key))
    .sort((left, right) => left.localeCompare(right))
  for (const key of discoveredKeys) {
    options.push({
      scope: processingTierScope(key),
      key,
      label: key,
      configured: true,
    })
  }
  return options
})
const localTiers = computed<PricingTier[]>({
  get() {
    const key = activeProcessingTierKey.value
    if (key === null) return standardTiers.value
    return processingTierConfigs.value[key]?.tiers ?? []
  },
  set(tiers) {
    const key = activeProcessingTierKey.value
    if (key === null) {
      standardTiers.value = tiers
      return
    }
    const config = processingTierConfigs.value[key]
    if (config) config.tiers = tiers
  },
})
const cacheManuallySet = computed(() => cacheManualStateByScope[activePricingScope.value])
const cachePriceModes = computed(() => cachePriceModesByScope[activePricingScope.value])
const cacheMultiplierDrafts = computed(() => cacheMultiplierDraftsByScope[activePricingScope.value])
const imageOutputPriceRows = computed<ImageOutputPriceRow[]>({
  get: () => imagePricingStateByScope[activePricingScope.value]?.rows ?? [],
  set: rows => setActiveImagePricingState({ rows }),
})
const imageOutputPriceRangeRows = computed<ImageOutputPriceRangeRow[]>({
  get: () => imagePricingStateByScope[activePricingScope.value]?.rangeRows ?? [],
  set: rangeRows => setActiveImagePricingState({ rangeRows }),
})
const imageOutputPriceDefault = computed<string>({
  get: () => imagePricingStateByScope[activePricingScope.value]?.defaultPrice ?? '',
  set: defaultPrice => setActiveImagePricingState({ defaultPrice }),
})

// 预设的阶梯上限选项
const THRESHOLD_OPTIONS = [
  { value: 64000, label: '64K' },
  { value: 128000, label: '128K' },
  { value: 200000, label: '200K' },
  { value: 272000, label: '272K' },
  { value: 500000, label: '500K' },
  { value: 1000000, label: '1M' },
  { value: -1, label: '自定义...' },  // 特殊值表示自定义输入
]

// 跟踪哪些阶梯正在使用自定义输入
const customInputMode = reactive<Record<number, boolean>>({})
const customInputValue = reactive<Record<number, string>>({})

// 初始化
watch(
  () => props.modelValue,
  (newValue) => {
    if (lastEmittedPricingJson.value && JSON.stringify(newValue ?? null) === lastEmittedPricingJson.value) {
      return
    }
    if (newValue?.tiers) {
      const clonedValue = cloneJson(newValue)
      basePricingConfig.value = clonedValue
      standardTiers.value = clonedValue.tiers
      processingTierConfigs.value = isRecord(clonedValue.processing_tiers)
        ? clonedValue.processing_tiers as Record<string, ProcessingTierPricingConfig>
        : {}
      originalEmptyProcessingTiers.value = clonedValue.processing_tiers === null
        ? 'null'
        : isRecord(clonedValue.processing_tiers) && Object.keys(clonedValue.processing_tiers).length === 0
          ? 'object'
          : 'absent'
      processingTierKeysEdited.value = false
      resetScopeState()
      initializeScopeCacheState(STANDARD_PRICING_SCOPE, standardTiers.value)
      initializeScopeImagePricingState(STANDARD_PRICING_SCOPE, clonedValue)
      for (const [key, config] of Object.entries(processingTierConfigs.value)) {
        const scope = processingTierScope(key)
        initializeScopeCacheState(scope, config.tiers ?? [])
        initializeScopeImagePricingState(scope, config)
      }
      if (
        activeProcessingTierKey.value !== null
        && !processingTierOptions.value.some(option => option.scope === activePricingScope.value)
      ) {
        activePricingScope.value = STANDARD_PRICING_SCOPE
      }
    } else {
      basePricingConfig.value = {}
      standardTiers.value = [{
        up_to: null,
        input_price_per_1m: 0,
        output_price_per_1m: 0,
      }]
      processingTierConfigs.value = {}
      processingTierKeysEdited.value = false
      originalEmptyProcessingTiers.value = 'absent'
      resetScopeState()
      initializeScopeCacheState(STANDARD_PRICING_SCOPE, standardTiers.value)
      initializeScopeImagePricingState(STANDARD_PRICING_SCOPE, {})
      activePricingScope.value = STANDARD_PRICING_SCOPE
    }
  },
  { immediate: true }
)

function processingTierScope(key: string): string {
  return `${PROCESSING_PRICING_SCOPE_PREFIX}${key}`
}

function processingTierKeyFromScope(scope: string): string | null {
  return scope.startsWith(PROCESSING_PRICING_SCOPE_PREFIX)
    ? scope.slice(PROCESSING_PRICING_SCOPE_PREFIX.length)
    : null
}

function pricingScopePolicy(scope: string): PricingScopePolicy {
  return processingTierKeyFromScope(scope) === null
    ? { allowEmptyTiers: false, terminalUpperBound: 'require-unbounded' }
    : { allowEmptyTiers: true, terminalUpperBound: 'finite-or-unbounded' }
}

function isTierUpperBoundEditable(index: number): boolean {
  return index < localTiers.value.length - 1
    || pricingScopePolicy(activePricingScope.value).terminalUpperBound === 'finite-or-unbounded'
}

function hasOwn(object: object, key: PropertyKey): boolean {
  return Object.prototype.hasOwnProperty.call(object, key)
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function cloneJson<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

function resetScopeState() {
  for (const scope of Object.keys(cacheManualStateByScope)) delete cacheManualStateByScope[scope]
  for (const scope of Object.keys(cachePriceModesByScope)) delete cachePriceModesByScope[scope]
  for (const scope of Object.keys(cacheMultiplierDraftsByScope)) delete cacheMultiplierDraftsByScope[scope]
  for (const scope of Object.keys(imagePricingStateByScope)) delete imagePricingStateByScope[scope]
  resetCustomThresholdState()
}

function resetCustomThresholdState() {
  for (const index of Object.keys(customInputMode)) delete customInputMode[Number(index)]
  for (const index of Object.keys(customInputValue)) delete customInputValue[Number(index)]
}

function initializeScopeCacheState(scope: string, tiers: PricingTier[]) {
  cacheManualStateByScope[scope] = Object.fromEntries(tiers.map((tier, index) => [
    index,
    {
      creation: tier.cache_creation_price_per_1m != null,
      read: tier.cache_read_price_per_1m != null,
      cache1h: tier.cache_ttl_pricing?.some(price => price.ttl_minutes === 60) ?? false,
    },
  ]))
  cachePriceModesByScope[scope] = Object.fromEntries(tiers.map((_, index) => [
    index,
    'multiplier' as CachePriceMode,
  ]))
  cacheMultiplierDraftsByScope[scope] = Object.fromEntries(tiers.map((tier, index) => [
    index,
    createCacheMultiplierDraft(tier),
  ]))
}

function createCacheMultiplierDraft(tier: PricingTier): CacheMultiplierDraft {
  return {
    creation: String(cacheMultiplierFromPrice(
      tier.input_price_per_1m,
      tier.cache_creation_price_per_1m,
      1.25,
    )),
    read: String(cacheMultiplierFromPrice(
      tier.input_price_per_1m,
      tier.cache_read_price_per_1m,
      0.1,
    )),
  }
}

function getCachePriceMode(index: number): CachePriceMode {
  return cachePriceModes.value?.[index] ?? 'multiplier'
}

function getCacheMultiplierDraft(index: number): CacheMultiplierDraft {
  const drafts = cacheMultiplierDrafts.value
  if (!drafts) {
    throw new Error(`Missing cache multiplier state for pricing scope: ${activePricingScope.value}`)
  }
  if (!drafts[index]) drafts[index] = createCacheMultiplierDraft(localTiers.value[index])
  return drafts[index]
}

function toggleCachePriceMode(index: number) {
  const tier = localTiers.value[index]
  const modes = cachePriceModes.value
  const drafts = cacheMultiplierDrafts.value
  if (!tier || !modes || !drafts) return
  if (getCachePriceMode(index) === 'multiplier') {
    tier.cache_creation_price_per_1m = getResolvedCacheCreationPrice(index)
    tier.cache_read_price_per_1m = getResolvedCacheReadPrice(index)
    modes[index] = 'price'
  } else {
    drafts[index] = createCacheMultiplierDraft(tier)
    modes[index] = 'multiplier'
  }
  syncToParent()
}

function getResolvedCacheCreationPrice(index: number): number {
  const tier = localTiers.value[index]
  if (!tier) return 0
  return resolveCachePriceForScope(activePricingScope.value, index, tier, 'creation')
}

function getResolvedCacheReadPrice(index: number): number {
  const tier = localTiers.value[index]
  if (!tier) return 0
  return resolveCachePriceForScope(activePricingScope.value, index, tier, 'read')
}

function resolveCachePriceForScope(
  scope: string,
  index: number,
  tier: PricingTier,
  kind: 'creation' | 'read',
): number {
  const mode = cachePriceModesByScope[scope]?.[index] ?? 'multiplier'
  if (mode === 'price') {
    return kind === 'creation'
      ? tier.cache_creation_price_per_1m ?? 0
      : tier.cache_read_price_per_1m ?? 0
  }
  const draft = cacheMultiplierDraftsByScope[scope]?.[index]
    ?? createCacheMultiplierDraft(tier)
  return cachePriceFromInputMultiplier(
    tier.input_price_per_1m,
    parseFloatInput(draft[kind]),
  )
}

function getCacheCreationEditorValue(index: number): string | number {
  return getCachePriceMode(index) === 'multiplier'
    ? getCacheMultiplierDraft(index).creation
    : localTiers.value[index]?.cache_creation_price_per_1m ?? ''
}

function getCacheReadEditorValue(index: number): string | number {
  return getCachePriceMode(index) === 'multiplier'
    ? getCacheMultiplierDraft(index).read
    : localTiers.value[index]?.cache_read_price_per_1m ?? ''
}

function initializeScopeImagePricingState(scope: string, config: ImagePricingConfig) {
  imagePricingStateByScope[scope] = {
    rows: createImageOutputPriceRows(config.image_output_prices),
    rangeRows: createImageOutputPriceRangeRows(config.image_output_price_ranges),
    defaultPrice: config.image_output_price_default != null
      ? String(config.image_output_price_default)
      : '',
  }
}

function setActiveImagePricingState(patch: Partial<ImagePricingState>) {
  const state = imagePricingStateByScope[activePricingScope.value]
  if (!state) return
  Object.assign(state, patch)
}

function requireActiveCacheManualState(): Record<number, CacheManualState> {
  const state = cacheManuallySet.value
  if (!state) {
    throw new Error(`Missing cache price state for pricing scope: ${activePricingScope.value}`)
  }
  return state
}

function selectPricingScope(scope: string) {
  activePricingScope.value = scope
  resetCustomThresholdState()
}

function addActiveProcessingTier() {
  const key = activeProcessingTierKey.value
  if (key === null || hasOwn(processingTierConfigs.value, key)) return

  const tiers = cloneJson(standardTiers.value)
  processingTierConfigs.value = Object.fromEntries([
    ...Object.entries(processingTierConfigs.value),
    [key, { tiers }],
  ])
  processingTierKeysEdited.value = true
  initializeScopeCacheState(activePricingScope.value, tiers)
  initializeScopeImagePricingState(activePricingScope.value, {})
  syncToParent()
}

function removeActiveProcessingTier() {
  const key = activeProcessingTierKey.value
  if (key === null || !hasOwn(processingTierConfigs.value, key)) return

  processingTierConfigs.value = Object.fromEntries(
    Object.entries(processingTierConfigs.value).filter(([existingKey]) => existingKey !== key),
  )
  delete cacheManualStateByScope[activePricingScope.value]
  delete cachePriceModesByScope[activePricingScope.value]
  delete cacheMultiplierDraftsByScope[activePricingScope.value]
  delete imagePricingStateByScope[activePricingScope.value]
  processingTierKeysEdited.value = true
  if (!KNOWN_PROCESSING_TIERS.some(tier => tier.key === key)) {
    activePricingScope.value = STANDARD_PRICING_SCOPE
  }
  syncToParent()
}

function tiersForScope(scope: string): PricingTier[] {
  const key = processingTierKeyFromScope(scope)
  if (key === null) return standardTiers.value
  return processingTierConfigs.value[key]?.tiers ?? []
}

function automaticCachePrice(tier: PricingTier | undefined, multiplier: number): number {
  const inputPrice = tier?.input_price_per_1m
  if (typeof inputPrice !== 'number' || !Number.isFinite(inputPrice)) return 0
  return parseFloat((inputPrice * multiplier).toFixed(4))
}

function replaceCacheTtlPrice(
  prices: PricingTier['cache_ttl_pricing'],
  ttlMinutes: number,
  price: number | null,
): NonNullable<PricingTier['cache_ttl_pricing']> {
  const next = prices ? cloneJson(prices) : []
  const index = next.findIndex(item => item.ttl_minutes === ttlMinutes)
  if (price === null) {
    return index === -1 ? next : next.filter((_, itemIndex) => itemIndex !== index)
  }
  const value = {
    ...(index >= 0 ? next[index] : {}),
    ttl_minutes: ttlMinutes,
    cache_creation_price_per_1m: price,
  }
  if (index >= 0) next[index] = value
  else next.push(value)
  return next
}

const validationError = computed(() => {
  const scopes = [
    STANDARD_PRICING_SCOPE,
    ...Object.keys(processingTierConfigs.value).map(processingTierScope),
  ]
  for (const scope of new Set(scopes)) {
    const error = validatePricingScope(scope)
    if (error) return error
  }
  return null
})

function validatePricingScope(scope: string): string | null {
  const processingTierKey = processingTierKeyFromScope(scope)
  const tierError = validatePricingTiers(tiersForScope(scope), pricingScopePolicy(scope))
  const imageError = props.showImagePricing ? validateImagePricingScope(scope) : null
  const error = tierError ?? imageError
  if (!error) return null
  const label = processingTierKey === null
    ? 'Standard'
    : KNOWN_PROCESSING_TIERS.find(tier => tier.key === processingTierKey)?.label
      ?? processingTierKey
  return `${label}: ${error}`
}

function validateImagePricingScope(scope: string): string | null {
  const state = imagePricingStateByScope[scope]
  if (!state) return '缺少图像价格状态'

  if (state.defaultPrice.trim() !== '') {
    const price = parseOptionalFloat(state.defaultPrice)
    if (price == null || price < 0) return '图像输出默认价格必须是非负有限数值'
  }

  const sizes = new Set<string>()
  for (const [index, row] of state.rows.entries()) {
    const knownPrices = Object.values(row.prices)
    if (knownPrices.some(price => !Number.isFinite(price) || price < 0)) {
      return `图像分辨率 ${index + 1} 的价格必须是非负有限数值`
    }
    const size = normalizeImageOutputSize(row.size)
    if (knownPrices.length > 0 && !size) return `图像分辨率 ${index + 1} 不能为空`
    if (size && sizes.has(size)) return `图像分辨率 ${size} 不能重复`
    if (size) sizes.add(size)
  }

  const limits = new Set<number | null>()
  for (const [index, row] of state.rangeRows.entries()) {
    const knownPrices = Object.values(row.prices)
    if (knownPrices.some(price => !Number.isFinite(price) || price < 0)) {
      return `图像像素区间 ${index + 1} 的价格必须是非负有限数值`
    }
    if (knownPrices.length === 0 && Object.keys(row.rawRange).length === 0) continue
    const limit = parseOptionalInteger(row.upToPixels)
    if (row.upToPixels.trim() !== '' && limit == null) {
      return `图像像素区间 ${index + 1} 的上限必须是正整数`
    }
    if (limits.has(limit)) return '图像像素区间上限不能重复'
    limits.add(limit)
  }
  return null
}

function validatePricingTiers(tiers: PricingTier[], policy: PricingScopePolicy): string | null {
  if (tiers.length === 0) {
    return policy.allowEmptyTiers ? null : '至少需要一个价格阶梯'
  }
  if (
    policy.terminalUpperBound === 'require-unbounded'
    && tiers[tiers.length - 1].up_to !== null
  ) {
    return '最后一个阶梯必须是无上限的'
  }

  let previousUpperBound = 0
  for (let index = 0; index < tiers.length; index += 1) {
    const upperBound = tiers[index].up_to
    if (upperBound === null) {
      if (index < tiers.length - 1) return `阶梯 ${index + 1} 的上限必须是有限值`
      continue
    }
    if (
      typeof upperBound !== 'number'
      || !Number.isFinite(upperBound)
      || !Number.isInteger(upperBound)
      || upperBound <= previousUpperBound
    ) {
      return `阶梯 ${index + 1} 的上限必须大于前一个阶梯`
    }
    previousUpperBound = upperBound
  }

  const priceFields: Array<[keyof PricingTier, string]> = [
    ['input_price_per_1m', '输入价格'],
    ['output_price_per_1m', '输出价格'],
    ['cache_creation_price_per_1m', '缓存创建价格'],
    ['cache_read_price_per_1m', '缓存读取价格'],
  ]
  for (const [index, tier] of tiers.entries()) {
    for (const [field, label] of priceFields) {
      const price = tier[field]
      if (price == null) continue
      if (typeof price !== 'number' || !Number.isFinite(price) || price < 0) {
        return `阶梯 ${index + 1} 的${label}必须是非负有限数值`
      }
    }
    for (const ttlPrice of tier.cache_ttl_pricing ?? []) {
      const price = ttlPrice.cache_creation_price_per_1m
      if (!Number.isFinite(price) || price < 0) {
        return `阶梯 ${index + 1} 的缓存 TTL 价格必须是非负有限数值`
      }
    }
  }
  return null
}

// 获取阶梯起始标签
function getTierStartLabel(index: number): string {
  if (index === 0) return '0'
  const prevTier = localTiers.value[index - 1]
  if (prevTier.up_to === null) return '0'
  return formatTokens(prevTier.up_to)
}

// 获取可用的阈值选项
function getAvailableThresholds(index: number) {
  const usedThresholds = new Set<number>()
  localTiers.value.forEach((t, i) => {
    if (i !== index && t.up_to !== null) {
      usedThresholds.add(t.up_to)
    }
  })

  const minValue = index > 0 ? (localTiers.value[index - 1].up_to || 0) : 0
  const currentValue = localTiers.value[index].up_to

  // 过滤可用的预设选项
  const options = THRESHOLD_OPTIONS.filter(opt =>
    (opt.value === -1) ||  // "自定义..."始终显示
    (!usedThresholds.has(opt.value) && opt.value > minValue)
  )

  if (
    index === localTiers.value.length - 1
    && pricingScopePolicy(activePricingScope.value).terminalUpperBound === 'finite-or-unbounded'
  ) {
    options.unshift({ value: UNBOUNDED_THRESHOLD_VALUE, label: '无上限' })
  }

  // 如果当前值是自定义的（不在预设中），添加到选项列表
  if (currentValue !== null && !THRESHOLD_OPTIONS.some(opt => opt.value === currentValue)) {
    options.unshift({ value: currentValue, label: formatTokens(currentValue) })
  }

  return options
}

function getAutoCache1h(index: number): number {
  return automaticCachePrice(localTiers.value[index], 2)
}

function getCache1hPlaceholder(index: number): string {
  const auto = getAutoCache1h(index)
  return auto > 0 ? String(auto) : '自动'
}

function getCache1hDisplay(index: number): string | number {
  const tier = localTiers.value[index]
  // 只有手动设置过才显示值
  if (cacheManuallySet.value[index]?.cache1h) {
    const ttl1h = tier?.cache_ttl_pricing?.find(t => t.ttl_minutes === 60)
    if (ttl1h) {
      // 修复浮点数精度问题
      return parseFloat(ttl1h.cache_creation_price_per_1m.toFixed(4))
    }
  }
  return ''
}

// 同步到父组件（只同步用户实际输入的值，不自动填充）
function syncToParent() {
  if (validationError.value) return

  const value = buildPricingConfig(false)
  lastEmittedPricingJson.value = JSON.stringify(value ?? null)
  emit('update:modelValue', value)
}

// 获取最终提交的数据（包含自动计算的缓存价格）
function getFinalTiers(): PricingTier[] {
  assertValidPricing()
  return buildTiersForScope(STANDARD_PRICING_SCOPE, true)
}

function getFinalPricing(): TieredPricingConfig {
  assertValidPricing()
  return buildPricingConfig(true)
}

function getValidationError(): string | null {
  return validationError.value
}

function assertValidPricing() {
  const error = getValidationError()
  if (error) throw new Error(error)
}

// 暴露给父组件调用
defineExpose({
  getFinalTiers,
  getFinalPricing,
  getValidationError,
})

function buildPricingConfig(includeAutomaticCache: boolean): TieredPricingConfig {
  const config = cloneJson(basePricingConfig.value) as TieredPricingConfig
  config.tiers = buildTiersForScope(STANDARD_PRICING_SCOPE, includeAutomaticCache)

  const processingTierEntries: Array<[string, ProcessingTierPricingConfig]> = []
  for (const [key, overlay] of Object.entries(processingTierConfigs.value)) {
    const serializedOverlay = cloneJson(overlay)
    if (Array.isArray(overlay.tiers)) {
      serializedOverlay.tiers = buildTiersForScope(processingTierScope(key), includeAutomaticCache)
    }
    if (props.showImagePricing) {
      applyImagePricing(serializedOverlay, processingTierScope(key))
    }
    processingTierEntries.push([key, serializedOverlay])
  }
  const processingTiers = Object.fromEntries(processingTierEntries)
  delete config.processing_tiers
  if (Object.keys(processingTiers).length > 0) {
    config.processing_tiers = processingTiers
  } else if (!processingTierKeysEdited.value && originalEmptyProcessingTiers.value === 'null') {
    config.processing_tiers = null
  } else if (!processingTierKeysEdited.value && originalEmptyProcessingTiers.value === 'object') {
    config.processing_tiers = {}
  }

  if (props.showImagePricing) {
    applyImagePricing(config, STANDARD_PRICING_SCOPE)
  }
  return config
}

function applyImagePricing(config: ImagePricingConfig, scope: string) {
  const state = imagePricingStateByScope[scope]
  if (!state) throw new Error(`Missing image price state for pricing scope: ${scope}`)

  const matrix = normalizedImageOutputPrices(state)
  if (Object.keys(matrix).length > 0) config.image_output_prices = matrix
  else delete config.image_output_prices

  const ranges = normalizedImageOutputPriceRanges(state)
  if (ranges.length > 0) config.image_output_price_ranges = ranges
  else delete config.image_output_price_ranges

  const defaultPrice = parseOptionalFloat(state.defaultPrice)
  if (defaultPrice != null) config.image_output_price_default = defaultPrice
  else delete config.image_output_price_default
}

function buildTiersForScope(scope: string, includeAutomaticCache: boolean): PricingTier[] {
  const tiers = tiersForScope(scope)
  const manualState = cacheManualStateByScope[scope] ?? {}
  return tiers.map((sourceTier, index) => {
    const tier = cloneJson(sourceTier)
    const state = manualState[index]

    tier.cache_creation_price_per_1m = resolveCachePriceForScope(
      scope,
      index,
      sourceTier,
      'creation',
    )
    tier.cache_read_price_per_1m = resolveCachePriceForScope(
      scope,
      index,
      sourceTier,
      'read',
    )

    if (props.showCache1h) {
      if (state?.cache1h && sourceTier.cache_ttl_pricing?.length) {
        tier.cache_ttl_pricing = cloneJson(sourceTier.cache_ttl_pricing)
      } else if (includeAutomaticCache) {
        tier.cache_ttl_pricing = replaceCacheTtlPrice(
          sourceTier.cache_ttl_pricing,
          60,
          automaticCachePrice(sourceTier, 2),
        )
      } else {
        const remaining = replaceCacheTtlPrice(sourceTier.cache_ttl_pricing, 60, null)
        if (remaining.length > 0) tier.cache_ttl_pricing = remaining
        else delete tier.cache_ttl_pricing
      }
    }

    return tier
  })
}

function createImageOutputPriceRows(value: TieredPricingConfig['image_output_prices']): ImageOutputPriceRow[] {
  const rows: ImageOutputPriceRow[] = []
  if (!value || typeof value !== 'object') {
    return DEFAULT_IMAGE_OUTPUT_SIZES.map(size => createImageOutputPriceRow(size))
  }
  for (const [size, prices] of Object.entries(value)) {
    if (!prices || typeof prices !== 'object') continue
    const rowPrices: Partial<Record<ImageOutputQuality, number>> = {}
    for (const quality of IMAGE_OUTPUT_QUALITIES) {
      const price = (prices as Record<string, unknown>)[quality]
      if (typeof price === 'number' && Number.isFinite(price)) {
        rowPrices[quality] = price
      }
    }
    rows.push(createImageOutputPriceRow(
      size,
      rowPrices,
      cloneJson(prices as ImageOutputQualityPricing),
    ))
  }
  if (rows.length > 0) return rows
  return DEFAULT_IMAGE_OUTPUT_SIZES.map(size => createImageOutputPriceRow(size))
}

function createImageOutputPriceRangeRows(value: TieredPricingConfig['image_output_price_ranges']): ImageOutputPriceRangeRow[] {
  const rows: ImageOutputPriceRangeRow[] = []
  if (!Array.isArray(value)) {
    return rows
  }
  for (const range of value) {
    if (!range || typeof range !== 'object') continue
    const rowPrices: Partial<Record<ImageOutputQuality, number>> = {}
    const rawPrices = 'prices' in range && range.prices && typeof range.prices === 'object'
      ? range.prices as Record<string, unknown>
      : range as Record<string, unknown>
    for (const quality of IMAGE_OUTPUT_QUALITIES) {
      const price = rawPrices[quality]
      if (typeof price === 'number' && Number.isFinite(price)) {
        rowPrices[quality] = price
      }
    }
    const upToPixels = 'up_to_pixels' in range && range.up_to_pixels != null
      ? String(range.up_to_pixels)
      : ''
    rows.push(createImageOutputPriceRangeRow(
      upToPixels,
      rowPrices,
      cloneJson(range as Record<string, unknown>),
    ))
  }
  return rows
}

function createImageOutputPriceRow(
  size = '',
  prices: Partial<Record<ImageOutputQuality, number>> = {},
  rawPrices: ImageOutputQualityPricing = {},
): ImageOutputPriceRow {
  imageOutputPriceRowId += 1
  return {
    id: `image-output-size-${imageOutputPriceRowId}`,
    size,
    prices: { ...prices },
    rawPrices,
  }
}

function createImageOutputPriceRangeRow(
  upToPixels = '',
  prices: Partial<Record<ImageOutputQuality, number>> = {},
  rawRange: Record<string, unknown> = {},
): ImageOutputPriceRangeRow {
  imageOutputPriceRangeRowId += 1
  return {
    id: `image-output-range-${imageOutputPriceRangeRowId}`,
    upToPixels,
    prices: { ...prices },
    rawRange,
  }
}

function normalizedImageOutputPrices(state: ImagePricingState): Record<string, ImageOutputQualityPricing> {
  const out: Record<string, ImageOutputQualityPricing> = {}
  for (const row of state.rows) {
    const size = normalizeImageOutputSize(row.size)
    if (!size) continue
    const prices = cloneJson(row.rawPrices)
    for (const quality of IMAGE_OUTPUT_QUALITIES) {
      const price = row.prices[quality]
      if (price != null && Number.isFinite(price)) {
        prices[quality] = price
      } else {
        delete prices[quality]
      }
    }
    if (Object.keys(prices).length > 0) {
      out[size] = { ...out[size], ...prices }
    }
  }
  return out
}

function normalizedImageOutputPriceRanges(state: ImagePricingState): ImageOutputPriceRange[] {
  const ranges: ImageOutputPriceRange[] = []
  for (const row of state.rangeRows) {
    const range = cloneJson(row.rawRange)
    const prices = isRecord(range.prices)
      ? cloneJson(range.prices) as ImageOutputQualityPricing
      : {}
    for (const quality of IMAGE_OUTPUT_QUALITIES) {
      const price = row.prices[quality]
      if (price != null && Number.isFinite(price)) {
        prices[quality] = price
      } else {
        delete prices[quality]
      }
    }
    if (Object.keys(prices).length === 0) continue
    range.up_to_pixels = parseOptionalInteger(row.upToPixels)
    range.prices = prices
    ranges.push(range as ImageOutputPriceRange)
  }
  return ranges.sort((a, b) => comparePricingUpperBounds(a.up_to_pixels, b.up_to_pixels))
}

function parseOptionalFloat(value: string | number): number | null {
  if (value === '' || value === null || value === undefined) return null
  const number = typeof value === 'string' ? parseFloat(value) : value
  return Number.isFinite(number) ? number : null
}

function parseOptionalInteger(value: string | number): number | null {
  if (value === '' || value === null || value === undefined) return null
  const number = typeof value === 'string' ? Number(value) : value
  return Number.isInteger(number) && number > 0 ? number : null
}

function normalizeImageOutputSize(size: string): string {
  return String(size || '').trim().replace(/\s*[xX×]\s*/g, 'x')
}

function getImageOutputPrice(row: ImageOutputPriceRow, quality: ImageOutputQuality): string | number {
  return row.prices[quality] ?? ''
}

function getImageOutputRangePrice(row: ImageOutputPriceRangeRow, quality: ImageOutputQuality): string | number {
  return row.prices[quality] ?? ''
}

function updateImageOutputSize(rowId: string, value: string | number) {
  const row = imageOutputPriceRows.value.find(item => item.id === rowId)
  if (!row) return
  row.size = normalizeImageOutputSize(String(value ?? ''))
  imageOutputPriceRows.value = [...imageOutputPriceRows.value]
  syncToParent()
}

function updateImageOutputPrice(rowId: string, quality: ImageOutputQuality, value: string | number) {
  const row = imageOutputPriceRows.value.find(item => item.id === rowId)
  if (!row) return
  const price = parseOptionalFloat(value)
  if (price == null) {
    delete row.prices[quality]
  } else {
    row.prices[quality] = price
  }
  imageOutputPriceRows.value = [...imageOutputPriceRows.value]
  syncToParent()
}

function addImageOutputSizeRow() {
  const usedSizes = new Set(imageOutputPriceRows.value.map(row => normalizeImageOutputSize(row.size)).filter(Boolean))
  const suggestedSize = DEFAULT_IMAGE_OUTPUT_SIZES.find(size => !usedSizes.has(size)) || ''
  imageOutputPriceRows.value = [...imageOutputPriceRows.value, createImageOutputPriceRow(suggestedSize)]
  syncToParent()
}

function removeImageOutputSizeRow(rowId: string) {
  imageOutputPriceRows.value = imageOutputPriceRows.value.filter(row => row.id !== rowId)
  syncToParent()
}

function updateImageOutputRangeLimit(rowId: string, value: string | number) {
  const row = imageOutputPriceRangeRows.value.find(item => item.id === rowId)
  if (!row) return
  row.upToPixels = String(value ?? '')
  imageOutputPriceRangeRows.value = [...imageOutputPriceRangeRows.value]
  syncToParent()
}

function updateImageOutputRangePrice(rowId: string, quality: ImageOutputQuality, value: string | number) {
  const row = imageOutputPriceRangeRows.value.find(item => item.id === rowId)
  if (!row) return
  const price = parseOptionalFloat(value)
  if (price == null) {
    delete row.prices[quality]
  } else {
    row.prices[quality] = price
  }
  imageOutputPriceRangeRows.value = [...imageOutputPriceRangeRows.value]
  syncToParent()
}

function addImageOutputRangeRow() {
  const usedLimits = new Set(imageOutputPriceRangeRows.value.map(row => parseOptionalInteger(row.upToPixels)).filter((value): value is number => value !== null))
  const suggestedLimit = DEFAULT_IMAGE_OUTPUT_PIXEL_LIMITS.find(limit => !usedLimits.has(limit))
  imageOutputPriceRangeRows.value = [...imageOutputPriceRangeRows.value, createImageOutputPriceRangeRow(suggestedLimit ? String(suggestedLimit) : '')]
  syncToParent()
}

function removeImageOutputRangeRow(rowId: string) {
  imageOutputPriceRangeRows.value = imageOutputPriceRangeRows.value.filter(row => row.id !== rowId)
  syncToParent()
}

function updateImageOutputPriceDefault(value: string | number) {
  imageOutputPriceDefault.value = String(value ?? '')
  syncToParent()
}

function parseFloatInput(value: string | number): number {
  const num = typeof value === 'string' ? parseFloat(value) : value
  return isNaN(num) ? 0 : num
}

// 更新输入价格（会触发缓存价格自动更新）
function updateInputPrice(index: number, value: number) {
  localTiers.value[index].input_price_per_1m = value
  syncToParent()
}

function updateOutputPrice(index: number, value: number) {
  localTiers.value[index].output_price_per_1m = value
  syncToParent()
}

// 获取下拉框当前选中值
function getSelectValue(index: number): number {
  const upTo = localTiers.value[index].up_to
  if (upTo === null) return UNBOUNDED_THRESHOLD_VALUE
  return upTo  // 直接返回当前值，让下拉框显示对应选项
}

// 处理下拉框选择变化
function handleThresholdChange(index: number, value: number) {
  if (value === UNBOUNDED_THRESHOLD_VALUE) {
    localTiers.value[index].up_to = null
    customInputMode[index] = false
    syncToParent()
  } else if (value === -1) {
    // 选择了"自定义..."，进入自定义输入模式
    customInputMode[index] = true
    customInputValue[index] = ''
  } else {
    localTiers.value[index].up_to = value
    syncToParent()
  }
}

// 确认自定义输入
function confirmCustomInput(index: number) {
  const inputK = parseInt(customInputValue[index])
  if (inputK > 0) {
    localTiers.value[index].up_to = inputK * 1000
    syncToParent()
  }
  customInputMode[index] = false
}

function updateCacheCreation(index: number, value: string | number) {
  if (getCachePriceMode(index) === 'multiplier') {
    getCacheMultiplierDraft(index).creation = String(value ?? '')
  } else {
    localTiers.value[index].cache_creation_price_per_1m = value === ''
      ? undefined
      : parseFloatInput(value)
  }
  syncToParent()
}

function updateCacheRead(index: number, value: string | number) {
  if (getCachePriceMode(index) === 'multiplier') {
    getCacheMultiplierDraft(index).read = String(value ?? '')
  } else {
    localTiers.value[index].cache_read_price_per_1m = value === ''
      ? undefined
      : parseFloatInput(value)
  }
  syncToParent()
}

function updateCache1h(index: number, value: string | number) {
  const tier = localTiers.value[index]
  const manualState = requireActiveCacheManualState()
  if (value === '' || value === null || value === undefined) {
    // 清空时恢复自动计算
    manualState[index] = { ...manualState[index], cache1h: false }
    const remaining = replaceCacheTtlPrice(tier.cache_ttl_pricing, 60, null)
    tier.cache_ttl_pricing = remaining.length > 0 ? remaining : undefined
  } else {
    const numValue = parseFloatInput(value)
    manualState[index] = { ...manualState[index], cache1h: true }
    tier.cache_ttl_pricing = replaceCacheTtlPrice(tier.cache_ttl_pricing, 60, numValue)
  }
  syncToParent()
}

// 阶梯操作
function addTier() {
  const manualState = requireActiveCacheManualState()
  const modes = cachePriceModes.value
  const drafts = cacheMultiplierDrafts.value
  if (!modes || !drafts) {
    throw new Error(`Missing cache editor state for pricing scope: ${activePricingScope.value}`)
  }
  if (localTiers.value.length === 0) {
    localTiers.value = [{
      up_to: null,
      input_price_per_1m: 0,
      output_price_per_1m: 0,
    }]
    manualState[0] = { creation: false, read: false, cache1h: false }
    modes[0] = 'multiplier'
    drafts[0] = createCacheMultiplierDraft(localTiers.value[0])
  } else {
    const lastTier = localTiers.value[localTiers.value.length - 1]
    if (lastTier.up_to === null) {
      const secondLastTier = localTiers.value[localTiers.value.length - 2]
      const minValue = secondLastTier?.up_to || 0
      const availableThresholds = THRESHOLD_OPTIONS.filter(opt => opt.value > minValue)
      lastTier.up_to = availableThresholds[0]?.value || minValue + 200000
    }

    // 添加新的无上限阶梯
    const newIndex = localTiers.value.length
    const newTier: PricingTier = {
      up_to: null,
      input_price_per_1m: 0,
      output_price_per_1m: 0,
    }

    localTiers.value.push(newTier)
    manualState[newIndex] = { creation: false, read: false, cache1h: false }
    modes[newIndex] = 'multiplier'
    drafts[newIndex] = createCacheMultiplierDraft(newTier)
  }

  syncToParent()
}

function removeTier(index: number) {
  if (localTiers.value.length <= 1) return
  const manualState = requireActiveCacheManualState()
  const previousManualState = { ...manualState }
  const previousModes = localTiers.value.map((_, tierIndex) => getCachePriceMode(tierIndex))
  const previousDrafts = localTiers.value.map((tier, tierIndex) => (
    cacheMultiplierDrafts.value?.[tierIndex] ?? createCacheMultiplierDraft(tier)
  ))
  previousModes.splice(index, 1)
  previousDrafts.splice(index, 1)
  localTiers.value.splice(index, 1)

  // 重新整理 cacheManuallySet 的索引
  const newManuallySet: Record<number, CacheManualState> = {}
  localTiers.value.forEach((_, i) => {
    const previousIndex = i < index ? i : i + 1
    newManuallySet[i] = previousManualState[previousIndex]
      ?? { creation: false, read: false, cache1h: false }
  })
  Object.keys(manualState).forEach(k => delete manualState[Number(k)])
  Object.assign(manualState, newManuallySet)

  const modes = cachePriceModes.value
  const drafts = cacheMultiplierDrafts.value
  if (!modes || !drafts) {
    throw new Error(`Missing cache editor state for pricing scope: ${activePricingScope.value}`)
  }
  for (const key of Object.keys(modes)) delete modes[Number(key)]
  for (const key of Object.keys(drafts)) delete drafts[Number(key)]
  localTiers.value.forEach((tier, tierIndex) => {
    modes[tierIndex] = previousModes[tierIndex] ?? 'multiplier'
    drafts[tierIndex] = previousDrafts[tierIndex] ?? createCacheMultiplierDraft(tier)
  })

  if (
    localTiers.value.length > 0
    && pricingScopePolicy(activePricingScope.value).terminalUpperBound === 'require-unbounded'
  ) {
    localTiers.value[localTiers.value.length - 1].up_to = null
  }

  resetCustomThresholdState()

  syncToParent()
}
</script>
