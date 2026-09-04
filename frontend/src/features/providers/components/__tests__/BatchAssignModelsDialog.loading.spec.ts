import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref, type App, type Ref } from 'vue'

import BatchAssignModelsDialog from '../BatchAssignModelsDialog.vue'

const globalModelMocks = vi.hoisted(() => ({
  getGlobalModels: vi.fn(),
}))

const endpointMocks = vi.hoisted(() => ({
  getProviderModels: vi.fn(),
  getProviderKeys: vi.fn(),
  batchAssignModelsToProvider: vi.fn(),
  createModel: vi.fn(),
  deleteModel: vi.fn(),
}))

const upstreamModelMocks = vi.hoisted(() => ({
  fetchModels: vi.fn(),
}))

vi.mock('@/api/endpoints/global-models', () => globalModelMocks)
vi.mock('@/api/endpoints', () => endpointMocks)
vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    error: vi.fn(),
    success: vi.fn(),
    warning: vi.fn(),
  }),
}))
vi.mock('@/composables/useConfirm', () => ({
  useConfirm: () => ({
    confirmWarning: vi.fn().mockResolvedValue(true),
  }),
}))
vi.mock('@/features/providers/composables/useUpstreamModelsCache', () => ({
  useUpstreamModelsCache: () => ({
    fetchModels: upstreamModelMocks.fetchModels,
  }),
}))
vi.mock('@/components/ui/dialog/Dialog.vue', async () => {
  const { defineComponent, h } = await import('vue')
  return {
    default: defineComponent({
      name: 'DialogStub',
      setup: (_props, { slots }) => () => h('section', [slots.default?.(), slots.footer?.()]),
    }),
  }
})
vi.mock('@/components/ui', async () => {
  const { defineComponent, h } = await import('vue')
  const passthrough = (name: string) => defineComponent({
    name,
    inheritAttrs: false,
    setup: (_props, { slots }) => () => slots.default?.(),
  })
  return {
    DropdownMenu: passthrough('DropdownMenuStub'),
    DropdownMenuTrigger: passthrough('DropdownMenuTriggerStub'),
    DropdownMenuContent: passthrough('DropdownMenuContentStub'),
    DropdownMenuItem: defineComponent({
      name: 'DropdownMenuItemStub',
      emits: ['select'],
      setup: (_props, { emit, slots }) => () => h(
        'button',
        { type: 'button', onClick: () => emit('select') },
        slots.default?.(),
      ),
    }),
  }
})

const mountedApps: Array<{ app: App, root: HTMLElement }> = []

async function settle() {
  for (let index = 0; index < 5; index += 1) {
    await Promise.resolve()
    await nextTick()
  }
}

/** 挂载可切换开关状态的关联弹窗，并等待首轮并行数据加载完成。 */
async function mountDialog(): Promise<{ root: HTMLElement; open: Ref<boolean> }> {
  const root = document.createElement('div')
  const open = ref(true)
  document.body.appendChild(root)
  const app = createApp(defineComponent({
    setup() {
      return () => h(BatchAssignModelsDialog, {
        open: open.value,
        providerId: 'provider-1',
        providerName: 'Provider One',
      })
    },
  }))
  app.mount(root)
  mountedApps.push({ app, root })
  await settle()
  return { root, open }
}

beforeEach(() => {
  globalModelMocks.getGlobalModels.mockReset()
  globalModelMocks.getGlobalModels.mockResolvedValue({ models: [], total: 0 })
  endpointMocks.getProviderModels.mockReset()
  endpointMocks.getProviderModels.mockResolvedValue([])
  endpointMocks.getProviderKeys.mockReset()
  endpointMocks.getProviderKeys.mockResolvedValue([])
  endpointMocks.batchAssignModelsToProvider.mockReset()
  endpointMocks.batchAssignModelsToProvider.mockResolvedValue({ success: [], errors: [] })
  endpointMocks.createModel.mockReset()
  endpointMocks.createModel.mockResolvedValue({})
  endpointMocks.deleteModel.mockReset()
  upstreamModelMocks.fetchModels.mockReset()
  upstreamModelMocks.fetchModels.mockResolvedValue({ models: [] })
})

afterEach(() => {
  for (const { app, root } of mountedApps.splice(0)) {
    app.unmount()
    root.remove()
  }
})

describe('BatchAssignModelsDialog loading', () => {
  it('loads model choices when lazily mounted in the open state', async () => {
    await mountDialog()

    expect(globalModelMocks.getGlobalModels).toHaveBeenCalledOnce()
    expect(globalModelMocks.getGlobalModels).toHaveBeenCalledWith({ limit: 1000 })
    expect(endpointMocks.getProviderModels).toHaveBeenCalledWith('provider-1')
    expect(endpointMocks.getProviderKeys).toHaveBeenCalledWith('provider-1')
  })

  it('creates a selected Global Model with the real upstream name and Endpoint', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [{ id: 'global-gemini', name: 'gemini-3.8', display_name: 'Gemini 3.8' }],
      total: 1,
    })
    endpointMocks.getProviderKeys.mockResolvedValue([{
      id: 'key-1',
      name: 'Gemini Key',
      api_key_masked: 'gm-***',
    }])
    upstreamModelMocks.fetchModels.mockResolvedValue({
      models: [{
        id: 'gemini-3.8-flash-high',
        api_formats: ['gemini'],
        endpoint_ids: ['endpoint-gemini', 'endpoint-gemini'],
      }],
    })

    const { root } = await mountDialog()
    const globalModelRow = root.querySelector<HTMLElement>('[data-global-model-id="global-gemini"]')
    expect(globalModelRow).not.toBeNull()
    globalModelRow?.click()

    const keyButton = Array.from(root.querySelectorAll('button'))
      .find(button => button.textContent?.includes('Gemini Key'))
    expect(keyButton).toBeDefined()
    keyButton?.click()
    await settle()

    expect(upstreamModelMocks.fetchModels).toHaveBeenCalledWith('provider-1', 'key-1', true)
    const upstreamSelect = root.querySelector<HTMLSelectElement>(
      'select[aria-label="为 Gemini 3.8 选择上游模型"]',
    )
    expect(upstreamSelect).not.toBeNull()
    if (!upstreamSelect) return
    upstreamSelect.value = 'gemini-3.8-flash-high'
    upstreamSelect.dispatchEvent(new Event('change', { bubbles: true }))
    await settle()

    const saveButton = Array.from(root.querySelectorAll('button'))
      .find(button => button.textContent?.trim() === '保存')
    expect(saveButton).toBeDefined()
    saveButton?.click()
    await settle()

    expect(endpointMocks.createModel).toHaveBeenCalledWith('provider-1', {
      global_model_id: 'global-gemini',
      provider_model_name: 'gemini-3.8-flash-high',
      endpoint_ids: ['endpoint-gemini'],
    })
    expect(endpointMocks.batchAssignModelsToProvider).not.toHaveBeenCalled()
  })

  it('keeps the batch inference fallback when no upstream model is selected', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [{ id: 'global-custom', name: 'custom-model', display_name: 'Custom Model' }],
      total: 1,
    })

    const { root } = await mountDialog()
    root.querySelector<HTMLElement>('[data-global-model-id="global-custom"]')?.click()
    await settle()

    const saveButton = Array.from(root.querySelectorAll('button'))
      .find(button => button.textContent?.trim() === '保存')
    saveButton?.click()
    await settle()

    expect(endpointMocks.batchAssignModelsToProvider).toHaveBeenCalledWith(
      'provider-1',
      ['global-custom'],
    )
    expect(endpointMocks.createModel).not.toHaveBeenCalled()
  })

  it('ignores an upstream response from a previous open session', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [{ id: 'global-custom', name: 'custom-model', display_name: 'Custom Model' }],
      total: 1,
    })
    endpointMocks.getProviderKeys.mockResolvedValue([{
      id: 'key-1',
      name: 'Gemini Key',
      api_key_masked: 'gm-***',
    }])
    let resolveOldRequest!: (value: {
      models: Array<{ id: string; api_formats: string[]; endpoint_ids: string[] }>
    }) => void
    upstreamModelMocks.fetchModels
      .mockImplementationOnce(() => new Promise(resolve => { resolveOldRequest = resolve }))
      .mockResolvedValue({
        models: [{ id: 'current-upstream', api_formats: ['gemini'], endpoint_ids: ['endpoint-2'] }],
      })

    const { root, open } = await mountDialog()
    const findKeyButton = () => Array.from(root.querySelectorAll('button'))
      .find(button => button.textContent?.includes('Gemini Key'))
    findKeyButton()?.click()
    await settle()

    open.value = false
    await settle()
    open.value = true
    await settle()
    findKeyButton()?.click()
    await settle()
    root.querySelector<HTMLElement>('[data-global-model-id="global-custom"]')?.click()
    await settle()

    resolveOldRequest({
      models: [{ id: 'stale-upstream', api_formats: ['gemini'], endpoint_ids: ['endpoint-1'] }],
    })
    await settle()

    const optionValues = Array.from(root.querySelectorAll<HTMLOptionElement>('select option'))
      .map(option => option.value)
    expect(optionValues).toContain('current-upstream')
    expect(optionValues).not.toContain('stale-upstream')
  })
})
