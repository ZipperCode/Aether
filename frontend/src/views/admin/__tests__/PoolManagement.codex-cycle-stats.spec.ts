import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, nextTick, type App } from 'vue'

import type { ProviderType } from '@/api/endpoints/types'
import PoolManagement from '@/views/admin/PoolManagement.vue'
import type { PoolKeyDetail, PoolOverviewItem, PoolKeysPageResponse } from '@/api/endpoints/pool'
import { POOL_MANAGEMENT_VIEW_STORAGE_KEY } from '@/features/pool/utils/poolManagementState'

const endpointMocks = vi.hoisted(() => ({
  getPoolOverview: vi.fn(),
  getPoolSchedulingPresets: vi.fn(),
  listPoolKeys: vi.fn(),
  createPoolKeySelectionSnapshot: vi.fn(),
  clearPoolCooldown: vi.fn(),
  getProvider: vi.fn(),
  updateProvider: vi.fn(),
  revealEndpointKey: vi.fn(),
  exportKey: vi.fn(),
  deleteEndpointKey: vi.fn(),
  updateProviderKey: vi.fn(),
  refreshProviderQuota: vi.fn(),
  resetProviderKeyCycleStats: vi.fn(),
  refreshProviderOAuth: vi.fn(),
  recoverKeyHealth: vi.fn(),
}))

const routeMocks = vi.hoisted(() => ({
  query: {} as Record<string, string>,
  patchQuery: vi.fn((patch: Record<string, string | undefined | null>) => {
    for (const [key, value] of Object.entries(patch)) {
      if (value == null || String(value).trim() === '') {
        delete routeMocks.query[key]
      } else {
        routeMocks.query[key] = String(value)
      }
    }
  }),
}))

const toastMocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
  warning: vi.fn(),
}))

const proxyStoreMocks = vi.hoisted(() => ({
  ensureLoaded: vi.fn(),
}))

vi.mock('@/api/endpoints/pool', () => ({
  getPoolOverview: endpointMocks.getPoolOverview,
  getPoolSchedulingPresets: endpointMocks.getPoolSchedulingPresets,
  listPoolKeys: endpointMocks.listPoolKeys,
  createPoolKeySelectionSnapshot: endpointMocks.createPoolKeySelectionSnapshot,
  clearPoolCooldown: endpointMocks.clearPoolCooldown,
}))

vi.mock('@/api/endpoints/keys', () => ({
  revealEndpointKey: endpointMocks.revealEndpointKey,
  exportKey: endpointMocks.exportKey,
  deleteEndpointKey: endpointMocks.deleteEndpointKey,
  updateProviderKey: endpointMocks.updateProviderKey,
  refreshProviderQuota: endpointMocks.refreshProviderQuota,
  resetProviderKeyCycleStats: endpointMocks.resetProviderKeyCycleStats,
}))

vi.mock('@/api/endpoints/provider_oauth', () => ({
  refreshProviderOAuth: endpointMocks.refreshProviderOAuth,
}))
vi.mock('@/api/endpoints/health', () => ({
  recoverKeyHealth: endpointMocks.recoverKeyHealth,
}))
vi.mock('@/api/endpoints', () => ({
  getProvider: endpointMocks.getProvider,
  updateProvider: endpointMocks.updateProvider,
}))

vi.mock('@/composables/useRouteQuery', () => ({
  useRouteQuery: () => ({
    getQueryValue: (key: string) => routeMocks.query[key],
    patchQuery: routeMocks.patchQuery,
  }),
}))

vi.mock('@/stores/proxy-nodes', () => ({
  useProxyNodesStore: () => ({
    nodes: [],
    ensureLoaded: proxyStoreMocks.ensureLoaded,
  }),
}))

// 共享 toast mock 实例，供单 Key 额度刷新的提示断言复用。
vi.mock('@/composables/useToast', () => ({
  useToast: () => toastMocks,
}))

vi.mock('@/composables/useRouteQuery', async () => {
  const { reactive } = await import('vue')
  // 响应式 query 让页面 watch(providerId) 可被测试直接驱动，用于验证 Provider 切换边界；
  // vitest 的 vi.mock 工厂先于静态 import 求值，只能在此处动态引入 vue。
  routeMocks.query = reactive(routeMocks.query)
  return {
    useRouteQuery: () => ({
      getQueryValue: (key: string) => routeMocks.query[key],
      patchQuery: routeMocks.patchQuery,
    }),
  }
})
vi.mock('@/composables/useClipboard', () => ({
  useClipboard: () => ({
    copyToClipboard: vi.fn().mockResolvedValue(undefined),
  }),
}))

vi.mock('@/composables/useConfirm', () => ({
  useConfirm: () => ({
    confirm: vi.fn().mockResolvedValue(true),
  }),
}))

vi.mock('@/composables/useCountdownTimer', async () => {
  const { ref } = await import('vue')
  return {
    useCountdownTimer: () => ({
      tick: ref(0),
      start: vi.fn(),
    }),
    getCodexResetCountdown: () => ({
      isExpired: false,
      text: '1h',
    }),
  }
})

vi.mock('lucide-vue-next', async () => {
  const { defineComponent, h } = await import('vue')
  const Icon = defineComponent({
    name: 'IconStub',
    setup() {
      return () => h('span')
    },
  })

  return {
    Search: Icon,
    Upload: Icon,
    ChevronDown: Icon,
    RefreshCw: Icon,
    Activity: Icon,
    Power: Icon,
    Database: Icon,
    KeyRound: Icon,
    Download: Icon,
    Copy: Icon,
    Shield: Icon,
    Globe: Icon,
    RotateCcw: Icon,
    SquarePen: Icon,
    Trash2: Icon,
    Users: Icon,
    Settings2: Icon,
    SlidersHorizontal: Icon,
    CircleHelp: Icon,
    Edit: Icon,
    Eye: Icon,
    ListChecks: Icon,
    SquareCheckBig: Icon,
  }
})

vi.mock('@/components/ui', async () => {
  const { computed, defineComponent, h, inject, provide } = await import('vue')
  const passthrough = (name: string, tag = 'div') => defineComponent({
    name,
    inheritAttrs: false,
    setup(_, { attrs, slots }) {
      return () => h(tag, attrs, slots.default?.())
    },
  })

  const Button = defineComponent({
    name: 'ButtonStub',
    inheritAttrs: false,
    props: {
      disabled: Boolean,
    },
    setup(props, { attrs, slots }) {
      return () => h('button', { ...attrs, disabled: props.disabled, type: attrs.type ?? 'button' }, slots.default?.())
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

  const Switch = defineComponent({
    name: 'SwitchStub',
    inheritAttrs: false,
    props: {
      modelValue: Boolean,
    },
    emits: ['update:modelValue'],
    setup(props, { attrs, emit }) {
      return () => h('input', {
        ...attrs,
        type: 'checkbox',
        role: 'switch',
        checked: props.modelValue,
        onChange: (event: Event) => emit('update:modelValue', (event.target as HTMLInputElement).checked),
      })
    },
  })

  const Checkbox = defineComponent({
    name: 'CheckboxStub',
    inheritAttrs: false,
    props: {
      checked: Boolean,
      modelValue: Boolean,
      indeterminate: Boolean,
      disabled: Boolean,
    },
    emits: ['update:checked', 'update:modelValue'],
    setup(props, { attrs, emit }) {
      return () => h('input', {
        ...attrs,
        type: 'checkbox',
        checked: props.checked || props.modelValue,
        disabled: props.disabled,
        'data-indeterminate': props.indeterminate ? 'true' : undefined,
        onChange: (event: Event) => {
          const checked = (event.target as HTMLInputElement).checked
          emit('update:checked', checked)
          emit('update:modelValue', checked)
        },
      })
    },
  })

  const Pagination = defineComponent({
    name: 'PaginationStub',
    setup() {
      return () => h('nav')
    },
  })

  const DropdownMenuItem = defineComponent({
    name: 'DropdownMenuItemStub',
    inheritAttrs: false,
    emits: ['select'],
    setup(_, { attrs, emit, slots }) {
      return () => h('button', {
        ...attrs,
        type: 'button',
        onClick: (event: Event) => emit('select', event),
      }, slots.default?.())
    },
  })

  const popoverContextKey = Symbol('PopoverStubContext')

  const Popover = defineComponent({
    name: 'PopoverStub',
    inheritAttrs: false,
    props: {
      open: Boolean,
    },
    emits: ['update:open'],
    setup(props, { slots, emit }) {
      const context = {
        open: computed(() => props.open),
        toggle: () => emit('update:open', !props.open),
      }
      provide(popoverContextKey, context)
      return () => slots.default?.()
    },
  })

  const PopoverTrigger = defineComponent({
    name: 'PopoverTriggerStub',
    inheritAttrs: false,
    setup(_, { attrs, slots }) {
      const context = inject<{ open: { value: boolean }, toggle: () => void } | null>(popoverContextKey, null)
      return () => {
        return h('span', {
          ...attrs,
          onClickCapture: () => {
            context?.toggle()
          },
        }, slots.default?.())
      }
    },
  })

  const PopoverContent = defineComponent({
    name: 'PopoverContentStub',
    inheritAttrs: false,
    setup(_, { attrs, slots }) {
      const context = inject<{ open: { value: boolean } } | null>(popoverContextKey, null)
      return () => {
        if (!context?.open.value) return null
        return h('div', { ...attrs, 'data-state': 'open' }, slots.default?.())
      }
    },
  })

  return {
    Card: passthrough('CardStub'),
    Badge: passthrough('BadgeStub', 'span'),
    Button,
    Checkbox,
    Input,
    Select: passthrough('SelectStub'),
    SelectTrigger: passthrough('SelectTriggerStub', 'button'),
    SelectValue: passthrough('SelectValueStub', 'span'),
    SelectContent: passthrough('SelectContentStub'),
    SelectItem: passthrough('SelectItemStub'),
    Table: passthrough('TableStub', 'table'),
    TableHeader: passthrough('TableHeaderStub', 'thead'),
    TableBody: passthrough('TableBodyStub', 'tbody'),
    TableRow: passthrough('TableRowStub', 'tr'),
    TableHead: passthrough('TableHeadStub', 'th'),
    SortableTableHead: passthrough('SortableTableHeadStub', 'th'),
    TableFilterMenu: passthrough('TableFilterMenuStub'),
    TableCell: passthrough('TableCellStub', 'td'),
    DropdownMenu: passthrough('DropdownMenuStub'),
    DropdownMenuTrigger: passthrough('DropdownMenuTriggerStub'),
    DropdownMenuContent: passthrough('DropdownMenuContentStub'),
    DropdownMenuItem,
    Switch,
    Pagination,
    Popover,
    PopoverTrigger,
    PopoverContent,
  }
})

vi.mock('@/components/ui/refresh-button.vue', async () => {
  const { defineComponent, h } = await import('vue')
  return {
    default: defineComponent({
      name: 'RefreshButtonStub',
      setup(_, { attrs }) {
        return () => h('button', attrs, '刷新')
      },
    }),
  }
})

vi.mock('@/features/pool/components/PoolSchedulingDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return {
    default: defineComponent({
      name: 'PoolSchedulingDialogStub',
      setup() {
        return () => null
      },
    }),
  }
})
vi.mock('@/features/pool/components/PoolAdvancedDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return {
    default: defineComponent({
      name: 'PoolAdvancedDialogStub',
      setup() {
        return () => null
      },
    }),
  }
})
vi.mock('@/features/pool/components/PoolDemandMetricsDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return {
    default: defineComponent({
      name: 'PoolDemandMetricsDialogStub',
      setup() {
        return () => null
      },
    }),
  }
})
vi.mock('@/features/pool/components/PoolAccountBatchDialog.vue', async () => {
  const { defineComponent, h } = await import('vue')
  return {
    default: defineComponent({
      name: 'PoolAccountBatchDialogStub',
      props: {
        modelValue: Boolean,
        selectedKeys: { type: Array, default: () => [] },
        selectAllFiltered: Boolean,
        selectedCount: { type: Number, default: 0 },
        selectionSnapshot: { type: Object, default: null },
        initialAction: { type: String, default: null },
      },
      emits: ['update:modelValue', 'changed', 'editConfig'],
      setup(props) {
        return () => h('div', {
          'data-testid': 'pool-account-batch-dialog',
          'data-open': props.modelValue ? 'true' : 'false',
          'data-selected-ids': (props.selectedKeys as PoolKeyDetail[])
            .map(key => key.key_id)
            .join(','),
          'data-select-all-filtered': props.selectAllFiltered ? 'true' : 'false',
          'data-selected-count': String(props.selectedCount),
          'data-selection-snapshot-id': (props.selectionSnapshot as { id?: string } | null)?.id || '',
          'data-initial-action': props.initialAction || '',
        })
      },
    }),
  }
})
vi.mock('@/features/pool/components/PoolKeyBatchEditDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return {
    default: defineComponent({
      name: 'PoolKeyBatchEditDialogStub',
      setup() {
        return () => null
      },
    }),
  }
})
vi.mock('@/features/providers/components/ProviderFormDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return {
    default: defineComponent({
      name: 'ProviderFormDialogStub',
      setup() {
        return () => null
      },
    }),
  }
})
vi.mock('@/features/providers/components/KeyAllowedModelsEditDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return {
    default: defineComponent({
      name: 'KeyAllowedModelsEditDialogStub',
      setup() {
        return () => null
      },
    }),
  }
})
vi.mock('@/features/providers/components/KeyFormDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return {
    default: defineComponent({
      name: 'KeyFormDialogStub',
      setup() {
        return () => null
      },
    }),
  }
})
vi.mock('@/features/providers/components/OAuthKeyEditDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return {
    default: defineComponent({
      name: 'OAuthKeyEditDialogStub',
      setup() {
        return () => null
      },
    }),
  }
})
vi.mock('@/features/providers/components/OAuthAccountDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return {
    default: defineComponent({
      name: 'OAuthAccountDialogStub',
      setup() {
        return () => null
      },
    }),
  }
})
vi.mock('@/features/providers/components/ProxyNodeSelect.vue', async () => {
  const { defineComponent } = await import('vue')
  return {
    default: defineComponent({
      name: 'ProxyNodeSelectStub',
      setup() {
        return () => null
      },
    }),
  }
})

const mountedApps: Array<{ app: App, root: HTMLElement }> = []

function createOverview(providerType: ProviderType): PoolOverviewItem {
  return {
    provider_id: `${providerType}-provider`,
    provider_name: `${providerType} Provider`,
    provider_type: providerType,
    total_keys: 1,
    active_keys: 1,
    cooldown_count: 0,
    pool_enabled: true,
  }
}

function createProvider(providerType: string, overrides: Record<string, unknown> = {}) {
  return {
    id: `${providerType}-provider`,
    name: `${providerType} Provider`,
    provider_type: providerType,
    is_active: true,
    api_formats: ['openai:chat'],
    proxy: null,
    pool_advanced: null,
    claude_code_advanced: null,
    ...overrides,
  }
}

function createPoolKey(providerType = 'codex', overrides: Partial<PoolKeyDetail> = {}): PoolKeyDetail {
  return {
    key_id: `${providerType}-key-1`,
    key_name: `${providerType} key`,
    is_active: true,
    auth_type: 'api_key',
    api_formats: ['openai:chat'],
    internal_priority: 50,
    account_quota: null,
    cooldown_reason: null,
    cooldown_ttl_seconds: null,
    cost_window_usage: 0,
    cost_limit: null,
    request_count: 9876,
    total_tokens: 4321000,
    total_cost_usd: '8.7654',
    sticky_sessions: 0,
    lru_score: null,
    created_at: '2026-05-05T00:00:00Z',
    imported_at: '2026-05-05T00:00:00Z',
    last_used_at: '2026-05-05T01:00:00Z',
    status_snapshot: {
      oauth: { code: 'none' },
      account: { code: 'ok', blocked: false },
      quota: {
        code: 'ok',
        exhausted: false,
        provider_type: providerType,
        windows: providerType === 'codex'
          ? [
              {
                code: '5h',
                remaining_ratio: 0.8,
                usage: { request_count: 7, total_tokens: 2500, total_cost_usd: '0.0045' },
              },
              {
                code: 'weekly',
                remaining_ratio: 0.5,
                usage: { request_count: 12, total_tokens: 5000, total_cost_usd: '0.012' },
              },
            ]
          : [],
      },
    },
    ...overrides,
  }
}

function createKeyPage(key: PoolKeyDetail): PoolKeysPageResponse {
  return {
    total: 1,
    page: 1,
    page_size: 50,
    keys: [key],
  }
}

function resetQuery() {
  for (const key of Object.keys(routeMocks.query)) {
    delete routeMocks.query[key]
  }
}

function mountPoolManagement() {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(PoolManagement)
  app.mount(root)
  mountedApps.push({ app, root })
  return root
}

async function settle() {
  for (let index = 0; index < 8; index += 1) {
    await Promise.resolve()
    await nextTick()
  }
}

beforeEach(() => {
  resetQuery()
  window.sessionStorage.clear()
  routeMocks.patchQuery.mockClear()
  proxyStoreMocks.ensureLoaded.mockClear()

  endpointMocks.getPoolOverview.mockReset()
  endpointMocks.getPoolSchedulingPresets.mockReset()
  endpointMocks.listPoolKeys.mockReset()
  endpointMocks.createPoolKeySelectionSnapshot.mockReset().mockImplementation(
    async (_providerId: string, request: { page?: number, page_size?: number, expected_total: number }) => ({
      total: request.expected_total,
      page: request.page ?? 1,
      page_size: request.page_size ?? 50,
      selection_snapshot: {
        id: 'selection-snapshot',
        total: request.expected_total,
        status: 'ready',
      },
    }),
  )
  endpointMocks.clearPoolCooldown.mockReset()
  endpointMocks.getProvider.mockReset()
  endpointMocks.updateProvider.mockReset()
  endpointMocks.revealEndpointKey.mockReset()
  endpointMocks.exportKey.mockReset()
  endpointMocks.deleteEndpointKey.mockReset()
  endpointMocks.updateProviderKey.mockReset()
  endpointMocks.refreshProviderQuota.mockReset()
  endpointMocks.resetProviderKeyCycleStats.mockReset()
  endpointMocks.refreshProviderOAuth.mockReset()
  endpointMocks.recoverKeyHealth.mockReset()
  toastMocks.success.mockClear()
  toastMocks.error.mockClear()
  toastMocks.warning.mockClear()

  endpointMocks.getPoolSchedulingPresets.mockResolvedValue([])
  endpointMocks.clearPoolCooldown.mockResolvedValue({ message: 'ok' })
  endpointMocks.refreshProviderQuota.mockResolvedValue({ success: 0, failed: 0 })
  endpointMocks.resetProviderKeyCycleStats.mockResolvedValue({ message: '已重置周期统计', reset_at: 123, windows: 2 })
})

afterEach(() => {
  for (const { app, root } of mountedApps.splice(0)) {
    app.unmount()
    root.remove()
  }
})

describe('PoolManagement Codex cycle stats mode', () => {
  it('renders Nous credits, monthly spend, balance, period, and configured rate-limit labels', async () => {
    const nousKey = createPoolKey('nous', {
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'available', blocked: false },
        quota: {
          code: 'ok',
          provider_type: 'nous',
          exhausted: false,
          plan_type: 'tier_5',
          observed_at: 1_750_000_000,
          current_period_end: 1_850_000_000,
          balance_usd: '12.50',
          purchased_credits_remaining: '8',
          total_usable_credits: '68',
          billing_available: true,
          rate_limits: { rpm: 50, tpm: 500000, rph: 2100, tph: 6000000, kind: 'configured_limits' },
          windows: [
            { code: 'subscription_credits', scope: 'account', unit: 'count', limit_value: '100.00', used_value: '40.00', remaining_value: '60.00', remaining_ratio: '0.6000', reset_at: 1_850_000_000 },
            { code: 'monthly_spend', scope: 'billing', unit: 'usd', limit_value: '20.00', used_value: '5.00', remaining_value: '15.00', remaining_ratio: '0.7500' },
          ],
        },
      },
    })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('nous')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(nousKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('nous'))

    const root = mountPoolManagement()
    await settle()

    expect(root.textContent).toContain('订阅 Credits')
    expect(root.textContent).toContain('60/100')
    expect(root.textContent).toContain('本月消费')
    expect(root.textContent).toContain('$5/$20')
    expect(root.textContent).toContain('可用 Credits 68')
    expect(root.textContent).toContain('购买 Credits 8')
    expect(root.textContent).toContain('余额 $12.5')
    expect(root.textContent).toContain('速率上限 RPM 50 / TPM 500,000 / RPH 2,100 / TPH 6,000,000')
    expect(root.textContent).not.toContain('RPM 剩余')
  })

  it('keeps Nous account quota visible when billing data is degraded and shows 429 recovery', async () => {
    const nousKey = createPoolKey('nous', {
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'available', blocked: false },
        quota: {
          code: 'cooldown',
          provider_type: 'nous',
          exhausted: false,
          billing_available: false,
          billing_stale: true,
          billing_source: 'preserved_snapshot',
          balance_usd: '7.25',
          windows: [
            { code: 'subscription_credits', scope: 'account', limit_value: '100.00', remaining_value: '25.00', remaining_ratio: '0.2500' },
            { code: 'monthly_spend', scope: 'billing', unit: 'usd', limit_value: '20.00', used_value: '12.00', remaining_value: '8.00', remaining_ratio: '0.4000' },
            { code: 'rate_limit', scope: 'account', is_exhausted: true, reset_seconds: 120 },
          ],
        },
      },
    })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('nous')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(nousKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('nous'))

    const root = mountPoolManagement()
    await settle()

    expect(root.textContent).toContain('订阅 Credits')
    expect(root.textContent).toContain('25/100')
    expect(root.textContent).toContain('本月消费')
    expect(root.textContent).toContain('$12/$20')
    expect(root.textContent).toContain('余额 $7.25')
    expect(root.textContent).toContain('账单数据为上次快照')
    expect(root.textContent).not.toContain('账单信息暂不可用')
    expect(root.textContent).toContain('临时限流')
    expect(root.textContent).toContain('429')
  })

  it('renders current-cycle comparison text without a mode toggle', async () => {
    const codexKey = createPoolKey('codex')
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(codexKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))

    const root = mountPoolManagement()
    await settle()

    expect(root.querySelector('[data-testid="pool-stats-mode-control"]')).toBeNull()
    expect(root.querySelector('[data-testid="pool-stats-cycle-text"]')).not.toBeNull()
    expect(root.querySelector('[data-testid="pool-stats-cycle-request_count"]')?.textContent?.trim()).toBe('7/12')
    expect(root.querySelector('[data-testid="pool-stats-cycle-total_tokens"]')?.textContent?.trim()).toBe('2.5K/5K')
    expect(root.querySelector('[data-testid="pool-stats-cycle-small-overlay"]')).toBeNull()
    expect(root.querySelector('[data-testid="pool-stats-cycle-large-base"]')).toBeNull()
    expect(endpointMocks.listPoolKeys).toHaveBeenLastCalledWith(
      'codex-provider',
      expect.objectContaining({
        sort_by: 'imported_at',
        sort_order: 'desc',
      }),
      expect.anything(),
    )
    expect(root.textContent).not.toContain('累计')
    expect(root.textContent).not.toContain('总计')
  })

  it('renders unified pool score in the key list with a calculation entry point', async () => {
    const scoredKey = createPoolKey('codex', {
      pool_score: {
        id: 'pms-account-score',
        capability: 'account',
        scope_kind: 'account',
        scope_id: null,
        score: 0.875,
        hard_state: 'available',
        score_version: 1,
        score_reason: { weights: { manual_priority: 0.3 } },
        last_ranked_at: 1_700_000_000,
        last_scheduled_at: 1_700_000_010,
        last_success_at: 1_700_000_020,
        last_failure_at: null,
        failure_count: 0,
        last_probe_attempt_at: 1_700_000_030,
        last_probe_success_at: 1_700_000_040,
        last_probe_failure_at: null,
        probe_failure_count: 0,
        probe_status: 'ok',
        updated_at: 1_700_000_050,
      },
    })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(scoredKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))

    const root = mountPoolManagement()
    await settle()

    expect(root.textContent).toContain('0.875')
    expect(root.querySelectorAll('button[title="查看评分计算结果"]').length).toBeGreaterThan(0)
  })

  it('shows ChatGPT Web image quota reset countdown above the quota bar', async () => {
    const chatgptWebKey = createPoolKey('chatgpt_web', {
      api_formats: ['openai:image'],
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'chatgpt_web',
          updated_at: 1_700_000_000,
          windows: [
            {
              code: 'image_gen',
              label: '生图',
              scope: 'account',
              remaining_ratio: 0.96,
              remaining_value: 24,
              limit_value: 25,
              reset_seconds: 3600,
            },
          ],
        },
      },
    })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('chatgpt_web')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(chatgptWebKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('chatgpt_web', {
      api_formats: ['openai:image'],
    }))

    const root = mountPoolManagement()
    await settle()

    const resetTexts = Array.from(root.querySelectorAll('[data-testid="pool-quota-reset-text"]'))
      .map((element) => element.textContent?.trim())
      .filter(Boolean)
    expect(resetTexts).toContain('1h')
    expect(root.textContent).toContain('生图')
  })

  it('labels Codex quota by the actual refresh window duration', async () => {
    const monthlyCodexKey = createPoolKey('codex', {
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          windows: [
            {
              code: 'weekly',
              remaining_ratio: 0.86,
              window_minutes: 43_800,
              usage: { request_count: 23, total_tokens: 45_600, total_cost_usd: '0.1234' },
            },
            {
              code: '5h',
              remaining_ratio: 1,
              window_minutes: 0,
            },
          ],
        },
      },
    })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(monthlyCodexKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))

    const root = mountPoolManagement()
    await settle()

    const periodLabels = Array.from(root.querySelectorAll('[data-testid="pool-quota-period-label"]'))
      .map((element) => element.textContent?.trim())
      .filter(Boolean)
    expect(periodLabels).toContain('月')
    expect(periodLabels).not.toContain('5H')
    expect(periodLabels).not.toContain('周')
    expect(root.querySelector('[data-testid="pool-stats-cycle-request_count"]')?.textContent?.trim()).toBe('-/23')
    expect(root.querySelector('[data-testid="pool-stats-cycle-small-overlay"]')).toBeNull()
    expect(root.querySelector('[data-testid="pool-stats-cycle-bar-request_count"]')).toBeNull()
    expect(root.querySelector('[data-testid="pool-stats-cycle-single-marker"]')).toBeNull()
  })

  it('renders separate account and model weekly quotas in desktop and mobile layouts', async () => {
    const codexKey = createPoolKey('codex', {
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          windows: [
            {
              code: 'weekly',
              label: '周',
              scope: 'account',
              remaining_ratio: 0.9,
              window_minutes: 10_080,
              usage: { request_count: 12 },
            },
            {
              code: 'additional_0_primary',
              label: 'gpt-reserve',
              scope: 'model',
              model: 'gpt-reserve',
              remaining_ratio: 0.4,
              window_minutes: 10_080,
              usage: { request_count: 99 },
            },
          ],
        },
      },
    })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(codexKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))

    const root = mountPoolManagement()
    await settle()

    const panels = root.querySelectorAll('[data-testid="pool-quota-rows"]')
    expect(panels).toHaveLength(2)
    for (const panel of panels) {
      const rows = Array.from(panel.children).map(row => ({
        label: row.querySelector('[data-testid="pool-quota-period-label"]')?.textContent?.trim(),
        meter: row.querySelector('[data-testid="pool-quota-meter-text"]')?.textContent?.trim(),
      }))
      expect(rows).toHaveLength(2)
      expect(rows).toEqual(expect.arrayContaining([
        { label: '周', meter: '90.0%' },
        { label: 'gpt-reserve 周', meter: '40.0%' },
      ]))
    }
    expect(root.querySelector('[data-testid="pool-stats-cycle-request_count"]')?.textContent?.trim()).toBe('-/12')
  })

  it('opens only one score popover across desktop and mobile layouts', async () => {
    const scoredKey = createPoolKey('codex', {
      pool_score: {
        id: 'pms-account-score',
        capability: 'account',
        scope_kind: 'account',
        scope_id: null,
        score: 0.662,
        hard_state: 'available',
        score_version: 1,
        score_reason: {
          rules: {
            probe_failure_penalty: 0.05,
          },
        },
        last_ranked_at: 1_700_000_000,
        last_scheduled_at: null,
        last_success_at: null,
        last_failure_at: null,
        failure_count: 0,
        last_probe_attempt_at: null,
        last_probe_success_at: null,
        last_probe_failure_at: null,
        probe_failure_count: 0,
        probe_status: 'ok',
        updated_at: 1_700_000_050,
      },
    })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(scoredKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))

    const root = mountPoolManagement()
    await settle()

    const helpButtons = root.querySelectorAll<HTMLButtonElement>('button[title="查看评分计算结果"]')
    expect(helpButtons.length).toBe(2)

    helpButtons[0]?.click()
    await settle()

    expect(root.querySelectorAll('pre').length).toBe(1)
    expect(root.textContent).toContain('评分计算结果')
    expect(root.textContent).toContain('0.662')
  })

  it('refreshes quota only for keys on the current page', async () => {
    const pageKeys = [
      createPoolKey('codex', { key_id: 'codex-page-key-1', quota_updated_at: null }),
      createPoolKey('codex', { key_id: 'codex-page-key-2', quota_updated_at: null }),
    ]
    endpointMocks.getPoolOverview.mockResolvedValue({
      items: [{ ...createOverview('codex'), total_keys: 120 }],
    })
    endpointMocks.listPoolKeys.mockResolvedValue({
      total: 120,
      page: 1,
      page_size: 50,
      keys: pageKeys,
    })
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))
    endpointMocks.refreshProviderQuota.mockResolvedValue({
      success: 2,
      failed: 0,
      total: 2,
      results: [],
    })

    const root = mountPoolManagement()
    await settle()

    const refreshButton = root.querySelector('button[title="刷新数据和额度"]') as HTMLButtonElement | null
    expect(refreshButton).not.toBeNull()
    refreshButton?.click()
    await settle()

    expect(endpointMocks.refreshProviderQuota).toHaveBeenCalledTimes(1)
    expect(endpointMocks.refreshProviderQuota).toHaveBeenCalledWith(
      'codex-provider',
      ['codex-page-key-1', 'codex-page-key-2'],
    )
    expect(endpointMocks.refreshProviderQuota).not.toHaveBeenCalledWith('codex-provider')
  })

  it('enables quota refresh for Nous providers', async () => {
    const nousKey = createPoolKey('nous', { key_id: 'nous-key-1', quota_updated_at: null })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('nous')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(nousKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('nous'))

    const root = mountPoolManagement()
    await settle()

    const refreshButton = root.querySelector('button[title="刷新数据和额度"]') as HTMLButtonElement | null
    expect(refreshButton).not.toBeNull()
    refreshButton?.click()
    await settle()

    expect(endpointMocks.refreshProviderQuota).toHaveBeenCalledWith(
      'nous-provider',
      ['nous-key-1'],
    )
  })

  it('ignores legacy account-total mode and removes it from the route', async () => {
    window.sessionStorage.setItem(
      POOL_MANAGEMENT_VIEW_STORAGE_KEY,
      JSON.stringify({ statsMode: 'account_total' }),
    )
    routeMocks.query.statsMode = 'account_total'
    const codexKey = createPoolKey('codex')
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(codexKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))

    const root = mountPoolManagement()
    await settle()

    expect(root.querySelector('[data-testid="pool-stats-mode-control"]')).toBeNull()
    expect(root.querySelector('[data-testid="pool-stats-cycle-text"]')).not.toBeNull()
    expect(root.querySelector('[data-testid="pool-stats-account-total"]')).toBeNull()
    expect(routeMocks.query.statsMode).toBeUndefined()
  })

  it('resets Codex cycle stats from the action column', async () => {
    const codexKey = createPoolKey('codex')
    const resetKey = createPoolKey('codex', {
      status_snapshot: {
        oauth: codexKey.status_snapshot?.oauth ?? { code: 'none' },
        account: codexKey.status_snapshot?.account ?? { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          ...codexKey.status_snapshot?.quota,
          windows: [
            {
              code: '5h',
              scope: 'account',
              window_minutes: 300,
              usage_reset_at: 123,
              usage: { request_count: 0, total_tokens: 0, total_cost_usd: '0.00000000' },
            },
            {
              code: 'spark_preview',
              scope: 'account',
              window_minutes: 300,
              usage: { request_count: 12, total_tokens: 500, total_cost_usd: '0.01' },
            },
            {
              code: 'model_window',
              scope: 'model',
              window_minutes: 300,
              usage: { request_count: 9, total_tokens: 400, total_cost_usd: '0.02' },
            },
            {
              code: 'lifetime',
              scope: 'account',
              window_minutes: 0,
              usage: { request_count: 8, total_tokens: 300, total_cost_usd: '0.03' },
            },
          ],
        },
      },
    })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys
      .mockResolvedValueOnce(createKeyPage(codexKey))
      .mockResolvedValue(createKeyPage(resetKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))

    const root = mountPoolManagement()
    await settle()

    const resetButton = root.querySelector<HTMLButtonElement>('[data-testid="pool-reset-cycle-stats"]')
    expect(resetButton).not.toBeNull()

    resetButton?.click()
    await settle()

    expect(endpointMocks.resetProviderKeyCycleStats).toHaveBeenCalledWith(codexKey.key_id)
    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(2)
    expect(root.querySelector('[data-testid="pool-stats-cycle-request_count"]')?.textContent?.trim()).toBe('-/0')
  })

  it('toggles a pool account and silently revalidates the current key page', async () => {
    const inactiveKey = createPoolKey('codex', {
      is_active: false,
      cooldown_reason: 'rate_limited_429',
      cooldown_ttl_seconds: 60,
      scheduling_status: 'blocked',
      scheduling_reason: 'inactive',
      scheduling_label: '已禁用',
      scheduling_reasons: [{
        code: 'inactive',
        label: '已禁用',
        blocking: true,
        source: 'manual',
      }],
    })
    const enabledKey = createPoolKey('codex', {
      is_active: true,
      cooldown_reason: 'rate_limited_429',
      cooldown_ttl_seconds: 60,
      scheduling_status: 'degraded',
      scheduling_reason: 'cooldown',
      scheduling_label: '冷却中',
      scheduling_reasons: [{
        code: 'cooldown',
        label: '冷却中',
        blocking: true,
        source: 'pool',
        ttl_seconds: 60,
        detail: 'rate_limited_429',
      }],
    })
    endpointMocks.getPoolOverview.mockResolvedValue({
      items: [{ ...createOverview('codex'), active_keys: 0 }],
    })
    endpointMocks.listPoolKeys
      .mockResolvedValueOnce(createKeyPage(inactiveKey))
      .mockResolvedValue(createKeyPage(enabledKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex', {
      total_keys: 1,
      active_keys: 0,
    }))
    endpointMocks.updateProviderKey.mockResolvedValue({ ...inactiveKey, is_active: true })

    const root = mountPoolManagement()
    await settle()

    const toggleButton = root.querySelector<HTMLButtonElement>(
      `[data-testid="pool-toggle-active-desktop-${inactiveKey.key_id}"]`,
    )
    expect(toggleButton).not.toBeNull()
    expect(toggleButton?.getAttribute('aria-label')).toBe('启用账号')
    const listCallsBeforeToggle = endpointMocks.listPoolKeys.mock.calls.length

    toggleButton?.click()
    await settle()

    expect(endpointMocks.updateProviderKey).toHaveBeenCalledWith(inactiveKey.key_id, { is_active: true })
    expect(toggleButton?.getAttribute('aria-label')).toBe('禁用账号')
    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(listCallsBeforeToggle + 1)
    expect(root.textContent).toContain('冷却中')
  })

  it('revalidates the pool page when OAuth refresh fails after server-side invalidation', async () => {
    const oauthKey = createPoolKey('codex', {
      auth_type: 'oauth',
      oauth_managed: true,
      can_refresh_oauth: true,
      status_snapshot: {
        account: { code: 'ok', blocked: false },
        quota: { code: 'ok', exhausted: false },
        oauth: { code: 'expired', expires_at: 1 },
      },
    })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys
      .mockResolvedValueOnce(createKeyPage(oauthKey))
      .mockResolvedValue({ total: 0, page: 1, page_size: 50, keys: [] })
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))
    endpointMocks.refreshProviderOAuth.mockRejectedValue(new Error('OAuth invalid'))

    const root = mountPoolManagement()
    await settle()

    const refreshButton = Array.from(root.querySelectorAll<HTMLButtonElement>('button'))
      .find(button => button.title === '重新授权')
    expect(refreshButton).not.toBeUndefined()
    refreshButton?.click()
    await settle()

    expect(endpointMocks.refreshProviderOAuth).toHaveBeenCalledWith(oauthKey.key_id)
    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(2)
    expect(root.textContent).not.toContain(oauthKey.key_name)
  })

  it('hides the stats mode switch for non-Codex providers and keeps account totals', async () => {
    const customKey = createPoolKey('custom', {
      request_count: 12,
      total_tokens: 3456,
      total_cost_usd: '1.25',
    })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('custom')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(customKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('custom'))

    const root = mountPoolManagement()
    await settle()

    expect(root.querySelector('[data-testid="pool-stats-mode-control"]')).toBeNull()
    expect(root.querySelector('[data-testid="pool-reset-cycle-stats"]')).toBeNull()
    expect(root.querySelector('[data-testid="pool-stats-cycle-text"]')).toBeNull()
    expect(root.querySelector('[data-testid="pool-stats-account-total"]')).not.toBeNull()
    expect(root.textContent).toContain('12')
    expect(root.textContent).toContain('3.5K')
    expect(root.textContent).toContain('$1.25')
  })

  it('supports page selection across desktop/mobile rows and seeds batch actions', async () => {
    const firstKey = createPoolKey('codex', { key_id: 'codex-selection-1', key_name: 'First key' })
    const secondKey = createPoolKey('codex', { key_id: 'codex-selection-2', key_name: 'Second key' })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys.mockResolvedValue({
      total: 2,
      page: 1,
      page_size: 50,
      keys: [firstKey, secondKey],
    })
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))

    const root = mountPoolManagement()
    await settle()

    let pageCheckbox = root.querySelector<HTMLInputElement>('[data-testid="pool-select-page-desktop"]')
    const firstDesktopCheckbox = root.querySelector<HTMLInputElement>('[data-testid="pool-select-desktop-codex-selection-1"]')
    const firstMobileCheckbox = root.querySelector<HTMLInputElement>('[data-testid="pool-select-mobile-codex-selection-1"]')
    expect(pageCheckbox).not.toBeNull()
    expect(firstDesktopCheckbox).not.toBeNull()
    expect(firstMobileCheckbox).not.toBeNull()
    expect(root.querySelector('[data-testid="pool-selected-count-desktop"]')).toBeNull()
    expect(root.querySelector('[data-testid="pool-selected-count-mobile"]')).toBeNull()
    expect(root.querySelector<HTMLElement>('colgroup col')?.style.width).toBe('19%')
    expect(root.querySelectorAll('colgroup col')).toHaveLength(8)
    expect(pageCheckbox?.closest('th')?.textContent).toContain('名称')
    expect(firstDesktopCheckbox?.closest('td')?.textContent).toContain('First key')

    firstDesktopCheckbox?.click()
    await settle()

    expect(firstDesktopCheckbox?.checked).toBe(true)
    expect(firstMobileCheckbox?.checked).toBe(true)
    expect(pageCheckbox?.dataset.indeterminate).toBe('true')
    const desktopSelectedCount = root.querySelector('[data-testid="pool-selected-count-desktop"]')
    expect(desktopSelectedCount?.textContent).toContain('已选 1 个')
    expect(desktopSelectedCount?.closest('th')).toBe(pageCheckbox?.closest('th'))
    expect(root.querySelector('[data-testid="pool-selected-count-mobile"]')?.textContent).toContain('已选 1 个')
    expect(root.querySelector('[data-testid="pool-selection-toolbar"]')).toBeNull()
    expect(root.querySelector<HTMLButtonElement>('[data-testid="pool-batch-actions-desktop"]')?.disabled).toBe(false)

    pageCheckbox?.click()
    await settle()
    expect(root.querySelector<HTMLInputElement>('[data-testid="pool-select-desktop-codex-selection-2"]')?.checked).toBe(true)
    expect(root.querySelector('[data-testid="pool-selected-count-desktop"]')?.textContent).toContain('已选 2 个')

    pageCheckbox?.click()
    await settle()
    expect(root.querySelector<HTMLButtonElement>('[data-testid="pool-batch-actions-desktop"]')?.disabled).toBe(true)
    expect(root.querySelector('[data-testid="pool-selected-count-desktop"]')).toBeNull()
    expect(root.querySelector('[data-testid="pool-selected-count-mobile"]')).toBeNull()

    pageCheckbox = root.querySelector<HTMLInputElement>('[data-testid="pool-select-page-desktop"]')
    pageCheckbox?.click()
    await settle()
    root.querySelector<HTMLButtonElement>('[data-testid="pool-batch-action-refresh_quota-desktop"]')?.click()
    await settle()

    const batchDialog = root.querySelector('[data-testid="pool-account-batch-dialog"]')
    expect(batchDialog?.getAttribute('data-open')).toBe('true')
    expect(batchDialog?.getAttribute('data-selected-ids')).toBe('codex-selection-1,codex-selection-2')
    expect(batchDialog?.getAttribute('data-select-all-filtered')).toBe('false')
    expect(batchDialog?.getAttribute('data-initial-action')).toBe('refresh_quota')
  })

  it('passes the current table search and status to filtered selection actions', async () => {
    routeMocks.query.providerId = 'codex-provider'
    routeMocks.query.search = 'inactive-account'
    routeMocks.query.status = 'inactive'
    const firstKey = createPoolKey('codex', { key_id: 'codex-filtered-1', key_name: 'inactive-account one' })
    const secondKey = createPoolKey('codex', { key_id: 'codex-filtered-2', key_name: 'inactive-account two' })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys.mockResolvedValue({
      total: 37,
      page: 1,
      page_size: 50,
      keys: [firstKey, secondKey],
    })
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))

    const root = mountPoolManagement()
    await settle()

    const selectAllButton = root.querySelector<HTMLButtonElement>('[data-testid="pool-select-all-desktop"]')
    expect(selectAllButton).not.toBeNull()
    expect(selectAllButton?.title).toBe('全选')
    selectAllButton?.click()
    await settle()

    expect(selectAllButton?.title).toBe('取消全选')
    expect(selectAllButton?.getAttribute('aria-pressed')).toBe('true')
    expect(root.querySelector<HTMLInputElement>('[data-testid="pool-select-desktop-codex-filtered-1"]')?.checked).toBe(true)
    expect(root.querySelector<HTMLInputElement>('[data-testid="pool-select-desktop-codex-filtered-1"]')?.disabled).toBe(true)
    expect(root.querySelector('[data-testid="pool-selected-count-desktop"]')?.textContent).toContain('已选 37 个')
    expect(root.querySelector('[data-testid="pool-selected-count-mobile"]')?.textContent).toContain('已选 37 个')
    const batchButton = root.querySelector<HTMLButtonElement>('[data-testid="pool-batch-actions-desktop"]')
    expect(batchButton?.disabled).toBe(false)
    expect(endpointMocks.createPoolKeySelectionSnapshot).toHaveBeenCalledWith('codex-provider', {
      page: 1,
      page_size: 50,
      search: 'inactive-account',
      status: 'inactive',
      sort_by: 'imported_at',
      sort_order: 'desc',
      expected_total: 37,
      expected_page_key_ids: ['codex-filtered-1', 'codex-filtered-2'],
    })

    selectAllButton?.click()
    await settle()
    expect(selectAllButton?.title).toBe('全选')
    expect(selectAllButton?.getAttribute('aria-pressed')).toBe('false')
    expect(root.querySelector<HTMLInputElement>('[data-testid="pool-select-desktop-codex-filtered-1"]')?.checked).toBe(false)
    expect(root.querySelector('[data-testid="pool-selected-count-desktop"]')).toBeNull()
    expect(batchButton?.disabled).toBe(true)

    selectAllButton?.click()
    await settle()
    root.querySelector<HTMLButtonElement>('[data-testid="pool-batch-action-refresh_quota-desktop"]')?.click()
    await settle()

    const batchDialog = root.querySelector('[data-testid="pool-account-batch-dialog"]')
    expect(batchDialog?.getAttribute('data-select-all-filtered')).toBe('true')
    expect(batchDialog?.getAttribute('data-selected-count')).toBe('37')
    expect(batchDialog?.getAttribute('data-selection-snapshot-id')).toBe('selection-snapshot')
    expect(batchDialog?.getAttribute('data-initial-action')).toBe('refresh_quota')
  })

  it('disables selection while a debounced search is waiting for fresh rows', async () => {
    const key = createPoolKey('codex', { key_id: 'codex-stale-search-row' })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(key))
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))

    const root = mountPoolManagement()
    await settle()

    const rowCheckbox = root.querySelector<HTMLInputElement>('[data-testid="pool-select-desktop-codex-stale-search-row"]')
    expect(rowCheckbox?.disabled).toBe(false)
    const searchInput = root.querySelector<HTMLInputElement>('input[placeholder="搜索账号..."]')
    expect(searchInput).not.toBeNull()
    if (searchInput) {
      searchInput.value = 'fresh-filter'
      searchInput.dispatchEvent(new Event('input', { bubbles: true }))
    }
    await nextTick()

    expect(rowCheckbox?.disabled).toBe(true)
    expect(root.querySelector<HTMLButtonElement>('[data-testid="pool-select-all-desktop"]')?.disabled).toBe(true)
  })

  it('shows adaptive hot pool metrics entry only when probing is enabled', async () => {
    endpointMocks.getPoolOverview.mockResolvedValue({
      items: [{ ...createOverview('codex'), provider_desired_hot: 4, provider_in_flight: 2, provider_ema_in_flight: 1.8 }],
    })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(createPoolKey('codex')))
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex', {
      pool_advanced: {
        probing_enabled: true,
      },
    }))

    const enabledRoot = mountPoolManagement()
    await settle()

    expect(enabledRoot.querySelectorAll('[data-testid="pool-demand-metrics-button"]').length).toBeGreaterThan(0)

    for (const { app, root } of mountedApps.splice(0)) {
      app.unmount()
      root.remove()
    }

    endpointMocks.getProvider.mockResolvedValue(createProvider('codex', {
      pool_advanced: {
        probing_enabled: false,
      },
    }))

    const disabledRoot = mountPoolManagement()
    await settle()

    expect(disabledRoot.querySelector('[data-testid="pool-demand-metrics-button"]')).toBeNull()
  })

  it('offers per-key health refresh on any key, blocks duplicate in-flight clicks, and refreshes list health after success', async () => {
    const degradedPage = createKeyPage(createPoolKey('codex', {
      health_score: 0.7,
      circuit_breaker_open: false,
    }))
    let recovered = false
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys.mockImplementation(async () => (
      recovered
        ? createKeyPage({ ...degradedPage.keys[0], health_score: 1 })
        : degradedPage
    ))
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))

    let resolveRecover: (value: {
      message: string
      details: { health_score: number, circuit_breaker_open: boolean, is_active: boolean }
    }) => void = () => {}
    endpointMocks.recoverKeyHealth.mockImplementation(
      () => new Promise((resolve) => {
        resolveRecover = resolve
      }),
    )

    const root = mountPoolManagement()
    await settle()

    // 健康度 0.7（未低于阈值）也必须出现入口：桌面行与移动卡片各一个。
    const recoverButtons = [...root.querySelectorAll('button')]
      .filter(button => button.title === '刷新健康度（管理恢复，不发起上游探测）')
    expect(recoverButtons.length).toBe(2)
    expect(root.querySelector('[data-testid="pool-key-health"]')?.textContent).toContain('70%')

    recoverButtons[0].click()
    await settle()
    // 请求进行中再次点击不得重复发起恢复。
    recoverButtons[0].click()
    await settle()
    expect(endpointMocks.recoverKeyHealth).toHaveBeenCalledTimes(1)
    expect(endpointMocks.recoverKeyHealth).toHaveBeenCalledWith('codex-key-1')

    recovered = true
    resolveRecover({
      message: 'Key 已恢复',
      details: { health_score: 1, circuit_breaker_open: false, is_active: true },
    })
    await settle()

    // 成功后当前列表健康数据反映恢复结果，并触发列表静默重载。
    expect(root.querySelector('[data-testid="pool-key-health"]')?.textContent).toContain('100%')
    expect(endpointMocks.listPoolKeys.mock.calls.length).toBeGreaterThan(1)
  })
})

describe('PoolManagement per-key quota refresh', () => {
  /** 定位包含指定账号名的桌面表格行。 */
  function desktopRowForKey(root: HTMLElement, keyName: string): HTMLTableRowElement | null {
    return Array.from(root.querySelectorAll('tr'))
      .find(row => row.textContent?.includes(keyName)) ?? null
  }

  it('refreshes only the targeted key and merges the quota result in place', async () => {
    const keyQuotaWindows = [
      { code: '5h', remaining_ratio: 0.8, window_minutes: 300 },
      { code: 'weekly', remaining_ratio: 0.5, window_minutes: 10_080 },
    ]
    const keyA = createPoolKey('codex', {
      key_id: 'codex-key-a',
      key_name: 'Account A',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: { code: 'ok', exhausted: false, provider_type: 'codex', windows: keyQuotaWindows },
      },
    })
    const keyB = createPoolKey('codex', {
      key_id: 'codex-key-b',
      key_name: 'Account B',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: { code: 'ok', exhausted: false, provider_type: 'codex', windows: keyQuotaWindows },
      },
    })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys.mockResolvedValue({ total: 2, page: 1, page_size: 50, keys: [keyA, keyB] })
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))
    endpointMocks.refreshProviderQuota.mockResolvedValue({
      success: 1,
      failed: 0,
      total: 1,
      results: [{
        key_id: 'codex-key-a',
        key_name: 'Account A',
        status: 'success',
        quota_snapshot: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          updated_at: Math.floor(Date.now() / 1000),
          windows: [{ code: '5h', remaining_ratio: 0.2, window_minutes: 300 }],
        },
      }],
    })

    const root = mountPoolManagement()
    await settle()
    const listCallsBeforeRefresh = endpointMocks.listPoolKeys.mock.calls.length

    const rowA = desktopRowForKey(root, 'Account A')
    const rowB = desktopRowForKey(root, 'Account B')
    const refreshButton = rowA?.querySelector<HTMLButtonElement>('[data-testid="pool-quota-refresh"]')
    expect(refreshButton).not.toBeNull()
    refreshButton?.click()
    await settle()

    expect(endpointMocks.refreshProviderQuota).toHaveBeenCalledTimes(1)
    expect(endpointMocks.refreshProviderQuota).toHaveBeenCalledWith('codex-provider', ['codex-key-a'])
    // 结果就地合并：仅目标 Key 的额度更新，兄弟 Key 保持原值，且不触发全页强刷。
    const metersA = Array.from(rowA?.querySelectorAll('[data-testid="pool-quota-meter-text"]') ?? [])
      .map(element => element.textContent?.trim())
    const metersB = Array.from(rowB?.querySelectorAll('[data-testid="pool-quota-meter-text"]') ?? [])
      .map(element => element.textContent?.trim())
    expect(metersA).toEqual(['20.0%'])
    expect(metersB).toEqual(expect.arrayContaining(['80.0%', '50.0%']))
    expect(metersB).toHaveLength(2)
    expect(endpointMocks.listPoolKeys.mock.calls.length).toBe(listCallsBeforeRefresh)
    expect(toastMocks.success).toHaveBeenCalledWith('额度已刷新')
  })

  it('blocks duplicate and sibling refreshes while one per-key quota refresh is in flight', async () => {
    const keyA = createPoolKey('codex', { key_id: 'codex-key-a', key_name: 'Account A' })
    const keyB = createPoolKey('codex', { key_id: 'codex-key-b', key_name: 'Account B' })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys.mockResolvedValue({ total: 2, page: 1, page_size: 50, keys: [keyA, keyB] })
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))

    let resolveRefresh: (value: {
      success: number
      failed: number
      total: number
      results: Array<{ key_id: string, key_name: string, status: string, quota_snapshot?: Record<string, unknown> }>
    }) => void = () => {}
    endpointMocks.refreshProviderQuota.mockImplementation(
      () => new Promise((resolve) => {
        resolveRefresh = resolve
      }),
    )

    const root = mountPoolManagement()
    await settle()
    const listCallsBeforeRefresh = endpointMocks.listPoolKeys.mock.calls.length

    const buttonA = desktopRowForKey(root, 'Account A')
      ?.querySelector<HTMLButtonElement>('[data-testid="pool-quota-refresh"]')
    const buttonB = desktopRowForKey(root, 'Account B')
      ?.querySelector<HTMLButtonElement>('[data-testid="pool-quota-refresh"]')
    expect(buttonA).not.toBeNull()
    expect(buttonB).not.toBeNull()

    buttonA?.click()
    await settle()
    // 加载中的 Key 与兄弟 Key 的入口都进入互斥禁用，重复点击不再发起请求。
    expect(buttonA?.disabled).toBe(true)
    expect(buttonB?.disabled).toBe(true)
    buttonA?.click()
    buttonB?.click()
    await settle()
    expect(endpointMocks.refreshProviderQuota).toHaveBeenCalledTimes(1)

    // 返回带真实窗口的快照，避免额度行退化为空态导致刷新入口被卸载。
    resolveRefresh({
      success: 1,
      failed: 0,
      total: 1,
      results: [{
        key_id: 'codex-key-a',
        key_name: 'Account A',
        status: 'success',
        quota_snapshot: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          windows: [{ code: '5h', remaining_ratio: 0.2, window_minutes: 300 }],
        },
      }],
    })
    await settle()

    // 就地合并会重建行内容，重新定位按钮再断言互斥解除。
    const reenabledButtonA = desktopRowForKey(root, 'Account A')
      ?.querySelector<HTMLButtonElement>('[data-testid="pool-quota-refresh"]')
    const reenabledButtonB = desktopRowForKey(root, 'Account B')
      ?.querySelector<HTMLButtonElement>('[data-testid="pool-quota-refresh"]')
    expect(reenabledButtonA?.disabled).toBe(false)
    expect(reenabledButtonB?.disabled).toBe(false)
    expect(endpointMocks.listPoolKeys.mock.calls.length).toBe(listCallsBeforeRefresh)
  })

  it('warns instead of calling the API while the key quota is still cooling down', async () => {
    const coolingKey = createPoolKey('codex', {
      key_id: 'codex-cooling-key',
      key_name: 'Cooling account',
      quota_updated_at: Math.floor(Date.now() / 1000),
    })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(coolingKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))

    const root = mountPoolManagement()
    await settle()

    const refreshButton = desktopRowForKey(root, 'Cooling account')
      ?.querySelector<HTMLButtonElement>('[data-testid="pool-quota-refresh"]')
    expect(refreshButton).not.toBeNull()
    refreshButton?.click()
    await settle()

    expect(endpointMocks.refreshProviderQuota).not.toHaveBeenCalled()
    expect(toastMocks.warning).toHaveBeenCalledWith(expect.stringContaining('冷却'))
  })

  it('keeps the page-level quota refresh from racing an in-flight per-key refresh', async () => {
    const key = createPoolKey('codex', { key_id: 'codex-key-a', key_name: 'Account A' })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(key))
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))
    let resolveRefresh: (value: unknown) => void = () => {}
    endpointMocks.refreshProviderQuota.mockImplementation(
      () => new Promise((resolve) => {
        resolveRefresh = resolve
      }),
    )

    const root = mountPoolManagement()
    await settle()

    desktopRowForKey(root, 'Account A')
      ?.querySelector<HTMLButtonElement>('[data-testid="pool-quota-refresh"]')
      ?.click()
    await settle()

    const pageRefreshButton = root.querySelector<HTMLButtonElement>('button[title="刷新数据和额度"]')
    pageRefreshButton?.click()
    await settle()

    // 页级刷新不得与单 Key 刷新并发第二个额度请求。
    expect(endpointMocks.refreshProviderQuota).toHaveBeenCalledTimes(1)

    resolveRefresh({ success: 1, failed: 0, total: 1, results: [] })
    await settle()
  })
  it('drops a per-key quota response that lands after switching providers', async () => {
    const codexKey = createPoolKey('codex', { key_id: 'shared-pool-key', key_name: 'Codex account' })
    const geminiKey = createPoolKey('gemini_cli', {
      key_id: 'shared-pool-key',
      key_name: 'Gemini account',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'gemini_cli',
          windows: [{
            code: 'weekly',
            label: '周',
            scope: 'model',
            model: 'gemini-2.5-pro',
            remaining_ratio: 0.5,
            window_minutes: 10_080,
          }],
        },
      },
    })
    routeMocks.query.providerId = 'codex-provider'
    endpointMocks.getPoolOverview.mockResolvedValue({
      items: [createOverview('codex'), createOverview('gemini_cli')],
    })
    endpointMocks.listPoolKeys.mockImplementation(async (providerId: string) => (
      providerId === 'gemini_cli-provider' ? createKeyPage(geminiKey) : createKeyPage(codexKey)
    ))
    endpointMocks.getProvider.mockImplementation(async (id: string) => createProvider(
      id === 'gemini_cli-provider' ? 'gemini_cli' : 'codex',
    ))

    let resolveRefresh: (value: unknown) => void = () => {}
    endpointMocks.refreshProviderQuota.mockImplementation(
      () => new Promise((resolve) => {
        resolveRefresh = resolve
      }),
    )

    const root = mountPoolManagement()
    await settle()

    const codexButtons = root.querySelectorAll<HTMLButtonElement>('[data-testid="pool-quota-refresh"]')
    expect(codexButtons.length).toBe(2)
    codexButtons[0]?.click()
    await settle()
    expect(endpointMocks.refreshProviderQuota).toHaveBeenCalledWith('codex-provider', ['shared-pool-key'])

    routeMocks.query.providerId = 'gemini_cli-provider'
    await settle()
    expect(root.textContent).toContain('Gemini account')

    resolveRefresh({
      success: 1,
      failed: 0,
      total: 1,
      results: [{
        key_id: 'shared-pool-key',
        key_name: 'Codex account',
        status: 'success',
        quota_snapshot: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          windows: [{ code: 'weekly', remaining_ratio: 0.2, window_minutes: 10_080 }],
        },
      }],
    })
    await settle()

    // 旧 Provider 的迟到响应不得写回当前页：Gemini 额度保持原值，Codex 快照不出现。
    const meters = Array.from(root.querySelectorAll('[data-testid="pool-quota-meter-text"]'))
      .map(element => element.textContent?.trim())
    expect(meters).toContain('50.0%')
    expect(meters).not.toContain('20.0%')
    expect(root.textContent).not.toContain('Codex account')
  })

  it('keeps the generic quota header refresh available for official keys without a snapshot', async () => {
    const officialKey = createPoolKey('deepseek', {
      key_id: 'deepseek-key-1',
      key_name: 'DeepSeek account',
      status_snapshot: undefined,
    })
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('deepseek')] })
    endpointMocks.listPoolKeys.mockResolvedValue(createKeyPage(officialKey))
    endpointMocks.getProvider.mockResolvedValue(createProvider('deepseek'))
    endpointMocks.refreshProviderQuota.mockResolvedValue({
      success: 1,
      failed: 0,
      total: 1,
      results: [{
        key_id: 'deepseek-key-1',
        key_name: 'DeepSeek account',
        status: 'success',
        quota_snapshot: {
          provider_type: 'deepseek',
          kind: 'balance',
          code: 'ok',
          exhausted: false,
          balances: [{ unit: 'CNY', available: '12.5' }],
        },
      }],
    })

    const root = mountPoolManagement()
    await settle()

    // 空额度（尚未查询）也是可查询入口：官方结构化路径复用额度卡头部刷新，不额外渲染第二个图标。
    const headerButtons = root.querySelectorAll<HTMLButtonElement>('[data-testid="provider-quota-header-refresh"]')
    expect(headerButtons.length).toBe(2)
    expect(root.querySelector('[data-testid="pool-quota-refresh"]')).toBeNull()
    expect(root.textContent).toContain('尚未查询额度')
    headerButtons[0]?.click()
    await settle()

    expect(endpointMocks.refreshProviderQuota).toHaveBeenCalledWith('deepseek-provider', ['deepseek-key-1'])
    expect(root.textContent).toContain('12.5')
  })
})
