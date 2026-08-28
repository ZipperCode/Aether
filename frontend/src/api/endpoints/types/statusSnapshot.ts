export interface OAuthStatusSnapshot {
  code: 'none' | 'valid' | 'expiring' | 'expired' | 'invalid' | 'reauth_required' | 'check_failed'
  label?: string | null
  reason?: string | null
  expires_at?: number | null
  invalid_at?: number | null
  source?: string | null
  requires_reauth?: boolean
  usable_until_expiry?: boolean
  expiring_soon?: boolean
}

export interface AccountStatusSnapshot {
  code: string
  label?: string | null
  reason?: string | null
  blocked: boolean
  source?: string | null
  recoverable?: boolean
}

export interface QuotaWindowUsageSnapshot {
  request_count?: number | null
  total_tokens?: number | null
  total_cost_usd?: number | string | null
}

export type DecimalJsonValue = number | string | null

export interface QuotaWindowSnapshot {
  code: string
  label?: string | null
  scope?: 'account' | 'workspace' | 'model' | string
  unit?: 'percent' | 'count' | 'usd' | 'tokens' | string
  model?: string | null
  used_ratio?: DecimalJsonValue
  remaining_ratio?: DecimalJsonValue
  used_value?: DecimalJsonValue
  remaining_value?: DecimalJsonValue
  limit_value?: DecimalJsonValue
  reset_at?: number | null
  reset_at_text?: string | null
  reset_seconds?: number | null
  window_minutes?: number | null
  usage_reset_at?: number | null
  is_exhausted?: boolean | null
  usage?: QuotaWindowUsageSnapshot | null
}

export interface QuotaCreditsSnapshot {
  has_credits?: boolean | null
  balance?: number | null
  remaining?: number | null
  consumed?: number | null
  total?: number | null
  unlimited?: boolean | null
  is_free_tier?: boolean | null
  is_management_key?: boolean | null
  is_provisioning_key?: boolean | null
  limit_reset?: string | number | null
  expires_at?: string | number | null
  trace_id?: string | null
  updated_at?: number | null
}

export interface QuotaResetCreditSnapshot {
  id?: string | null
  display_key?: string | null
  status?: string | null
  granted_at?: number | null
  expires_at?: number | null
  remaining_seconds?: number | null
}

export interface QuotaResetCreditsSnapshot {
  available_count?: number | null
  updated_at?: number | null
  detail_source?: string | null
  detail_status?: string | null
  detail_error?: string | null
  credits?: QuotaResetCreditSnapshot[] | null
}

export interface QuotaBalanceSnapshot {
  unit: string
  available?: DecimalJsonValue
  total?: DecimalJsonValue
  granted?: DecimalJsonValue
  topped_up?: DecimalJsonValue
  used?: DecimalJsonValue
}

export interface QuotaRefreshStateSnapshot {
  last_attempt_at?: number | null
  last_success_at?: number | null
  error?: string | null
  next_eligible_at?: number | null
  failure_count?: number | null
}

export interface QuotaStatusSnapshot {
  schema_version?: number | null
  kind?: 'balance' | 'subscription' | string | null
  version?: number | null
  provider_type?: string | null
  code: 'unknown' | 'ok' | 'exhausted' | 'cooldown' | 'forbidden' | 'banned' | string
  label?: string | null
  reason?: string | null
  freshness?: 'fresh' | 'stale' | 'unknown' | 'error' | string | null
  source?: string | null
  observed_at?: number | null
  exhausted: boolean
  unlimited?: boolean | null
  is_free_tier?: boolean | null
  is_management_key?: boolean | null
  is_provisioning_key?: boolean | null
  limit_reset?: string | number | null
  expires_at?: string | number | null
  membership_level?: string | null
  subscription_type?: string | null
  parallel_limit?: DecimalJsonValue
  usage_ratio?: number | null
  updated_at?: number | null
  reset_at?: number | null
  reset_seconds?: number | null
  plan_type?: string | null
  pool_tier?: string | null
  token_plan_scope?: 'personal' | 'team' | string | null
  token_plan_status?: string | null
  token_plan_error?: string | null
  token_plan_scheduling_blocked?: boolean | null
  balance_status?: string | null
  balance_insufficient?: boolean | null
  credits?: QuotaCreditsSnapshot | null
  reset_credits?: QuotaResetCreditsSnapshot | null
  allowed_models_count?: number | null
  rate_limit?: Record<string, unknown> | null
  balances?: QuotaBalanceSnapshot[] | null
  refresh_state?: QuotaRefreshStateSnapshot | null
  /** Nous account and billing summary. Decimal values may arrive as strings. */
  balance_usd?: number | string | null
  purchased_credits_remaining?: number | string | null
  total_usable_credits?: number | string | null
  current_period_end?: number | null
  billing_available?: boolean | null
  billing_stale?: boolean | null
  billing_source?: string | null
  billing_error?: string | null
  rate_limits?: {
    rpm?: number | null
    tpm?: number | null
    rph?: number | null
    tph?: number | null
    kind?: 'configured_limits' | string | null
  } | null
  windows?: QuotaWindowSnapshot[] | null
}

export interface ModelProbeStatusSnapshot {
  status: 'ok' | 'failed' | string
  model?: string | null
  api_format?: string | null
  tested_at?: number | null
  status_code?: number | null
  error?: string | null
  source?: 'admin_model_test' | string | null
}

export interface SchedulingStatusSnapshot {
  code: 'quota_suspected' | 'quota_exhausted' | string
  blocked: boolean
  requires_manual_recovery?: boolean
  source?: string | null
  confidence?: 'strong' | 'weak' | string | null
  confirmation_count?: number | null
  status_code?: number | null
  error_code?: string | null
  reason?: string | null
  first_observed_at?: number | null
  last_observed_at?: number | null
}

export interface ProviderKeyStatusSnapshot {
  oauth: OAuthStatusSnapshot
  account: AccountStatusSnapshot
  quota: QuotaStatusSnapshot
  scheduling?: SchedulingStatusSnapshot | null
  model_probe?: ModelProbeStatusSnapshot | null
}
