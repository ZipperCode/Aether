import apiClient from '@/api/client'
import type {
  WebDavSource,
  CreateWebDavSourceRequest,
  UpdateWebDavSourceRequest,
  SiteAccount,
  SyncRun,
  SyncItem,
  CheckinRun,
  CheckinItem,
  PaginatedResponse,
  ActionResult,
} from './types'

const BASE = '/api/admin/site-management'

export const siteManagementApi = {
  // Source CRUD
  async listSources(): Promise<WebDavSource[]> {
    const { data } = await apiClient.get<WebDavSource[]>(`${BASE}/sources`)
    return data
  },
  async createSource(payload: CreateWebDavSourceRequest): Promise<WebDavSource> {
    const { data } = await apiClient.post<WebDavSource>(`${BASE}/sources`, payload)
    return data
  },
  async updateSource(sourceId: string, payload: UpdateWebDavSourceRequest): Promise<WebDavSource> {
    const { data } = await apiClient.put<WebDavSource>(`${BASE}/sources/${sourceId}`, payload)
    return data
  },
  async deleteSource(sourceId: string): Promise<void> {
    await apiClient.delete(`${BASE}/sources/${sourceId}`)
  },
  async testConnection(sourceId: string): Promise<{ success: boolean; message: string }> {
    const { data } = await apiClient.post<{ success: boolean; message: string }>(`${BASE}/sources/${sourceId}/test`)
    return data
  },
  async syncSource(sourceId: string, options?: { dry_run?: boolean; force_refresh?: boolean }): Promise<SyncRun> {
    const { data } = await apiClient.post<SyncRun>(`${BASE}/sources/${sourceId}/sync`, options)
    return data
  },

  // Account operations
  async listAccounts(sourceId: string, params?: { page?: number; page_size?: number; search?: string }): Promise<PaginatedResponse<SiteAccount>> {
    const { data } = await apiClient.get<PaginatedResponse<SiteAccount>>(`${BASE}/sources/${sourceId}/accounts`, { params })
    return data
  },
  async checkinAccount(sourceId: string, accountId: string): Promise<ActionResult> {
    const { data } = await apiClient.post<ActionResult>(`${BASE}/sources/${sourceId}/accounts/${accountId}/checkin`)
    return data
  },
  async balanceAccount(sourceId: string, accountId: string): Promise<ActionResult> {
    const { data } = await apiClient.post<ActionResult>(`${BASE}/sources/${sourceId}/accounts/${accountId}/balance`)
    return data
  },
  async batchCheckin(sourceId: string, accountIds?: string[]): Promise<CheckinRun> {
    const { data } = await apiClient.post<CheckinRun>(`${BASE}/sources/${sourceId}/accounts/checkin`, { account_ids: accountIds })
    return data
  },
  async batchBalance(sourceId: string, accountIds?: string[]): Promise<ActionResult> {
    const { data } = await apiClient.post<ActionResult>(`${BASE}/sources/${sourceId}/accounts/balance`, { account_ids: accountIds })
    return data
  },

  // History
  async listSyncRuns(params?: { page?: number; page_size?: number; source_id?: string }): Promise<PaginatedResponse<SyncRun>> {
    const { data } = await apiClient.get<PaginatedResponse<SyncRun>>(`${BASE}/sync-runs`, { params })
    return data
  },
  async getSyncRunItems(runId: string, params?: { page?: number; page_size?: number }): Promise<PaginatedResponse<SyncItem>> {
    const { data } = await apiClient.get<PaginatedResponse<SyncItem>>(`${BASE}/sync-runs/${runId}/items`, { params })
    return data
  },
  async listCheckinRuns(params?: { page?: number; page_size?: number }): Promise<PaginatedResponse<CheckinRun>> {
    const { data } = await apiClient.get<PaginatedResponse<CheckinRun>>(`${BASE}/checkin-runs`, { params })
    return data
  },
  async getCheckinRunItems(runId: string, params?: { page?: number; page_size?: number }): Promise<PaginatedResponse<CheckinItem>> {
    const { data } = await apiClient.get<PaginatedResponse<CheckinItem>>(`${BASE}/checkin-runs/${runId}/items`, { params })
    return data
  },
}
