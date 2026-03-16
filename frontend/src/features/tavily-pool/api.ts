import apiClient from '@/api/client'
import type {
  TavilyAccount,
  TavilyToken,
  TavilyHealthCheck,
  TavilyMaintenanceRun,
  TavilyPoolLease,
  TavilyPoolStats,
  TavilyAccountsImportRequest,
  TavilyAccountsImportResult
} from './types'

const BASE = '/api/admin/tavily-pool'

export const tavilyPoolApi = {
  async listAccounts(): Promise<TavilyAccount[]> {
    const { data } = await apiClient.get<TavilyAccount[]>(`${BASE}/accounts`)
    return data
  },
  async createAccount(payload: { email: string; password: string; api_key?: string; source?: string; notes?: string }): Promise<TavilyAccount> {
    const { data } = await apiClient.post<TavilyAccount>(`${BASE}/accounts`, payload)
    return data
  },
  async updateAccountStatus(accountId: string, status: 'active' | 'disabled'): Promise<TavilyAccount> {
    const { data } = await apiClient.put<TavilyAccount>(`${BASE}/accounts/${accountId}/status`, { status })
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
  async deleteToken(tokenId: string): Promise<{ ok: boolean }> {
    const { data } = await apiClient.delete<{ ok: boolean }>(`${BASE}/tokens/${tokenId}`)
    return data
  },
  async runHealthCheck(): Promise<{ total: number; success: number; failed: number }> {
    const { data } = await apiClient.post<{ total: number; success: number; failed: number }>(`${BASE}/health-check/run`)
    return data
  },
  async syncUsage(): Promise<{ total_accounts: number; synced_accounts: number; failed_accounts: number }> {
    const { data } = await apiClient.post<{ total_accounts: number; synced_accounts: number; failed_accounts: number }>(`${BASE}/usage/sync`)
    return data
  },
  async leasePoolToken(): Promise<TavilyPoolLease> {
    const { data } = await apiClient.post<TavilyPoolLease>(`${BASE}/pool/lease`)
    return data
  },
  async reportPoolResult(payload: { token_id: string; success: boolean; endpoint?: string; latency_ms?: number; error_message?: string }): Promise<{ token_id: string; success: boolean; is_active: boolean; consecutive_fail_count: number }> {
    const { data } = await apiClient.post<{ token_id: string; success: boolean; is_active: boolean; consecutive_fail_count: number }>(`${BASE}/pool/report`, payload)
    return data
  },
  async getPoolStatsOverview(): Promise<TavilyPoolStats> {
    const { data } = await apiClient.get<TavilyPoolStats>(`${BASE}/stats/overview`)
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
  },
  async importAccounts(payload: TavilyAccountsImportRequest): Promise<TavilyAccountsImportResult> {
    const { data } = await apiClient.post<TavilyAccountsImportResult>(`${BASE}/accounts/import`, payload)
    return data
  }
}
