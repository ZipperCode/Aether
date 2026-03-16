export interface TavilyAccount {
  id: string
  email: string
  status: string
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
