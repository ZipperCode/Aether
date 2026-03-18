import { describe, expect, it } from 'vitest'
import type { SearchPoolWorkspace } from '../types'

describe('search pool workspace types', () => {
  it('accepts workspace payload with keys and tokens', () => {
    const payload: SearchPoolWorkspace = {
      service: 'tavily',
      title: 'Tavily',
      description: 'workspace',
      service_badge: 'Tavily',
      keys_active: 1,
      keys_total: 1,
      tokens_total: 1,
      requests_today: 0,
      real_remaining: 0,
      route_label: 'POST /api/search',
      route_path: '/api/search',
      last_synced_at: null,
      route_summary: {
        title: 'Tavily',
        description: 'workspace',
        service_badge: 'Tavily',
        route_label: 'POST /api/search',
        route_path: '/api/search',
      },
      stats: {
        service: 'tavily',
        keys_total: 1,
        keys_active: 1,
        keys_inactive: 0,
        tokens_total: 1,
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
    }

    expect(payload.route_summary.route_path).toBe('/api/search')
    expect(payload.tokens).toHaveLength(0)
  })
})
