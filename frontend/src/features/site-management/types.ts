export interface WebDavSource {
  id: string
  name: string
  url: string
  username: string
  is_active: boolean
  sync_enabled: boolean
  last_sync_at: string | null
  last_sync_status: string | null
  created_at: string
  updated_at: string
  account_count: number
}

export interface CreateWebDavSourceRequest {
  name: string
  url: string
  username: string
  password: string
}

export interface UpdateWebDavSourceRequest {
  name?: string
  url?: string
  username?: string
  password?: string
  is_active?: boolean
  sync_enabled?: boolean
}

export interface SiteAccount {
  id: string
  webdav_source_id: string
  domain: string
  site_url: string | null
  architecture_id: string | null
  base_url: string | null
  auth_type: string
  checkin_enabled: boolean
  balance_sync_enabled: boolean
  is_active: boolean
  last_checkin_status: string | null
  last_checkin_message: string | null
  last_checkin_at: string | null
  last_balance_status: string | null
  last_balance_message: string | null
  last_balance_total: number | null
  last_balance_currency: string | null
  last_balance_at: string | null
  created_at: string
  updated_at: string
}

export interface SyncRun {
  id: string
  webdav_source_id: string | null
  trigger_source: string
  status: string
  error_message: string | null
  dry_run: boolean
  total_accounts: number
  started_at: string | null
  finished_at: string | null
  created_at: string
}

export interface SyncItem {
  id: string
  run_id: string
  domain: string
  site_url: string | null
  status: string
  message: string | null
  created_at: string
}

export interface CheckinRun {
  id: string
  trigger_source: string
  status: string
  error_message: string | null
  total_providers: number
  success_count: number
  failed_count: number
  skipped_count: number
  started_at: string | null
  finished_at: string | null
  created_at: string
}

export interface CheckinItem {
  id: string
  run_id: string
  provider_id: string | null
  provider_name: string | null
  provider_domain: string | null
  status: string
  message: string | null
  balance_total: number | null
  balance_currency: string | null
  created_at: string
}

export interface PaginatedResponse<T> {
  items: T[]
  total: number
  page: number
  page_size: number
}

export interface ActionResult {
  status: string
  action_type: string
  data: unknown
  message: string | null
  executed_at: string
  response_time_ms: number | null
}
