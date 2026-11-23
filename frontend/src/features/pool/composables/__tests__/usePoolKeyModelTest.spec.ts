import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h } from 'vue'
import type { PoolKeyDetail } from '@/api/endpoints/pool'
import type { ProviderEndpoint } from '@/api/endpoints/types'
import {
  getCompatiblePoolKeyEndpoints,
  isModelAllowedByPoolKey,
  normalizePoolModelTestResultForDialog,
  usePoolKeyModelTest,
} from '../usePoolKeyModelTest'

const apiMocks = vi.hoisted(() => ({
  queryProviderModels: vi.fn(),
  testModel: vi.fn(),
}))

vi.mock('@/api/admin', () => ({
  adminApi: {
    queryProviderModels: apiMocks.queryProviderModels,
  },
}))

vi.mock('@/api/endpoints/providers', () => ({
  testModel: apiMocks.testModel,
}))

function buildPoolKey(overrides: Partial<PoolKeyDetail> = {}): PoolKeyDetail {
  return {
    key_id: 'key-1',
    key_name: 'Account 1',
    is_active: true,
    auth_type: 'api_key',
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

function buildEndpoint(overrides: Partial<ProviderEndpoint> = {}): ProviderEndpoint {
  return {
    id: 'ep-1',
    provider_id: 'prov-1',
    provider_name: 'OpenAI',
    api_format: 'openai:chat',
    base_url: 'https://api.example.com/v1',
    is_active: true,
    max_retries: 0,
    total_keys: 1,
    active_keys: 1,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides,
  }
}

function mountComposable<T>(factory: () => T): { result: T; unmount: () => void } {
  let result!: T
  const container = document.createElement('div')
  const app = createApp(defineComponent({
    setup() {
      result = factory()
      return () => h('div')
    },
  }))
  app.mount(container)
  return {
    result,
    unmount: () => app.unmount(),
  }
}

describe('usePoolKeyModelTest', () => {
  beforeEach(() => {
    apiMocks.queryProviderModels.mockReset()
    apiMocks.testModel.mockReset()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('fetches account-scoped models lazily and supports force refresh', async () => {
    apiMocks.queryProviderModels
      .mockResolvedValueOnce({
        success: true,
        data: {
          models: [
            { id: 'gpt-4.1' },
            { id: 'gpt-4.1' },
            { id: 'o4-mini' },
          ],
          from_cache: true,
        },
        provider: { id: 'prov-1', name: 'OpenAI', display_name: 'OpenAI' },
      })
      .mockResolvedValueOnce({
        success: true,
        data: {
          models: [{ id: 'gpt-5' }],
          from_cache: false,
        },
        provider: { id: 'prov-1', name: 'OpenAI', display_name: 'OpenAI' },
      })

    const { result, unmount } = mountComposable(() =>
      usePoolKeyModelTest({ providerId: () => 'prov-1' }),
    )

    await result.fetchModelsForKey('key-1')
    expect(apiMocks.queryProviderModels).toHaveBeenCalledTimes(1)
    expect(apiMocks.queryProviderModels).toHaveBeenCalledWith('prov-1', 'key-1', false)
    expect(result.getModelsState('key-1').models.map(item => item.id)).toEqual(['gpt-4.1', 'o4-mini'])
    expect(result.getModelsState('key-1').fromCache).toBe(true)

    // 已加载且无错时再次非强制调用不重复请求
    await result.fetchModelsForKey('key-1')
    expect(apiMocks.queryProviderModels).toHaveBeenCalledTimes(1)

    // 强制刷新穿透缓存
    await result.fetchModelsForKey('key-1', { forceRefresh: true })
    expect(apiMocks.queryProviderModels).toHaveBeenCalledTimes(2)
    expect(apiMocks.queryProviderModels).toHaveBeenLastCalledWith('prov-1', 'key-1', true)
    expect(result.getModelsState('key-1').models.map(item => item.id)).toEqual(['gpt-5'])
    expect(result.getModelsState('key-1').fromCache).toBe(false)

    unmount()
  })

  it('runs direct single-account model test and normalizes result for detail dialog', async () => {
    const endpoint = buildEndpoint()
    apiMocks.testModel.mockResolvedValueOnce({
      success: true,
      model: 'gpt-4.1',
      provider: { id: 'prov-1', name: 'OpenAI' },
      attempts: [
        {
          candidate_index: 0,
          endpoint_api_format: 'openai:chat',
          endpoint_base_url: endpoint.base_url,
          key_name: 'Account 1',
          key_id: 'key-1',
          auth_type: 'api_key',
          effective_model: 'gpt-4.1',
          status: 'success',
          status_code: 200,
          latency_ms: 420,
          response_body: {
            choices: [{ message: { content: 'Pong from upstream' } }],
          },
        },
      ],
      total_candidates: 1,
      total_attempts: 1,
    })

    const { result, unmount } = mountComposable(() =>
      usePoolKeyModelTest({ providerId: () => 'prov-1', providerName: () => 'OpenAI' }),
    )

    const outcome = await result.runModelTest({
      keyId: 'key-1',
      modelName: 'gpt-4.1',
      endpoint,
    })

    expect(apiMocks.testModel).toHaveBeenCalledWith(
      expect.objectContaining({
        provider_id: 'prov-1',
        api_key_id: 'key-1',
        model_name: 'gpt-4.1',
        endpoint_id: 'ep-1',
        api_format: 'openai:chat',
        mode: 'direct',
        apply_model_mapping: false,
      }),
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    )
    expect(outcome?.status).toBe('success')
    expect(outcome?.statusCode).toBe(200)
    expect(outcome?.latencyMs).toBe(420)
    expect(outcome?.responsePreview).toBe('Pong from upstream')

    const dialogResult = normalizePoolModelTestResultForDialog(
      outcome!,
      'prov-1',
      'OpenAI',
      buildPoolKey(),
    )
    expect(dialogResult.success).toBe(true)
    expect(dialogResult.attempts[0]?.status_code).toBe(200)
    expect(dialogResult.model).toBe('gpt-4.1')

    unmount()
  })

  it('ignores stale fetch responses after provider changes', async () => {
    let currentProviderId = 'prov-1'
    let resolveFirstFetch!: (value: unknown) => void
    const firstPromise = new Promise((resolve) => {
      resolveFirstFetch = resolve
    })
    apiMocks.queryProviderModels.mockImplementationOnce(() => firstPromise)

    const { result, unmount } = mountComposable(() =>
      usePoolKeyModelTest({ providerId: () => currentProviderId }),
    )

    const pending = result.fetchModelsForKey('key-1')
    currentProviderId = 'prov-2'
    result.resetAll()

    resolveFirstFetch({
      success: true,
      data: { models: [{ id: 'stale-model' }] },
      provider: { id: 'prov-1', name: 'P1', display_name: 'P1' },
    })
    await pending

    expect(result.getModelsState('key-1').models).toEqual([])
    expect(result.getModelsState('key-1').loaded).toBe(false)
    unmount()
  })

  it('allows inactive pool keys when matching compatible endpoints and checks allowlist', () => {
    const inactiveKey = buildPoolKey({
      is_active: false,
      api_formats: ['openai:chat'],
      allowed_models: ['gpt-4.1'],
    })
    const endpoints = [
      buildEndpoint({ id: 'ep-inactive', api_format: 'openai:chat', is_active: false }),
      buildEndpoint({ id: 'ep-realtime', api_format: 'openai:realtime', is_active: true }),
      buildEndpoint({ id: 'ep-claude', api_format: 'claude:messages', is_active: true }),
    ]

    const compatible = getCompatiblePoolKeyEndpoints(inactiveKey, endpoints, 'openai')
    expect(compatible.map(ep => ep.id)).toEqual(['ep-inactive'])

    expect(isModelAllowedByPoolKey(inactiveKey, 'gpt-4.1')).toBe(true)
    expect(isModelAllowedByPoolKey(inactiveKey, 'GPT-4.1')).toBe(true)
    expect(isModelAllowedByPoolKey(inactiveKey, 'o4-mini')).toBe(false)
    expect(isModelAllowedByPoolKey(buildPoolKey({ allowed_models: null }), 'anything')).toBe(true)
  })
})
