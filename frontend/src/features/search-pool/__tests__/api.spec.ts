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
})
