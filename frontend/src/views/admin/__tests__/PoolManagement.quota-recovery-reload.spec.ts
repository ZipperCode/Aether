import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, nextTick, type App } from 'vue'

import type { ProviderType } from '@/api/endpoints/types'
import type { PoolKeyDetail, PoolKeysPageResponse, PoolOverviewItem } from '@/api/endpoints/pool'
import { setI18nLocale } from '@/i18n'
import PoolManagement from '@/views/admin/PoolManagement.vue'

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
  consumeCodexResetCredit: vi.fn(),
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
  consumeCodexResetCredit: endpointMocks.consumeCodexResetCredit,
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
vi.mock('@/stores/proxy-nodes', () => ({
  useProxyNodesStore: () => ({
    nodes: [],
    ensureLoaded: vi.fn(),
  }),
}))
vi.mock('@/composables/useToast', () => ({
  useToast: () => toastMocks,
}))
vi.mock('@/composables/useRouteQuery', async () => {
  const { reactive } = await import('vue')
  routeMocks.query = reactive(routeMocks.query)
  return {
    useRouteQuery: () => ({
      getQueryValue: (key: string) => routeMocks.query[key],
      patchQuery: routeMocks.patchQuery,
    }),
  }
})
vi.mock('@/composables/useClipboard', () => ({
  useClipboard: () => ({ copyToClipboard: vi.fn().mockResolvedValue(undefined) }),
}))
vi.mock('@/composables/useConfirm', () => ({
  useConfirm: () => ({ confirm: vi.fn().mockResolvedValue(true) }),
}))
vi.mock('@/composables/useCountdownTimer', () => ({
  useCountdownTimer: () => ({ tick: { value: 0 }, start: vi.fn() }),
}))
vi.mock('@/features/pool/components/PoolSchedulingDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return { default: defineComponent({ name: 'PoolSchedulingDialogStub', setup: () => () => null }) }
})
vi.mock('@/features/pool/components/PoolAdvancedDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return { default: defineComponent({ name: 'PoolAdvancedDialogStub', setup: () => () => null }) }
})
vi.mock('@/features/pool/components/PoolDemandMetricsDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return { default: defineComponent({ name: 'PoolDemandMetricsDialogStub', setup: () => () => null }) }
})
vi.mock('@/features/pool/components/PoolAccountBatchDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return { default: defineComponent({ name: 'PoolAccountBatchDialogStub', setup: () => () => null }) }
})
vi.mock('@/features/pool/components/PoolKeyBatchEditDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return { default: defineComponent({ name: 'PoolKeyBatchEditDialogStub', setup: () => () => null }) }
})
vi.mock('@/features/providers/components/ProviderFormDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return { default: defineComponent({ name: 'ProviderFormDialogStub', setup: () => () => null }) }
})
vi.mock('@/features/providers/components/KeyAllowedModelsEditDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return { default: defineComponent({ name: 'KeyAllowedModelsEditDialogStub', setup: () => () => null }) }
})
vi.mock('@/features/providers/components/KeyFormDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return { default: defineComponent({ name: 'KeyFormDialogStub', setup: () => () => null }) }
})
vi.mock('@/features/providers/components/OAuthKeyEditDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return { default: defineComponent({ name: 'OAuthKeyEditDialogStub', setup: () => () => null }) }
})
vi.mock('@/features/providers/components/OAuthAccountDialog.vue', async () => {
  const { defineComponent } = await import('vue')
  return { default: defineComponent({ name: 'OAuthAccountDialogStub', setup: () => () => null }) }
})
vi.mock('@/features/providers/components/ProxyNodeSelect.vue', async () => {
  const { defineComponent } = await import('vue')
  return { default: defineComponent({ name: 'ProxyNodeSelectStub', setup: () => () => null }) }
})

const mountedApps: Array<{ app: App, root: HTMLElement }> = []

function createOverview(providerType: ProviderType): PoolOverviewItem {
  return {
    provider_id: `${providerType}-provider`,
    provider_name: `${providerType} Provider`,
    provider_type: providerType,
    total_keys: 60,
    active_keys: 60,
    cooldown_count: 0,
    pool_enabled: true,
  }
}

function createProvider(providerType: string) {
  return {
    id: `${providerType}-provider`,
    name: `${providerType} Provider`,
    provider_type: providerType,
    is_active: true,
    pool_advanced: {},
  }
}

function createPoolKey(overrides: Partial<PoolKeyDetail> = {}): PoolKeyDetail {
  return {
    key_id: 'key-1',
    key_name: 'Key 1',
    is_active: true,
    auth_type: 'oauth',
    account_quota: null,
    cooldown_reason: null,
    cooldown_ttl_seconds: null,
    cost_window_usage: 0,
    cost_limit: null,
    request_count: 0,
    total_tokens: 0,
    total_cost_usd: '0',
    sticky_sessions: 0,
    lru_score: null,
    created_at: null,
    last_used_at: null,
    ...overrides,
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
  for (let i = 0; i < 8; i += 1) {
    await Promise.resolve()
    await nextTick()
  }
}

function desktopRowForKey(root: HTMLElement, keyName: string): HTMLTableRowElement | null {
  return Array.from(root.querySelectorAll<HTMLTableRowElement>('tbody tr'))
    .find(row => row.textContent?.includes(keyName)) ?? null
}

beforeEach(() => {
  for (const key of Object.keys(routeMocks.query)) delete routeMocks.query[key]
  window.sessionStorage.clear()
  setI18nLocale('zh-CN')
  for (const mockFn of Object.values(endpointMocks)) {
    mockFn.mockReset()
  }
  for (const mockFn of Object.values(toastMocks)) {
    mockFn.mockReset()
  }
  routeMocks.patchQuery.mockClear()
  endpointMocks.getPoolSchedulingPresets.mockResolvedValue([])
})

afterEach(() => {
  for (const { app, root } of mountedApps.splice(0)) {
    app.unmount()
    root.remove()
  }
  document.body.innerHTML = ''
  window.sessionStorage.clear()
  setI18nLocale('zh-CN')
})

describe('PoolManagement runtime quota_exhausted authoritative reload', () => {
  it('reloads authoritative page 2 preserving selection after single-key refresh on runtime quota_exhausted Codex row even at 0%', async () => {
    routeMocks.query.providerId = 'codex-provider'
    routeMocks.query.page = '2'

    const blockedCodex = createPoolKey({
      key_id: 'codex-blocked',
      key_name: 'Codex Blocked',
      scheduling_status: 'blocked',
      scheduling_reason: 'key_quota_exhausted',
      scheduling_label: '额度耗尽·需人工恢复',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          windows: [{ code: '5h', remaining_ratio: 0, window_minutes: 300 }],
        },
        scheduling: { code: 'quota_exhausted', blocked: true, requires_manual_recovery: true },
      },
    })
    const recoveredCodex = createPoolKey({
      key_id: 'codex-blocked',
      key_name: 'Codex Blocked',
      scheduling_status: 'available',
      scheduling_reason: 'available',
      scheduling_label: '可用',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          windows: [{ code: '5h', remaining_ratio: 0, window_minutes: 300 }],
        },
        scheduling: null,
      },
    })

    const page2Blocked: PoolKeysPageResponse = { total: 60, page: 2, page_size: 50, keys: [blockedCodex] }
    const page2Recovered: PoolKeysPageResponse = { total: 60, page: 2, page_size: 50, keys: [recoveredCodex] }

    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))
    endpointMocks.listPoolKeys
      .mockResolvedValueOnce(page2Blocked)
      .mockResolvedValueOnce(page2Recovered)
    endpointMocks.refreshProviderQuota.mockResolvedValue({
      success: 1,
      failed: 0,
      total: 1,
      results: [{
        key_id: 'codex-blocked',
        key_name: 'Codex Blocked',
        status: 'success',
        quota_snapshot: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          updated_at: Math.floor(Date.now() / 1000),
          windows: [{ code: '5h', remaining_ratio: 0, window_minutes: 300 }],
        },
      }],
    })

    const root = mountPoolManagement()
    await settle()

    const codexBadge = root.querySelector('[data-testid="pool-key-status-badge-desktop-codex-blocked"]')
    expect(codexBadge?.textContent?.trim()).toBe('额度耗尽')
    expect(codexBadge?.getAttribute('title')).toContain('可刷新额度核对')
    expect(codexBadge?.getAttribute('title')).not.toContain('需人工恢复')

    root.querySelector<HTMLButtonElement>('[data-testid="pool-select-desktop-codex-blocked"]')?.click()
    await settle()
    expect(root.querySelector('[data-testid="pool-selected-count-desktop"]')?.textContent?.trim()).toBe('已选 1 个')

    desktopRowForKey(root, 'Codex Blocked')
      ?.querySelector<HTMLButtonElement>('[data-testid="pool-quota-refresh"]')
      ?.click()
    await settle()

    expect(endpointMocks.refreshProviderQuota).toHaveBeenCalledWith('codex-provider', ['codex-blocked'])
    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(2)
    expect(endpointMocks.listPoolKeys).toHaveBeenLastCalledWith(
      'codex-provider',
      expect.objectContaining({ page: 2, page_size: 50 }),
      { cacheTtlMs: 0 },
    )
    expect(root.querySelector('[data-testid="pool-key-status-badge-desktop-codex-blocked"]')?.textContent?.trim()).toBe('可用')
    expect(root.querySelector('[data-testid="pool-selected-count-desktop"]')?.textContent?.trim()).toBe('已选 1 个')
    expect(routeMocks.query.page).toBe('2')
  })

  it('reloads authoritative page after single-key refresh on runtime quota_exhausted Antigravity row', async () => {
    routeMocks.query.providerId = 'antigravity-provider'

    const blockedAg = createPoolKey({
      key_id: 'ag-blocked',
      key_name: 'AG Blocked',
      scheduling_status: 'blocked',
      scheduling_reason: 'key_quota_exhausted',
      scheduling_label: '额度耗尽·需人工恢复',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'antigravity',
          windows: [{ code: 'claude', label: 'Claude', scope: 'model', remaining_ratio: 0 }],
        },
        scheduling: { code: 'quota_exhausted', blocked: true, requires_manual_recovery: true },
      },
    })
    const recoveredAg = createPoolKey({
      key_id: 'ag-blocked',
      key_name: 'AG Blocked',
      scheduling_status: 'available',
      scheduling_reason: 'available',
      scheduling_label: '可用',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'antigravity',
          windows: [{ code: 'claude', label: 'Claude', scope: 'model', remaining_ratio: 1 }],
        },
        scheduling: null,
      },
    })

    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('antigravity')] })
    endpointMocks.getProvider.mockResolvedValue(createProvider('antigravity'))
    endpointMocks.listPoolKeys
      .mockResolvedValueOnce({ total: 1, page: 1, page_size: 50, keys: [blockedAg] })
      .mockResolvedValueOnce({ total: 1, page: 1, page_size: 50, keys: [recoveredAg] })
    endpointMocks.refreshProviderQuota.mockResolvedValue({
      success: 1,
      failed: 0,
      total: 1,
      results: [{
        key_id: 'ag-blocked',
        key_name: 'AG Blocked',
        status: 'success',
        quota_snapshot: {
          code: 'ok',
          exhausted: false,
          provider_type: 'antigravity',
          windows: [{ code: 'claude', label: 'Claude', scope: 'model', remaining_ratio: 1 }],
        },
      }],
    })

    const root = mountPoolManagement()
    await settle()

    const agBadge = root.querySelector('[data-testid="pool-key-status-badge-desktop-ag-blocked"]')
    expect(agBadge?.textContent?.trim()).toBe('额度耗尽')
    expect(agBadge?.getAttribute('title')).toContain('可刷新额度核对')
    expect(agBadge?.getAttribute('title')).not.toContain('需人工恢复')

    desktopRowForKey(root, 'AG Blocked')
      ?.querySelector<HTMLButtonElement>('[data-testid="pool-quota-refresh"]')
      ?.click()
    await settle()

    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(2)
    expect(root.querySelector('[data-testid="pool-key-status-badge-desktop-ag-blocked"]')?.textContent?.trim()).toBe('可用')
  })

  it('reloads authoritative page 2 preserving selection on page refresh and Codex reset-credit success, while keeping normal unblocked refresh no-reload', async () => {
    routeMocks.query.providerId = 'codex-provider'
    routeMocks.query.page = '2'

    const blockedResetKey = createPoolKey({
      key_id: 'codex-reset-blocked',
      key_name: 'Codex Reset Blocked',
      scheduling_status: 'blocked',
      scheduling_reason: 'key_quota_exhausted',
      scheduling_label: '额度耗尽·需人工恢复',
      upstream_metadata: {
        codex: {
          credential_generation: 'gen-1',
          reset_credits: {
            available_count: 1,
            credits: [{ display_key: '5h', expires_at: Math.floor(Date.now() / 1000) + 3600 }],
          },
        },
      },
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          windows: [{ code: '5h', remaining_ratio: 0, window_minutes: 300 }],
        },
        scheduling: { code: 'quota_exhausted', blocked: true, requires_manual_recovery: true },
      },
    })
    const unblockedResetKey = createPoolKey({
      key_id: 'codex-reset-unblocked',
      key_name: 'Codex Reset Unblocked',
      scheduling_status: 'available',
      scheduling_reason: 'available',
      scheduling_label: '可用',
      upstream_metadata: {
        codex: {
          credential_generation: 'gen-2',
          reset_credits: {
            available_count: 1,
            credits: [{ display_key: '5h', expires_at: Math.floor(Date.now() / 1000) + 3600 }],
          },
        },
      },
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          windows: [{ code: '5h', remaining_ratio: 0.4, window_minutes: 300 }],
        },
      },
    })
    const recoveredResetKey = createPoolKey({
      ...blockedResetKey,
      scheduling_status: 'available',
      scheduling_reason: 'available',
      scheduling_label: '可用',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          windows: [{ code: '5h', remaining_ratio: 1, window_minutes: 300 }],
        },
        scheduling: null,
      },
    })

    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))
    endpointMocks.listPoolKeys
      .mockResolvedValueOnce({ total: 60, page: 2, page_size: 50, keys: [blockedResetKey, unblockedResetKey] })
      .mockResolvedValueOnce({ total: 60, page: 2, page_size: 50, keys: [recoveredResetKey, unblockedResetKey] })
      .mockResolvedValueOnce({ total: 60, page: 2, page_size: 50, keys: [recoveredResetKey, unblockedResetKey] })

    endpointMocks.consumeCodexResetCredit.mockResolvedValue({
      key_id: 'codex-reset-blocked',
      status: 'success',
      outcome: 'reset',
      idempotency_key: 'idem-1',
      refresh_status: 'success',
      quota_snapshot: {
        code: 'ok',
        exhausted: false,
        provider_type: 'codex',
        windows: [{ code: '5h', remaining_ratio: 1, window_minutes: 300 }],
      },
    })
    endpointMocks.refreshProviderQuota.mockResolvedValue({
      success: 2,
      failed: 0,
      total: 2,
      results: [{
        key_id: 'codex-reset-blocked',
        key_name: 'Codex Reset Blocked',
        status: 'success',
        quota_snapshot: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          windows: [{ code: '5h', remaining_ratio: 1, window_minutes: 300 }],
        },
      }],
    })

    const root = mountPoolManagement()
    await settle()

    root.querySelector<HTMLButtonElement>('[data-testid="pool-select-desktop-codex-reset-unblocked"]')?.click()
    await settle()
    expect(root.querySelector('[data-testid="pool-selected-count-desktop"]')?.textContent?.trim()).toBe('已选 1 个')

    // 1. Reset-credit success on runtime quota_exhausted row reloads authoritative page 2 and preserves selection
    desktopRowForKey(root, 'Codex Reset Blocked')
      ?.querySelector<HTMLButtonElement>('[data-testid="pool-quota-reset-credits"] button')
      ?.click()
    await settle()

    expect(endpointMocks.consumeCodexResetCredit).toHaveBeenCalledTimes(1)
    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(2)
    expect(endpointMocks.listPoolKeys).toHaveBeenLastCalledWith(
      'codex-provider',
      expect.objectContaining({ page: 2, page_size: 50 }),
      { cacheTtlMs: 0 },
    )
    expect(root.querySelector('[data-testid="pool-key-status-badge-desktop-codex-reset-blocked"]')?.textContent?.trim()).toBe('可用')
    expect(root.querySelector('[data-testid="pool-selected-count-desktop"]')?.textContent?.trim()).toBe('已选 1 个')

    // 2. Normal unblocked reset-credit success remains no-reload
    endpointMocks.consumeCodexResetCredit.mockResolvedValueOnce({
      key_id: 'codex-reset-unblocked',
      status: 'success',
      outcome: 'reset',
      idempotency_key: 'idem-2',
      refresh_status: 'success',
      quota_snapshot: {
        code: 'ok',
        exhausted: false,
        provider_type: 'codex',
        windows: [{ code: '5h', remaining_ratio: 1, window_minutes: 300 }],
      },
    })
    desktopRowForKey(root, 'Codex Reset Unblocked')
      ?.querySelector<HTMLButtonElement>('[data-testid="pool-quota-reset-credits"] button')
      ?.click()
    await settle()

    expect(endpointMocks.consumeCodexResetCredit).toHaveBeenCalledTimes(2)
    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(2)

    // 3. Page refresh preserves page 2 and selection
    root.querySelector<HTMLButtonElement>('button[title="刷新数据和额度"]')?.click()
    await settle()

    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(3)
    expect(endpointMocks.listPoolKeys).toHaveBeenLastCalledWith(
      'codex-provider',
      expect.objectContaining({ page: 2, page_size: 50 }),
      { cacheTtlMs: 0 },
    )
    expect(root.querySelector('[data-testid="pool-selected-count-desktop"]')?.textContent?.trim()).toBe('已选 1 个')
    expect(routeMocks.query.page).toBe('2')
  })

  it('guards current provider so late quota refresh or reset-credit responses do not reload or mutate after switching providers', async () => {
    routeMocks.query.providerId = 'codex-provider'

    const blockedCodex = createPoolKey({
      key_id: 'shared-key',
      key_name: 'Codex Blocked',
      scheduling_status: 'blocked',
      scheduling_reason: 'key_quota_exhausted',
      scheduling_label: '额度耗尽·需人工恢复',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          windows: [{ code: '5h', remaining_ratio: 0, window_minutes: 300 }],
        },
        scheduling: { code: 'quota_exhausted', blocked: true },
      },
    })
    const agKey = createPoolKey({
      key_id: 'shared-key',
      key_name: 'AG Healthy',
      scheduling_status: 'available',
      scheduling_reason: 'available',
      scheduling_label: '可用',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'antigravity',
          windows: [{ code: 'claude', label: 'Claude', scope: 'quota_group', remaining_ratio: 0.8 }],
        },
      },
    })

    endpointMocks.getPoolOverview.mockResolvedValue({
      items: [createOverview('codex'), createOverview('antigravity')],
    })
    endpointMocks.getProvider.mockImplementation(async (id: string) => createProvider(
      id === 'antigravity-provider' ? 'antigravity' : 'codex',
    ))
    endpointMocks.listPoolKeys.mockImplementation(async (providerId: string) => ({
      total: 1,
      page: 1,
      page_size: 50,
      keys: [providerId === 'antigravity-provider' ? agKey : blockedCodex],
    }))

    let resolveRefresh: (value: unknown) => void = () => {}
    endpointMocks.refreshProviderQuota.mockImplementation(
      () => new Promise((resolve) => {
        resolveRefresh = resolve
      }),
    )

    const root = mountPoolManagement()
    await settle()
    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(1)

    desktopRowForKey(root, 'Codex Blocked')
      ?.querySelector<HTMLButtonElement>('[data-testid="pool-quota-refresh"]')
      ?.click()
    await settle()

    routeMocks.query.providerId = 'antigravity-provider'
    await settle()
    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(2)
    expect(root.textContent).toContain('AG Healthy')

    resolveRefresh({
      success: 1,
      failed: 0,
      total: 1,
      results: [{
        key_id: 'shared-key',
        key_name: 'Codex Blocked',
        status: 'success',
        quota_snapshot: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          windows: [{ code: '5h', remaining_ratio: 0.1, window_minutes: 300 }],
        },
      }],
    })
    await settle()

    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(2)
    expect(toastMocks.success).not.toHaveBeenCalled()
    expect(root.querySelector('[data-testid="pool-quota-meter-text"]')?.textContent?.trim()).toBe('80.0%')
  })

  it('keeps normal unblocked single-key refresh no-reload and guards late reset-credit responses after switching providers', async () => {
    routeMocks.query.providerId = 'codex-provider'

    const unblockedCodex = createPoolKey({
      key_id: 'codex-unblocked',
      key_name: 'Codex Unblocked',
      scheduling_status: 'available',
      scheduling_reason: 'available',
      scheduling_label: '可用',
      upstream_metadata: {
        codex: {
          credential_generation: 'gen-1',
          reset_credits: {
            available_count: 1,
            credits: [{ display_key: '5h', expires_at: Math.floor(Date.now() / 1000) + 3600 }],
          },
        },
      },
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          windows: [{ code: '5h', remaining_ratio: 0.5, window_minutes: 300 }],
        },
      },
    })
    const agKey = createPoolKey({
      key_id: 'codex-unblocked',
      key_name: 'AG Healthy',
      scheduling_status: 'available',
      scheduling_reason: 'available',
      scheduling_label: '可用',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          code: 'ok',
          exhausted: false,
          provider_type: 'antigravity',
          windows: [{ code: 'claude', label: 'Claude', scope: 'quota_group', remaining_ratio: 0.8 }],
        },
      },
    })

    endpointMocks.getPoolOverview.mockResolvedValue({
      items: [createOverview('codex'), createOverview('antigravity')],
    })
    endpointMocks.getProvider.mockImplementation(async (id: string) => createProvider(
      id === 'antigravity-provider' ? 'antigravity' : 'codex',
    ))
    endpointMocks.listPoolKeys.mockImplementation(async (providerId: string) => ({
      total: 1,
      page: 1,
      page_size: 50,
      keys: [providerId === 'antigravity-provider' ? agKey : unblockedCodex],
    }))
    endpointMocks.refreshProviderQuota.mockResolvedValue({
      success: 1,
      failed: 0,
      total: 1,
      results: [{
        key_id: 'codex-unblocked',
        key_name: 'Codex Unblocked',
        status: 'success',
        quota_snapshot: {
          code: 'ok',
          exhausted: false,
          provider_type: 'codex',
          windows: [{ code: '5h', remaining_ratio: 0.9, window_minutes: 300 }],
        },
      }],
    })

    let resolveReset: (value: unknown) => void = () => {}
    endpointMocks.consumeCodexResetCredit.mockImplementation(
      () => new Promise((resolve) => {
        resolveReset = resolve
      }),
    )

    const root = mountPoolManagement()
    await settle()
    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(1)

    // Normal unblocked single-key refresh updates in place without reloading listPoolKeys
    desktopRowForKey(root, 'Codex Unblocked')
      ?.querySelector<HTMLButtonElement>('[data-testid="pool-quota-refresh"]')
      ?.click()
    await settle()

    expect(endpointMocks.refreshProviderQuota).toHaveBeenCalledTimes(1)
    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(1)
    expect(root.querySelector('[data-testid="pool-quota-meter-text"]')?.textContent?.trim()).toBe('90.0%')

    // Start reset-credit, then switch provider before it resolves
    toastMocks.success.mockClear()
    desktopRowForKey(root, 'Codex Unblocked')
      ?.querySelector<HTMLButtonElement>('[data-testid="pool-quota-reset-credits"] button')
      ?.click()
    await settle()

    routeMocks.query.providerId = 'antigravity-provider'
    await settle()
    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(2)
    expect(root.textContent).toContain('AG Healthy')

    resolveReset({
      key_id: 'codex-unblocked',
      status: 'success',
      outcome: 'reset',
      idempotency_key: 'idem-late',
      refresh_status: 'success',
      quota_snapshot: {
        code: 'ok',
        exhausted: false,
        provider_type: 'codex',
        windows: [{ code: '5h', remaining_ratio: 1, window_minutes: 300 }],
      },
    })
    await settle()

    expect(endpointMocks.listPoolKeys).toHaveBeenCalledTimes(2)
    expect(toastMocks.success).not.toHaveBeenCalled()
    expect(root.querySelector('[data-testid="pool-quota-meter-text"]')?.textContent?.trim()).toBe('80.0%')
  })
})
