import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref, type App, type Ref } from 'vue'

import BatchAssignModelsDialog from '../BatchAssignModelsDialog.vue'
import type { ProviderEndpoint } from '@/api/endpoints'

const globalModelMocks = vi.hoisted(() => ({
  getGlobalModels: vi.fn(),
}))

const endpointMocks = vi.hoisted(() => ({
  getProviderModels: vi.fn(),
  getProviderKeys: vi.fn(),
  getProviderEndpoints: vi.fn(),
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
vi.mock('@/features/models/components/GlobalModelFormDialog.vue', async () => {
  const { defineComponent, h } = await import('vue')
  return {
    default: defineComponent({
      name: 'GlobalModelFormDialogStub',
      props: {
        open: { type: Boolean, default: false },
        model: { type: Object, default: null },
        zIndex: { type: Number, default: undefined },
      },
      emits: ['update:open', 'success'],
      setup(props, { emit }) {
        return () => {
          if (!props.open) return null
          return h('div', { 'data-testid': 'global-model-form-dialog-stub' }, [
            h('button', {
              type: 'button',
              'data-testid': 'global-model-form-dialog-stub-success',
              onClick: () => emit('success'),
            }, 'submit'),
          ])
        }
      },
    }),
  }
})
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

/** 挂载可切换开关状态与 Provider 的关联弹窗，并等待首轮并行数据加载完成。 */
async function mountDialog(providerId: Ref<string> = ref('provider-1')): Promise<{ root: HTMLElement; open: Ref<boolean> }> {
  const root = document.createElement('div')
  const open = ref(true)
  document.body.appendChild(root)
  const app = createApp(defineComponent({
    setup() {
      return () => h(BatchAssignModelsDialog, {
        open: open.value,
        providerId: providerId.value,
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
  endpointMocks.getProviderEndpoints.mockReset()
  endpointMocks.getProviderEndpoints.mockResolvedValue([])
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

function createGlobalModel(id: string, name: string, displayName = name) {
  return {
    id,
    name,
    display_name: displayName,
    is_active: true,
    default_tiered_pricing: { tiers: [] },
    created_at: '2026-01-01T00:00:00Z',
  }
}

function createProviderModel(id: string, globalModelId: string) {
  return {
    id,
    provider_id: 'provider-1',
    global_model_id: globalModelId,
    provider_model_name: globalModelId,
    is_active: true,
    is_available: true,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
  }
}

/** 构造 Provider Endpoint 返回数据。 */
function createProviderEndpoint(id: string, providerId = 'provider-1'): ProviderEndpoint {
  return {
    id,
    provider_id: providerId,
    provider_name: `Provider ${providerId}`,
    api_format: 'openai',
    base_url: `https://${id}.example.com`,
    max_retries: 3,
    is_active: true,
    total_keys: 1,
    active_keys: 1,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
  }
}

function visibleModelIds(root: HTMLElement): string[] {
  return Array.from(root.querySelectorAll('[data-testid^="batch-assign-model-"]'))
    .map(node => node.getAttribute('data-testid')?.replace('batch-assign-model-', '') ?? '')
    .filter(Boolean)
}

/** 与组件实现约定的手动模式上游选项值，不与任何真实模型 ID 冲突。 */
const MANUAL_UPSTREAM_OPTION_VALUE = '__manual__'

/** 定位某行手动模式编辑区容器。 */
function manualConfig(root: HTMLElement, globalModelId: string): HTMLElement | null {
  return root.querySelector<HTMLElement>(`[data-testid="manual-config-${globalModelId}"]`)
}

/** 定位某行手动模式的真实上游名称输入。 */
function manualNameInput(root: HTMLElement, globalModelId: string): HTMLInputElement | null {
  return manualConfig(root, globalModelId)?.querySelector<HTMLInputElement>(
    'input[data-testid="manual-model-name-input"]',
  ) ?? null
}

/** 列出某行手动模式的 Endpoint 勾选框及其 Endpoint ID。 */
function manualEndpointChoices(root: HTMLElement, globalModelId: string): Array<{ input: HTMLInputElement; endpointId: string }> {
  const config = manualConfig(root, globalModelId)
  if (!config) return []
  return Array.from(config.querySelectorAll<HTMLInputElement>('input[type="checkbox"]'))
    .map(input => ({ input, endpointId: input.value }))
}

/** 读取某行手动模式的可见校验提示；仅断言存在性，不断言文案。 */
function manualHint(
  root: HTMLElement,
  globalModelId: string,
  hint: 'model-name-error' | 'endpoint-error',
): HTMLElement | null {
  return manualConfig(root, globalModelId)?.querySelector<HTMLElement>(
    `[data-testid="manual-${hint}"]`,
  ) ?? null
}

/** 通过行内上游模型选择切换到手动模式，返回真实上游名称输入。 */
async function switchToManualMode(
  root: HTMLElement,
  globalModelId: string,
  displayName: string,
): Promise<HTMLInputElement> {
  const row = root.querySelector<HTMLElement>(`[data-testid="batch-assign-model-${globalModelId}"]`)
  const upstreamSelect = row?.querySelector<HTMLSelectElement>(
    `select[aria-label="为 ${displayName} 选择上游模型"]`,
  )
  if (!upstreamSelect) throw new Error(`Missing upstream select for ${globalModelId}`)
  upstreamSelect.value = MANUAL_UPSTREAM_OPTION_VALUE
  upstreamSelect.dispatchEvent(new Event('change', { bubbles: true }))
  await settle()
  const nameInput = manualNameInput(root, globalModelId)
  if (!nameInput) throw new Error(`Missing manual name input for ${globalModelId}`)
  return nameInput
}

/** 更新手动模式的真实上游名称输入。 */
function setManualName(input: HTMLInputElement, value: string) {
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

/** 勾选手动模式中的指定 Endpoint；缺失视为契约破坏。 */
function checkManualEndpoint(root: HTMLElement, globalModelId: string, endpointId: string) {
  const choice = manualEndpointChoices(root, globalModelId)
    .find(item => item.endpointId === endpointId)
  if (!choice) throw new Error(`Missing manual endpoint checkbox: ${endpointId}`)
  choice.input.click()
}

/** 定位底部保存按钮。 */
function findSaveButton(root: HTMLElement): HTMLButtonElement | undefined {
  return Array.from(root.querySelectorAll('button'))
    .find(button => button.textContent?.trim() === '保存') as HTMLButtonElement | undefined
}

/** 绕过禁用态直接触发保存，用于证明 handleSave 的函数级守卫仍然生效。 */
function forceClickSave(root: HTMLElement) {
  findSaveButton(root)?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
}

describe('BatchAssignModelsDialog loading', () => {

  it('creates an exact same-name Global Model from aggregate upstream evidence without choosing a Key', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [{
        id: 'global-gemini',
        name: 'GEMINI-3.7-FLASH',
        display_name: 'Gemini 3.7 Flash',
      }],
      total: 1,
    })
    upstreamModelMocks.fetchModels.mockResolvedValue({
      models: [{
        id: 'gemini-3.7-flash',
        api_formats: ['gemini'],
        endpoint_ids: ['endpoint-gemini', 'endpoint-gemini'],
      }],
    })

    const { root } = await mountDialog()
    root.querySelector<HTMLElement>('[data-global-model-id="global-gemini"]')?.click()
    await settle()

    const saveButton = Array.from(root.querySelectorAll('button'))
      .find(button => button.textContent?.trim() === '保存')
    saveButton?.click()
    await settle()

    expect(upstreamModelMocks.fetchModels).toHaveBeenCalledWith('provider-1')
    expect(endpointMocks.createModel).toHaveBeenCalledWith('provider-1', {
      global_model_id: 'global-gemini',
      provider_model_name: 'gemini-3.7-flash',
      endpoint_ids: ['endpoint-gemini'],
    })
    expect(endpointMocks.batchAssignModelsToProvider).not.toHaveBeenCalled()
  })

  it('keeps explicit different-name selection after refreshing models from a Key', async () => {
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
    expect(upstreamSelect.value).toBe('')
    const upstreamOption = Array.from(upstreamSelect.options)
      .find(opt => opt.textContent?.trim() === 'gemini-3.8-flash-high')
    expect(upstreamOption).toBeDefined()
    upstreamSelect.value = upstreamOption!.value
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

  it('does not save while the initial aggregate upstream query is pending', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [{ id: 'global-custom', name: 'custom-model', display_name: 'Custom Model' }],
      total: 1,
    })
    let resolveModels!: (value: { models: [] }) => void
    upstreamModelMocks.fetchModels.mockImplementationOnce(
      () => new Promise(resolve => { resolveModels = resolve }),
    )

    const { root } = await mountDialog()
    root.querySelector<HTMLElement>('[data-global-model-id="global-custom"]')?.click()
    await settle()

    const saveButton = Array.from(root.querySelectorAll('button'))
      .find(button => button.textContent?.trim() === '保存') as HTMLButtonElement | undefined
    expect(saveButton?.disabled).toBe(true)
    // 主动绕过 DOM 禁用态，单独证明 handleSave 仍有函数级竞态守卫。
    saveButton?.removeAttribute('disabled')
    saveButton?.click()
    await settle()
    expect(endpointMocks.batchAssignModelsToProvider).not.toHaveBeenCalled()
    expect(endpointMocks.createModel).not.toHaveBeenCalled()

    resolveModels({ models: [] })
    await settle()
    expect(saveButton?.disabled).toBe(false)
  })

  it('guards save before aggregate discovery starts while base data is still loading', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [{ id: 'global-custom', name: 'custom-model', display_name: 'Custom Model' }],
      total: 1,
    })
    let resolveProviderKeys!: (value: []) => void
    endpointMocks.getProviderKeys.mockImplementationOnce(
      () => new Promise(resolve => { resolveProviderKeys = resolve }),
    )

    const { root } = await mountDialog()
    root.querySelector<HTMLElement>('[data-global-model-id="global-custom"]')?.click()
    await settle()

    const saveButton = Array.from(root.querySelectorAll('button'))
      .find(button => button.textContent?.trim() === '保存') as HTMLButtonElement | undefined
    expect(upstreamModelMocks.fetchModels).not.toHaveBeenCalled()
    expect(saveButton?.disabled).toBe(true)
    saveButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await settle()
    expect(endpointMocks.batchAssignModelsToProvider).not.toHaveBeenCalled()
    expect(endpointMocks.createModel).not.toHaveBeenCalled()

    resolveProviderKeys([])
    await settle()
    expect(upstreamModelMocks.fetchModels).toHaveBeenCalledWith('provider-1')
  })

  it('ignores an initial aggregate upstream response from a previous open session', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [{ id: 'global-custom', name: 'custom-model', display_name: 'Custom Model' }],
      total: 1,
    })
    let resolveOldRequest!: (value: {
      models: Array<{ id: string; api_formats: string[]; endpoint_ids: string[] }>
    }) => void
    upstreamModelMocks.fetchModels
      .mockImplementationOnce(() => new Promise(resolve => { resolveOldRequest = resolve }))
      .mockResolvedValue({
        models: [{ id: 'current-upstream', api_formats: ['gemini'], endpoint_ids: ['endpoint-2'] }],
      })

    const { root, open } = await mountDialog()
    open.value = false
    await settle()
    open.value = true
    await settle()
    root.querySelector<HTMLElement>('[data-global-model-id="global-custom"]')?.click()
    await settle()

    resolveOldRequest({
      models: [{ id: 'stale-upstream', api_formats: ['gemini'], endpoint_ids: ['endpoint-1'] }],
    })
    await settle()

    const optionLabels = Array.from(root.querySelectorAll<HTMLOptionElement>('select option'))
      .map(option => option.textContent?.trim())
    expect(upstreamModelMocks.fetchModels).toHaveBeenCalledTimes(2)
    expect(optionLabels).toContain('current-upstream')
    expect(optionLabels).not.toContain('stale-upstream')
  })

  it('blocks every write while a manual entry is missing its name or endpoints', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [
        createGlobalModel('global-sol', 'gpt-6.1-sol', 'GPT 6.1 Sol'),
        createGlobalModel('global-bound', 'bound-model', 'Bound Model'),
      ],
      total: 2,
    })
    endpointMocks.getProviderModels.mockResolvedValue([
      createProviderModel('pm-bound', 'global-bound'),
    ])
    endpointMocks.getProviderEndpoints.mockResolvedValue([
      createProviderEndpoint('endpoint-alpha'),
    ])

    const { root } = await mountDialog()
    // 新增一项手动关联，同时取消一项已有关联，形成“新增 + 删除”混合变更。
    root.querySelector<HTMLElement>('[data-global-model-id="global-sol"]')?.click()
    root.querySelector<HTMLElement>('[data-global-model-id="global-bound"]')?.click()
    await settle()
    expect(findSaveButton(root)?.disabled).toBe(false)

    const nameInput = await switchToManualMode(root, 'global-sol', 'GPT 6.1 Sol')
    const expectNoWrites = () => {
      expect(endpointMocks.createModel).not.toHaveBeenCalled()
      expect(endpointMocks.batchAssignModelsToProvider).not.toHaveBeenCalled()
      expect(endpointMocks.deleteModel).not.toHaveBeenCalled()
    }

    // 名称已预填但未选任何 Endpoint：阻止保存并给出可见提示。
    expect(nameInput.value).toBe('gpt-6.1-sol')
    expect(findSaveButton(root)?.disabled).toBe(true)
    expect(manualHint(root, 'global-sol', 'endpoint-error')).not.toBeNull()
    forceClickSave(root)
    await settle()
    expectNoWrites()

    // 勾选 Endpoint 后恢复可用；随后清空名称再次阻止。
    checkManualEndpoint(root, 'global-sol', 'endpoint-alpha')
    await settle()
    expect(findSaveButton(root)?.disabled).toBe(false)
    setManualName(nameInput, '')
    await settle()
    expect(findSaveButton(root)?.disabled).toBe(true)
    expect(manualHint(root, 'global-sol', 'model-name-error')).not.toBeNull()
    expect(manualHint(root, 'global-sol', 'endpoint-error')).toBeNull()
    forceClickSave(root)
    await settle()
    expectNoWrites()

    // 仅空白名称同样视为未填写。
    setManualName(nameInput, '   ')
    await settle()
    expect(findSaveButton(root)?.disabled).toBe(true)
    forceClickSave(root)
    await settle()
    expectNoWrites()

    // 补齐后保存：手动项精确创建，已取消的删除照常执行，不回退批量推断。
    setManualName(nameInput, 'gpt-6.1-sol')
    await settle()
    expect(findSaveButton(root)?.disabled).toBe(false)
    expect(manualHint(root, 'global-sol', 'model-name-error')).toBeNull()
    expect(manualHint(root, 'global-sol', 'endpoint-error')).toBeNull()
    findSaveButton(root)?.click()
    await settle()

    expect(endpointMocks.createModel).toHaveBeenCalledWith('provider-1', {
      global_model_id: 'global-sol',
      provider_model_name: 'gpt-6.1-sol',
      endpoint_ids: ['endpoint-alpha'],
    })
    expect(endpointMocks.deleteModel).toHaveBeenCalledWith('provider-1', 'pm-bound')
    expect(endpointMocks.batchAssignModelsToProvider).not.toHaveBeenCalled()
  })

  it('keeps manual edits when a Key refresh discovers a same-name upstream model', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [createGlobalModel('global-sol', 'gpt-6.1-sol', 'GPT 6.1 Sol')],
      total: 1,
    })
    endpointMocks.getProviderKeys.mockResolvedValue([{
      id: 'key-1',
      name: 'Sol Key',
      api_key_masked: 'sol-***',
    }])
    endpointMocks.getProviderEndpoints.mockResolvedValue([
      createProviderEndpoint('endpoint-alpha'),
      createProviderEndpoint('endpoint-beta'),
    ])
    upstreamModelMocks.fetchModels
      .mockResolvedValueOnce({ models: [] })
      .mockResolvedValueOnce({
        models: [{
          id: 'gpt-6.1-sol',
          api_formats: ['openai'],
          endpoint_ids: ['endpoint-beta'],
        }],
      })

    const { root } = await mountDialog()
    root.querySelector<HTMLElement>('[data-global-model-id="global-sol"]')?.click()
    await settle()

    const nameInput = await switchToManualMode(root, 'global-sol', 'GPT 6.1 Sol')
    setManualName(nameInput, '  gpt-6.1-sol  ')
    checkManualEndpoint(root, 'global-sol', 'endpoint-alpha')
    await settle()

    // 通过 Key 强制刷新发现：返回同名模型，但绑定的是另一个 Endpoint。
    const keyButton = Array.from(root.querySelectorAll('button'))
      .find(button => button.textContent?.includes('Sol Key'))
    expect(keyButton).toBeDefined()
    keyButton?.click()
    await settle()

    // 手动输入未被刷新覆盖，保存仍可用。
    expect(manualNameInput(root, 'global-sol')?.value).toBe('  gpt-6.1-sol  ')
    expect(findSaveButton(root)?.disabled).toBe(false)
    findSaveButton(root)?.click()
    await settle()

    // 手动配置优先于同名发现绑定：名称去除首尾空白，Endpoint 保持手选。
    expect(endpointMocks.createModel).toHaveBeenCalledWith('provider-1', {
      global_model_id: 'global-sol',
      provider_model_name: 'gpt-6.1-sol',
      endpoint_ids: ['endpoint-alpha'],
    })
    expect(endpointMocks.batchAssignModelsToProvider).not.toHaveBeenCalled()
  })

  it('drops manual bindings when the association is deselected', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [createGlobalModel('global-sol', 'gpt-6.1-sol', 'GPT 6.1 Sol')],
      total: 1,
    })
    endpointMocks.getProviderEndpoints.mockResolvedValue([
      createProviderEndpoint('endpoint-alpha'),
    ])

    const { root } = await mountDialog()
    root.querySelector<HTMLElement>('[data-global-model-id="global-sol"]')?.click()
    await settle()
    const nameInput = await switchToManualMode(root, 'global-sol', 'GPT 6.1 Sol')
    setManualName(nameInput, 'renamed-sol')
    checkManualEndpoint(root, 'global-sol', 'endpoint-alpha')
    await settle()

    // 取消勾选后重新勾选：旧手动状态不得回流到保存路径。
    root.querySelector<HTMLElement>('[data-global-model-id="global-sol"]')?.click()
    await settle()
    root.querySelector<HTMLElement>('[data-global-model-id="global-sol"]')?.click()
    await settle()

    forceClickSave(root)
    await settle()
    expect(endpointMocks.batchAssignModelsToProvider).toHaveBeenCalledWith('provider-1', ['global-sol'])
    expect(endpointMocks.createModel).not.toHaveBeenCalled()
  })

  it('clears manual bindings after closing and reopening the dialog', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [createGlobalModel('global-sol', 'gpt-6.1-sol', 'GPT 6.1 Sol')],
      total: 1,
    })
    endpointMocks.getProviderEndpoints.mockResolvedValue([
      createProviderEndpoint('endpoint-alpha'),
    ])

    const { root, open } = await mountDialog()
    root.querySelector<HTMLElement>('[data-global-model-id="global-sol"]')?.click()
    await settle()
    const nameInput = await switchToManualMode(root, 'global-sol', 'GPT 6.1 Sol')
    setManualName(nameInput, 'renamed-sol')
    checkManualEndpoint(root, 'global-sol', 'endpoint-alpha')
    await settle()

    open.value = false
    await settle()
    open.value = true
    await settle()

    // 重开后重新勾选：保存应回到批量推断，而非沿用上一会话的手动配置。
    root.querySelector<HTMLElement>('[data-global-model-id="global-sol"]')?.click()
    await settle()
    forceClickSave(root)
    await settle()
    expect(endpointMocks.batchAssignModelsToProvider).toHaveBeenCalledWith('provider-1', ['global-sol'])
    expect(endpointMocks.createModel).not.toHaveBeenCalled()
  })

  it('clears manual bindings when switching providers', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [createGlobalModel('global-sol', 'gpt-6.1-sol', 'GPT 6.1 Sol')],
      total: 1,
    })
    endpointMocks.getProviderEndpoints.mockResolvedValueOnce([
      createProviderEndpoint('endpoint-one'),
    ])

    const providerId = ref('provider-1')
    const { root } = await mountDialog(providerId)
    root.querySelector<HTMLElement>('[data-global-model-id="global-sol"]')?.click()
    await settle()
    const nameInput = await switchToManualMode(root, 'global-sol', 'GPT 6.1 Sol')
    setManualName(nameInput, 'renamed-sol')
    checkManualEndpoint(root, 'global-sol', 'endpoint-one')
    await settle()

    providerId.value = 'provider-2'
    await settle()

    // 切换后重新勾选同一全局模型：不得沿用 provider-1 的手动名称与 Endpoint。
    root.querySelector<HTMLElement>('[data-global-model-id="global-sol"]')?.click()
    await settle()
    forceClickSave(root)
    await settle()
    expect(endpointMocks.batchAssignModelsToProvider).toHaveBeenCalledWith('provider-2', ['global-sol'])
    expect(endpointMocks.createModel).not.toHaveBeenCalled()
  })

  it('ignores a stale endpoint response from a previous provider session', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [createGlobalModel('global-sol', 'gpt-6.1-sol', 'GPT 6.1 Sol')],
      total: 1,
    })
    let resolveStaleEndpoints!: (endpoints: ProviderEndpoint[]) => void
    endpointMocks.getProviderEndpoints
      .mockImplementationOnce(() => new Promise<ProviderEndpoint[]>(resolve => {
        resolveStaleEndpoints = resolve
      }))
      .mockResolvedValue([createProviderEndpoint('endpoint-two', 'provider-2')])

    const providerId = ref('provider-1')
    const { root } = await mountDialog(providerId)
    providerId.value = 'provider-2'
    await settle()

    // 上一 Provider 的延迟响应不得覆盖当前 Provider 的 Endpoint 选项。
    resolveStaleEndpoints([createProviderEndpoint('endpoint-one')])
    await settle()

    root.querySelector<HTMLElement>('[data-global-model-id="global-sol"]')?.click()
    await settle()
    await switchToManualMode(root, 'global-sol', 'GPT 6.1 Sol')

    const endpointIds = manualEndpointChoices(root, 'global-sol').map(choice => choice.endpointId)
    expect(endpointIds).toContain('endpoint-two')
    expect(endpointIds).not.toContain('endpoint-one')
  })

  it('pins already associated models to the top of the list', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [
        createGlobalModel('gm-zeta', 'zeta-model', 'Zeta'),
        createGlobalModel('gm-alpha', 'alpha-model', 'Alpha'),
        createGlobalModel('gm-mu', 'mu-model', 'Mu'),
      ],
      total: 3,
    })
    endpointMocks.getProviderModels.mockResolvedValue([
      createProviderModel('pm-mu', 'gm-mu'),
    ])

    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(defineComponent({
      setup() {
        return () => h(BatchAssignModelsDialog, {
          open: true,
          providerId: 'provider-1',
        })
      },
    }))
    app.mount(root)
    mountedApps.push({ app, root })
    await settle()

    expect(visibleModelIds(root)).toEqual(['gm-mu', 'gm-alpha', 'gm-zeta'])
  })

  it('keeps selected matches pinned above other search results', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [
        createGlobalModel('gm-beta', 'beta-flash', 'Beta Flash'),
        createGlobalModel('gm-alpha', 'alpha-flash', 'Alpha Flash'),
        createGlobalModel('gm-other', 'other-model', 'Other'),
      ],
      total: 3,
    })
    endpointMocks.getProviderModels.mockResolvedValue([
      createProviderModel('pm-beta', 'gm-beta'),
    ])

    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(defineComponent({
      setup() {
        return () => h(BatchAssignModelsDialog, {
          open: true,
          providerId: 'provider-1',
        })
      },
    }))
    app.mount(root)
    mountedApps.push({ app, root })
    await settle()

    const search = root.querySelector('input') as HTMLInputElement
    search.value = 'flash'
    search.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()

    expect(visibleModelIds(root)).toEqual(['gm-beta', 'gm-alpha'])
  })

  it('distinguishes a discovered upstream model named __manual__ from manual mode', async () => {
    globalModelMocks.getGlobalModels.mockResolvedValue({
      models: [createGlobalModel('global-special', '__manual__', 'Special Manual Name')],
      total: 1,
    })
    endpointMocks.getProviderEndpoints.mockResolvedValue([
      createProviderEndpoint('endpoint-manual-test'),
    ])
    upstreamModelMocks.fetchModels.mockResolvedValue({
      models: [{
        id: '__manual__',
        api_formats: ['openai'],
        endpoint_ids: ['endpoint-manual-test'],
      }],
    })

    const { root } = await mountDialog()
    root.querySelector<HTMLElement>('[data-global-model-id="global-special"]')?.click()
    await settle()

    const select = root.querySelector<HTMLSelectElement>(
      'select[aria-label="为 Special Manual Name 选择上游模型"]',
    )
    expect(select).not.toBeNull()
    if (!select) return

    const discoveredOption = Array.from(select.options)
      .find(opt => opt.textContent?.trim() === '__manual__' && opt.value !== MANUAL_UPSTREAM_OPTION_VALUE)
    expect(discoveredOption).toBeDefined()
    select.value = discoveredOption!.value
    select.dispatchEvent(new Event('change', { bubbles: true }))
    await settle()

    expect(manualConfig(root, 'global-special')).toBeNull()

    const saveButton = findSaveButton(root)
    expect(saveButton?.disabled).toBe(false)
    saveButton?.click()
    await settle()

    expect(endpointMocks.createModel).toHaveBeenCalledWith('provider-1', {
      global_model_id: 'global-special',
      provider_model_name: '__manual__',
      endpoint_ids: ['endpoint-manual-test'],
    })
  })
})

describe('BatchAssignModelsDialog create model entry', () => {
  async function mountDialog() {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(defineComponent({
      setup() {
        return () => h(BatchAssignModelsDialog, {
          open: true,
          providerId: 'provider-1',
        })
      },
    }))
    app.mount(root)
    mountedApps.push({ app, root })
    await settle()
    return root
  }

  it('opens the unified model dialog from the create button', async () => {
    const root = await mountDialog()
    expect(root.querySelector('[data-testid="global-model-form-dialog-stub"]')).toBeNull()

    const createButton = root.querySelector('[data-testid="batch-assign-create-model"]') as HTMLButtonElement
    expect(createButton).toBeTruthy()
    createButton.click()
    await settle()

    expect(root.querySelector('[data-testid="global-model-form-dialog-stub"]')).not.toBeNull()
  })

  it('refreshes the global model list after a model is created', async () => {
    globalModelMocks.getGlobalModels
      .mockResolvedValueOnce({
        models: [createGlobalModel('gm-old', 'old-model', 'Old Model')],
        total: 1,
      })
      .mockResolvedValueOnce({
        models: [
          createGlobalModel('gm-old', 'old-model', 'Old Model'),
          createGlobalModel('gm-new', 'new-model', 'New Model'),
        ],
        total: 2,
      })

    const root = await mountDialog()
    expect(visibleModelIds(root)).toEqual(['gm-old'])

    const createButton = root.querySelector('[data-testid="batch-assign-create-model"]') as HTMLButtonElement
    createButton.click()
    await settle()

    const successButton = root.querySelector('[data-testid="global-model-form-dialog-stub-success"]') as HTMLButtonElement
    expect(successButton).toBeTruthy()
    successButton.click()
    await settle()

    expect(globalModelMocks.getGlobalModels).toHaveBeenCalledTimes(2)
    expect(visibleModelIds(root)).toEqual(['gm-new', 'gm-old'])
  })
})
