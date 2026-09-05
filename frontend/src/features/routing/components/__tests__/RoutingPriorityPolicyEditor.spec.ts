import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, reactive, type App } from 'vue'

import RoutingPriorityPolicyEditor from '../RoutingPriorityPolicyEditor.vue'
import {
  DEFAULT_ROUTING_POLICY_MODEL,
  createEmptyModelPolicy,
  createEmptyRoutingGroupConfig,
  type RoutingGroupConfig,
  type RoutingPriorityMode,
} from '../../utils/routingPolicy'

const apiMocks = vi.hoisted(() => ({
  getProvidersSummary: vi.fn(),
  getGlobalKeys: vi.fn(),
}))

vi.mock('@/api/endpoints', () => ({
  getProvidersSummary: apiMocks.getProvidersSummary,
}))
vi.mock('@/api/client', () => ({
  default: { get: apiMocks.getGlobalKeys },
}))

const providerFixtures = [
  {
    id: 'provider-a',
    name: 'Provider A',
    provider_priority: 0,
    is_active: true,
    active_keys: 2,
    global_model_ids: ['global-a'],
    api_formats: ['openai:chat'],
  },
  {
    id: 'provider-a-2',
    name: 'Provider A2',
    provider_priority: 1,
    is_active: true,
    active_keys: 1,
    global_model_ids: ['global-a'],
    api_formats: ['openai:chat'],
  },
  {
    id: 'provider-b',
    name: 'Provider B',
    provider_priority: 2,
    is_active: true,
    active_keys: 1,
    global_model_ids: ['global-b'],
    api_formats: ['anthropic:messages'],
  },
  {
    id: 'provider-inactive',
    name: 'Provider Inactive',
    provider_priority: 3,
    is_active: false,
    active_keys: 1,
    global_model_ids: ['global-a'],
    api_formats: ['openai:chat'],
  },
  {
    id: 'provider-no-key',
    name: 'Provider No Key',
    provider_priority: 4,
    is_active: true,
    active_keys: 0,
    global_model_ids: ['global-a'],
    api_formats: ['openai:chat'],
  },
]

const mounted: Array<{ app: App; root: HTMLElement }> = []

/** 构造单个按模型策略，便于验证隐藏 Provider 覆盖值的往返保留。 */
function createConfig(overrides: Record<string, number> = {}): RoutingGroupConfig {
  const config = createEmptyRoutingGroupConfig()
  config.model_policies = [{
    ...createEmptyModelPolicy('model-a'),
    provider_priority_overrides: overrides,
  }]
  return config
}

/** 挂载真实编辑器并模拟父组件 v-model 回写及受控模式切换，使连续操作读取最新属性。 */
async function mountEditor(
  config = createConfig(),
  model = 'model-a',
  globalModelId: string | undefined = 'global-a',
  priorityMode: RoutingPriorityMode = 'provider',
) {
  const emitted: RoutingGroupConfig[] = []
  const state = reactive({
    config,
    model,
    globalModelId,
    priorityMode,
    showPriorityMode: false,
    showSchedulingMode: false,
  })
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(defineComponent({
    /** 复现生产父组件接收配置事件后立即回传新属性的行为。 */
    setup() {
      /** 保存发出的配置，同时驱动组件进入最新属性状态。 */
      const updateConfig = (value: RoutingGroupConfig): void => {
        emitted.push(value)
        state.config = value
      }
      return () => h(RoutingPriorityPolicyEditor, {
        ...state,
        'onUpdate:config': updateConfig,
      })
    },
  }))
  app.mount(root)
  mounted.push({ app, root })
  await flushPromises()
  return { emitted, root, state }
}

/** 等待挂载时的 Provider 请求和随后一次 Vue 渲染完成。 */
async function flushPromises(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
  await nextTick()
}

beforeEach(() => {
  vi.clearAllMocks()
  apiMocks.getProvidersSummary.mockResolvedValue({ items: providerFixtures })
  apiMocks.getGlobalKeys.mockResolvedValue({ data: {} })
})

afterEach(() => {
  for (const item of mounted.splice(0)) {
    item.app.unmount()
    item.root.remove()
  }
})

describe('RoutingPriorityPolicyEditor Provider 可排序范围', () => {
  it('按全局模型 ID 动态筛选，并让统一模式继续展示全量 Provider', async () => {
    const { root, state } = await mountEditor()

    expect(apiMocks.getProvidersSummary).toHaveBeenLastCalledWith({
      page: 1,
      page_size: 9999,
      model_id: 'global-a',
    })
    expect(root.textContent).toContain('Provider A')
    expect(root.textContent).toContain('Provider A2')
    expect(root.textContent).not.toContain('Provider B')
    expect(root.textContent).not.toContain('Provider Inactive')
    expect(root.textContent).not.toContain('Provider No Key')

    state.model = 'model-b'
    state.globalModelId = 'global-b'
    await flushPromises()
    expect(apiMocks.getProvidersSummary).toHaveBeenLastCalledWith({
      page: 1,
      page_size: 9999,
      model_id: 'global-b',
    })
    expect(root.textContent).toContain('Provider B')
    expect(root.textContent).not.toContain('Provider A')

    state.model = 'unknown-model'
    state.globalModelId = 'missing-global-model'
    await flushPromises()
    expect(apiMocks.getProvidersSummary).toHaveBeenLastCalledWith({
      page: 1,
      page_size: 9999,
      model_id: 'missing-global-model',
    })
    expect(root.textContent).toContain('当前模型暂无可排序 Provider')

    const callsBeforeMissingId = apiMocks.getProvidersSummary.mock.calls.length
    state.globalModelId = undefined
    await flushPromises()
    expect(apiMocks.getProvidersSummary).toHaveBeenCalledTimes(callsBeforeMissingId)
    expect(root.textContent).toContain('未找到当前模型对应的全局模型')

    state.model = DEFAULT_ROUTING_POLICY_MODEL
    await flushPromises()
    expect(apiMocks.getProvidersSummary).toHaveBeenLastCalledWith({ page: 1, page_size: 9999 })
    expect(root.textContent).toContain('Provider A')
    expect(root.textContent).toContain('Provider B')
    expect(root.textContent).toContain('Provider Inactive')
    expect(root.textContent).toContain('Provider No Key')
  })

  it('优先级模式变化时重载 Provider，并让 Key 排序查询保持全量', async () => {
    const { state } = await mountEditor()

    state.priorityMode = 'global_key'
    await flushPromises()
    expect(apiMocks.getProvidersSummary).toHaveBeenLastCalledWith({ page: 1, page_size: 9999 })
    expect(apiMocks.getGlobalKeys).toHaveBeenCalledTimes(1)

    state.priorityMode = 'provider'
    await flushPromises()
    expect(apiMocks.getProvidersSummary).toHaveBeenLastCalledWith({
      page: 1,
      page_size: 9999,
      model_id: 'global-a',
    })
  })

  it('丢弃旧作用域请求的成功和失败结果', async () => {
    let resolveStaleSuccess!: (value: { items: typeof providerFixtures }) => void
    const staleSuccess = new Promise<{ items: typeof providerFixtures }>((resolve) => {
      resolveStaleSuccess = resolve
    })
    apiMocks.getProvidersSummary
      .mockImplementationOnce(() => staleSuccess)
      .mockResolvedValueOnce({ items: providerFixtures.filter(provider => provider.id === 'provider-b') })

    const { root, state } = await mountEditor()
    state.model = 'model-b'
    state.globalModelId = 'global-b'
    await flushPromises()
    expect(root.textContent).toContain('Provider B')

    resolveStaleSuccess({
      items: providerFixtures.filter(provider => provider.id === 'provider-a'),
    })
    await flushPromises()
    expect(root.textContent).toContain('Provider B')
    expect(root.textContent).not.toContain('Provider A')

    let rejectStaleFailure!: (reason?: unknown) => void
    const staleFailure = new Promise<never>((_, reject) => {
      rejectStaleFailure = reject
    })
    apiMocks.getProvidersSummary
      .mockImplementationOnce(() => staleFailure)
      .mockResolvedValueOnce({ items: providerFixtures.filter(provider => provider.id === 'provider-b') })

    state.model = 'model-a'
    state.globalModelId = 'global-a'
    await nextTick()
    state.model = 'model-b'
    state.globalModelId = 'global-b'
    await flushPromises()
    rejectStaleFailure(new Error('stale provider failure'))
    await flushPromises()

    expect(root.textContent).toContain('Provider B')
    expect(root.textContent).not.toContain('Provider A')
    expect(root.textContent).not.toContain('stale provider failure')
  })

  it('拖拽、移动和手动改优先级都保留隐藏 Provider 覆盖值', async () => {
    const hiddenOverrides = {
      'provider-a': 0,
      'provider-a-2': 1,
      'provider-b': 77,
      'provider-inactive': 88,
      'provider-no-key': 99,
    }
    const { emitted, root } = await mountEditor(createConfig(hiddenOverrides))

    let rows = root.querySelectorAll<HTMLElement>('[draggable="true"]')
    rows[0]?.dispatchEvent(new Event('dragstart', { bubbles: true }))
    rows[1]?.dispatchEvent(new Event('drop', { bubbles: true }))
    await nextTick()
    let overrides = emitted[emitted.length - 1]?.model_policies[0]?.provider_priority_overrides
    expect(overrides).toMatchObject({
      'provider-b': 77,
      'provider-inactive': 88,
      'provider-no-key': 99,
    })

    rows = root.querySelectorAll<HTMLElement>('[draggable="true"]')
    rows[0]?.querySelectorAll('button')[1]?.click()
    await nextTick()
    overrides = emitted[emitted.length - 1]?.model_policies[0]?.provider_priority_overrides
    expect(overrides).toMatchObject({
      'provider-b': 77,
      'provider-inactive': 88,
      'provider-no-key': 99,
    })

    const priorityInput = root.querySelector<HTMLInputElement>('.priority-input')
    if (!priorityInput) throw new Error('priority input not found')
    priorityInput.value = '42'
    priorityInput.dispatchEvent(new Event('change', { bubbles: true }))
    await nextTick()
    overrides = emitted[emitted.length - 1]?.model_policies[0]?.provider_priority_overrides
    expect(overrides).toMatchObject({
      'provider-b': 77,
      'provider-inactive': 88,
      'provider-no-key': 99,
    })
  })
})
