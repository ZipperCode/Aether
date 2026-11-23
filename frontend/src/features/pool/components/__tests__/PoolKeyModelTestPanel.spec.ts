import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, nextTick, reactive, type App } from 'vue'
import type { PoolKeyDetail } from '@/api/endpoints/pool'
import type { ProviderEndpoint } from '@/api/endpoints/types'
import { setI18nLocale } from '@/i18n'
import PoolKeyModelTestPanel from '../PoolKeyModelTestPanel.vue'
import type {
  PoolAccountModelTestResult,
  PoolAccountModelsState,
} from '../../composables/usePoolKeyModelTest'

vi.mock('@/composables/useClipboard', () => ({
  useClipboard: () => ({ copyToClipboard: vi.fn().mockResolvedValue(undefined) }),
}))

function buildPoolKey(overrides: Partial<PoolKeyDetail> = {}): PoolKeyDetail {
  return {
    key_id: 'key-1',
    key_name: 'Demo Account',
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

describe('PoolKeyModelTestPanel', () => {
  let app: App<Element> | null = null
  let container: HTMLDivElement | null = null

  afterEach(() => {
    app?.unmount()
    app = null
    container?.remove()
    container = null
  })

  it('filters fetched models, emits test events, and supports custom model direct test', async () => {
    setI18nLocale('zh-CN')
    const emittedTests: Array<Record<string, unknown>> = []
    const emittedInspects: Array<Record<string, unknown>> = []
    const resultsMap = reactive<Record<string, PoolAccountModelTestResult>>({
      'gpt-4.1': {
        keyId: 'key-1',
        modelName: 'gpt-4.1',
        endpointId: 'ep-1',
        apiFormat: 'openai:chat',
        endpointBaseUrl: 'https://api.example.com/v1',
        testing: false,
        status: 'success',
        statusCode: 200,
        latencyMs: 312,
        errorMessage: null,
        responsePreview: 'Hello back',
        rawResult: null,
        requestHeaders: {},
        requestBody: null,
        testedAt: Date.now(),
      },
      'custom-model-x': {
        keyId: 'key-1',
        modelName: 'custom-model-x',
        endpointId: 'ep-1',
        apiFormat: 'openai:chat',
        endpointBaseUrl: 'https://api.example.com/v1',
        testing: false,
        status: 'failed',
        statusCode: 403,
        latencyMs: 180,
        errorMessage: 'model not permitted',
        responsePreview: null,
        rawResult: null,
        requestHeaders: {},
        requestBody: null,
        testedAt: Date.now(),
      },
    })

    const modelsState: PoolAccountModelsState = {
      loading: false,
      loaded: true,
      models: [{ id: 'gpt-4.1' }, { id: 'o4-mini' }],
      fromCache: true,
      error: null,
      warning: null,
    }

    container = document.createElement('div')
    document.body.appendChild(container)
    app = createApp(PoolKeyModelTestPanel, {
      poolKey: buildPoolKey({ is_active: false, allowed_models: ['gpt-4.1'] }),
      providerType: 'openai',
      endpoints: [buildEndpoint()],
      modelsState,
      customTestedModels: ['custom-model-x'],
      getModelResult: (_keyId: string, modelName: string) => resultsMap[modelName] ?? null,
      onTestModel: (payload: Record<string, unknown>) => emittedTests.push(payload),
      onInspectModel: (payload: Record<string, unknown>) => emittedInspects.push(payload),
    })
    app.mount(container)
    await nextTick()

    // 停用账号提示 + 白名单提示 + 已有结果展示
    expect(container.textContent).toContain('当前账号已停用')
    expect(container.textContent).toContain('未在白名单')
    expect(container.textContent).toContain('通过 200')
    expect(container.textContent).toContain('312 ms')
    expect(container.textContent).toContain('custom-model-x')
    expect(container.textContent).toContain('model not permitted')

    // 点击已获取模型的测试按钮
    const runBtn = container.querySelector<HTMLButtonElement>('[data-testid="pool-model-test-run-key-1-o4-mini"]')
    expect(runBtn).not.toBeNull()
    runBtn!.click()
    await nextTick()
    expect(emittedTests).toHaveLength(1)
    expect(emittedTests[0]).toMatchObject({
      modelName: 'o4-mini',
      isCustom: false,
    })

    // 点击详情按钮
    const detailBtn = container.querySelector<HTMLButtonElement>('[data-testid="pool-model-test-detail-key-1-gpt-4.1"]')
    detailBtn!.click()
    await nextTick()
    expect(emittedInspects).toHaveLength(1)
    expect(emittedInspects[0]).toMatchObject({
      modelName: 'gpt-4.1',
    })

    // 输入自定义模型名并点击直测
    const customInput = container.querySelector<HTMLInputElement>('[data-testid="pool-model-test-custom-input-key-1"]')
    expect(customInput).not.toBeNull()
    customInput!.value = 'gpt-5-preview'
    customInput!.dispatchEvent(new Event('input'))
    await nextTick()

    const customRunBtn = container.querySelector<HTMLButtonElement>('[data-testid="pool-model-test-custom-run-key-1"]')
    expect(customRunBtn?.disabled).toBe(false)
    customRunBtn!.click()
    await nextTick()
    expect(emittedTests).toHaveLength(2)
    expect(emittedTests[1]).toMatchObject({
      modelName: 'gpt-5-preview',
      isCustom: true,
    })

    // 搜索过滤只剩匹配模型
    const searchInput = container.querySelector<HTMLInputElement>('[data-testid="pool-model-test-search-key-1"]')
    searchInput!.value = 'o4'
    searchInput!.dispatchEvent(new Event('input'))
    await nextTick()
    expect(container.querySelector('[data-testid="pool-model-test-row-key-1-o4-mini"]')).not.toBeNull()
    expect(container.querySelector('[data-testid="pool-model-test-row-key-1-gpt-4.1"]')).toBeNull()
  })
})
