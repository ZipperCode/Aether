import { beforeEach, describe, expect, it, vi } from 'vitest'

const { apiClient } = vi.hoisted(() => ({
  apiClient: {
    get: vi.fn(),
    post: vi.fn(),
    put: vi.fn(),
    delete: vi.fn(),
  },
}))

vi.mock('@/api/client', () => ({
  default: apiClient,
}))

import { searchPoolApi } from '../api'

describe('searchPoolApi', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('list keys calls /api/admin/search-pool/keys', async () => {
    apiClient.get.mockResolvedValue({
      data: { keys: [] },
    })

    await searchPoolApi.listKeys('tavily')

    expect(apiClient.get).toHaveBeenCalledWith('/api/admin/search-pool/keys', {
      params: { service: 'tavily' },
    })
  })

  it('list service summaries calls /api/admin/search-pool/services/summary', async () => {
    apiClient.get.mockResolvedValue({
      data: { services: [] },
    })

    await searchPoolApi.listServiceSummaries()

    expect(apiClient.get).toHaveBeenCalledWith('/api/admin/search-pool/services/summary')
  })

  it('get workspace calls /api/admin/search-pool/services/{service}/workspace', async () => {
    apiClient.get.mockResolvedValue({
      data: { service: 'tavily' },
    })

    await searchPoolApi.getWorkspace('tavily')

    expect(apiClient.get).toHaveBeenCalledWith('/api/admin/search-pool/services/tavily/workspace')
  })

  it('import keys posts to /api/admin/search-pool/keys/import', async () => {
    apiClient.post.mockResolvedValue({
      data: { service: 'firecrawl', created: 2, keys: [] },
    })

    await searchPoolApi.importKeys({
      service: 'firecrawl',
      content: 'key-1\nkey-2',
    })

    expect(apiClient.post).toHaveBeenCalledWith('/api/admin/search-pool/keys/import', {
      service: 'firecrawl',
      content: 'key-1\nkey-2',
    })
  })

  it('update token puts to /api/admin/search-pool/tokens/{id}', async () => {
    apiClient.put.mockResolvedValue({
      data: { id: 'token-1' },
    })

    await searchPoolApi.updateToken('token-1', {
      name: 'ops',
      hourly_limit: 20,
      daily_limit: 100,
      monthly_limit: 500,
    })

    expect(apiClient.put).toHaveBeenCalledWith('/api/admin/search-pool/tokens/token-1', {
      name: 'ops',
      hourly_limit: 20,
      daily_limit: 100,
      monthly_limit: 500,
    })
  })
})
