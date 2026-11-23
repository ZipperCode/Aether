import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, nextTick, type App } from 'vue'

import type { ProviderType } from '@/api/endpoints/types'
import type { PoolKeyDetail, PoolKeysPageResponse, PoolOverviewItem } from '@/api/endpoints/pool'
import { formatUnixSecondsWithSeconds } from '@/features/pool/utils/poolKeyStatus'
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
    total_keys: 1,
    active_keys: 1,
    cooldown_count: 0,
    pool_enabled: true,
  }
}

function createProvider(providerType: string, poolAdvanced: Record<string, unknown> = {}) {
  return {
    id: `${providerType}-provider`,
    name: `${providerType} Provider`,
    provider_type: providerType,
    is_active: true,
    pool_advanced: poolAdvanced,
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

beforeEach(() => {
  for (const key of Object.keys(routeMocks.query)) delete routeMocks.query[key]
  window.sessionStorage.clear()
  setI18nLocale('zh-CN')
  vi.clearAllMocks()
  endpointMocks.getPoolSchedulingPresets.mockResolvedValue({ presets: [] })
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

describe('PoolManagement Codex discovery, auto_publish_discovered_models, and Antigravity 403 UI', () => {
  it('renders Codex OAuth discovery pending, failed with guidance, and ready without overriding hard blocks', async () => {
    routeMocks.query.providerId = 'codex-provider'
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.getProvider.mockResolvedValue(createProvider('codex'))
    const page: PoolKeysPageResponse = {
      total: 4,
      page: 1,
      page_size: 50,
      keys: [
        createPoolKey({
          key_id: 'codex-pending',
          key_name: 'Codex Pending',
          auto_fetch_models: false,
          codex_model_catalog_ready: false,
          allowed_models: ['gpt-5'],
          scheduling_status: 'available',
          scheduling_reason: 'available',
        }),
        createPoolKey({
          key_id: 'codex-failed',
          key_name: 'Codex Failed',
          auto_fetch_models: true,
          codex_model_catalog_ready: false,
          last_models_fetch_error: '401 invalid token',
          scheduling_status: 'available',
          scheduling_reason: 'available',
        }),
        createPoolKey({
          key_id: 'codex-ready',
          key_name: 'Codex Ready',
          auto_fetch_models: false,
          codex_model_catalog_ready: true,
          last_models_fetch_at: 1760000200,
          allowed_models: null,
          scheduling_status: 'available',
          scheduling_reason: 'available',
        }),
        createPoolKey({
          key_id: 'codex-banned-ready',
          key_name: 'Codex Banned Ready',
          codex_model_catalog_ready: true,
          scheduling_status: 'available',
          scheduling_reason: 'available',
          status_snapshot: {
            oauth: { code: 'valid' },
            account: { code: 'account_banned', label: '账号封禁', blocked: true },
            quota: { code: 'ok', exhausted: false },
          },
        }),
      ],
    }
    endpointMocks.listPoolKeys.mockResolvedValue(page)

    const root = mountPoolManagement()
    await settle()

    expect(root.querySelector('[data-testid="pool-key-model-fetch-desktop-codex-pending"]')?.textContent?.trim()).toBe('模型待发现')
    expect(root.querySelector('[data-testid="pool-key-model-fetch-hint-desktop-codex-pending"]')?.textContent).toContain('模型权限')

    const failedChip = root.querySelector('[data-testid="pool-key-model-fetch-desktop-codex-failed"]')
    expect(failedChip?.textContent?.trim()).toBe('模型发现失败')
    expect(failedChip?.getAttribute('title')).not.toContain('401 invalid token')
    expect(failedChip?.getAttribute('title')).toContain('请检查授权或代理后在模型权限中刷新模型')
    expect(root.querySelector('[data-testid="pool-key-model-fetch-hint-desktop-codex-failed"]')?.textContent).toContain('请检查授权或代理后在模型权限中刷新模型')

    expect(root.querySelector('[data-testid="pool-key-model-fetch-desktop-codex-ready"]')?.textContent?.trim()).toBe('模型目录就绪')
    expect(root.querySelector('[data-testid="pool-key-model-fetch-hint-desktop-codex-ready"]')).toBeNull()

    expect(root.querySelector('[data-testid="pool-key-model-fetch-desktop-codex-banned-ready"]')?.textContent?.trim()).toBe('模型目录就绪')
    expect(root.querySelector('[data-testid="pool-key-status-badge-desktop-codex-banned-ready"]')?.textContent?.trim()).toBe('账号封禁')
  })

  it('saves and rehydrates auto_publish_discovered_models including explicit false through PoolAdvancedDialog', async () => {
    routeMocks.query.providerId = 'codex-provider'
    let serverPoolAdvanced: Record<string, unknown> = { scheduling_presets: ['lru'] }
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('codex')] })
    endpointMocks.getProvider.mockImplementation(async () => createProvider('codex', serverPoolAdvanced))
    endpointMocks.updateProvider.mockImplementation(async (_id: string, payload: { pool_advanced: Record<string, unknown> }) => {
      serverPoolAdvanced = { ...payload.pool_advanced }
      return createProvider('codex', serverPoolAdvanced)
    })
    endpointMocks.listPoolKeys.mockResolvedValue({ total: 0, page: 1, page_size: 50, keys: [] })

    const root = mountPoolManagement()
    await settle()

    const openAdvancedBtn = [...root.querySelectorAll<HTMLButtonElement>('button')]
      .find(btn => btn.getAttribute('title') === '高级设置')
    expect(openAdvancedBtn).toBeDefined()
    openAdvancedBtn!.click()
    await settle()

    const toggle = document.getElementById('pool-auto-publish-discovered-models')!
    expect(toggle.getAttribute('aria-checked')).toBe('false')
    toggle.click()
    await settle()

    const saveBtn = [...document.body.querySelectorAll<HTMLButtonElement>('button')]
      .find(btn => btn.textContent?.trim() === '保存')!
    saveBtn.click()
    await settle()

    expect(serverPoolAdvanced.auto_publish_discovered_models).toBe(true)

    openAdvancedBtn!.click()
    await settle()
    const reopenedToggle = document.getElementById('pool-auto-publish-discovered-models')!
    expect(reopenedToggle.getAttribute('aria-checked')).toBe('true')
    reopenedToggle.click()
    await settle()

    const saveBtn2 = [...document.body.querySelectorAll<HTMLButtonElement>('button')]
      .find(btn => btn.textContent?.trim() === '保存')!
    saveBtn2.click()
    await settle()

    expect(serverPoolAdvanced.auto_publish_discovered_models).toBe(false)

    openAdvancedBtn!.click()
    await settle()
    expect(document.getElementById('pool-auto-publish-discovered-models')!.getAttribute('aria-checked')).toBe('false')
  })

  it('renders Antigravity 403 推理被拒 cooldown, historical reason after recovery, and verification/ban distinctions', async () => {
    routeMocks.query.providerId = 'antigravity-provider'
    const observedAt = 1760000000
    const retryAt = 1760000300
    endpointMocks.getPoolOverview.mockResolvedValue({ items: [createOverview('antigravity')] })
    endpointMocks.getProvider.mockResolvedValue(createProvider('antigravity'))
    endpointMocks.listPoolKeys.mockResolvedValue({
      total: 4,
      page: 1,
      page_size: 50,
      keys: [
        createPoolKey({
          key_id: 'ag-cooldown',
          key_name: 'AG Cooldown',
          cooldown_reason: 'forbidden_403',
          cooldown_ttl_seconds: 300,
          scheduling_status: 'degraded',
          scheduling_reason: 'cooldown',
          scheduling_label: '冷却中',
          last_inference_rejection: { observed_at: observedAt, retry_at: retryAt },
        }),
        createPoolKey({
          key_id: 'ag-recovered',
          key_name: 'AG Recovered',
          cooldown_reason: null,
          cooldown_ttl_seconds: null,
          scheduling_status: 'available',
          scheduling_reason: 'available',
          scheduling_label: '可用',
          last_inference_rejection: { observed_at: observedAt, retry_at: retryAt },
        }),
        createPoolKey({
          key_id: 'ag-verify',
          key_name: 'AG Verify',
          status_snapshot: {
            oauth: { code: 'valid' },
            account: {
              code: 'account_verification',
              label: '需要验证',
              blocked: true,
              validation_url: 'https://accounts.google.com/ServiceLogin',
            },
            quota: { code: 'ok', exhausted: false },
          },
        }),
        createPoolKey({
          key_id: 'ag-ban',
          key_name: 'AG Ban',
          status_snapshot: {
            oauth: { code: 'valid' },
            account: {
              code: 'account_banned',
              label: '账号封禁',
              blocked: true,
              appeal_url: 'https://support.google.com/accounts/appeal',
            },
            quota: { code: 'ok', exhausted: false },
          },
        }),
      ],
    })

    const root = mountPoolManagement()
    await settle()

    expect(root.querySelector('[data-testid="pool-key-status-badge-desktop-ag-cooldown"]')?.textContent?.trim()).toBe('推理被拒')
    const cooldownNote = root.querySelector('[data-testid="pool-key-antigravity-rejection-desktop-ag-cooldown"]')?.textContent || ''
    expect(cooldownNote).toContain(`最近发生：${formatUnixSecondsWithSeconds(observedAt)}`)
    expect(cooldownNote).toContain(`重试检查：${formatUnixSecondsWithSeconds(retryAt)}`)
    expect(cooldownNote).toContain('5m 0s')
    expect(cooldownNote).not.toContain('账号封禁')

    expect(root.querySelector('[data-testid="pool-key-status-badge-desktop-ag-recovered"]')?.textContent?.trim()).toBe('可用')
    const recoveredNote = root.querySelector('[data-testid="pool-key-antigravity-rejection-desktop-ag-recovered"]')?.textContent || ''
    expect(recoveredNote).toContain('历史：推理被拒')
    expect(recoveredNote).toContain(`最近发生：${formatUnixSecondsWithSeconds(observedAt)}`)
    expect(recoveredNote).toContain(`重试检查：${formatUnixSecondsWithSeconds(retryAt)}`)

    expect(root.querySelector('[data-testid="pool-key-status-badge-desktop-ag-verify"]')?.textContent?.trim()).toBe('需要验证')
    expect(root.querySelector('[data-testid="pool-key-antigravity-rejection-desktop-ag-verify"]')).toBeNull()

    expect(root.querySelector('[data-testid="pool-key-status-badge-desktop-ag-ban"]')?.textContent?.trim()).toBe('账号封禁')
    expect(root.querySelector('[data-testid="pool-key-antigravity-rejection-desktop-ag-ban"]')).toBeNull()
  })
})
