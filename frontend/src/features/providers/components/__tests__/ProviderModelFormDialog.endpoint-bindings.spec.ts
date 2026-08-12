import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue'

import type { ProviderEndpoint } from '@/api/endpoints'
import ProviderModelFormDialog from '../ProviderModelFormDialog.vue'

const modelMocks = vi.hoisted(() => ({
  createModel: vi.fn(),
  updateModel: vi.fn(),
  getProviderModels: vi.fn(),
}))
const globalModelMocks = vi.hoisted(() => ({
  createGlobalModel: vi.fn(),
  getGlobalModel: vi.fn(),
  listGlobalModels: vi.fn(),
}))
const toastMocks = vi.hoisted(() => ({ error: vi.fn(), success: vi.fn() }))

vi.mock('@/api/endpoints/models', () => modelMocks)
vi.mock('@/api/global-models', () => globalModelMocks)
vi.mock('@/composables/useToast', () => ({ useToast: () => toastMocks }))

const mountedApps: Array<{ app: App, root: HTMLElement }> = []
const globalModel = {
  id: 'global-model-1',
  name: 'gpt-test',
  display_name: 'GPT Test',
  is_active: true,
  default_tiered_pricing: {
    tiers: [{ up_to: null, input_price_per_1m: 1, output_price_per_1m: 2 }],
  },
  created_at: '2026-01-01T00:00:00Z',
}

function endpoint(id: string, apiFormat: string): ProviderEndpoint {
  return {
    id,
    provider_id: 'provider-1',
    provider_name: 'Provider 1',
    api_format: apiFormat,
    base_url: `https://${id}.example`,
    max_retries: 0,
    is_active: true,
    total_keys: 1,
    active_keys: 1,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
  }
}

function mountDialog(endpoints: ProviderEndpoint[]) {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const open = ref(false)
  const app = createApp(defineComponent({
    setup() {
      return () => h(ProviderModelFormDialog, {
        open: open.value,
        providerId: 'provider-1',
        endpoints,
      })
    },
  }))
  app.mount(root)
  mountedApps.push({ app, root })
  open.value = true
}

async function settle() {
  for (let index = 0; index < 6; index += 1) {
    await Promise.resolve()
    await nextTick()
  }
}

function fillProviderModelName() {
  const input = document.body.querySelector<HTMLInputElement>('#provider-model-name')
  if (!input) throw new Error('Missing provider model name input')
  setInputValue(input, 'gpt-test-upstream')
}

function setInputValue(input: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set
  setter?.call(input, value)
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

beforeEach(() => {
  modelMocks.createModel.mockReset()
  modelMocks.createModel.mockResolvedValue({})
  modelMocks.updateModel.mockReset()
  modelMocks.getProviderModels.mockReset()
  modelMocks.getProviderModels.mockResolvedValue([])
  globalModelMocks.createGlobalModel.mockReset()
  globalModelMocks.getGlobalModel.mockReset()
  globalModelMocks.listGlobalModels.mockReset()
  globalModelMocks.listGlobalModels.mockResolvedValue({ models: [globalModel], total: 1 })
  toastMocks.error.mockReset()
  toastMocks.success.mockReset()
})

afterEach(() => {
  for (const { app, root } of mountedApps.splice(0)) {
    app.unmount()
    root.remove()
  }
  document.body.innerHTML = ''
})

describe('ProviderModelFormDialog Endpoint 绑定', () => {
  it('单 Endpoint 自动选择并随创建 payload 提交', async () => {
    mountDialog([endpoint('endpoint-chat', 'openai:chat')])
    await settle()

    expect(document.body.querySelector<HTMLInputElement>('[aria-label="绑定 Endpoint endpoint-chat"]')?.checked).toBe(true)
    const manualButton = [...document.body.querySelectorAll<HTMLButtonElement>('button')]
      .find(button => button.textContent?.trim() === '手动添加')
    manualButton?.click()
    await nextTick()
    const globalModelName = document.body.querySelector<HTMLInputElement>('#manual-global-model-name')
    if (!globalModelName) throw new Error('Missing manual global model name input')
    setInputValue(globalModelName, 'gpt-test')
    await nextTick()
    fillProviderModelName()
    await nextTick()
    globalModelMocks.createGlobalModel.mockResolvedValueOnce(globalModel)
    const addButton = [...document.body.querySelectorAll<HTMLButtonElement>('button')]
      .find(button => button.textContent?.trim() === '添加')
    expect(addButton?.disabled).toBe(false)
    addButton?.click()
    await settle()

    expect(modelMocks.createModel).toHaveBeenCalledWith('provider-1', expect.objectContaining({
      global_model_id: 'global-model-1',
      endpoint_ids: ['endpoint-chat'],
    }))
  })

  it('多 Endpoint 未选择时禁用创建，选择后只提交勾选项', async () => {
    mountDialog([
      endpoint('endpoint-chat', 'openai:chat'),
      endpoint('endpoint-responses', 'openai:responses'),
    ])
    await settle()

    expect(document.body.textContent).toContain('此 Provider 有多个 Endpoint')
    const addButton = [...document.body.querySelectorAll<HTMLButtonElement>('button')]
      .find(button => button.textContent?.trim() === '添加')
    expect(addButton?.disabled).toBe(true)
    const selected = document.body.querySelector<HTMLInputElement>(
      '[aria-label="绑定 Endpoint endpoint-responses"]',
    )
    selected?.click()
    await nextTick()
    expect(selected?.checked).toBe(true)
  })
})
