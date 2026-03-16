import apiClient from '@/api/client'
import type {
  TavilyAccount,
  TavilyToken,
  TavilyHealthCheck,
  TavilyMaintenanceRun
} from './types'

const BASE = '/api/admin/tavily-pool'

export const tavilyPoolApi = {
  async listAccounts(): Promise<TavilyAccount[]> {
    const { data } = await apiClient.get<TavilyAccount[]>(`${BASE}/accounts`)
    return data
  },
  async createAccount(payload: { email: string; password: string; source?: string; notes?: string }): Promise<TavilyAccount> {
    const { data } = await apiClient.post<TavilyAccount>(`${BASE}/accounts`, payload)
    return data
  },
  async listTokens(accountId: string): Promise<TavilyToken[]> {
    const { data } = await apiClient.get<TavilyToken[]>(`${BASE}/accounts/${accountId}/tokens`)
    return data
  },
  async createToken(accountId: string, token: string): Promise<TavilyToken> {
    const { data } = await apiClient.post<TavilyToken>(`${BASE}/accounts/${accountId}/tokens`, { token })
    return data
  },
  async activateToken(tokenId: string): Promise<TavilyToken> {
    const { data } = await apiClient.post<TavilyToken>(`${BASE}/tokens/${tokenId}/activate`)
    return data
  },
  async runHealthCheck(): Promise<{ total: number; success: number; failed: number }> {
    const { data } = await apiClient.post<{ total: number; success: number; failed: number }>(`${BASE}/health-check/run`)
    return data
  },
  async listHealthChecks(): Promise<TavilyHealthCheck[]> {
    const { data } = await apiClient.get<TavilyHealthCheck[]>(`${BASE}/health-check/runs`)
    return data
  },
  async runMaintenance(): Promise<{ run_id: string; status: string; total: number; success: number; failed: number; skipped: number }> {
    const { data } = await apiClient.post<{ run_id: string; status: string; total: number; success: number; failed: number; skipped: number }>(`${BASE}/maintenance/run`)
    return data
  },
  async listMaintenanceRuns(): Promise<TavilyMaintenanceRun[]> {
    const { data } = await apiClient.get<TavilyMaintenanceRun[]>(`${BASE}/maintenance/runs`)
    return data
  }
}
