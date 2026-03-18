import apiClient from '@/api/client'
import type {
  SearchPoolKeyImportResult,
  SearchPoolKey,
  SearchPoolServiceSummary,
  SearchPoolStatsOverview,
  SearchPoolToken,
  SearchPoolUsageSyncResult,
  SearchPoolWorkspace,
  SearchService,
} from './types'

const BASE = '/api/admin/search-pool'

export const searchPoolApi = {
  async listServiceSummaries(): Promise<SearchPoolServiceSummary[]> {
    const { data } = await apiClient.get<{ services: SearchPoolServiceSummary[] }>(`${BASE}/services/summary`)
    return data.services
  },
  async getWorkspace(service: SearchService): Promise<SearchPoolWorkspace> {
    const { data } = await apiClient.get<SearchPoolWorkspace>(`${BASE}/services/${service}/workspace`)
    return data
  },
  async listKeys(service?: SearchService): Promise<SearchPoolKey[]> {
    const { data } = await apiClient.get<{ keys: SearchPoolKey[] }>(`${BASE}/keys`, {
      params: service ? { service } : undefined,
    })
    return data.keys
  },
  async createKey(payload: { service: SearchService; key: string; email?: string }): Promise<SearchPoolKey> {
    const { data } = await apiClient.post<SearchPoolKey>(`${BASE}/keys`, payload)
    return data
  },
  async importKeys(payload: {
    service: SearchService
    content?: string
    keys?: string[]
  }): Promise<SearchPoolKeyImportResult> {
    const { data } = await apiClient.post<SearchPoolKeyImportResult>(`${BASE}/keys/import`, payload)
    return data
  },
  async toggleKey(keyId: string, active: boolean): Promise<{ id: string; active: boolean }> {
    const { data } = await apiClient.put<{ id: string; active: boolean }>(`${BASE}/keys/${keyId}/toggle`, { active })
    return data
  },
  async deleteKey(keyId: string): Promise<{ ok: boolean }> {
    const { data } = await apiClient.delete<{ ok: boolean }>(`${BASE}/keys/${keyId}`)
    return data
  },
  async listTokens(service?: SearchService): Promise<SearchPoolToken[]> {
    const { data } = await apiClient.get<{ tokens: SearchPoolToken[] }>(`${BASE}/tokens`, {
      params: service ? { service } : undefined,
    })
    return data.tokens
  },
  async createToken(payload: {
    service: SearchService
    name?: string
    hourly_limit?: number
    daily_limit?: number
    monthly_limit?: number
  }): Promise<SearchPoolToken> {
    const { data } = await apiClient.post<SearchPoolToken>(`${BASE}/tokens`, payload)
    return data
  },
  async updateToken(tokenId: string, payload: {
    name: string
    hourly_limit: number
    daily_limit: number
    monthly_limit: number
  }): Promise<SearchPoolToken> {
    const { data } = await apiClient.put<SearchPoolToken>(`${BASE}/tokens/${tokenId}`, payload)
    return data
  },
  async deleteToken(tokenId: string): Promise<{ ok: boolean }> {
    const { data } = await apiClient.delete<{ ok: boolean }>(`${BASE}/tokens/${tokenId}`)
    return data
  },
  async syncUsage(service?: SearchService, force = false): Promise<SearchPoolUsageSyncResult> {
    const { data } = await apiClient.post<{ result: SearchPoolUsageSyncResult }>(`${BASE}/usage/sync`, {
      service,
      force,
    })
    return data.result
  },
  async getStatsOverview(service?: SearchService): Promise<SearchPoolStatsOverview> {
    const { data } = await apiClient.get<SearchPoolStatsOverview>(`${BASE}/stats/overview`, {
      params: service ? { service } : undefined,
    })
    return data
  },
}
