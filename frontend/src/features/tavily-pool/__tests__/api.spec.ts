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

import { tavilyPoolApi } from '../api'

describe('tavilyPoolApi', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('posts payload to account import endpoint', async () => {
    apiClient.post.mockResolvedValue({
      data: {
        stats: { total: 1, created: 1, updated: 0, skipped: 0, failed: 0, api_keys_created: 1 },
        errors: [],
      },
    })

    const payload = {
      file_type: 'json' as const,
      merge_mode: 'skip' as const,
      content: '[{"email":"a@example.com","password":"pwd"}]',
    }

    await tavilyPoolApi.importAccounts(payload)

    expect(apiClient.post).toHaveBeenCalledWith('/api/admin/tavily-pool/accounts/import', payload)
  })
})
