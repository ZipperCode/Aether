<template>
  <Dialog
    :model-value="open"
    :title="`配额详情 - ${keyName}`"
    :icon="BarChart3"
    size="2xl"
    :z-index="70"
    @update:model-value="$emit('update:open', $event)"
  >
    <template #header-actions>
      <Badge
        v-if="tierBadgeLabel"
        variant="outline"
        class="text-[10px] px-1.5 py-0 shrink-0"
        :class="tierBadgeClass"
        data-testid="antigravity-quota-dialog-tier-badge"
      >
        {{ tierBadgeLabel }}
      </Badge>
      <Button
        v-if="!hasProject && keyId"
        variant="outline"
        size="sm"
        class="h-7 text-xs px-2 text-primary shrink-0"
        :disabled="isInitializing"
        data-testid="antigravity-initialize-project-header-button"
        @click="showConfirm = true"
      >
        <Loader2
          v-if="isInitializing"
          class="w-3 h-3 mr-1 animate-spin"
        />
        <Sparkles
          v-else
          class="w-3 h-3 mr-1"
        />
        初始化伴生项目
      </Button>
      <DropdownMenu
        v-if="providerId && rawItems.length > 0"
        :modal="false"
      >
        <DropdownMenuTrigger as-child>
          <Button
            variant="ghost"
            size="icon"
            class="h-8 w-8"
            title="测试模型"
            :disabled="!!testingModel"
          >
            <Loader2
              v-if="testingModel"
              class="w-3.5 h-3.5 animate-spin"
            />
            <Play
              v-else
              class="w-3.5 h-3.5"
            />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem
            v-for="item in rawItems"
            :key="item.model"
            :title="item.model"
            @select="handleTestModel(item.model)"
          >
            <span class="truncate">{{ item.label }}</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </template>

    <div class="py-2">
      <div
        v-if="accountInfoStatus === 'error'"
        class="mb-3 flex items-center justify-between rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive"
        data-testid="antigravity-account-info-error"
      >
        <div class="flex items-center gap-1.5">
          <AlertCircle class="w-3.5 h-3.5 shrink-0" />
          <span>账号状态查询失败</span>
        </div>
        <span
          v-if="accountInfoCheckedAtText"
          class="text-[10px] text-muted-foreground"
        >
          检查于 {{ accountInfoCheckedAtText }}
        </span>
      </div>
      <div
        v-else-if="accountInfoStatus === 'pending'"
        class="mb-3 flex items-center gap-1.5 rounded-md border border-muted bg-muted/30 px-3 py-2 text-xs text-muted-foreground"
        data-testid="antigravity-account-info-pending"
      >
        <Loader2 class="w-3.5 h-3.5 shrink-0 animate-spin" />
        <span>正在获取账号信息...</span>
      </div>
      <div
        v-if="displayItems.length > 0"
        class="grid grid-cols-2 gap-3"
      >
        <div
          v-for="item in displayItems"
          :key="item.model"
        >
          <div class="flex items-center justify-between text-[10px] mb-0.5">
            <div class="min-w-0 flex-1 mr-2">
              <div
                class="text-muted-foreground truncate"
                :title="item.label"
              >
                {{ item.label }}
              </div>
            </div>
            <span :class="getQuotaRemainingClass(item.usedPercent)">
              {{ item.detail || `${item.remainingPercent.toFixed(1)}%` }}
            </span>
          </div>
          <div class="relative w-full h-1.5 bg-border rounded-full overflow-hidden">
            <div
              class="absolute left-0 top-0 h-full transition-all duration-300"
              :class="getQuotaRemainingBarColor(item.usedPercent)"
              :style="{ width: `${Math.max(item.remainingPercent, 0)}%` }"
            />
          </div>
          <div
            v-if="item.resetSeconds !== null"
            class="text-[9px] text-muted-foreground/70 mt-0.5"
          >
            <template v-if="item.resetSeconds > 0">
              {{ formatResetTime(item.resetSeconds) }}后重置
            </template>
            <template v-else>
              已重置
            </template>
          </div>
        </div>
      </div>
      <div
        v-else
        class="text-center py-8 space-y-3"
        data-testid="antigravity-quota-empty"
      >
        <p class="text-sm text-muted-foreground">
          {{ !hasProject ? '该账号尚未绑定或初始化伴生项目' : '暂无配额数据' }}
        </p>
      </div>
    </div>
    <template #footer>
      <Button
        variant="outline"
        @click="$emit('update:open', false)"
      >
        关闭
      </Button>
    </template>
    <AlertDialog
      :model-value="showConfirm"
      title="初始化伴生项目"
      description="将为该 Google 账号在上游 Google Cloud Code 执行伴生项目发现与初始化（onboardUser）。此操作将在上游 Google 账号创建/激活伴生项目。确认继续吗？"
      :loading="isInitializing"
      confirm-text="确认初始化"
      cancel-text="取消"
      @confirm="handleInitializeProject"
      @cancel="showConfirm = false"
    />
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { BarChart3, Play, Loader2, Sparkles, AlertCircle } from 'lucide-vue-next'
import { Dialog } from '@/components/ui'
import AlertDialog from '@/components/common/AlertDialog.vue'
import { initializeAntigravityKeyProject, type InitializeAntigravityProjectResponse } from '@/api/endpoints/provider_oauth'
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from '@/components/ui'
import Button from '@/components/ui/button.vue'
import Badge from '@/components/ui/badge.vue'
import { testModel } from '@/api/endpoints/providers'
import type { UpstreamMetadata, QuotaStatusSnapshot, QuotaWindowSnapshot } from '@/api/endpoints/types'
import { useToast } from '@/composables/useToast'
import { useI18n } from '@/i18n'
import { parseApiError } from '@/utils/errorParser'
import { getOAuthPlanTypeClass } from '@/utils/oauthPlanType'
import {
  compareAntigravityQuotaItems,
  dedupeAntigravityQuotaItemsByLabel,
  resolveAntigravityQuotaGroupLabel,
  resolveAntigravityQuotaLabel,
  resolveAntigravityAccountTier,
  resolveAntigravityAccountTierLabel,
} from '@/features/providers/utils/antigravityQuota'

const props = defineProps<{
  open: boolean
  metadata: UpstreamMetadata | null
  quotaSnapshot?: QuotaStatusSnapshot | null
  keyName: string
  providerId?: string
  keyId?: string
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  'update:metadata': [value: UpstreamMetadata]
  'initialized': [value: InitializeAntigravityProjectResponse]
}>()

const showConfirm = ref(false)
const isInitializing = ref(false)

const hasProject = computed(() => {
  const project = props.metadata?.antigravity?.project_id
  return typeof project === 'string' && project.trim().length > 0
})

const accountInfoStatus = computed(() => {
  return props.metadata?.antigravity?.account_info_status ?? 'unknown'
})
function formatTimestampText(unixSecs?: number | null): string | null {
  if (!unixSecs || !Number.isFinite(unixSecs)) return null
  const date = new Date(unixSecs * 1000)
  if (Number.isNaN(date.getTime())) return null
  const diffSec = Math.floor((Date.now() - date.getTime()) / 1000)
  if (diffSec < 60) return '刚刚'
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)} 分钟前`
  if (diffSec < 86400) return `${Math.floor(diffSec / 3600)} 小时前`
  return `${Math.floor(diffSec / 86400)} 天前`
}

const accountInfoCheckedAtText = computed(() => {
  return formatTimestampText(props.metadata?.antigravity?.account_info_checked_at)
})

async function handleInitializeProject() {
  const targetKeyId = props.keyId
  if (!targetKeyId) return
  isInitializing.value = true
  try {
    const res = await initializeAntigravityKeyProject(targetKeyId)
    if (props.keyId !== targetKeyId) return
    showSuccess(res.message || '伴生项目初始化成功')
    emit('update:metadata', res.metadata)
    emit('initialized', res)
    showConfirm.value = false
  } catch (error) {
    if (props.keyId !== targetKeyId) return
    showError(parseApiError(error, '伴生项目初始化失败'))
  } finally {
    if (props.keyId === targetKeyId) {
      isInitializing.value = false
    }
  }
}

/** Antigravity 原始模型额度与可选家族范围展示字段。 */
interface QuotaItem {
  model: string
  label: string
  usedPercent: number
  remainingPercent: number
  resetSeconds: number | null
  /** 汇总行的百分比单值或范围；原始测试模型条目可为空。 */
  detail?: string
}

const { error: showError, success: showSuccess } = useToast()
const { t, legacyT } = useI18n()
const accountTier = computed(() => resolveAntigravityAccountTier(props.metadata))
const tierBadgeLabel = computed(() => resolveAntigravityAccountTierLabel(props.metadata, legacyT))
const tierBadgeClass = computed(() => getOAuthPlanTypeClass(accountTier.value))
const testingModel = ref<string | null>(null)

function getQuotaSnapshotUpdatedAt(quota: QuotaStatusSnapshot | null | undefined): number | undefined {
  const updatedAt = quota?.updated_at ?? quota?.observed_at
  return typeof updatedAt === 'number' ? updatedAt : undefined
}

function getQuotaWindowLiveResetSeconds(
  quota: QuotaStatusSnapshot | null | undefined,
  window: QuotaWindowSnapshot | null | undefined,
): number | null {
  if (!window) return null

  const now = Math.floor(Date.now() / 1000)
  if (typeof window.reset_at === 'number') {
    return Math.max(window.reset_at - now, 0)
  }

  if (typeof window.reset_seconds === 'number') {
    const updatedAt = getQuotaSnapshotUpdatedAt(quota)
    const elapsed = typeof updatedAt === 'number' ? Math.max(now - updatedAt, 0) : 0
    return Math.max(window.reset_seconds - elapsed, 0)
  }

  return null
}

function coercePercent(value: unknown): number | null {
  const numericValue = Number(value)
  if (!Number.isFinite(numericValue)) return null
  return Math.min(Math.max(numericValue, 0), 100)
}

function coerceRemainingFraction(value: unknown): number | null {
  const numericValue = Number(value)
  if (!Number.isFinite(numericValue)) return null
  return Math.min(Math.max(numericValue, 0), 1)
}

/** 将绝对 Unix 重置时间换算为非负剩余秒数，非法输入返回空。 */
function secondsUntilUnixReset(resetAt: unknown): number | null {
  const numericResetAt = Number(resetAt)
  if (!Number.isFinite(numericResetAt) || numericResetAt <= 0) return null
  const now = Math.floor(Date.now() / 1000)
  return Math.max(Math.floor(numericResetAt - now), 0)
}

/** 从统一 quota windows 构建原始模型条目，并保留模型 ID 供测试选择。 */
function buildItemsFromQuotaSnapshot(quota: QuotaStatusSnapshot | null | undefined): QuotaItem[] {
  if (!quota) return []

  const providerType = String(quota.provider_type || '').trim().toLowerCase()
  if (providerType && providerType !== 'antigravity') return []

  const windows = Array.isArray(quota.windows)
    ? quota.windows.filter(window => String(window?.scope || '').trim().toLowerCase() === 'model')
    : []
  if (windows.length === 0) return []
  const opaqueDisplayIndex = { value: 1 }

  const items = windows
    .map((window) => {
      const model = String(window.model || window.label || window.code || '').trim()
      if (!model) return null

      const usedPercent =
        typeof window.used_ratio === 'number'
          ? Math.max(Math.min(window.used_ratio * 100, 100), 0)
          : typeof window.remaining_ratio === 'number'
            ? Math.max(Math.min((1 - window.remaining_ratio) * 100, 100), 0)
            : null
      if (usedPercent == null) return null

      const remainingPercent =
        typeof window.remaining_ratio === 'number'
          ? Math.max(Math.min(window.remaining_ratio * 100, 100), 0)
          : Math.max(100 - usedPercent, 0)

      return {
        model,
        label: resolveAntigravityQuotaLabel(model, window.label || window.model, opaqueDisplayIndex),
        usedPercent,
        remainingPercent,
        resetSeconds: getQuotaWindowLiveResetSeconds(quota, window),
      } satisfies QuotaItem
    })
    .filter((item): item is QuotaItem => item !== null)

  items.sort(compareAntigravityQuotaItems)
  return dedupeAntigravityQuotaItemsByLabel(items)
}

function buildGroupedItemsFromQuotaSnapshot(
  quota: QuotaStatusSnapshot | null | undefined,
): QuotaItem[] {
  if (!quota) return []

  const providerType = String(quota.provider_type || '').trim().toLowerCase()
  if (providerType && providerType !== 'antigravity') return []

  const windows = Array.isArray(quota.windows)
    ? quota.windows.filter(window => String(window?.scope || '').trim().toLowerCase() === 'quota_group')
    : []

  return windows
    .map((window) => {
      const code = String(window.code || '').trim()
      const label = resolveAntigravityQuotaGroupLabel(window, t)
      if (!code || !label) return null

      const usedPercent =
        typeof window.used_ratio === 'number'
          ? Math.max(Math.min(window.used_ratio * 100, 100), 0)
          : typeof window.remaining_ratio === 'number'
            ? Math.max(Math.min((1 - window.remaining_ratio) * 100, 100), 0)
            : null
      if (usedPercent == null) return null

      const remainingPercent =
        typeof window.remaining_ratio === 'number'
          ? Math.max(Math.min(window.remaining_ratio * 100, 100), 0)
          : Math.max(100 - usedPercent, 0)

      return {
        model: code,
        label,
        usedPercent,
        remainingPercent,
        resetSeconds: getQuotaWindowLiveResetSeconds(quota, window),
      } satisfies QuotaItem
    })
    .filter((item): item is QuotaItem => item !== null)
}

/** 保留所有真实模型条目供“测试模型”下拉选择，不能被两行摘要替代。 */
const rawItems = computed<QuotaItem[]>(() => {
  const snapshotItems = buildItemsFromQuotaSnapshot(props.quotaSnapshot)
  if (snapshotItems.length > 0) return snapshotItems

  const antigravity = props.metadata?.antigravity
  if (!antigravity || typeof antigravity !== 'object') return []
  const quotaByModel = antigravity.quota_by_model
  if (!quotaByModel || typeof quotaByModel !== 'object') return []

  const result: QuotaItem[] = []
  const opaqueDisplayIndex = { value: 1 }
  for (const [model, rawInfo] of Object.entries(quotaByModel)) {
    if (!model) continue
    const info = (rawInfo || {}) as Record<string, unknown>

    let usedPercent = coercePercent(info['used_percent'])
    if (usedPercent === null) {
      const remainingFraction = coerceRemainingFraction(info['remaining_fraction'])
      if (remainingFraction !== null) {
        usedPercent = (1 - remainingFraction) * 100
      } else {
        continue
      }
    }

    usedPercent = coercePercent(usedPercent) ?? 0

    const remainingPercent = Math.max(100 - usedPercent, 0)

    let resetSeconds = secondsUntilUnixReset(info['reset_at'])
    const resetTime = info['reset_time']
    if (typeof resetTime === 'string' && resetTime.trim()) {
      const ts = Date.parse(resetTime.trim())
      if (!Number.isNaN(ts)) {
        const diff = Math.floor((ts - Date.now()) / 1000)
        resetSeconds = diff > 0 ? diff : 0
      }
    }

    result.push({
      model,
      label: resolveAntigravityQuotaLabel(model, info['display_name'], opaqueDisplayIndex),
      usedPercent,
      remainingPercent,
      resetSeconds,
    })
  }

  result.sort(compareAntigravityQuotaItems)
  return dedupeAntigravityQuotaItemsByLabel(result)
})

const groupedItems = computed<QuotaItem[]>(() => buildGroupedItemsFromQuotaSnapshot(props.quotaSnapshot))
const displayItems = computed<QuotaItem[]>(() => groupedItems.value)

async function handleTestModel(modelName: string) {
  if (!props.providerId || testingModel.value) return

  testingModel.value = modelName

  try {
    const result = await testModel({
      provider_id: props.providerId,
      model_name: modelName,
      api_key_id: props.keyId,
      api_format: 'gemini:generate_content',
    })

    if (result.success) {
      const content =
        result.data?.response?.choices?.[0]?.message?.content
        || result.data?.content_preview
      if (content) {
        showSuccess(`测试成功，响应: ${String(content).substring(0, 100)}${String(content).length > 100 ? '...' : ''}`)
      } else {
        showSuccess(`模型 "${modelName}" 测试成功`)
      }
    } else {
      showError(`模型测试失败: ${result.error || '未知错误'}`)
    }
  } catch (err: unknown) {
    showError(`模型测试失败: ${parseApiError(err, '测试请求失败')}`)
  } finally {
    testingModel.value = null
  }
}

function getQuotaRemainingClass(usedPercent: number): string {
  const remaining = 100 - usedPercent
  if (remaining <= 10) return 'text-red-600 dark:text-red-400'
  if (remaining <= 30) return 'text-yellow-600 dark:text-yellow-400'
  return 'text-green-600 dark:text-green-400'
}

function getQuotaRemainingBarColor(usedPercent: number): string {
  const remaining = 100 - usedPercent
  if (remaining <= 10) return 'bg-red-500 dark:bg-red-400'
  if (remaining <= 30) return 'bg-yellow-500 dark:bg-yellow-400'
  return 'bg-green-500 dark:bg-green-400'
}

function formatResetTime(seconds: number): string {
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)

  if (days > 0) return `${days}天 ${hours}小时`
  if (hours > 0) return `${hours}小时 ${minutes}分钟`
  return `${minutes}分钟`
}
</script>
