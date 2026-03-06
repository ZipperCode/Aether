import apiClient from './client'
import { cachedRequest, buildCacheKey } from '@/utils/cache'

// LDAP 配置导出结构
export interface LDAPConfigExport {
  server_url: string
  bind_dn: string
  bind_password?: string
  base_dn: string
  user_search_filter?: string
  username_attr?: string
  email_attr?: string
  display_name_attr?: string
  is_enabled?: boolean
  is_exclusive?: boolean
  use_starttls?: boolean
  connect_timeout?: number
}

// OAuth Provider 导出结构
export interface OAuthProviderExport {
  provider_type: string
  display_name: string
  client_id: string
  client_secret?: string
  authorization_url_override?: string | null
  token_url_override?: string | null
  userinfo_url_override?: string | null
  scopes?: string[] | null
  redirect_uri: string
  frontend_callback_url: string
  attribute_mapping?: Record<string, unknown>
  extra_config?: Record<string, unknown>
  is_enabled?: boolean
}

// 配置导出数据结构
export interface ConfigExportData {
  version: string
  exported_at: string
  global_models: GlobalModelExport[]
  providers: ProviderExport[]
  ldap_config?: LDAPConfigExport | null
  oauth_providers?: OAuthProviderExport[]
}

// 用户导出数据结构
export interface UsersExportData {
  version: string
  exported_at: string
  users: UserExport[]
  standalone_keys?: StandaloneKeyExport[]
}

export interface UserExport {
  email: string
  username: string
  password_hash: string
  role: string
  allowed_providers?: string[] | null
  allowed_api_formats?: string[] | null
  allowed_models?: string[] | null
  model_capability_settings?: Record<string, Record<string, boolean>>
  quota_usd?: number | null
  used_usd?: number
  total_usd?: number
  is_active: boolean
  api_keys: UserApiKeyExport[]
}

export interface UserApiKeyExport {
  key_hash: string
  key_encrypted?: string | null
  name?: string | null
  is_standalone: boolean
  balance_used_usd?: number
  current_balance_usd?: number | null
  allowed_providers?: string[] | null
  allowed_api_formats?: string[] | null
  allowed_models?: string[] | null
  rate_limit?: number | null  // null = 无限制
  concurrent_limit?: number | null
  force_capabilities?: Record<string, boolean>
  is_active: boolean
  expires_at?: string | null
  auto_delete_on_expiry?: boolean
  total_requests?: number
  total_cost_usd?: number
}

// 独立余额 Key 导出结构（与 UserApiKeyExport 相同，但不包含 is_standalone）
export type StandaloneKeyExport = Omit<UserApiKeyExport, 'is_standalone'>

export interface GlobalModelExport {
  name: string
  display_name: string
  default_price_per_request?: number | null
  default_tiered_pricing: Record<string, unknown>
  supported_capabilities?: string[] | null
  config?: Record<string, unknown>
  is_active: boolean
}

export interface ProviderExport {
  name: string
  description?: string | null
  website?: string | null
  billing_type?: string | null
  monthly_quota_usd?: number | null
  quota_reset_day?: number
  rpm_limit?: number | null
  provider_priority?: number
  is_active: boolean
  concurrent_limit?: number | null
  max_retries?: number | null
  proxy?: Record<string, unknown>
  config?: Record<string, unknown>
  endpoints: EndpointExport[]
  api_keys: ProviderKeyExport[]
  models: ModelExport[]
}

export interface EndpointExport {
  api_format: string
  base_url: string
  headers?: Record<string, unknown>
  max_retries?: number
  is_active: boolean
  custom_path?: string | null
  config?: Record<string, unknown>
  proxy?: Record<string, unknown>
}

export interface ProviderKeyExport {
  api_key: string
  name?: string | null
  note?: string | null
  api_formats: string[]
  rate_multipliers?: Record<string, number> | null
  internal_priority?: number
  global_priority_by_format?: Record<string, number> | null
  rpm_limit?: number | null
  allowed_models?: string[] | null
  capabilities?: Record<string, boolean>
  cache_ttl_minutes?: number
  max_probe_interval_minutes?: number
  is_active: boolean
}

export interface ModelExport {
  global_model_name: string | null
  provider_model_name: string
  provider_model_mappings?: Record<string, unknown>
  price_per_request?: number | null
  tiered_pricing?: Record<string, unknown>
  supports_vision?: boolean | null
  supports_function_calling?: boolean | null
  supports_streaming?: boolean | null
  supports_extended_thinking?: boolean | null
  supports_image_generation?: boolean | null
  is_active: boolean
  config?: Record<string, unknown>
}

// 邮件模板接口
export interface EmailTemplateInfo {
  type: string
  name: string
  variables: string[]
  subject: string
  html: string
  is_custom: boolean
  default_subject?: string
  default_html?: string
}

export interface EmailTemplatesResponse {
  templates: EmailTemplateInfo[]
}

export interface EmailTemplatePreviewResponse {
  html: string
  variables: Record<string, string>
}

export interface EmailTemplateResetResponse {
  message: string
  template: {
    type: string
    name: string
    subject: string
    html: string
  }
}

// 检查更新响应
export interface CheckUpdateResponse {
  current_version: string
  latest_version: string | null
  has_update: boolean
  release_url: string | null
  release_notes: string | null
  published_at: string | null
  error: string | null
}

export interface SiteSyncRun {
  id: string
  trigger_source: string
  status: string
  error_message?: string | null
  dry_run: boolean
  total_accounts: number
  total_providers: number
  matched_providers: number
  updated_providers: number
  skipped_no_provider_ops: number
  skipped_no_cookie: number
  skipped_not_changed: number
  started_at?: string | null
  finished_at?: string | null
  created_at?: string | null
}

export interface SiteSyncItem {
  id: string
  domain: string
  site_url?: string | null
  provider_id?: string | null
  provider_name?: string | null
  status: string
  message?: string | null
  cookie_field?: string | null
  before_fingerprint?: string | null
  after_fingerprint?: string | null
  created_at?: string | null
}

export interface SiteCheckinRun {
  id: string
  trigger_source: string
  status: string
  error_message?: string | null
  total_providers: number
  success_count: number
  failed_count: number
  skipped_count: number
  started_at?: string | null
  finished_at?: string | null
  created_at?: string | null
}

export interface SiteCheckinItem {
  id: string
  provider_id?: string | null
  provider_name?: string | null
  provider_domain?: string | null
  status: string
  message?: string | null
  manual_verification_required?: boolean
  balance_total?: number | null
  balance_currency?: string | null
  created_at?: string | null
}

export interface SiteManagementAccount {
  site_url: string
  domain: string
  provider_id?: string | null
  provider_name?: string | null
  checkin_enabled?: boolean
  auth_type: string
  user_id?: string | null
  access_token?: string | null
  cookie?: string | null
}

export interface SiteSyncTriggerResponse {
  run_id: string
  total_accounts: number
  total_providers: number
  matched_providers: number
  updated_providers: number
  skipped_no_provider_ops: number
  skipped_no_cookie: number
  skipped_not_changed: number
  checkin_pref_updated?: number
  dry_run: boolean
}

export interface SiteManualCheckinResponse {
  status: string
  action_type: string
  data: unknown
  message?: string | null
  executed_at: string
  response_time_ms?: number | null
  cache_ttl_seconds: number
}

export interface SiteProviderCheckinStatus {
  status: string
  message?: string | null
  manual_verification_required?: boolean
  created_at?: string | null
}

// LDAP 配置响应
export interface LdapConfigResponse {
  server_url: string | null
  bind_dn: string | null
  base_dn: string | null
  has_bind_password: boolean
  user_search_filter: string
  username_attr: string
  email_attr: string
  display_name_attr: string
  is_enabled: boolean
  is_exclusive: boolean
  use_starttls: boolean
  connect_timeout: number
}

// LDAP 配置更新请求
export interface LdapConfigUpdateRequest {
  server_url: string
  bind_dn: string
  bind_password?: string
  base_dn: string
  user_search_filter?: string
  username_attr?: string
  email_attr?: string
  display_name_attr?: string
  is_enabled?: boolean
  is_exclusive?: boolean
  use_starttls?: boolean
  connect_timeout?: number
}

// LDAP 连接测试响应
export interface LdapTestResponse {
  success: boolean
  message: string
}

// Provider 模型查询响应
export interface ProviderModelsQueryResponse {
  success: boolean
  data: {
    models: Array<{
      id: string
      object?: string
      created?: number
      owned_by?: string
      display_name?: string
      api_format?: string
    }>
    error?: string
    from_cache?: boolean
  }
  provider: {
    id: string
    name: string
    display_name: string
  }
}

export interface ConfigImportRequest extends ConfigExportData {
  merge_mode: 'skip' | 'overwrite' | 'error'
}

export interface UsersImportRequest extends UsersExportData {
  merge_mode: 'skip' | 'overwrite' | 'error'
}

export interface UsersImportResponse {
  message: string
  stats: {
    users: { created: number; updated: number; skipped: number }
    api_keys: { created: number; skipped: number }
    standalone_keys?: { created: number; skipped: number }
    errors: string[]
  }
}

export interface ConfigImportResponse {
  message: string
  stats: {
    global_models: { created: number; updated: number; skipped: number }
    providers: { created: number; updated: number; skipped: number }
    endpoints: { created: number; updated: number; skipped: number }
    keys: { created: number; updated: number; skipped: number }
    models: { created: number; updated: number; skipped: number }
    ldap?: { created: number; updated: number; skipped: number }
    oauth?: { created: number; updated: number; skipped: number }
    errors: string[]
  }
}

// API密钥管理相关接口定义
export interface AdminApiKey {
  id: string // UUID
  user_id: string // UUID
  user_email?: string
  username?: string
  name?: string
  key_display?: string  // 脱敏后的密钥显示
  is_active: boolean
  is_locked: boolean  // 管理员锁定标志
  is_standalone: boolean  // 是否为独立余额Key
  balance_used_usd?: number  // 已使用余额（仅独立Key）
  current_balance_usd?: number | null  // 当前余额（独立Key预付费模式，null表示无限制）
  total_requests?: number
  total_tokens?: number
  total_cost_usd?: number
  rate_limit?: number | null  // null = 无限制
  allowed_providers?: string[] | null  // 允许的提供商列表
  allowed_api_formats?: string[] | null  // 允许的 API 格式列表
  allowed_models?: string[] | null  // 允许的模型列表
  auto_delete_on_expiry?: boolean  // 过期后是否自动删除
  last_used_at?: string
  expires_at?: string
  created_at: string
  updated_at?: string
}

export interface CreateStandaloneApiKeyRequest {
  name?: string
  allowed_providers?: string[] | null
  allowed_api_formats?: string[] | null
  allowed_models?: string[] | null
  rate_limit?: number | null  // null = 无限制
  expires_at?: string | null  // ISO 日期字符串，如 "2025-12-31"，null = 永不过期
  initial_balance_usd: number  // 初始余额，必须设置
  auto_delete_on_expiry?: boolean  // 过期后是否自动删除
}

export interface AdminApiKeysResponse {
  api_keys: AdminApiKey[]
  total: number
  limit: number
  skip: number
}

export interface LeaderboardItem {
  rank: number
  id: string
  name: string
  value: number
  requests: number
  tokens: number
  cost: number
}

export interface LeaderboardResponse {
  items: LeaderboardItem[]
  total: number
  metric: string
  start_date?: string | null
  end_date?: string | null
}

export interface CostForecastResponse {
  history: Array<{ date: string; total_cost: number }>
  forecast: Array<{ date: string; total_cost: number }>
  slope: number
  intercept: number
  start_date: string
  end_date: string
}

export interface CostSavingsResponse {
  cache_read_tokens: number
  cache_read_cost: number
  cache_creation_cost: number
  estimated_full_cost: number
  cache_savings: number
}

export interface QuotaUsageProvider {
  id: string
  name: string
  quota_usd: number
  used_usd: number
  remaining_usd: number
  usage_percent: number
  quota_expires_at?: string | null
  estimated_exhaust_at?: string | null
}

export interface QuotaUsageResponse {
  providers: QuotaUsageProvider[]
}

export interface PercentileItem {
  date: string
  p50_response_time_ms?: number | null
  p90_response_time_ms?: number | null
  p99_response_time_ms?: number | null
  p50_first_byte_time_ms?: number | null
  p90_first_byte_time_ms?: number | null
  p99_first_byte_time_ms?: number | null
}

export interface ErrorDistributionItem {
  category: string
  count: number
}

export interface ErrorTrendItem {
  date: string
  total: number
  categories: Record<string, number>
}

export interface ErrorDistributionResponse {
  distribution: ErrorDistributionItem[]
  trend: ErrorTrendItem[]
}

export interface ComparisonMetric {
  total_requests: number
  total_tokens: number
  total_cost: number
  actual_total_cost: number
  avg_response_time_ms: number
  error_requests: number
}

export interface ComparisonResponse {
  current: ComparisonMetric
  comparison: ComparisonMetric
  change_percent: Record<string, number | null>
  current_start: string
  current_end: string
  comparison_start: string
  comparison_end: string
}


export interface ApiKeyToggleResponse {
  id: string // UUID
  is_active: boolean
  message: string
}

export interface ApiKeyLockResponse {
  id: string // UUID
  is_locked: boolean
  message: string
}

async function purge<T>(target: string): Promise<T> {
  const response = await apiClient.post<T>(`/api/admin/system/purge/${target}`)
  return response.data
}

// 管理员API密钥管理相关API
export const adminApi = {
  // 获取所有独立余额Keys列表
  async getAllApiKeys(params?: {
    skip?: number
    limit?: number
    is_active?: boolean
  }): Promise<AdminApiKeysResponse> {
    const response = await apiClient.get<AdminApiKeysResponse>('/api/admin/api-keys', {
      params
    })
    return response.data
  },

  // 创建独立余额Key
  async createStandaloneApiKey(data: CreateStandaloneApiKeyRequest): Promise<AdminApiKey & { key: string }> {
    const response = await apiClient.post<AdminApiKey & { key: string }>(
      '/api/admin/api-keys',
      data
    )
    return response.data
  },

  // 更新独立余额Key
  async updateApiKey(keyId: string, data: Partial<CreateStandaloneApiKeyRequest>): Promise<AdminApiKey & { message: string }> {
    const response = await apiClient.put<AdminApiKey & { message: string }>(
      `/api/admin/api-keys/${keyId}`,
      data
    )
    return response.data
  },

  // 切换API密钥状态（启用/禁用）
  async toggleApiKey(keyId: string): Promise<ApiKeyToggleResponse> {
    const response = await apiClient.patch<ApiKeyToggleResponse>(
      `/api/admin/api-keys/${keyId}`
    )
    return response.data
  },

  // 删除API密钥
  async deleteApiKey(keyId: string): Promise<{ message: string }> {
    const response = await apiClient.delete<{ message: string}>(
      `/api/admin/api-keys/${keyId}`
    )
    return response.data
  },

  // 切换API密钥锁定状态（锁定/解锁）
  async toggleLockApiKey(keyId: string): Promise<ApiKeyLockResponse> {
    const response = await apiClient.patch<ApiKeyLockResponse>(
      `/api/admin/api-keys/${keyId}/lock`
    )
    return response.data
  },

  // 为独立余额Key调整余额
  async addApiKeyBalance(keyId: string, amountUsd: number): Promise<AdminApiKey & { message: string }> {
    const response = await apiClient.patch<AdminApiKey & { message: string }>(
      `/api/admin/api-keys/${keyId}/balance`,
      { amount_usd: amountUsd }
    )
    return response.data
  },

  // 重置独立余额Key的已使用额度
  async resetApiKeyUsage(keyId: string): Promise<AdminApiKey & { message: string }> {
    const response = await apiClient.patch<AdminApiKey & { message: string }>(
      `/api/admin/api-keys/${keyId}/reset-usage`
    )
    return response.data
  },

  // 获取API密钥详情（可选包含完整密钥）
  async getApiKeyDetail(keyId: string, includeKey: boolean = false): Promise<AdminApiKey & { key?: string }> {
    const response = await apiClient.get<AdminApiKey & { key?: string }>(
      `/api/admin/api-keys/${keyId}`,
      { params: { include_key: includeKey } }
    )
    return response.data
  },

  // 获取完整的API密钥（用于复制）- 便捷方法
  async getFullApiKey(keyId: string): Promise<{ key: string }> {
    const response = await apiClient.get<{ key: string }>(
      `/api/admin/api-keys/${keyId}`,
      { params: { include_key: true } }
    )
    return response.data
  },

  // 系统配置相关
  // 获取所有系统配置
  async getAllSystemConfigs(): Promise<Array<{ key: string; value: unknown; description?: string }>> {
    const response = await apiClient.get<Array<{ key: string; value: unknown; description?: string }>>('/api/admin/system/configs')
    return response.data
  },

  // 获取特定系统配置
  async getSystemConfig(key: string): Promise<{ key: string; value: unknown }> {
    const response = await apiClient.get<{ key: string; value: unknown }>(
      `/api/admin/system/configs/${key}`
    )
    return response.data
  },

  // 更新系统配置
  async updateSystemConfig(
    key: string,
    value: unknown,
    description?: string
  ): Promise<{ key: string; value: unknown; description?: string }> {
    const response = await apiClient.put<{ key: string; value: unknown; description?: string }>(
      `/api/admin/system/configs/${key}`,
      { value, description }
    )
    return response.data
  },

  // 删除系统配置
  async deleteSystemConfig(key: string): Promise<{ message: string }> {
    const response = await apiClient.delete<{ message: string }>(
      `/api/admin/system/configs/${key}`
    )
    return response.data
  },

  // 获取系统统计
  async getSystemStats(): Promise<Record<string, unknown>> {
    const response = await apiClient.get<Record<string, unknown>>('/api/admin/system/stats')
    return response.data
  },

  // 获取可用的API格式列表
  async getApiFormats(): Promise<{ formats: Array<{ value: string; label: string; default_path: string; aliases: string[] }> }> {
    const response = await apiClient.get<{ formats: Array<{ value: string; label: string; default_path: string; aliases: string[] }> }>(
      '/api/admin/system/api-formats'
    )
    return response.data
  },

  // 导出配置
  async exportConfig(): Promise<ConfigExportData> {
    const response = await apiClient.get<ConfigExportData>('/api/admin/system/config/export')
    return response.data
  },

  // 导入配置
  async importConfig(data: ConfigImportRequest): Promise<ConfigImportResponse> {
    const response = await apiClient.post<ConfigImportResponse>(
      '/api/admin/system/config/import',
      data
    )
    return response.data
  },

  // 导出用户数据
  async exportUsers(): Promise<UsersExportData> {
    const response = await apiClient.get<UsersExportData>('/api/admin/system/users/export')
    return response.data
  },

  // 导入用户数据
  async importUsers(data: UsersImportRequest): Promise<UsersImportResponse> {
    const response = await apiClient.post<UsersImportResponse>(
      '/api/admin/system/users/import',
      data
    )
    return response.data
  },

  // 查询 Provider 可用模型（从上游 API 获取）
  async queryProviderModels(providerId: string, apiKeyId?: string, forceRefresh = false): Promise<ProviderModelsQueryResponse> {
    const response = await apiClient.post<ProviderModelsQueryResponse>(
      '/api/admin/provider-query/models',
      { provider_id: providerId, api_key_id: apiKeyId, force_refresh: forceRefresh }
    )
    return response.data
  },

  // 测试 SMTP 连接，支持传入未保存的配置
  async testSmtpConnection(config: Record<string, unknown> = {}): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post<{ success: boolean; message: string }>(
      '/api/admin/system/smtp/test',
      config
    )
    return response.data
  },

  // 邮件模板相关
  // 获取所有邮件模板
  async getEmailTemplates(): Promise<EmailTemplatesResponse> {
    const response = await apiClient.get<EmailTemplatesResponse>('/api/admin/system/email/templates')
    return response.data
  },

  // 获取指定类型的邮件模板
  async getEmailTemplate(templateType: string): Promise<EmailTemplateInfo> {
    const response = await apiClient.get<EmailTemplateInfo>(
      `/api/admin/system/email/templates/${templateType}`
    )
    return response.data
  },

  // 更新邮件模板
  async updateEmailTemplate(
    templateType: string,
    data: { subject?: string; html?: string }
  ): Promise<{ message: string }> {
    const response = await apiClient.put<{ message: string }>(
      `/api/admin/system/email/templates/${templateType}`,
      data
    )
    return response.data
  },

  // 预览邮件模板
  async previewEmailTemplate(
    templateType: string,
    data?: { html?: string } & Record<string, string>
  ): Promise<EmailTemplatePreviewResponse> {
    const response = await apiClient.post<EmailTemplatePreviewResponse>(
      `/api/admin/system/email/templates/${templateType}/preview`,
      data || {}
    )
    return response.data
  },

  // 重置邮件模板为默认值
  async resetEmailTemplate(templateType: string): Promise<EmailTemplateResetResponse> {
    const response = await apiClient.post<EmailTemplateResetResponse>(
      `/api/admin/system/email/templates/${templateType}/reset`
    )
    return response.data
  },

  // 获取系统版本信息
  async getSystemVersion(): Promise<{ version: string }> {
    const response = await apiClient.get<{ version: string }>(
      '/api/admin/system/version'
    )
    return response.data
  },

  // 检查系统更新
  async checkUpdate(): Promise<CheckUpdateResponse> {
    const response = await apiClient.get<CheckUpdateResponse>(
      '/api/admin/system/check-update'
    )
    return response.data
  },

  // Site management
  async triggerSiteSync(
    payload: { dry_run?: boolean; backup?: Record<string, unknown> }
  ): Promise<SiteSyncTriggerResponse> {
    const response = await apiClient.post<SiteSyncTriggerResponse>(
      '/api/admin/site-management/sync/trigger',
      payload
    )
    return response.data
  },

  async getSiteAccounts(): Promise<SiteManagementAccount[]> {
    const response = await apiClient.get<SiteManagementAccount[]>('/api/admin/site-management/accounts')
    return response.data
  },

  async applySiteAccountsSync(payload: {
    accounts: SiteManagementAccount[]
    dry_run?: boolean
  }): Promise<SiteSyncTriggerResponse> {
    const response = await apiClient.post<SiteSyncTriggerResponse>(
      '/api/admin/site-management/accounts/apply-sync',
      payload
    )
    return response.data
  },

  async checkinProvider(providerId: string): Promise<SiteManualCheckinResponse> {
    const response = await apiClient.post<SiteManualCheckinResponse>(
      `/api/admin/provider-ops/providers/${providerId}/checkin`
    )
    return response.data
  },

  async getSiteAccountsCheckinStatuses(
    providerIds: string[]
  ): Promise<Record<string, SiteProviderCheckinStatus>> {
    const response = await apiClient.post<Record<string, SiteProviderCheckinStatus>>(
      '/api/admin/site-management/accounts/checkin-statuses',
      { provider_ids: providerIds }
    )
    return response.data
  },

  async triggerSiteCheckin(): Promise<{ ok: boolean; latest_run_id?: string | null }> {
    const response = await apiClient.post('/api/admin/site-management/checkin/trigger')
    return response.data
  },

  async getSiteSyncRuns(limit = 20): Promise<SiteSyncRun[]> {
    const response = await apiClient.get<SiteSyncRun[]>('/api/admin/site-management/sync-runs', {
      params: { limit },
    })
    return response.data
  },

  async getSiteSyncRunItems(runId: string, limit = 500): Promise<SiteSyncItem[]> {
    const response = await apiClient.get<SiteSyncItem[]>(
      `/api/admin/site-management/sync-runs/${runId}/items`,
      { params: { limit } }
    )
    return response.data
  },

  async getSiteCheckinRuns(limit = 20): Promise<SiteCheckinRun[]> {
    const response = await apiClient.get<SiteCheckinRun[]>(
      '/api/admin/site-management/checkin-runs',
      { params: { limit } }
    )
    return response.data
  },

  async getSiteCheckinRunItems(runId: string, limit = 500): Promise<SiteCheckinItem[]> {
    const response = await apiClient.get<SiteCheckinItem[]>(
      `/api/admin/site-management/checkin-runs/${runId}/items`,
      { params: { limit } }
    )
    return response.data
  },

  // LDAP 配置相关
  // 获取 LDAP 配置
  async getLdapConfig(): Promise<LdapConfigResponse> {
    const response = await apiClient.get<LdapConfigResponse>('/api/admin/ldap/config')
    return response.data
  },

  // 更新 LDAP 配置
  async updateLdapConfig(config: LdapConfigUpdateRequest): Promise<{ message: string }> {
    const response = await apiClient.put<{ message: string }>(
      '/api/admin/ldap/config',
      config
    )
    return response.data
  },

  // 测试 LDAP 连接
  async testLdapConnection(config: LdapConfigUpdateRequest): Promise<LdapTestResponse> {
    const response = await apiClient.post<LdapTestResponse>('/api/admin/ldap/test', config)
    return response.data
  },

  // Stats / Leaderboards
  async getLeaderboardUsers(params?: {
    start_date?: string
    end_date?: string
    preset?: string
    timezone?: string
    tz_offset_minutes?: number
    metric?: 'requests' | 'tokens' | 'cost'
    order?: 'asc' | 'desc'
    limit?: number
    offset?: number
    provider_name?: string
    model?: string
    include_inactive?: boolean
    exclude_admin?: boolean
  }): Promise<LeaderboardResponse> {
    const cacheKey = buildCacheKey('admin:stats:leaderboard:users', params)
    return cachedRequest(
      cacheKey,
      async () => {
        const response = await apiClient.get<LeaderboardResponse>('/api/admin/stats/leaderboard/users', {
          params
        })
        return response.data
      },
      20 * 1000
    )
  },

  async getLeaderboardApiKeys(params?: {
    start_date?: string
    end_date?: string
    preset?: string
    timezone?: string
    tz_offset_minutes?: number
    metric?: 'requests' | 'tokens' | 'cost'
    order?: 'asc' | 'desc'
    limit?: number
    offset?: number
    provider_name?: string
    model?: string
    include_inactive?: boolean
    exclude_admin?: boolean
  }): Promise<LeaderboardResponse> {
    const cacheKey = buildCacheKey('admin:stats:leaderboard:api-keys', params)
    return cachedRequest(
      cacheKey,
      async () => {
        const response = await apiClient.get<LeaderboardResponse>('/api/admin/stats/leaderboard/api-keys', {
          params
        })
        return response.data
      },
      20 * 1000
    )
  },

  async getLeaderboardModels(params?: {
    start_date?: string
    end_date?: string
    preset?: string
    timezone?: string
    tz_offset_minutes?: number
    metric?: 'requests' | 'tokens' | 'cost'
    order?: 'asc' | 'desc'
    limit?: number
    offset?: number
    provider_name?: string
    model?: string
  }): Promise<LeaderboardResponse> {
    const cacheKey = buildCacheKey('admin:stats:leaderboard:models', params)
    return cachedRequest(
      cacheKey,
      async () => {
        const response = await apiClient.get<LeaderboardResponse>('/api/admin/stats/leaderboard/models', {
          params
        })
        return response.data
      },
      20 * 1000
    )
  },

  async getCostForecast(params?: {
    start_date?: string
    end_date?: string
    preset?: string
    timezone?: string
    tz_offset_minutes?: number
    days?: number
    forecast_days?: number
  }): Promise<CostForecastResponse> {
    const cacheKey = buildCacheKey('admin:stats:cost:forecast', params)
    return cachedRequest(
      cacheKey,
      async () => {
        const response = await apiClient.get<CostForecastResponse>('/api/admin/stats/cost/forecast', {
          params
        })
        return response.data
      },
      30 * 1000
    )
  },

  async getCostSavings(params?: {
    start_date?: string
    end_date?: string
    preset?: string
    timezone?: string
    tz_offset_minutes?: number
    provider_name?: string
    model?: string
  }): Promise<CostSavingsResponse> {
    const cacheKey = buildCacheKey('admin:stats:cost:savings', params)
    return cachedRequest(
      cacheKey,
      async () => {
        const response = await apiClient.get<CostSavingsResponse>('/api/admin/stats/cost/savings', {
          params
        })
        return response.data
      },
      30 * 1000
    )
  },

  async getQuotaUsage(): Promise<QuotaUsageResponse> {
    return cachedRequest(
      'admin:stats:providers:quota-usage',
      async () => {
        const response = await apiClient.get<QuotaUsageResponse>('/api/admin/stats/providers/quota-usage')
        return response.data
      },
      30 * 1000
    )
  },

  async getPercentiles(params?: {
    start_date?: string
    end_date?: string
    preset?: string
    timezone?: string
    tz_offset_minutes?: number
  }): Promise<PercentileItem[]> {
    const cacheKey = buildCacheKey('admin:stats:performance:percentiles', params)
    return cachedRequest(
      cacheKey,
      async () => {
        const response = await apiClient.get<PercentileItem[]>('/api/admin/stats/performance/percentiles', {
          params
        })
        return response.data
      },
      20 * 1000
    )
  },

  async getErrorDistribution(params?: {
    start_date?: string
    end_date?: string
    preset?: string
    timezone?: string
    tz_offset_minutes?: number
  }): Promise<ErrorDistributionResponse> {
    const cacheKey = buildCacheKey('admin:stats:errors:distribution', params)
    return cachedRequest(
      cacheKey,
      async () => {
        const response = await apiClient.get<ErrorDistributionResponse>('/api/admin/stats/errors/distribution', {
          params
        })
        return response.data
      },
      20 * 1000
    )
  },

  async getComparison(params: {
    current_start: string
    current_end: string
    comparison_type?: 'period' | 'year'
    timezone?: string
    tz_offset_minutes?: number
  }): Promise<ComparisonResponse> {
    const response = await apiClient.get<ComparisonResponse>('/api/admin/stats/comparison', {
      params
    })
    return response.data
  },

  // 数据清空
  purgeConfig: () => purge<{ message: string; deleted: Record<string, number> }>('config'),
  purgeUsers: () => purge<{ message: string; deleted: Record<string, number> }>('users'),
  purgeUsage: () => purge<{ message: string; deleted: Record<string, number> }>('usage'),
  purgeAuditLogs: () => purge<{ message: string; deleted: Record<string, number> }>('audit-logs'),
  purgeRequestBodies: () => purge<{ message: string; cleaned: Record<string, number> }>('request-bodies'),
  purgeStats: () => purge<{ message: string }>('stats'),

  async getTimeSeries(params?: {
    start_date?: string
    end_date?: string
    preset?: string
    granularity?: 'hour' | 'day' | 'week' | 'month'
    timezone?: string
    tz_offset_minutes?: number
    user_id?: string
    model?: string
    provider_name?: string
  }): Promise<Array<Record<string, unknown>>> {
    const cacheKey = buildCacheKey('admin:stats:time-series', params)
    return cachedRequest(
      cacheKey,
      async () => {
        const response = await apiClient.get<Array<Record<string, unknown>>>('/api/admin/stats/time-series', { params })
        return response.data
      },
      20 * 1000
    )
  },

}
