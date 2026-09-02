import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue'

import PoolSchedulingDialog from '../PoolSchedulingDialog.vue'

const endpointMocks = vi.hoisted(() => ({
  getPoolSchedulingPresets: vi.fn(),
  getProvider: vi.fn(),
  updateProvider: vi.fn(),
}))

vi.mock('@/api/endpoints', () => ({
  getProvider: endpointMocks.getProvider,
  updateProvider: endpointMocks.updateProvider,
}))

vi.mock('@/api/endpoints/pool', () => ({
  getPoolSchedulingPresets: endpointMocks.getPoolSchedulingPresets,
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
  }),
}))

const mountedApps: Array<{ app: App, root: HTMLElement }> = []
const cacheAffinityConfig = {
  scheduling_presets: [{
    preset: 'cache_affinity',
    enabled: true,
    mode: 'single_account',
  }],
}

/** 等待对话框异步加载与 Vue 渲染队列稳定，避免测试依赖单次 tick 时序。 */
async function settle(): Promise<void> {
  for (let index = 0; index < 4; index += 1) {
    await Promise.resolve()
    await nextTick()
  }
}

/** 挂载先关闭后打开的测试宿主，以覆盖真实对话框初始化与保存流程。 */
function mountDialog(): void {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const TestHost = defineComponent({
    /** 管理受控打开状态并把更新事件回写到宿主。 */
    setup() {
      const open = ref(false)
      void nextTick(() => { open.value = true })
      return () => h(PoolSchedulingDialog, {
        modelValue: open.value,
        providerId: 'provider-1',
        providerType: 'openai',
        currentConfig: cacheAffinityConfig,
        // 该回调模拟父组件对 v-model 更新的同步接收边界。
        'onUpdate:modelValue': (value: boolean) => { open.value = value },
      })
    },
  })
  const app = createApp(TestHost)
  app.mount(root)
  mountedApps.push({ app, root })
}

beforeEach(() => {
  endpointMocks.getPoolSchedulingPresets.mockReset()
  endpointMocks.getProvider.mockReset()
  endpointMocks.updateProvider.mockReset()
  endpointMocks.getPoolSchedulingPresets.mockResolvedValue([{
    name: 'cache_affinity',
    label: '缓存亲和',
    description: '同一用户持续复用 Key，首次分配可集中或轮转',
    providers: [],
    default_enabled: true,
    modes: [
      { value: 'single_account', label: '单号优先' },
      { value: 'lru', label: 'LRU 轮号' },
    ],
    default_mode: 'single_account',
    mutex_group: 'distribution_mode',
  }])
  endpointMocks.getProvider.mockResolvedValue({
    id: 'provider-1',
    pool_advanced: cacheAffinityConfig,
  })
  endpointMocks.updateProvider.mockResolvedValue({ id: 'provider-1' })
})

afterEach(() => {
  for (const { app, root } of mountedApps.splice(0)) {
    app.unmount()
    root.remove()
  }
  document.body.innerHTML = ''
})

describe('PoolSchedulingDialog cache affinity modes', () => {
  /** 验证用户选择 LRU 后，保存载荷仍保留缓存亲和并只更新其二级模式。 */
  it('shows and saves the LRU secondary mode', async () => {
    mountDialog()
    await settle()

    const modeControl = document.body.querySelector(
      '[data-testid="pool-cache-affinity-secondary-mode"]',
    )
    expect(modeControl?.textContent).toContain('单号优先')
    expect(modeControl?.textContent).toContain('LRU 轮号')

    modeControl?.querySelector<HTMLButtonElement>('[data-mode="lru"]')?.click()
    await nextTick()
    const saveButton = [...document.body.querySelectorAll<HTMLButtonElement>('button')]
      .find(button => button.textContent?.trim() === '保存')
    saveButton?.click()
    await settle()

    expect(endpointMocks.updateProvider).toHaveBeenCalledWith('provider-1', {
      pool_advanced: {
        scheduling_presets: [{
          preset: 'cache_affinity',
          enabled: true,
          mode: 'lru',
        }],
      },
    })
  })
})
