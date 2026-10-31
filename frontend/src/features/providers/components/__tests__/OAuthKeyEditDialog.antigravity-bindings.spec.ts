import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, type App } from 'vue'
import OAuthKeyEditDialog from '@/features/providers/components/OAuthKeyEditDialog.vue'
import type { EndpointAPIKey } from '@/api/endpoints'
import { createI18n, setI18nLocale } from '@/i18n'

const endpointMocks = vi.hoisted(() => ({
  updateProviderKey: vi.fn(),
}))

const toastMocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
}))

const confirmMocks = vi.hoisted(() => ({
  confirmWarning: vi.fn().mockResolvedValue(true),
}))

vi.mock('@/api/endpoints', () => ({
  updateProviderKey: endpointMocks.updateProviderKey,
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => toastMocks,
}))

vi.mock('@/composables/useConfirm', () => ({
  useConfirm: () => confirmMocks,
}))

vi.mock('@/components/ui', async () => {
  // vi.mock 工厂会被提升到静态 import 之前，需按测试加载边界动态获取 Vue。
  const { defineComponent, h } = await import('vue')

  const Dialog = defineComponent({
    name: 'DialogStub',
    props: { modelValue: Boolean },
    setup(props, { slots }) {
      return () => props.modelValue
        ? h('section', [slots.default?.(), slots.footer?.()])
        : null
    },
  })

  const Input = defineComponent({
    name: 'InputStub',
    inheritAttrs: false,
    props: {
      modelValue: { type: [String, Number], default: '' },
    },
    emits: ['update:modelValue'],
    setup(props, { attrs, emit }) {
      return () => h('input', {
        ...attrs,
        value: props.modelValue ?? '',
        onInput: (event: Event) => emit('update:modelValue', (event.target as HTMLInputElement).value),
      })
    },
  })

  const Label = defineComponent({
    name: 'LabelStub',
    inheritAttrs: false,
    props: { for: String },
    setup(props, { attrs, slots }) {
      return () => h('label', { ...attrs, for: props.for }, slots.default?.())
    },
  })

  const Button = defineComponent({
    name: 'ButtonStub',
    inheritAttrs: false,
    props: { disabled: Boolean, variant: String },
    setup(props, { attrs, slots }) {
      return () => h('button', {
        ...attrs,
        disabled: props.disabled,
        type: attrs.type ?? 'button',
      }, slots.default?.())
    },
  })

  const Switch = defineComponent({
    name: 'SwitchStub',
    inheritAttrs: false,
    props: { modelValue: Boolean },
    emits: ['update:modelValue'],
    setup(props, { attrs, emit }) {
      return () => h('input', {
        ...attrs,
        type: 'checkbox',
        checked: props.modelValue,
        onChange: (event: Event) => emit('update:modelValue', (event.target as HTMLInputElement).checked),
      })
    },
  })

  return {
    Dialog,
    Button,
    Input,
    Label,
    Switch,
  }
})

vi.mock('lucide-vue-next', async () => {
  // vi.mock 工厂会被提升到静态 import 之前，需按测试加载边界动态获取 Vue。
  const { defineComponent, h } = await import('vue')
  const Icon = defineComponent({
    name: 'IconStub',
    setup() {
      return () => h('span')
    },
  })
  return {
    SquarePen: Icon,
  }
})

const mountedApps: Array<{ app: App, root: HTMLElement }> = []

function createAntigravityKey(overrides: Partial<EndpointAPIKey> = {}): EndpointAPIKey {
  return {
    id: 'ag-key-1',
    provider_id: 'provider-ag',
    provider_type: 'antigravity',
    api_formats: ['gemini:generate_content'],
    api_key_masked: 'oauth***',
    auth_type: 'oauth',
    name: 'Antigravity Pro Account',
    internal_priority: 10,
    rpm_limit: null,
    concurrent_limit: null,
    cache_ttl_minutes: 5,
    max_probe_interval_minutes: 32,
    health_score: 100,
    consecutive_failures: 0,
    request_count: 0,
    success_count: 0,
    error_count: 0,
    success_rate: 1,
    avg_response_time_ms: 0,
    is_active: true,
    note: '',
    created_at: '2026-10-31T00:00:00Z',
    updated_at: '2026-10-31T00:00:00Z',
    auto_fetch_models: true,
    model_include_patterns: [],
    model_exclude_patterns: [],
    antigravity_quota_bindings: {},
    antigravity_quota_models: [
      'gemini-4-argon-medium',
      'gemini-3.8-flash-medium',
      'display-only-discovered-model',
    ],
    status_snapshot: {
      oauth: { code: 'valid' },
      account: { code: 'ok', blocked: false },
      quota: {
        code: 'ok',
        provider_type: 'antigravity',
        exhausted: false,
        windows: [
          {
            code: 'group:0:argon-weekly',
            scope: 'quota_group',
            quota_group: 'buckets:["gemini-argon-wtu-weekly"]',
            quota_group_label: 'Gemini 4 Argon',
            bucket_id: 'gemini-argon-wtu-weekly',
            window: 'weekly',
            remaining_ratio: 0.5,
            is_included: true,
          },
          {
            code: 'group:1:3p-5h',
            scope: 'quota_group',
            quota_group: 'id:claude-gpt-shared',
            quota_group_label: 'Claude and GPT models',
            bucket_id: '3p-5h',
            window: '5h',
            remaining_ratio: 1,
            is_included: false,
          },
          {
            code: 'group:1:3p-weekly',
            scope: 'quota_group',
            quota_group: 'id:claude-gpt-shared',
            quota_group_label: 'Claude and GPT models',
            bucket_id: '3p-weekly',
            window: 'weekly',
            remaining_ratio: 0.8,
            is_included: true,
          },
        ],
      },
    },
    ...overrides,
  }
}

async function settle() {
  await Promise.resolve()
  await nextTick()
  await Promise.resolve()
  await nextTick()
}

function mountDialog(
  editingKey: EndpointAPIKey,
  providerType?: string | null,
  onBindingsSaved?: (key: EndpointAPIKey) => void,
) {
  setI18nLocale('zh-CN')
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(defineComponent({
    setup() {
      return () => h(OAuthKeyEditDialog, {
        open: true,
        editingKey,
        providerType,
        onBindingsSaved,
      })
    },
  }))
  app.use(createI18n())
  app.mount(root)
  mountedApps.push({ app, root })
  return root
}

function selectOption(select: HTMLSelectElement, value: string) {
  select.value = value
  select.dispatchEvent(new Event('change', { bubbles: true }))
}

describe('OAuthKeyEditDialog Antigravity quota bindings', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    endpointMocks.updateProviderKey.mockImplementation(async (id: string, data: Record<string, unknown>) => ({
      ...createAntigravityKey({ id }),
      ...(data.antigravity_quota_bindings !== undefined
        ? { antigravity_quota_bindings: data.antigravity_quota_bindings as Record<string, string> }
        : {}),
    }))
  })

  afterEach(() => {
    while (mountedApps.length > 0) {
      const mounted = mountedApps.pop()
      mounted?.app.unmount()
      mounted?.root.remove()
    }
  })

  it('hides Antigravity binding controls for non-Antigravity OAuth accounts', async () => {
    const codexKey = createAntigravityKey({
      provider_type: 'codex',
      antigravity_quota_bindings: null,
      antigravity_quota_models: null,
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: { code: 'ok', provider_type: 'codex', exhausted: false, windows: [] },
      },
    })
    const root = mountDialog(codexKey, 'codex')
    await settle()

    expect(root.querySelector('[data-testid="antigravity-quota-bindings-section"]')).toBeNull()
  })

  it('saves and clears explicit bindings via dedicated binding-only PUT requests without mixing form fields', async () => {
    const onBindingsSaved = vi.fn()
    const key = createAntigravityKey()
    const root = mountDialog(key, 'antigravity', onBindingsSaved)
    await settle()

    const addBtn = root.querySelector<HTMLButtonElement>('[data-testid="antigravity-binding-add"]')
    expect(addBtn).not.toBeNull()
    addBtn!.click()
    await settle()

    const modelSelect = root.querySelector<HTMLSelectElement>('[data-testid="antigravity-binding-model-select"]')
    const groupSelect = root.querySelector<HTMLSelectElement>('[data-testid="antigravity-binding-group-select"]')
    expect(modelSelect).not.toBeNull()
    expect(groupSelect).not.toBeNull()

    const modelValues = Array.from(modelSelect!.options).map(opt => opt.value).filter(Boolean)
    expect(modelValues).toContain('display-only-discovered-model')
    expect(modelValues).toContain('gemini-4-argon-medium')

    const groupValues = Array.from(groupSelect!.options).map(opt => opt.value).filter(Boolean)
    expect(groupValues).toEqual([
      'buckets:["gemini-argon-wtu-weekly"]',
      'id:claude-gpt-shared',
    ])

    selectOption(modelSelect!, 'gemini-4-argon-medium')
    selectOption(groupSelect!, 'buckets:["gemini-argon-wtu-weekly"]')
    await settle()

    const saveBtn = root.querySelector<HTMLButtonElement>('[data-testid="antigravity-binding-save"]')
    expect(saveBtn?.disabled).toBe(false)
    saveBtn!.click()
    await settle()

    expect(endpointMocks.updateProviderKey).toHaveBeenCalledTimes(1)
    expect(endpointMocks.updateProviderKey).toHaveBeenCalledWith('ag-key-1', {
      antigravity_quota_bindings: {
        'gemini-4-argon-medium': 'buckets:["gemini-argon-wtu-weekly"]',
      },
    })
    expect(onBindingsSaved).toHaveBeenCalledTimes(1)
    expect(root.querySelector('[data-testid="antigravity-binding-success"]')?.textContent).toContain('已保存 1 项')

    const clearBtn = root.querySelector<HTMLButtonElement>('[data-testid="antigravity-binding-clear"]')
    expect(clearBtn?.disabled).toBe(false)
    clearBtn!.click()
    await settle()

    expect(endpointMocks.updateProviderKey).toHaveBeenCalledTimes(2)
    expect(endpointMocks.updateProviderKey).toHaveBeenLastCalledWith('ag-key-1', {
      antigravity_quota_bindings: {},
    })
    expect(root.querySelectorAll('[data-testid^="antigravity-binding-row-"]')).toHaveLength(0)

    const form = root.querySelector('form')
    form!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await settle()

    const formPayload = endpointMocks.updateProviderKey.mock.calls[2]?.[1] as Record<string, unknown>
    expect(formPayload).toBeDefined()
    expect(formPayload).not.toHaveProperty('antigravity_quota_bindings')
  })

  it('preserves stale saved bindings visibly with warning, blocks invalid save, and allows clearing', async () => {
    const staleKey = createAntigravityKey({
      antigravity_quota_bindings: {
        'gemini-4-argon-medium': 'buckets:["obsolete-bucket"]',
      },
    })
    const root = mountDialog(staleKey, 'antigravity')
    await settle()

    expect(root.querySelector('[data-testid="antigravity-binding-stale-warning"]')).not.toBeNull()
    expect(root.querySelectorAll('[data-testid^="antigravity-binding-row-"]')).toHaveLength(1)

    const addBtn = root.querySelector<HTMLButtonElement>('[data-testid="antigravity-binding-add"]')
    addBtn!.click()
    await settle()

    const rows = root.querySelectorAll('[data-testid^="antigravity-binding-row-"]')
    const secondModel = rows[1]?.querySelector<HTMLSelectElement>('[data-testid="antigravity-binding-model-select"]')
    const secondGroup = rows[1]?.querySelector<HTMLSelectElement>('[data-testid="antigravity-binding-group-select"]')
    selectOption(secondModel!, 'gemini-3.8-flash-medium')
    selectOption(secondGroup!, 'id:claude-gpt-shared')
    await settle()

    const saveBtn = root.querySelector<HTMLButtonElement>('[data-testid="antigravity-binding-save"]')
    expect(saveBtn?.disabled).toBe(true)

    const clearBtn = root.querySelector<HTMLButtonElement>('[data-testid="antigravity-binding-clear"]')
    expect(clearBtn?.disabled).toBe(false)
    clearBtn!.click()
    await settle()

    expect(endpointMocks.updateProviderKey).toHaveBeenCalledWith('ag-key-1', {
      antigravity_quota_bindings: {},
    })
  })

  it('preserves user draft input and displays error feedback when saving bindings fails', async () => {
    endpointMocks.updateProviderKey.mockRejectedValueOnce(new Error('未知配额组标识'))
    const root = mountDialog(createAntigravityKey(), 'antigravity')
    await settle()

    root.querySelector<HTMLButtonElement>('[data-testid="antigravity-binding-add"]')!.click()
    await settle()

    const modelSelect = root.querySelector<HTMLSelectElement>('[data-testid="antigravity-binding-model-select"]')!
    const groupSelect = root.querySelector<HTMLSelectElement>('[data-testid="antigravity-binding-group-select"]')!
    selectOption(modelSelect, 'gemini-4-argon-medium')
    selectOption(groupSelect, 'id:claude-gpt-shared')
    await settle()

    root.querySelector<HTMLButtonElement>('[data-testid="antigravity-binding-save"]')!.click()
    await settle()

    expect(root.querySelector('[data-testid="antigravity-binding-error"]')?.textContent).toContain('未知配额组标识')
    expect(modelSelect.value).toBe('gemini-4-argon-medium')
    expect(groupSelect.value).toBe('id:claude-gpt-shared')
  })
})
