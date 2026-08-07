import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, type App } from 'vue'
import { createPinia } from 'pinia'

import type { ProviderEndpoint, ProviderWithEndpointsSummary } from '@/api/endpoints'
import EndpointFormDialog from '../EndpointFormDialog.vue'

const endpointMocks = vi.hoisted(() => ({
  createEndpoint: vi.fn(),
  deleteEndpoint: vi.fn(),
  getDefaultBodyRules: vi.fn(),
  updateEndpoint: vi.fn(),
}))

vi.mock('@/api/endpoints', async (importOriginal) => ({
  ...await importOriginal<typeof import('@/api/endpoints')>(),
  ...endpointMocks,
}))

vi.mock('@/api/admin', () => ({
  adminApi: {
    getApiFormats: vi.fn().mockResolvedValue({ formats: [] }),
  },
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
  }),
}))

const mountedApps: Array<{ app: App, root: HTMLElement }> = []

function makeProvider(overrides: Partial<ProviderWithEndpointsSummary> = {}): ProviderWithEndpointsSummary {
  return {
    id: 'provider-1',
    name: 'Claude compatible',
    provider_type: 'custom',
    provider_priority: 100,
    keep_priority_on_conversion: false,
    enable_format_conversion: false,
    is_active: true,
    total_endpoints: 1,
    active_endpoints: 1,
    total_keys: 1,
    active_keys: 1,
    total_models: 1,
    active_models: 1,
    global_model_ids: [],
    avg_health_score: 1,
    unhealthy_endpoints: 0,
    api_formats: ['claude:messages'],
    endpoint_health_details: [],
    ops_configured: false,
    created_at: '2026-08-07T00:00:00Z',
    updated_at: '2026-08-07T00:00:00Z',
    ...overrides,
  }
}

function makeEndpoint(overrides: Partial<ProviderEndpoint> = {}): ProviderEndpoint {
  return {
    id: 'endpoint-1',
    provider_id: 'provider-1',
    provider_name: 'Claude compatible',
    api_format: 'claude:messages',
    base_url: 'https://api.example.com/v1',
    max_retries: 2,
    is_active: true,
    config: {
      upstream_stream_policy: 'force_stream',
      anthropic: {
        compatibility_profile: 'native_transparent',
        supported_operations: ['messages', 'count_tokens'],
      },
    },
    total_keys: 1,
    active_keys: 1,
    created_at: '2026-08-07T00:00:00Z',
    updated_at: '2026-08-07T00:00:00Z',
    ...overrides,
  }
}

async function settle() {
  for (let index = 0; index < 4; index += 1) {
    await Promise.resolve()
    await nextTick()
  }
}

async function mountDialog(provider = makeProvider(), endpoint = makeEndpoint()) {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(defineComponent({
    setup() {
      return () => h(EndpointFormDialog, {
        modelValue: true,
        provider,
        endpoints: [endpoint],
      })
    },
  }))
  app.use(createPinia())
  app.mount(root)
  mountedApps.push({ app, root })
  await settle()
}

beforeEach(() => {
  endpointMocks.createEndpoint.mockReset()
  endpointMocks.deleteEndpoint.mockReset()
  endpointMocks.getDefaultBodyRules.mockReset().mockResolvedValue({
    api_format: 'claude:messages',
    body_rules: [],
  })
  endpointMocks.updateEndpoint.mockReset().mockImplementation(async (_endpointId, payload) => ({
    ...makeEndpoint(),
    ...payload,
  }))
})

afterEach(() => {
  for (const { app, root } of mountedApps.splice(0)) {
    app.unmount()
    root.remove()
  }
  document.body.innerHTML = ''
})

describe('EndpointFormDialog Anthropic operation support', () => {
  it('preserves endpoint config while disabling Token count', async () => {
    await mountDialog()
    const countTokensSwitch = document.body.querySelector<HTMLButtonElement>(
      '[data-testid="claude-count-tokens-switch"]',
    )

    expect(countTokensSwitch?.getAttribute('aria-checked')).toBe('true')
    countTokensSwitch?.click()
    await settle()

    expect(endpointMocks.updateEndpoint).toHaveBeenCalledWith('endpoint-1', {
      config: {
        upstream_stream_policy: 'force_stream',
        anthropic: {
          compatibility_profile: 'native_transparent',
          supported_operations: ['messages'],
        },
      },
    })
  })

  it('locks Token count for private adapters', async () => {
    await mountDialog(makeProvider({ provider_type: 'grok' }))
    const countTokensSwitch = document.body.querySelector<HTMLButtonElement>(
      '[data-testid="claude-count-tokens-switch"]',
    )

    expect(countTokensSwitch?.disabled).toBe(true)
    expect(countTokensSwitch?.getAttribute('aria-checked')).toBe('false')
  })
})
