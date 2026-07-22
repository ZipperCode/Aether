import { beforeEach, describe, expect, it, vi } from 'vitest'

const { getMock, postMock } = vi.hoisted(() => ({
  getMock: vi.fn(),
  postMock: vi.fn(),
}))

vi.mock('@/api/client', () => ({
  default: {
    get: getMock,
    post: postMock,
  },
}))

import {
  getGlobalModelMappingPreview,
  getGlobalModelRoutingPreview,
} from '@/api/endpoints/global-models'

describe('global model routing endpoints', () => {
  beforeEach(() => {
    getMock.mockReset()
    postMock.mockReset()
  })

  it('requests the lightweight routing response and deduplicates in-flight calls', async () => {
    let resolveRequest: ((value: { data: { providers: unknown[] } }) => void) | undefined
    getMock.mockReturnValue(new Promise(resolve => { resolveRequest = resolve }))

    const first = getGlobalModelRoutingPreview('lazy-routing-model')
    const second = getGlobalModelRoutingPreview('lazy-routing-model')
    expect(getMock).toHaveBeenCalledTimes(1)
    expect(getMock).toHaveBeenCalledWith(
      '/api/admin/models/global/lazy-routing-model/routing',
      { params: { include_whitelist: false } },
    )

    resolveRequest?.({ data: { providers: [] } })
    await expect(Promise.all([first, second])).resolves.toEqual([
      { providers: [] },
      { providers: [] },
    ])
  })

  it('posts unsaved rules and expanded-page parameters to mapping preview', async () => {
    postMock.mockResolvedValue({ data: { global_model_id: 'model-1', rules: [], expanded: null } })

    await getGlobalModelMappingPreview('model-1', {
      mappings: ['  gpt-5.*  '],
      expanded_rule_index: 0,
      page: 2,
      page_size: 25,
    })

    expect(postMock).toHaveBeenCalledWith(
      '/api/admin/models/global/model-1/mapping-preview',
      {
        mappings: ['gpt-5.*'],
        expanded_rule_index: 0,
        page: 2,
        page_size: 25,
      },
    )
  })
})
