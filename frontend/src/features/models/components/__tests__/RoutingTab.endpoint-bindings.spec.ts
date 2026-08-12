import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, nextTick } from 'vue'
import type { ModelRoutingPreviewResponse } from '@/api/global-models'

const modelApiMocks = vi.hoisted(() => ({
  updateModel: vi.fn(),
}))

const routingApiMocks = vi.hoisted(() => ({
  getGlobalModelRoutingPreview: vi.fn(),
}))

const toastMocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
}))

vi.mock('@/api/endpoints/models', () => modelApiMocks)
vi.mock('@/api/global-models', async importOriginal => ({
  ...await importOriginal<typeof import('@/api/global-models')>(),
  getGlobalModelRoutingPreview: routingApiMocks.getGlobalModelRoutingPreview,
}))
vi.mock('@/api/endpoints/health', () => ({
  recoverKeyHealth: vi.fn(),
}))
vi.mock('@/composables/useToast', () => ({
  useToast: () => toastMocks,
}))
vi.mock('@/composables/useCountdownTimer', () => ({
  useCountdownTimer: () => ({ tick: { value: 0 }, start: vi.fn() }),
  getProbeCountdown: vi.fn(() => null),
}))

import RoutingTab from '@/features/models/components/RoutingTab.vue'

interface MountedTab {
  app: ReturnType<typeof createApp>
  root: HTMLDivElement
  onRefresh: ReturnType<typeof vi.fn>
}

const mountedTabs: MountedTab[] = []

function routingPayload(
  bindingActive: boolean,
  runtimeCapabilityQuarantines: Array<{
    key_id: string
    client_api_format: string
    request_mode: string
    request_operation?: string | null
  }> = [],
  priorityMode: 'provider' | 'global_key' = 'provider',
): ModelRoutingPreviewResponse {
  return {
    global_model_id: 'global-claude',
    global_model_name: 'claude-opus',
    display_name: 'Claude Opus',
    is_active: true,
    global_model_mappings: [],
    providers: [{
      id: 'provider-anthropic',
      name: 'Anthropic',
      model_id: 'model-claude',
      provider_priority: 1,
      enable_format_conversion: true,
      is_active: true,
      provider_model_name: 'claude-opus-upstream',
      model_mappings: [],
      model_is_active: true,
      model_is_available: true,
      endpoints: [{
        id: 'endpoint-messages',
        api_format: 'claude:messages',
        base_url: 'https://api.anthropic.example',
        is_active: true,
        model_binding: { source: 'manual', is_active: bindingActive },
        runtime_capability_quarantines: runtimeCapabilityQuarantines,
        keys: [{
          id: 'key-1',
          name: 'primary',
          masked_key: 'sk-***',
          internal_priority: 1,
          is_adaptive: false,
          cache_ttl_minutes: 5,
          health_score: 1,
          is_active: true,
          api_formats: ['claude:messages'],
          matched_models: ['claude-opus-upstream'],
          circuit_breaker_open: false,
          circuit_breaker_formats: [],
        }],
        total_keys: 1,
        active_keys: 1,
      }],
      total_endpoints: 1,
      active_endpoints: bindingActive ? 1 : 0,
    }],
    total_providers: 1,
    active_providers: 1,
    scheduling_mode: 'fixed_order',
    priority_mode: priorityMode,
  }
}

function sharedEndpointPayload(): ModelRoutingPreviewResponse {
  const payload = routingPayload(true)
  const sharedEndpoint = payload.providers[0].endpoints[0]
  payload.providers.push({
    ...payload.providers[0],
    model_id: 'model-claude-alt',
    provider_model_name: 'claude-opus-alt',
    endpoints: [{
      ...sharedEndpoint,
      model_binding: { source: 'manual', is_active: true },
    }],
  })
  payload.total_providers = 2
  payload.active_providers = 2
  return payload
}

function multipleEndpointsForSameModelPayload(): ModelRoutingPreviewResponse {
  const payload = routingPayload(true)
  const firstEndpoint = payload.providers[0].endpoints[0]
  payload.providers[0].endpoints.push({
    ...firstEndpoint,
    id: 'endpoint-messages-secondary',
    base_url: 'https://api-secondary.anthropic.example',
    keys: firstEndpoint.keys.map(key => ({ ...key, id: 'key-2', name: 'secondary' })),
  })
  payload.providers[0].total_endpoints = 2
  payload.providers[0].active_endpoints = 2
  return payload
}

function mountTab(payload: ModelRoutingPreviewResponse): MountedTab {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const onRefresh = vi.fn()
  const app = createApp(RoutingTab, {
    globalModelId: payload.global_model_id,
    routingData: payload,
    onRefresh,
  })
  app.mount(root)
  const mounted = { app, root, onRefresh }
  mountedTabs.push(mounted)
  return mounted
}

async function expandFormat(root: HTMLElement, apiFormat = 'claude:messages') {
  const formatGroup = root.querySelector(
    `[data-testid="routing-format-group"][data-api-format="${apiFormat}"]`,
  ) as HTMLElement | null
  expect(formatGroup).not.toBeNull()
  const header = formatGroup?.querySelector('[data-testid="routing-format-header"]') as HTMLElement | null
  expect(header).not.toBeNull()
  header?.click()
  await nextTick()
  return formatGroup as HTMLElement
}

async function flushAsyncState() {
  await Promise.resolve()
  await Promise.resolve()
  await nextTick()
}

beforeEach(() => {
  modelApiMocks.updateModel.mockReset()
  modelApiMocks.updateModel.mockResolvedValue({})
  routingApiMocks.getGlobalModelRoutingPreview.mockReset()
  toastMocks.success.mockReset()
  toastMocks.error.mockReset()
})

afterEach(() => {
  for (const mounted of mountedTabs.splice(0)) {
    mounted.app.unmount()
    mounted.root.remove()
  }
})

describe('RoutingTab Endpoint 绑定', () => {
  it('禁用绑定后仅保留原生格式中的 Endpoint 管理入口', async () => {
    const { root } = mountTab(routingPayload(false))
    await flushAsyncState()

    expect(root.textContent).not.toContain('模型与 Endpoint 绑定')
    expect(root.querySelectorAll('[data-testid="routing-format-group"]')).toHaveLength(1)

    const formatGroup = await expandFormat(root)
    expect(formatGroup.querySelectorAll('[data-testid="endpoint-routing-card"]')).toHaveLength(1)
    expect(formatGroup.querySelectorAll('[data-testid="endpoint-binding-control"]')).toHaveLength(1)
    expect(formatGroup.textContent).toContain('Claude Messages')
    expect(formatGroup.textContent).toContain('Endpoint ID · endpoint-messages')
    expect(formatGroup.textContent).toContain('绑定来源 · 人工设置')
    expect(formatGroup.textContent).toContain('当前模型已禁用')
    expect(formatGroup.querySelector('[data-testid="endpoint-priority-badge"]')).toBeNull()
    expect(formatGroup.querySelector('[data-testid="endpoint-status-dot"]')?.className).toContain('bg-gray-400')
    expect(formatGroup.querySelector('[role="switch"]')?.getAttribute('aria-checked')).toBe('false')
  })

  it('开关仅 PATCH 当前模型与 Endpoint 绑定并在原位置即时更新', async () => {
    const { root, onRefresh } = mountTab(routingPayload(true))
    await flushAsyncState()

    const formatGroup = await expandFormat(root)
    const endpointSwitch = formatGroup.querySelector(
      '[role="switch"][aria-label="允许当前模型使用 Anthropic 的 Endpoint endpoint-messages"]',
    ) as HTMLButtonElement
    expect(endpointSwitch).not.toBeNull()
    endpointSwitch.click()
    await flushAsyncState()

    expect(modelApiMocks.updateModel).toHaveBeenCalledWith(
      'provider-anthropic',
      'model-claude',
      { endpoint_bindings: [{ endpoint_id: 'endpoint-messages', is_active: false }] },
    )
    expect(toastMocks.success).toHaveBeenCalledWith('已禁用模型 Endpoint 链路')
    expect(endpointSwitch.getAttribute('aria-checked')).toBe('false')
    expect(formatGroup.textContent).toContain('当前模型已禁用')
    expect(formatGroup.textContent).toContain('0/1 Keys')
    expect(formatGroup.textContent).toContain('0/1 提供商')
    expect(formatGroup.querySelector('[data-testid="endpoint-routing-card"]')).not.toBeNull()
    expect(onRefresh).not.toHaveBeenCalled()
    expect(routingApiMocks.getGlobalModelRoutingPreview).not.toHaveBeenCalled()
  })

  it('启用绑定后根据现有 Key 列表即时恢复活跃统计', async () => {
    const { root } = mountTab(routingPayload(false))
    await flushAsyncState()

    const formatGroup = await expandFormat(root)
    const endpointSwitch = formatGroup.querySelector(
      '[role="switch"][aria-label="允许当前模型使用 Anthropic 的 Endpoint endpoint-messages"]',
    ) as HTMLButtonElement
    endpointSwitch.click()
    await flushAsyncState()

    expect(modelApiMocks.updateModel).toHaveBeenCalledWith(
      'provider-anthropic',
      'model-claude',
      { endpoint_bindings: [{ endpoint_id: 'endpoint-messages', is_active: true }] },
    )
    expect(endpointSwitch.getAttribute('aria-checked')).toBe('true')
    expect(formatGroup.textContent).toContain('1/1 Keys')
    expect(formatGroup.textContent).toContain('1/1 提供商')
  })

  it('保存失败时仅回滚当前 Endpoint，不刷新整个链路', async () => {
    modelApiMocks.updateModel.mockRejectedValueOnce(new Error('save failed'))
    const { root, onRefresh } = mountTab(routingPayload(true))
    await flushAsyncState()

    const formatGroup = await expandFormat(root)
    const endpointSwitch = formatGroup.querySelector(
      '[role="switch"][aria-label="允许当前模型使用 Anthropic 的 Endpoint endpoint-messages"]',
    ) as HTMLButtonElement
    endpointSwitch.click()
    await flushAsyncState()

    expect(endpointSwitch.getAttribute('aria-checked')).toBe('true')
    expect(formatGroup.textContent).not.toContain('当前模型已禁用')
    expect(toastMocks.error).toHaveBeenCalled()
    expect(onRefresh).not.toHaveBeenCalled()
    expect(routingApiMocks.getGlobalModelRoutingPreview).not.toHaveBeenCalled()
  })

  it('同一 Endpoint 的两个模型使用独立的绑定覆盖键', async () => {
    const { root } = mountTab(sharedEndpointPayload())
    await flushAsyncState()

    const formatGroup = await expandFormat(root)
    const endpointSwitches = Array.from(formatGroup.querySelectorAll(
      '[role="switch"][aria-label="允许当前模型使用 Anthropic 的 Endpoint endpoint-messages"]',
    )) as HTMLButtonElement[]
    expect(endpointSwitches).toHaveLength(2)

    endpointSwitches[0].click()
    await flushAsyncState()

    expect(modelApiMocks.updateModel).toHaveBeenCalledWith(
      'provider-anthropic',
      'model-claude',
      { endpoint_bindings: [{ endpoint_id: 'endpoint-messages', is_active: false }] },
    )
    expect(endpointSwitches[0].getAttribute('aria-checked')).toBe('false')
    expect(endpointSwitches[1].getAttribute('aria-checked')).toBe('true')
  })

  it('同一 Provider 和 Endpoint 的两个模型使用独立的展开状态', async () => {
    const { root } = mountTab(sharedEndpointPayload())
    await flushAsyncState()

    const formatGroup = await expandFormat(root)
    const headers = Array.from(formatGroup.querySelectorAll(
      '[data-testid="endpoint-routing-card-header"]',
    )) as HTMLElement[]
    expect(headers).toHaveLength(2)

    headers[0].click()
    await nextTick()

    expect(formatGroup.querySelectorAll(
      '[data-testid="endpoint-routing-key-details"]',
    )).toHaveLength(1)
  })

  it('同一模型的多个 Endpoint 只统计为一个提供商模型', async () => {
    const { root } = mountTab(multipleEndpointsForSameModelPayload())
    await flushAsyncState()

    const formatGroup = await expandFormat(root)
    expect(formatGroup.querySelectorAll('[data-testid="endpoint-routing-card"]')).toHaveLength(2)
    expect(formatGroup.textContent).toContain('1/1 提供商')
    expect(formatGroup.textContent).not.toContain('2/2 提供商')
  })

  it('不可用模型不计入活跃 Provider 或 Key', async () => {
    const payload = routingPayload(true)
    payload.providers[0].model_is_available = false
    payload.providers[0].active_endpoints = 0
    payload.providers[0].endpoints[0].active_keys = 0
    payload.active_providers = 0
    const { root } = mountTab(payload)
    await flushAsyncState()

    const formatGroup = await expandFormat(root)
    expect(formatGroup.textContent).toContain('0/1 Keys')
    expect(formatGroup.textContent).toContain('0/1 提供商')
    expect(formatGroup.querySelector('[data-testid="endpoint-status-dot"]')?.className).toContain('bg-yellow-500')
  })

  it('运行时能力隔离独立显示且不改变持久化绑定开关', async () => {
    const { root } = mountTab(routingPayload(true, [{
      key_id: 'key-1',
      client_api_format: 'openai:responses',
      request_mode: 'stream',
      request_operation: 'compact',
    }]))
    await flushAsyncState()

    const formatGroup = await expandFormat(root)
    expect(formatGroup.textContent).toContain('运行时能力暂不可用 · Key key-1 / OpenAI Responses / 流式 / compact')
    const endpointSwitch = formatGroup.querySelector(
      '[role="switch"][aria-label="允许当前模型使用 Anthropic 的 Endpoint endpoint-messages"]',
    ) as HTMLButtonElement
    expect(endpointSwitch.getAttribute('aria-checked')).toBe('true')
  })

  it('全局 Key 优先模式也在 Endpoint 卡片内提供绑定开关', async () => {
    const { root } = mountTab(routingPayload(true, [], 'global_key'))
    await flushAsyncState()

    const formatGroup = await expandFormat(root)
    const endpointCard = formatGroup.querySelector('[data-testid="endpoint-routing-card"]')
    expect(endpointCard).not.toBeNull()
    expect(endpointCard?.querySelectorAll('[data-testid="endpoint-binding-control"]')).toHaveLength(1)
    expect(endpointCard?.textContent).toContain('Anthropic')
    expect(endpointCard?.textContent).toContain('Endpoint ID · endpoint-messages')
  })
})
