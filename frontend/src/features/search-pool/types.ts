export type SearchService = 'tavily' | 'firecrawl'

export interface SearchPoolKey {
  id: string
  service: SearchService | string
  key_masked: string
  email: string
  active: boolean
}

export interface SearchPoolToken {
  id: string
  service: SearchService | string
  token: string
  name: string
  hourly_limit: number
  daily_limit: number
  monthly_limit: number
}

export interface SearchPoolStatsOverview {
  service: string
  keys_total: number
  keys_active: number
  tokens_total: number
  requests_total: number
  requests_success: number
  requests_failed: number
  success_rate: number
}

export interface SearchPoolUsageSyncResult {
  service: string
  synced_keys: number
  synced_at: string
}
