import { createApp, nextTick } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { getRequestDetailMock } = vi.hoisted(() => ({
  getRequestDetailMock: vi.fn(),
}))

vi.mock('@/api/request-details', () => ({
  requestDetailsApi: {
    getRequestDetail: getRequestDetailMock,
    getCurlData: vi.fn(),
    replayRequest: vi.fn(),
  },
}))

import RequestDetailDrawer from '../components/RequestDetailDrawer.vue'
import HorizontalRequestTimeline from '../components/HorizontalRequestTimeline.vue'
import type { RequestDetail } from '@/api/request-details'
import type { RequestTrace } from '@/api/requestTrace'

type MountedApp = {
  container: HTMLDivElement
  cleanup: () => void
}

async function flushUi(): Promise<void> {
  await Promise.resolve()
  await nextTick()
  await Promise.resolve()
  await nextTick()
}

async function mountComponent(component: object, props: Record<string, unknown>): Promise<MountedApp> {
  const container = document.createElement('div')
  document.body.appendChild(container)

  const app = createApp(component, props)
  app.mount(container)
  await flushUi()

  return {
    container,
    cleanup: () => {
      app.unmount()
      container.remove()
    },
  }
}

function buildRequestDetail(): RequestDetail {
  return {
    id: 'detail-1',
    request_id: 'req-1',
    user: { id: 'user-1', username: 'tester', email: 'tester@example.com' },
    api_key: { id: 'key-1', name: 'key', display: 'sk-***1234' },
    provider_api_key: { id: 'provider-key-1', name: 'provider-key' },
    provider: 'openai',
    api_format: 'openai',
    model: 'gpt-4.1',
    target_model: null,
    tokens: { input: 10, output: 20, total: 30 },
    cost: { input: 0.001, output: 0.002, total: 0.003 },
    request_type: 'chat',
    is_stream: false,
    status_code: 200,
    status: 'success',
    response_time_ms: 320,
    created_at: '2026-03-26T10:00:00Z',
    request_headers: { 'x-test': '1' },
    request_body: undefined,
    provider_request_headers: { authorization: 'Bearer x' },
    provider_request_body: undefined,
    response_headers: { 'content-type': 'application/json' },
    client_response_headers: { 'content-type': 'application/json' },
    response_body: undefined,
    client_response_body: undefined,
    has_request_body: false,
    has_provider_request_body: false,
    has_response_body: false,
    has_client_response_body: false,
    metadata: {},
    transformer_diagnostics_summary: {
      count: 3,
      by_code: {
        sampling_clamped: 2,
        tool_id_generated: 1,
      },
      by_transformer: {
        sampling: 2,
        tooluse: 1,
      },
    },
    transformer_diagnostics: [
      {
        stage: 'request',
        transformer: 'sampling',
        code: 'sampling_clamped',
        message: 'temperature clamped to 1.0',
        severity: 'info',
      },
      {
        stage: 'request',
        transformer: 'sampling',
        code: 'sampling_clamped',
        message: 'top_p dropped',
        severity: 'warning',
      },
      {
        stage: 'request',
        transformer: 'tooluse',
        code: 'tool_id_generated',
        message: 'generated tool id for missing call',
        severity: 'info',
        details: {
          generated_tool_id: 'toolu_123',
        },
      },
    ],
    tiered_pricing: null,
    video_billing: null,
  }
}

function buildRequestDetailWithoutSummary(): RequestDetail {
  return {
    ...buildRequestDetail(),
    transformer_diagnostics_summary: null,
  }
}

function buildRequestTraceWithoutSummary(): RequestTrace {
  return {
    ...buildRequestTrace(),
    transformer_diagnostics_summary: null,
    transformer_diagnostics: [
      {
        stage: 'request',
        transformer: 'sampling',
        code: 'sampling_clamped',
        message: 'temperature clamped to 1.0',
        severity: 'info',
      },
      {
        stage: 'request',
        transformer: 'tooluse',
        code: 'tool_id_generated',
        message: 'generated tool id',
        severity: 'info',
      },
    ],
  }
}

function buildRequestTrace(): RequestTrace {
  return {
    request_id: 'req-1',
    total_candidates: 1,
    final_status: 'success',
    total_latency_ms: 320,
    transformer_diagnostics: [
      {
        stage: 'request',
        transformer: 'sampling',
        code: 'sampling_clamped',
        message: 'temperature clamped to 1.0',
        severity: 'info',
      },
    ],
    transformer_diagnostics_summary: {
      count: 1,
      by_code: {
        sampling_clamped: 1,
      },
      by_transformer: {
        sampling: 1,
      },
    },
    candidates: [
      {
        id: 'candidate-1',
        request_id: 'req-1',
        candidate_index: 0,
        retry_index: 0,
        provider_id: 'provider-1',
        provider_name: 'OpenAI',
        endpoint_id: 'endpoint-1',
        endpoint_name: 'openai',
        key_id: 'key-1',
        key_name: 'key-a',
        status: 'success',
        is_cached: false,
        status_code: 200,
        latency_ms: 320,
        created_at: '2026-03-26T10:00:00Z',
        started_at: '2026-03-26T10:00:00Z',
        finished_at: '2026-03-26T10:00:00Z',
      },
    ],
  }
}

describe('transformer diagnostics display', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    getRequestDetailMock.mockReset()
  })

  afterEach(() => {
    document.body.innerHTML = ''
  })

  it('shows transformer diagnostics summary and entries in request detail drawer', async () => {
    getRequestDetailMock.mockResolvedValue(buildRequestDetail())

    const view = await mountComponent(RequestDetailDrawer, {
      isOpen: true,
      requestId: 'req-1',
      onClose: vi.fn(),
    })

    await flushUi()

    expect(document.body.textContent).toContain('转换诊断')
    expect(document.body.textContent).toContain('3 次')
    expect(document.body.textContent).toContain('sampling_clamped x2')
    expect(document.body.textContent).toContain('sampling x2')
    expect(document.body.textContent).toContain('generated tool id for missing call')
    expect(document.body.textContent).toContain('generated_tool_id')

    view.cleanup()
  })

  it('shows transformer diagnostics summary in horizontal request timeline', async () => {
    const view = await mountComponent(HorizontalRequestTimeline, {
      requestId: 'req-1',
      traceData: buildRequestTrace(),
    })

    await flushUi()

    expect(view.container.textContent).toContain('转换诊断 1 次')
    expect(view.container.textContent).toContain('sampling_clamped x1')
    expect(view.container.textContent).toContain('sampling x1')

    view.cleanup()
  })

  it('falls back to raw diagnostics when summary is missing', async () => {
    getRequestDetailMock.mockResolvedValue(buildRequestDetailWithoutSummary())

    const drawer = await mountComponent(RequestDetailDrawer, {
      isOpen: true,
      requestId: 'req-1',
      onClose: vi.fn(),
    })
    const timeline = await mountComponent(HorizontalRequestTimeline, {
      requestId: 'req-1',
      traceData: buildRequestTraceWithoutSummary(),
    })

    await flushUi()

    expect(document.body.textContent).toContain('sampling_clamped x2')
    expect(document.body.textContent).toContain('tooluse x1')
    expect(timeline.container.textContent).toContain('转换诊断 2 次')
    expect(timeline.container.textContent).toContain('tool_id_generated x1')

    drawer.cleanup()
    timeline.cleanup()
  })
})
