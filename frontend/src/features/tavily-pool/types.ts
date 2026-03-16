export interface TavilyAccount {
  id: string
  email: string
  status: string
  health_status: string
  fail_count: number
  usage_plan: string | null
  usage_account_used: number | null
  usage_account_limit: number | null
  usage_account_remaining: number | null
  usage_synced_at: string | null
  usage_sync_error: string | null
  source: string
  notes: string | null
  created_at: string
  updated_at: string
}

export interface TavilyToken {
  id: string
  account_id: string
  token_masked: string
  is_active: boolean
  consecutive_fail_count: number
  last_checked_at: string | null
  last_success_at: string | null
  last_response_ms: number | null
  last_error: string | null
  created_at: string
  updated_at: string
}

export interface TavilyHealthCheck {
  id: string
  account_id: string | null
  token_id: string | null
  check_type: string
  status: string
  error_message: string | null
  checked_at: string
}

export interface TavilyMaintenanceRun {
  id: string
  job_name: string
  status: string
  total: number
  success: number
  failed: number
  skipped: number
  started_at: string
  finished_at: string | null
}

export interface TavilyPoolLease {
  account_id: string
  token_id: string
  token: string
  token_masked: string
}

export interface TavilyPoolStats {
  total_requests: number
  success_requests: number
  failed_requests: number
  success_rate: number
  avg_latency_ms: number
}

export type TavilyImportFileType = 'json' | 'csv'
export type TavilyImportMergeMode = 'skip' | 'overwrite' | 'error'

export interface TavilyAccountsImportRequest {
  file_type: TavilyImportFileType
  merge_mode: TavilyImportMergeMode
  content: string
}

export interface TavilyAccountsImportResult {
  stats: {
    total: number
    created: number
    updated: number
    skipped: number
    failed: number
    api_keys_created: number
  }
  errors: Array<{
    row: number
    email: string | null
    reason: string
  }>
}
