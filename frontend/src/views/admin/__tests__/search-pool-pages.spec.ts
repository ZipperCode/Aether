import { describe, expect, it, vi } from 'vitest'
import SearchPoolDashboard from '../SearchPoolDashboard.vue'
import SearchPoolServiceWorkspace from '../SearchPoolServiceWorkspace.vue'

vi.mock('vue-router', () => ({
  useRoute: () => ({
    params: {
      service: 'tavily',
    },
  }),
  useRouter: () => ({
    push: vi.fn(),
  }),
}))

vi.mock('@/features/search-pool/api', () => ({
  searchPoolApi: {
    listServiceSummaries: vi.fn().mockResolvedValue([]),
    getWorkspace: vi.fn().mockResolvedValue({
      service: 'tavily',
      title: 'Tavily',
      description: '',
      route_summary: {
        title: 'Tavily',
        description: '',
        service_badge: 'Tavily',
        route_label: 'POST /api/search',
        route_path: '/api/search',
      },
      stats: {
        keys_total: 0,
        keys_active: 0,
        keys_inactive: 0,
        tokens_total: 0,
        requests_total: 0,
        requests_success: 0,
        requests_failed: 0,
        requests_today: 0,
        requests_this_month: 0,
        success_rate: 0,
        real_used: 0,
        real_remaining: 0,
        real_limit: 0,
        synced_keys: 0,
        last_synced_at: null,
      },
      usage_examples: {
        base_url: '/api/search',
        curl_examples: [],
      },
      keys: [],
      tokens: [],
    }),
    syncUsage: vi.fn().mockResolvedValue({}),
    listKeys: vi.fn().mockResolvedValue([]),
    createKey: vi.fn().mockResolvedValue({}),
    importKeys: vi.fn().mockResolvedValue({ service: 'tavily', created: 0, keys: [] }),
    toggleKey: vi.fn().mockResolvedValue({}),
    deleteKey: vi.fn().mockResolvedValue({ ok: true }),
    listTokens: vi.fn().mockResolvedValue([]),
    createToken: vi.fn().mockResolvedValue({}),
    updateToken: vi.fn().mockResolvedValue({}),
    deleteToken: vi.fn().mockResolvedValue({ ok: true }),
  },
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
  }),
}))

function runSetupSafely(component: object) {
  const target = component as {
    setup?: (props: Record<string, unknown>, ctx: Record<string, unknown>) => unknown
  }
  if (typeof target.setup !== 'function') {
    throw new Error('component setup is not available')
  }

  target.setup({}, {
    attrs: {},
    slots: {},
    emit: () => undefined,
    expose: () => undefined,
  })
}

describe('search pool admin pages', () => {
  it('mounts search pool dashboard without runtime setup errors', () => {
    expect(() => runSetupSafely(SearchPoolDashboard)).not.toThrow()
  })

  it('mounts search pool workspace page without runtime setup errors', () => {
    expect(() => runSetupSafely(SearchPoolServiceWorkspace)).not.toThrow()
  })
})
