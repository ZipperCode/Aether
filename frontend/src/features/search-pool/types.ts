export type SearchService = 'tavily' | 'firecrawl'

export interface SearchPoolKey {
  id: string
  service: SearchService | string
  key_masked: string
  email: string
  active: boolean
  total_used: number
  total_failed: number
  consecutive_fails: number
  last_used_at: string | null
  usage_key_used: number | null
  usage_key_limit: number | null
  usage_key_remaining: number | null
  usage_account_plan: string
  usage_account_used: number | null
  usage_account_limit: number | null
  usage_account_remaining: number | null
  usage_synced_at: string | null
  usage_sync_error: string
  created_at: string | null
  updated_at: string | null
}

export interface SearchPoolToken {
  id: string
  service: SearchService | string
  token: string
  name: string
  hourly_limit: number
  daily_limit: number
  monthly_limit: number
  created_at: string | null
  updated_at: string | null
  usage_success: number
  usage_failed: number
  usage_this_month: number
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

export interface SearchPoolWorkspaceRouteSummary {
  title: string
  description: string
  service_badge: string
  route_label: string
  route_path: string
}

export interface SearchPoolWorkspaceStats extends SearchPoolStatsOverview {
  keys_inactive: number
  requests_today: number
  requests_this_month: number
  real_used: number
  real_remaining: number
  real_limit: number
  synced_keys: number
  last_synced_at: string | null
}

export interface SearchPoolUsageExamples {
  base_url: string
  curl_examples: string[]
}

export interface SearchPoolServiceSummary {
  service: SearchService
  title: string
  description: string
  service_badge: string
  keys_active: number
  keys_total: number
  tokens_total: number
  requests_today: number
  real_remaining: number
  route_label: string
  route_path: string
  last_synced_at: string | null
}

export interface SearchPoolWorkspace extends SearchPoolServiceSummary {
  route_summary: SearchPoolWorkspaceRouteSummary
  stats: SearchPoolWorkspaceStats
  usage_examples: SearchPoolUsageExamples
  keys: SearchPoolKey[]
  tokens: SearchPoolToken[]
}

export interface SearchPoolKeyImportResult {
  service: string
  created: number
  keys: SearchPoolKey[]
}

export interface SearchPoolUsageSyncResult {
  service: string
  synced_keys: number
  synced_at: string
}
