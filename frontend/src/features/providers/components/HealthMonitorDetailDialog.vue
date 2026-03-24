<template>
  <Dialog
    v-model="open"
    size="5xl"
    close-on-backdrop
    :title="monitor ? `${formatApiFormat(monitor.api_format)} 详情` : '监控详情'"
    :description="dialogDescription"
  >
    <template #header-actions>
      <Button
        variant="ghost"
        size="icon"
        class="h-8 w-8"
        aria-label="关闭详情"
        @click="open = false"
      >
        <X class="h-4 w-4" />
      </Button>
    </template>

    <div
      v-if="monitor"
      class="space-y-4"
    >
      <div class="grid grid-cols-2 gap-3 xl:grid-cols-5">
        <Card
          variant="inset"
          class="p-3"
        >
          <div class="text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
            当前状态
          </div>
          <div class="mt-2">
            <Badge :variant="getMonitorStatusVariant(monitor)">
              {{ getMonitorStatusLabel(monitor) }}
            </Badge>
          </div>
        </Card>

        <Card
          variant="inset"
          class="p-3"
        >
          <div class="text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
            成功率
          </div>
          <div class="mt-2 text-xl font-semibold">
            {{ formatPercent(monitor.success_rate) }}
          </div>
        </Card>

        <Card
          variant="inset"
          class="p-3"
        >
          <div class="text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
            请求总数
          </div>
          <div class="mt-2 text-xl font-semibold">
            {{ monitor.total_attempts }}
          </div>
        </Card>

        <Card
          variant="inset"
          class="p-3"
        >
          <div class="text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
            最近失败
          </div>
          <div class="mt-2 text-xl font-semibold">
            {{ failureEvents.length }}
          </div>
        </Card>

        <Card
          variant="inset"
          class="p-3"
        >
          <div class="text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
            最近事件
          </div>
          <div class="mt-2 text-sm font-medium">
            {{ monitor.last_event_at ? formatAbsoluteTime(monitor.last_event_at) : '暂无' }}
          </div>
        </Card>
      </div>

      <div
        v-if="monitorApiPath"
        class="rounded-xl border border-border/60 bg-muted/20 px-4 py-3 text-sm"
      >
        <div class="text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
          请求入口
        </div>
        <div class="mt-2 font-mono text-foreground">
          {{ monitorApiPath }}
        </div>
      </div>

      <div
        v-if="isAdmin"
        class="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,0.85fr)_minmax(0,1.15fr)]"
      >
        <Card
          variant="inset"
          class="p-4"
        >
          <div class="mb-3 flex items-center justify-between">
            <h4 class="text-sm font-semibold">
              受影响 Provider
            </h4>
            <span class="text-xs text-muted-foreground">
              {{ providerSummaries.length }} 个
            </span>
          </div>

          <div
            v-if="loadingFormatKeys"
            class="flex items-center gap-2 py-6 text-sm text-muted-foreground"
          >
            <Loader2 class="h-4 w-4 animate-spin" />
            正在加载 Key 诊断数据...
          </div>

          <div
            v-else-if="providerSummaries.length === 0"
            class="py-6 text-sm text-muted-foreground"
          >
            当前格式下没有可诊断的 provider/key 数据。
          </div>

          <div
            v-else
            class="max-h-[420px] space-y-2 overflow-y-auto pr-1"
          >
            <div
              v-for="provider in providerSummaries"
              :key="provider.provider_id"
              class="rounded-xl border p-3 transition-colors"
              :class="providerFilter === provider.provider_id
                ? 'border-primary/60 bg-primary/5'
                : 'border-border/50 bg-card/70'"
            >
              <div class="flex items-center justify-between gap-3">
                <div class="min-w-0">
                  <div class="flex items-center gap-2 flex-wrap">
                    <span class="text-sm font-medium">
                      {{ provider.provider_name }}
                    </span>
                    <Badge :variant="provider.circuitOpenKeys > 0 ? 'destructive' : provider.providerActive ? 'success' : 'outline'">
                      {{ provider.circuitOpenKeys > 0 ? `${provider.circuitOpenKeys} 熔断` : provider.providerActive ? '活跃' : '停用' }}
                    </Badge>
                  </div>
                  <div class="mt-1 text-xs text-muted-foreground">
                    {{ provider.totalKeys }} 个 Key
                    <span class="mx-1">·</span>
                    活跃 {{ provider.activeKeys }}
                    <span class="mx-1">·</span>
                    平均健康 {{ formatScore(provider.avgHealthScore) }}
                  </div>
                </div>
                <div class="text-right text-xs text-muted-foreground">
                  <div>{{ provider.requestCount }} 请求</div>
                </div>
              </div>

              <div class="mt-3 flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  :class="FILTER_BUTTON_CLASS"
                  @click="toggleProviderFilter(provider.provider_id)"
                >
                  {{ providerFilter === provider.provider_id ? '取消筛选' : '只看该 Provider' }}
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  :class="FILTER_BUTTON_CLASS"
                  @click="$emit('navigateProvider', provider.provider_id)"
                >
                  前往 Provider
                </Button>
                <Button
                  v-if="provider.poolEnabled"
                  variant="outline"
                  size="sm"
                  :class="FILTER_BUTTON_CLASS"
                  @click="$emit('navigatePool', provider.provider_id)"
                >
                  前往号池
                </Button>
              </div>
            </div>
          </div>
        </Card>

        <Card
          variant="inset"
          class="p-4"
        >
          <div class="mb-3 flex flex-col gap-3">
            <div class="flex items-center justify-between gap-3">
              <h4 class="text-sm font-semibold">
                Key 诊断
              </h4>
              <span class="text-xs text-muted-foreground">
                {{ filteredFormatKeys.length }} / {{ sortedFormatKeys.length }} 个 Key
              </span>
            </div>

            <div class="flex flex-wrap gap-2">
              <Button
                variant="outline"
                size="sm"
                :class="[FILTER_BUTTON_CLASS, keyScopeFilter === 'all' ? 'border-primary/60 bg-primary/5 text-primary' : '']"
                @click="keyScopeFilter = 'all'"
              >
                全部
              </Button>
              <Button
                variant="outline"
                size="sm"
                :class="[FILTER_BUTTON_CLASS, keyScopeFilter === 'circuit-open' ? 'border-primary/60 bg-primary/5 text-primary' : '']"
                @click="keyScopeFilter = 'circuit-open'"
              >
                仅看熔断
              </Button>
              <Button
                variant="outline"
                size="sm"
                :class="[FILTER_BUTTON_CLASS, keyScopeFilter === 'recoverable' ? 'border-primary/60 bg-primary/5 text-primary' : '']"
                @click="keyScopeFilter = 'recoverable'"
              >
                仅看可恢复项
              </Button>
              <Button
                v-if="providerFilter !== 'all' || keyScopeFilter !== 'all'"
                variant="outline"
                size="sm"
                :class="FILTER_BUTTON_CLASS"
                @click="resetFilters"
              >
                清空局部筛选
              </Button>
            </div>

            <div
              v-if="providerFilter !== 'all' || keyScopeFilter !== 'all'"
              class="text-xs text-muted-foreground"
            >
              <span v-if="providerFilter !== 'all'">
                Provider：{{ providerFilterName }}
              </span>
              <span v-if="providerFilter !== 'all' && keyScopeFilter !== 'all'"> · </span>
              <span v-if="keyScopeFilter === 'circuit-open'">范围：仅熔断项</span>
              <span v-else-if="keyScopeFilter === 'recoverable'">范围：仅可恢复项</span>
            </div>
          </div>

          <div
            v-if="loadingFormatKeys"
            class="flex items-center gap-2 py-6 text-sm text-muted-foreground"
          >
            <Loader2 class="h-4 w-4 animate-spin" />
            正在加载 Key 诊断数据...
          </div>

          <div
            v-else-if="sortedFormatKeys.length === 0"
            class="py-6 text-sm text-muted-foreground"
          >
            当前格式下没有 Key 样本。
          </div>

          <div
            v-else-if="filteredFormatKeys.length === 0"
            class="py-6 text-sm text-muted-foreground"
          >
            当前局部筛选下没有匹配的 Key。
          </div>

          <div
            v-else
            class="max-h-[420px] space-y-2 overflow-y-auto pr-1"
          >
            <div
              v-for="key in filteredFormatKeys"
              :key="key.id"
              class="rounded-xl border border-border/50 bg-card/70 p-3"
            >
              <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                <div class="min-w-0">
                  <div class="flex items-center gap-2 flex-wrap">
                    <span class="text-sm font-medium">
                      {{ key.name }}
                    </span>
                    <Badge variant="outline">
                      {{ key.provider_name }}
                    </Badge>
                    <Badge :variant="key.circuit_breaker_open ? 'destructive' : key.is_active && key.provider_active ? 'success' : 'outline'">
                      {{ key.circuit_breaker_open ? '已熔断' : key.is_active && key.provider_active ? '可用' : '停用' }}
                    </Badge>
                    <Badge
                      v-if="shouldShowRecoverAction(key)"
                      variant="warning"
                    >
                      建议恢复
                    </Badge>
                  </div>
                  <div class="mt-1 font-mono text-xs text-muted-foreground">
                    {{ key.api_key_masked }}
                  </div>
                  <div class="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
                    <span>健康 {{ formatScore(key.health_score) }}</span>
                    <span v-if="key.success_rate != null">{{ formatPercent(key.success_rate) }}</span>
                    <span>{{ key.request_count }} 请求</span>
                    <span v-if="key.avg_response_time_ms != null">均延迟 {{ formatLatency(key.avg_response_time_ms) }}</span>
                  </div>
                </div>

                <div class="flex flex-wrap gap-2 sm:justify-end">
                  <Button
                    v-if="key.pool_enabled"
                    variant="outline"
                    size="sm"
                    :class="ACTION_BUTTON_CLASS"
                    @click="$emit('navigatePool', key.provider_id, key.name)"
                  >
                    号池内查看
                  </Button>
                  <Button
                    v-if="shouldShowRecoverAction(key)"
                    variant="outline"
                    size="sm"
                    :class="ACTION_BUTTON_CLASS"
                    :disabled="recoveringKeyId !== null"
                    @click="$emit('recoverKey', key)"
                  >
                    <Loader2
                      v-if="recoveringKeyId === key.id"
                      class="mr-2 h-3.5 w-3.5 animate-spin"
                    />
                    恢复健康度
                  </Button>
                </div>
              </div>
            </div>
          </div>
        </Card>
      </div>

      <div class="grid grid-cols-1 gap-4 xl:grid-cols-2">
        <Card
          variant="inset"
          class="p-4"
        >
          <div class="mb-3 flex items-center justify-between">
            <h4 class="text-sm font-semibold">
              最近失败
            </h4>
            <span class="text-xs text-muted-foreground">
              最近 {{ Math.min(failureEvents.length, 8) }} 条
            </span>
          </div>

          <div
            v-if="failureEvents.length === 0"
            class="py-6 text-sm text-muted-foreground"
          >
            当前窗口内没有失败事件。
          </div>

          <div
            v-else
            class="space-y-2"
          >
            <div
              v-for="event in failureEvents.slice(0, 8)"
              :key="buildEventKey(event)"
              class="rounded-xl border border-border/50 bg-card/70 p-3"
            >
              <div class="flex items-center justify-between gap-3">
                <div class="flex items-center gap-2">
                  <Badge variant="destructive">
                    失败
                  </Badge>
                  <span
                    v-if="event.status_code"
                    class="text-xs font-medium text-foreground"
                  >
                    HTTP {{ event.status_code }}
                  </span>
                </div>
                <span class="text-xs text-muted-foreground">
                  {{ formatAbsoluteTime(event.timestamp) }}
                </span>
              </div>
              <div class="mt-2 text-sm text-foreground">
                {{ event.error_type || event.error_message || '未提供错误类型' }}
              </div>
              <div class="mt-1 text-xs text-muted-foreground">
                {{ event.latency_ms != null ? `延迟 ${formatLatency(event.latency_ms)}` : '延迟未知' }}
              </div>
            </div>
          </div>
        </Card>

        <Card
          variant="inset"
          class="p-4"
        >
          <div class="mb-3">
            <h4 class="text-sm font-semibold">
              错误分布
            </h4>
          </div>

          <div class="space-y-4">
            <div>
              <div class="mb-2 text-xs uppercase tracking-[0.18em] text-muted-foreground">
                Error Type
              </div>
              <div
                v-if="errorTypes.length === 0"
                class="text-sm text-muted-foreground"
              >
                当前没有可归类的错误类型。
              </div>
              <div
                v-else
                class="flex flex-wrap gap-2"
              >
                <Badge
                  v-for="item in errorTypes"
                  :key="item.label"
                  variant="outline"
                >
                  {{ item.label }} × {{ item.count }}
                </Badge>
              </div>
            </div>

            <div>
              <div class="mb-2 text-xs uppercase tracking-[0.18em] text-muted-foreground">
                Status Code
              </div>
              <div
                v-if="statusCodes.length === 0"
                class="text-sm text-muted-foreground"
              >
                当前没有状态码样本。
              </div>
              <div
                v-else
                class="flex flex-wrap gap-2"
              >
                <Badge
                  v-for="item in statusCodes"
                  :key="item.label"
                  variant="outline"
                >
                  {{ item.label }} × {{ item.count }}
                </Badge>
              </div>
            </div>

            <div>
              <div class="mb-2 text-xs uppercase tracking-[0.18em] text-muted-foreground">
                延迟样本
              </div>
              <div class="grid grid-cols-3 gap-2 text-sm">
                <div class="rounded-lg border border-border/50 bg-card/70 px-3 py-2">
                  <div class="text-[11px] text-muted-foreground">
                    P50
                  </div>
                  <div class="mt-1 font-semibold">
                    {{ latencyStats.p50 }}
                  </div>
                </div>
                <div class="rounded-lg border border-border/50 bg-card/70 px-3 py-2">
                  <div class="text-[11px] text-muted-foreground">
                    P95
                  </div>
                  <div class="mt-1 font-semibold">
                    {{ latencyStats.p95 }}
                  </div>
                </div>
                <div class="rounded-lg border border-border/50 bg-card/70 px-3 py-2">
                  <div class="text-[11px] text-muted-foreground">
                    Max
                  </div>
                  <div class="mt-1 font-semibold">
                    {{ latencyStats.max }}
                  </div>
                </div>
              </div>
            </div>
          </div>
        </Card>
      </div>

      <Card
        variant="inset"
        class="p-4"
      >
        <div class="mb-3 flex items-center justify-between">
          <h4 class="text-sm font-semibold">
            最近活动
          </h4>
          <span class="text-xs text-muted-foreground">
            最近 {{ Math.min(sortedEvents.length, 12) }} 条事件
          </span>
        </div>

        <div
          v-if="sortedEvents.length === 0"
          class="py-6 text-sm text-muted-foreground"
        >
          当前没有事件样本。
        </div>

        <div
          v-else
          class="space-y-2"
        >
          <div
            v-for="event in sortedEvents.slice(0, 12)"
            :key="buildEventKey(event)"
            class="flex flex-col gap-2 rounded-xl border border-border/50 bg-card/70 p-3 sm:flex-row sm:items-center sm:justify-between"
          >
            <div class="flex min-w-0 items-center gap-2 flex-wrap">
              <Badge :variant="getEventStatusVariant(event.status)">
                {{ getEventStatusLabel(event.status) }}
              </Badge>
              <span
                v-if="event.status_code"
                class="text-xs text-foreground"
              >
                HTTP {{ event.status_code }}
              </span>
              <span
                v-if="event.latency_ms != null"
                class="text-xs text-muted-foreground"
              >
                {{ formatLatency(event.latency_ms) }}
              </span>
              <span
                v-if="event.error_type"
                class="truncate text-xs text-muted-foreground"
              >
                {{ event.error_type }}
              </span>
            </div>
            <div class="text-xs text-muted-foreground">
              {{ formatAbsoluteTime(event.timestamp) }}
            </div>
          </div>
        </div>
      </Card>
    </div>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Loader2, X } from 'lucide-vue-next'
import Card from '@/components/ui/card.vue'
import Badge from '@/components/ui/badge.vue'
import Button from '@/components/ui/button.vue'
import Dialog from '@/components/ui/dialog/Dialog.vue'
import type { GroupedFormatKey } from '@/api/endpoints/keys'
import type { EndpointHealthEvent, EndpointStatusMonitor, PublicEndpointStatusMonitor, PublicHealthEvent } from '@/api/endpoints/types'
import { formatApiFormat } from '@/api/endpoints/types/api-format'

type MonitorItem = EndpointStatusMonitor | PublicEndpointStatusMonitor
type MonitorEvent = EndpointHealthEvent | PublicHealthEvent
type MonitorStatus = 'healthy' | 'warning' | 'unhealthy' | 'unknown'
type CountItem = { label: string; count: number }
type KeyScopeFilter = 'all' | 'circuit-open' | 'recoverable'
type ProviderImpactSummary = {
  provider_id: string
  provider_name: string
  providerActive: boolean
  poolEnabled: boolean
  totalKeys: number
  activeKeys: number
  circuitOpenKeys: number
  avgHealthScore: number | null
  requestCount: number
}

const props = defineProps<{
  monitor: MonitorItem | null
  isAdmin: boolean
  formatKeys: GroupedFormatKey[]
  loadingFormatKeys: boolean
  recoveringKeyId: string | null
}>()

defineEmits<{
  navigateProvider: [providerId: string]
  navigatePool: [providerId: string, search?: string]
  recoverKey: [key: GroupedFormatKey]
}>()

const open = defineModel<boolean>({ default: false })

const FILTER_BUTTON_CLASS = 'h-8 px-3 text-xs'
const ACTION_BUTTON_CLASS = 'h-8 px-3 text-xs'

const providerFilter = ref<string>('all')
const keyScopeFilter = ref<KeyScopeFilter>('all')

watch(open, (value) => {
  if (!value) {
    providerFilter.value = 'all'
    keyScopeFilter.value = 'all'
  }
})

const monitorApiPath = computed(() =>
  props.monitor && 'api_path' in props.monitor ? props.monitor.api_path : null
)

const sortedEvents = computed<MonitorEvent[]>(() => {
  const events = props.monitor?.events ?? []
  return [...events].sort((left, right) =>
    new Date(right.timestamp).getTime() - new Date(left.timestamp).getTime()
  )
})

const failureEvents = computed<MonitorEvent[]>(() =>
  sortedEvents.value.filter(event => event.status === 'failed')
)

const sortedFormatKeys = computed<GroupedFormatKey[]>(() =>
  [...props.formatKeys].sort((left, right) => {
    const circuitGap = Number(right.circuit_breaker_open) - Number(left.circuit_breaker_open)
    if (circuitGap !== 0) return circuitGap

    const leftHealth = left.health_score ?? 1
    const rightHealth = right.health_score ?? 1
    if (leftHealth !== rightHealth) return leftHealth - rightHealth

    if (left.request_count !== right.request_count) {
      return right.request_count - left.request_count
    }

    return left.name.localeCompare(right.name)
  })
)

const providerSummaries = computed<ProviderImpactSummary[]>(() => {
  const grouped = new Map<string, ProviderImpactSummary & { healthValues: number[] }>()

  for (const key of sortedFormatKeys.value) {
    const current = grouped.get(key.provider_id) ?? {
      provider_id: key.provider_id,
      provider_name: key.provider_name,
      providerActive: key.provider_active,
      poolEnabled: key.pool_enabled,
      totalKeys: 0,
      activeKeys: 0,
      circuitOpenKeys: 0,
      avgHealthScore: null,
      requestCount: 0,
      healthValues: []
    }

    current.totalKeys += 1
    current.requestCount += key.request_count
    current.providerActive = current.providerActive || key.provider_active
    current.poolEnabled = current.poolEnabled || key.pool_enabled

    if (key.is_active) current.activeKeys += 1
    if (key.circuit_breaker_open) current.circuitOpenKeys += 1
    if (key.health_score != null) current.healthValues.push(key.health_score)

    grouped.set(key.provider_id, current)
  }

  return Array.from(grouped.values())
    .map((provider) => ({
      provider_id: provider.provider_id,
      provider_name: provider.provider_name,
      providerActive: provider.providerActive,
      poolEnabled: provider.poolEnabled,
      totalKeys: provider.totalKeys,
      activeKeys: provider.activeKeys,
      circuitOpenKeys: provider.circuitOpenKeys,
      avgHealthScore: provider.healthValues.length > 0
        ? provider.healthValues.reduce((sum, value) => sum + value, 0) / provider.healthValues.length
        : null,
      requestCount: provider.requestCount,
    }))
    .sort((left, right) => {
      if (left.circuitOpenKeys !== right.circuitOpenKeys) {
        return right.circuitOpenKeys - left.circuitOpenKeys
      }

      const leftHealth = left.avgHealthScore ?? 1
      const rightHealth = right.avgHealthScore ?? 1
      if (leftHealth !== rightHealth) return leftHealth - rightHealth

      return right.requestCount - left.requestCount
    })
})

const providerFilterName = computed(() =>
  providerSummaries.value.find(p => p.provider_id === providerFilter.value)?.provider_name ?? '当前 Provider'
)

const filteredFormatKeys = computed<GroupedFormatKey[]>(() =>
  sortedFormatKeys.value.filter((key) => {
    if (providerFilter.value !== 'all' && key.provider_id !== providerFilter.value) {
      return false
    }

    if (keyScopeFilter.value === 'circuit-open') {
      return key.circuit_breaker_open
    }

    if (keyScopeFilter.value === 'recoverable') {
      return shouldShowRecoverAction(key)
    }

    return true
  })
)

const dialogDescription = computed(() => {
  if (!props.monitor) return '查看最近失败、错误分布与事件样本'
  const timeText = props.monitor.last_event_at
    ? `最近事件 ${formatRelativeTime(props.monitor.last_event_at)}`
    : '当前窗口没有事件'
  return `${getMonitorStatusLabel(props.monitor)} · ${props.monitor.total_attempts} 次请求 · ${timeText}`
})

const errorTypes = computed<CountItem[]>(() =>
  buildCountItems(
    failureEvents.value
      .map(event => event.error_type?.trim())
      .filter((value): value is string => Boolean(value))
  )
)

const statusCodes = computed<CountItem[]>(() =>
  buildCountItems(
    failureEvents.value
      .map(event => event.status_code != null ? String(event.status_code) : null)
      .filter((value): value is string => Boolean(value))
  )
)

const latencyStats = computed(() => {
  const values = sortedEvents.value
    .map(event => event.latency_ms)
    .filter((value): value is number => value != null)
    .sort((left, right) => left - right)

  if (values.length === 0) {
    return { p50: '--', p95: '--', max: '--' }
  }

  return {
    p50: formatLatency(pickPercentile(values, 0.5)),
    p95: formatLatency(pickPercentile(values, 0.95)),
    max: formatLatency(values[values.length - 1])
  }
})

watch(sortedFormatKeys, (keys) => {
  if (providerFilter.value === 'all') return
  if (!keys.some(key => key.provider_id === providerFilter.value)) {
    providerFilter.value = 'all'
  }
})

function resetFilters() {
  providerFilter.value = 'all'
  keyScopeFilter.value = 'all'
}

function toggleProviderFilter(providerId: string) {
  providerFilter.value = providerFilter.value === providerId ? 'all' : providerId
}

function shouldShowRecoverAction(key: GroupedFormatKey): boolean {
  if (key.health_score == null) return false
  return key.health_score < 0.999
}

function getLatestTimelineStatus(monitor: MonitorItem): MonitorStatus | null {
  const timeline = monitor.timeline ?? []
  for (let index = timeline.length - 1; index >= 0; index -= 1) {
    const status = timeline[index]
    if (status === 'healthy' || status === 'warning' || status === 'unhealthy') {
      return status
    }
  }
  return null
}

function getMonitorStatus(monitor: MonitorItem): MonitorStatus {
  const timelineStatus = getLatestTimelineStatus(monitor)
  if (timelineStatus === 'unhealthy') return 'unhealthy'
  if (timelineStatus === 'warning') return 'warning'

  if (monitor.total_attempts <= 0) {
    return 'unknown'
  }

  if (monitor.success_rate < 0.7) return 'unhealthy'
  if (monitor.success_rate < 0.95) return 'warning'
  if (timelineStatus === 'healthy' || monitor.success_rate >= 0.95) return 'healthy'
  return 'unknown'
}

function getMonitorStatusLabel(monitor: MonitorItem): string {
  switch (getMonitorStatus(monitor)) {
    case 'unhealthy':
      return '异常'
    case 'warning':
      return '预警'
    case 'healthy':
      return '健康'
    default:
      return '无流量'
  }
}

function getMonitorStatusVariant(monitor: MonitorItem): 'success' | 'warning' | 'destructive' | 'outline' {
  switch (getMonitorStatus(monitor)) {
    case 'healthy':
      return 'success'
    case 'warning':
      return 'warning'
    case 'unhealthy':
      return 'destructive'
    default:
      return 'outline'
  }
}

function getEventStatusVariant(status: string): 'success' | 'warning' | 'destructive' | 'outline' {
  switch (status) {
    case 'success':
      return 'success'
    case 'failed':
      return 'destructive'
    case 'skipped':
      return 'warning'
    default:
      return 'outline'
  }
}

function getEventStatusLabel(status: string): string {
  switch (status) {
    case 'success':
      return '成功'
    case 'failed':
      return '失败'
    case 'skipped':
      return '跳过'
    case 'started':
      return '执行中'
    default:
      return '未知'
  }
}

function buildCountItems(values: string[]): CountItem[] {
  const counter = new Map<string, number>()
  for (const value of values) {
    counter.set(value, (counter.get(value) ?? 0) + 1)
  }
  return Array.from(counter.entries())
    .map(([label, count]) => ({ label, count }))
    .sort((left, right) => right.count - left.count)
    .slice(0, 8)
}

function pickPercentile(sortedValues: number[], percentile: number): number {
  if (sortedValues.length === 0) return 0
  const index = Math.min(sortedValues.length - 1, Math.max(0, Math.ceil(sortedValues.length * percentile) - 1))
  return sortedValues[index]
}

function formatPercent(rate: number): string {
  return `${(rate * 100).toFixed(0)}%`
}

function formatScore(score: number | null | undefined): string {
  if (score == null) return '--'
  return `${(score * 100).toFixed(0)}%`
}

function formatLatency(latencyMs: number): string {
  if (latencyMs < 1000) return `${latencyMs}ms`
  return `${(latencyMs / 1000).toFixed(2)}s`
}

function formatRelativeTime(dateString: string): string {
  const diff = Date.now() - new Date(dateString).getTime()
  if (Number.isNaN(diff)) return '未知'
  if (diff < 60_000) return '刚刚'

  const minutes = Math.floor(diff / 60_000)
  if (minutes < 60) return `${minutes} 分钟前`

  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours} 小时前`

  const days = Math.floor(hours / 24)
  if (days < 7) return `${days} 天前`

  return formatAbsoluteTime(dateString)
}

function formatAbsoluteTime(dateString: string): string {
  return new Date(dateString).toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit'
  })
}

function buildEventKey(event: MonitorEvent): string {
  return `${event.timestamp}-${event.status}-${event.status_code ?? 'na'}-${event.error_type ?? 'none'}-${event.latency_ms ?? 'na'}`
}
</script>
