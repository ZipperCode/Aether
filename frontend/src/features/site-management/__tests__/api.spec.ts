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

import { siteManagementApi } from '../api'

describe('siteManagementApi', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('passes source_id when listing checkin runs', async () => {
    apiClient.get.mockResolvedValue({
      data: {
        items: [],
        total: 0,
        page: 1,
        page_size: 20,
      },
    })

    await siteManagementApi.listCheckinRuns({ page: 1, source_id: 's1' })

    expect(apiClient.get).toHaveBeenCalledWith('/api/admin/site-management/checkin-runs', {
      params: { page: 1, source_id: 's1' },
    })
  })
})
