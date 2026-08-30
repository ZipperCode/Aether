import client from '../client'
import { buildCacheKey, cachedRequest, dedupedRequest } from '@/utils/cache'
import type {
  ClaudeCodeAdvancedConfig,
  FailoverRulesConfig,
  PoolAdvancedConfig,
  ProviderConfig,
  ProviderType,
  ProviderWithEndpointsSummary,
  ProxyConfig,
} from './types'
import {
  normalizeChatPiiRedactionProviderConfig as normalizeChatPiiRedactionProvider,
  normalizePoolAdvancedConfig as normalizePoolAdvanced,
} from './types'

interface ProviderRequestOptions {
  timeout?: number
}

interface ProviderReadOptions {
  timeout?: number
  cacheTtlMs?: number
}

/**
 * 获取 Providers 摘要（分页）
 */
export interface ProviderSummaryQuery {
  page?: number
  page_size?: number
  search?: string
  status?: string
  api_format?: string
  model_id?: string
}

export interface ProviderSummaryPageResponse {
  total: number
  page: number
  page_size: number
  items: ProviderWithEndpointsSummary[]
}

type ProviderSummaryResponse = ProviderSummaryPageResponse | ProviderWithEndpointsSummary[]

function normalizeProviderSummary(
  provider: ProviderWithEndpointsSummary,
): ProviderWithEndpointsSummary {
  return {
    ...provider,
    chat_pii_redaction: normalizeChatPiiRedactionProvider(provider.chat_pii_redaction),
    pool_advanced: normalizePoolAdvanced(provider.pool_advanced),
    codex_fingerprint_convergence_enabled: provider.codex_fingerprint_convergence_enabled ?? false,
    kiro_simulated_cache_enabled: provider.kiro_simulated_cache_enabled ?? false,
    max_transfer_count: provider.max_transfer_count ?? 0,
    max_transfer_timeout_seconds: provider.max_transfer_timeout_seconds ?? 0,
    responses_websocket_enabled: provider.responses_websocket_enabled ?? false,
  }
}

export async function getProvidersSummary(
  params: ProviderSummaryQuery = {},
  options: ProviderReadOptions = {},
): Promise<ProviderSummaryPageResponse> {
  const cacheTtlMs = options.cacheTtlMs ?? 0
  const cacheKey = buildCacheKey('providers:summary', params as Record<string, unknown>)
  return cachedRequest(
    cacheKey,
    async () => {
      const response = await client.get<ProviderSummaryResponse>(
        '/api/admin/providers/summary',
        {
          params,
          timeout: options.timeout,
        },
      )
      const data = response.data
      if (Array.isArray(data)) {
        return {
          total: data.length,
          page: params.page ?? 1,
          page_size: params.page_size ?? data.length,
          items: data.map(normalizeProviderSummary),
        }
      }

      return {
        ...data,
        items: (data.items ?? []).map(normalizeProviderSummary),
      }
    },
    cacheTtlMs,
  )
}

/**
 * 获取单个 Provider 的详细信息
 */
export async function getProvider(providerId: string): Promise<ProviderWithEndpointsSummary> {
  return dedupedRequest(`providers:detail:${providerId}`, async () => {
    const response = await client.get<ProviderWithEndpointsSummary>(`/api/admin/providers/${providerId}/summary`)
    return normalizeProviderSummary(response.data)
  })
}

export type ProviderUpdatePayload = Partial<{
  name: string
  provider_type: ProviderType
  description: string | null
  website: string
  provider_priority: number
  keep_priority_on_conversion: boolean
  responses_websocket_enabled: boolean
  billing_type: 'monthly_quota' | 'pay_as_you_go' | 'free_tier'
  monthly_quota_usd: number
  quota_reset_day: number
  quota_last_reset_at: string  // 周期开始时间
  quota_expires_at: string
  rpm_limit: number | null
  // 请求配置（从 Endpoint 迁移）
  max_retries: number
  max_transfer_count: number
  max_transfer_timeout_seconds: number
  stream_first_byte_timeout: number | null
  request_timeout: number | null
  proxy: ProxyConfig | null
  cache_ttl_minutes: number  // 0表示不支持缓存，>0表示支持缓存并设置TTL(分钟)
  max_probe_interval_minutes: number
  enable_format_conversion: boolean  // 是否允许格式转换（提供商级别开关）
  is_active: boolean
  claude_code_advanced: ClaudeCodeAdvancedConfig | null
  codex_fingerprint_convergence_enabled: boolean
  pool_advanced: PoolAdvancedConfig | null
  failover_rules: FailoverRulesConfig | null
  config: ProviderConfig | null
}>

/**
 * 更新 Provider 基础配置
 */
export async function updateProvider(
  providerId: string,
  data: ProviderUpdatePayload,
  requestOptions?: ProviderRequestOptions,
): Promise<ProviderWithEndpointsSummary> {
  const response = await client.patch(`/api/admin/providers/${providerId}`, data, requestOptions)
  return normalizeProviderSummary(response.data)
}

/**
 * 创建 Provider
 */
export async function createProvider(
  data: {
    name: string
    provider_type?: ProviderType
    description?: string
    website?: string
    billing_type?: 'monthly_quota' | 'pay_as_you_go' | 'free_tier'
    monthly_quota_usd?: number
    quota_reset_day?: number
    quota_last_reset_at?: string
    quota_expires_at?: string
    provider_priority?: number
    keep_priority_on_conversion?: boolean
    responses_websocket_enabled?: boolean
    is_active?: boolean
    max_retries?: number
    max_transfer_count?: number
    max_transfer_timeout_seconds?: number
    stream_first_byte_timeout?: number | null
    request_timeout?: number | null
    proxy?: ProxyConfig | null
    claude_code_advanced?: ClaudeCodeAdvancedConfig | null
    codex_fingerprint_convergence_enabled?: boolean
    pool_advanced?: PoolAdvancedConfig | null
    failover_rules?: FailoverRulesConfig | null
    config?: ProviderConfig | null
  }
): Promise<{ id: string; name: string; message?: string }> {
  const response = await client.post('/api/admin/providers/', data)
  return response.data
}

/**
 * 删除 Provider
 */
export interface ProviderDeleteSubmitResponse {
  task_id: string
  status: string
  message: string
}

export interface ProviderDeleteTaskResponse {
  task_id: string
  provider_id: string
  status: string
  stage: string
  total_keys: number
  deleted_keys: number
  total_endpoints: number
  deleted_endpoints: number
  message: string
}

export async function deleteProvider(providerId: string): Promise<ProviderDeleteSubmitResponse> {
  const response = await client.delete<ProviderDeleteSubmitResponse>(`/api/admin/providers/${providerId}`)
  return response.data
}

export async function getProviderDeleteTask(
  providerId: string,
  taskId: string,
): Promise<ProviderDeleteTaskResponse> {
  const response = await client.get<ProviderDeleteTaskResponse>(
    `/api/admin/providers/${providerId}/delete-task/${taskId}`,
  )
  return response.data
}

/**
 * 测试模型连接性
 */
export interface TestModelRequest {
  provider_id: string
  model_name: string
  api_key_id?: string
  api_key_ids?: string[]
  endpoint_id?: string
  message?: string
  api_format?: string
  mode?: 'global' | 'direct' | 'pool'
  apply_model_mapping?: boolean
  mapped_model_name?: string
  request_headers?: Record<string, unknown>
  request_body?: Record<string, unknown>
  request_id?: string
}

export interface TestModelResponse {
  success: boolean
  error?: string
  attempts?: TestAttemptDetail[]
  total_candidates?: number
  total_attempts?: number
  candidate_summary?: TestCandidateSummary
  data?: {
    response?: {
      status_code?: number
      error?: string | { message?: string }
      choices?: Array<{ message?: { content?: string } }>
    }
    content_preview?: string
  }
  provider?: {
    id: string
    name: string
    provider_type?: string
  }
  model?: string
}

export async function testModel(
  data: TestModelRequest,
  options: { signal?: AbortSignal } = {},
): Promise<TestModelResponse> {
  const response = await client.post('/api/admin/provider-query/test-model', data, {
    timeout: 10 * 60 * 1000,
    signal: options.signal,
  })
  return response.data
}

/** 能力检测规模：40 题快筛或 100 题复核。 */
export type ModelCapabilityMode = 'quick' | 'verify'

/** 能力检测题面语言；双语在每个维度内中英文各半。 */
export type ModelCapabilityLanguage = 'zh' | 'en' | 'bilingual'

/** 服务端允许返回的固定能力偏离结论。 */
export type ModelCapabilityVerdict =
  | 'profile_only'
  | 'no_large_deviation'
  | 'needs_verification'
  | 'no_significant_deviation'
  | 'significant_deviation'
  | 'inconclusive'

/** 无法判断时的机器可读原因。 */
export type ModelCapabilityInconclusiveReason =
  | 'total_timeout'
  | 'target_coverage'
  | 'reference_coverage'
  | 'paired_coverage'

/** 五个等权随机客观题维度。 */
export type ModelCapabilityDimension =
  | 'quantitative'
  | 'logical'
  | 'algorithmic'
  | 'language'
  | 'instruction'

/** 单题结果状态；只有 scored 会进入正确率分母。 */
export type ModelCapabilityItemStatus =
  | 'scored'
  | 'network_failure'
  | 'rate_limited'
  | 'timeout'
  | 'filtered'
  | 'refused'
  | 'truncated'
  | 'unparseable'
  | 'upstream_error'
  | 'cancelled'

/** 能力检测请求；浏览器不传题目、答案、模型名或 seed。 */
export interface TestModelCapabilityRequest {
  /** 目标提供商内部 ID。 */
  provider_id: string
  /** 目标 ProviderModel 内部 ID。 */
  model_id: string
  /** 本次固定使用的目标 endpoint ID。 */
  endpoint_id: string
  /** 本次固定使用的目标 Key ID。 */
  api_key_id: string
  /** 快筛或复核。 */
  mode: ModelCapabilityMode
  /** 中文、英文或双语题面。 */
  language: ModelCapabilityLanguage
  /** 是否读取目标模型已保存并重新校验的可信参考。 */
  use_saved_reference: boolean
  /** 客户端诊断 ID；不参与出题。 */
  request_id?: string
}

/** 返回结果中的固定候选，不包含 URL、Key 名称或凭据。 */
export interface ModelCapabilitySubject {
  /** 提供商 ID。 */
  provider_id: string
  /** ProviderModel ID。 */
  model_id: string
  /** endpoint ID。 */
  endpoint_id: string
  /** Key ID。 */
  api_key_id: string
  /** 映射前模型名。 */
  requested_model: string
  /** 实际发往上游的模型名。 */
  effective_model: string
  /** 实际上游协议格式。 */
  api_format: string
}

/** 上游可取得的 token 与费用；缺失字段不会由前端猜测。 */
export interface ModelCapabilityUsage {
  /** 输入 token。 */
  input_tokens: number | null
  /** 输出 token。 */
  output_tokens: number | null
  /** 总 token。 */
  total_tokens: number | null
  /** 上游明确给出的美元成本。 */
  cost_usd: number | null
}

/** 单题对目标或参考的脱敏观察。 */
export interface ModelCapabilityObservation {
  /** 解析出的 A-D 选项。 */
  parsed_option: 'A' | 'B' | 'C' | 'D' | null
  /** 失败分类或已评分状态。 */
  status: ModelCapabilityItemStatus
  /** 已评分时的正确性。 */
  correct: boolean | null
  /** 上游耗时。 */
  latency_ms: number | null
  /** 单题可取得的 token 与费用。 */
  usage: ModelCapabilityUsage | null
}

/** 逐题结果；刻意不含题面、原始响应或推理。 */
export interface ModelCapabilityItemResult {
  /** 服务端题目 ID。 */
  question_id: string
  /** 能力维度。 */
  dimension: ModelCapabilityDimension
  /** 实际题面语言。 */
  language: 'zh' | 'en'
  /** 唯一正确选项。 */
  expected_option: 'A' | 'B' | 'C' | 'D'
  /** 目标观察。 */
  target: ModelCapabilityObservation
  /** 参考观察；未启用参考时为 null。 */
  reference: ModelCapabilityObservation | null
}

/** 不进入正确率分母的失败分类计数。 */
export interface ModelCapabilityFailureCounts {
  /** 网络失败。 */
  network_failure: number
  /** 限流。 */
  rate_limited: number
  /** 超时。 */
  timeout: number
  /** 内容过滤。 */
  filtered: number
  /** 明确拒答。 */
  refused: number
  /** 输出截断。 */
  truncated: number
  /** 无法解析。 */
  unparseable: number
  /** 其他上游错误。 */
  upstream_error: number
  /** 已取消。 */
  cancelled: number
}

/** 单个能力维度的评分。 */
export interface ModelCapabilityDimensionMetrics {
  /** 能力维度。 */
  dimension: ModelCapabilityDimension
  /** 计划题数。 */
  planned: number
  /** 已评分题数。 */
  scored: number
  /** 正确题数。 */
  correct: number
  /** 已解析覆盖率。 */
  coverage: number
  /** 正确率；无样本时为 null。 */
  score: number | null
}

/** 目标或参考的总体评分。 */
export interface ModelCapabilityMetrics {
  /** 计划题数。 */
  planned: number
  /** 已评分题数。 */
  scored: number
  /** 正确题数。 */
  correct: number
  /** 已解析覆盖率。 */
  coverage: number
  /** 五维等权总体分。 */
  score: number | null
  /** 95% Wilson 下界。 */
  wilson_low: number | null
  /** 95% Wilson 上界。 */
  wilson_high: number | null
  /** 五维评分。 */
  dimensions: ModelCapabilityDimensionMetrics[]
  /** 失败分类。 */
  failures: ModelCapabilityFailureCounts
  /** 单题上游耗时之和。 */
  elapsed_ms: number
  /** 可取得的 token 与费用汇总。 */
  usage: ModelCapabilityUsage | null
}

/** 目标与参考的同题配对统计。 */
export interface ModelCapabilityComparison {
  /** 双方均解析的题数。 */
  paired: number
  /** 同题配对覆盖率。 */
  paired_coverage: number
  /** 仅参考正确的题数。 */
  reference_only_correct: number
  /** 仅目标正确的题数。 */
  target_only_correct: number
  /** 参考分减目标分。 */
  score_gap: number | null
  /** 单侧精确 McNemar p 值。 */
  p_value: number
}

/** 能力检测实际采用的确定性请求轮廓。 */
export interface ModelCapabilityRequestProfile {
  /** 是否流式。 */
  stream: boolean
  /** 温度。 */
  temperature: number
  /** 最大输出 token。 */
  max_output_tokens: number
  /** 是否提供工具。 */
  tools_enabled: boolean
  /** 是否启用搜索。 */
  search_enabled: boolean
}

/** 完整能力检测结果。 */
export interface TestModelCapabilityResponse {
  /** 单次运行 ID。 */
  run_id: string
  /** 题集合同版本。 */
  suite_version: string
  /** 服务端随机 seed。 */
  seed: string
  /** 实际模式。 */
  mode: ModelCapabilityMode
  /** 实际语言配置。 */
  language: ModelCapabilityLanguage
  /** 固定结论。 */
  verdict: ModelCapabilityVerdict
  /** 无法判断原因。 */
  inconclusive_reason: ModelCapabilityInconclusiveReason | null
  /** 目标固定候选。 */
  target: ModelCapabilitySubject
  /** 参考固定候选。 */
  reference: ModelCapabilitySubject | null
  /** 目标评分。 */
  target_metrics: ModelCapabilityMetrics
  /** 参考评分。 */
  reference_metrics: ModelCapabilityMetrics | null
  /** 配对统计。 */
  comparison: ModelCapabilityComparison | null
  /** 逐题脱敏结果。 */
  items: ModelCapabilityItemResult[]
  /** 同步请求墙钟耗时。 */
  elapsed_ms: number
  /** 实际请求轮廓。 */
  request_profile: ModelCapabilityRequestProfile
  /** 身份认证边界免责声明。 */
  disclaimer: string
}

/** 调用独立能力检测 API；AbortSignal 取消时浏览器会丢弃后端剩余 future。 */
export async function testModelCapability(
  data: TestModelCapabilityRequest,
  options: { signal?: AbortSignal } = {},
): Promise<TestModelCapabilityResponse> {
  const response = await client.post<TestModelCapabilityResponse>(
    '/api/admin/provider-query/test-model-capability',
    data,
    {
      // 给服务端 10/20 分钟硬时限预留 30 秒返回 inconclusive 结果，避免客户端先行截断。
      timeout: data.mode === 'verify' ? 20 * 60 * 1000 + 30_000 : 10 * 60 * 1000 + 30_000,
      signal: options.signal,
    },
  )
  return response.data
}

/**
 * 带故障转移的模型测试
 */
export interface TestModelFailoverRequest {
  provider_id: string
  mode: 'global' | 'direct' | 'pool'
  model_name: string
  failover_models?: string[]
  api_key_ids?: string[]
  api_format?: string
  endpoint_id?: string
  message?: string
  apply_model_mapping?: boolean
  mapped_model_name?: string
  request_headers?: Record<string, unknown>
  request_body?: Record<string, unknown>
  request_id?: string
}

export interface TestAttemptDetail {
  candidate_index: number
  retry_index?: number
  endpoint_api_format: string
  endpoint_base_url: string
  key_name: string | null
  key_id: string
  auth_type: string
  effective_model?: string | null
  status: 'success' | 'failed' | 'skipped' | 'cancelled' | 'pending' | 'streaming' | 'stream_interrupted' | 'available' | 'unused'
  skip_reason?: string | null
  error_message?: string | null
  status_code?: number | null
  latency_ms?: number | null
  request_url?: string | null
  request_headers?: Record<string, unknown> | null
  request_body?: unknown
  response_headers?: Record<string, unknown> | null
  response_body?: unknown
}

export interface TestCandidateSummary {
  total_candidates: number
  attempted: number
  success: number
  failed: number
  skipped: number
  unused: number
  pending?: number
  available?: number
  completed?: number
  stop_reason?: 'first_success' | 'exhausted' | 'all_skipped' | 'no_candidate' | 'pending' | string
  winning_candidate_index?: number | null
  winning_key_name?: string | null
  winning_key_id?: string | null
  winning_auth_type?: string | null
  winning_effective_model?: string | null
  winning_endpoint_api_format?: string | null
  winning_endpoint_base_url?: string | null
  winning_latency_ms?: number | null
  winning_status_code?: number | null
}

export interface TestModelFailoverResponse {
  success: boolean
  model: string
  provider: { id: string; name: string; provider_type?: string }
  attempts: TestAttemptDetail[]
  total_candidates: number
  total_attempts: number
  candidate_summary?: TestCandidateSummary
  data?: Record<string, unknown> | null
  error?: string | null
}

export async function testModelFailover(
  data: TestModelFailoverRequest,
  options: { signal?: AbortSignal } = {}
): Promise<TestModelFailoverResponse> {
  const normalizedModelName = typeof data.model_name === 'string' ? data.model_name.trim() : ''
  const failoverModels = Array.isArray(data.failover_models) && data.failover_models.length > 0
    ? data.failover_models
    : (normalizedModelName ? [normalizedModelName] : undefined)
  const response = await client.post('/api/admin/provider-query/test-model-failover', {
    ...data,
    ...(failoverModels ? { failover_models: failoverModels } : {}),
  }, {
    timeout: 10 * 60 * 1000,
    signal: options.signal,
  })
  return response.data
}

/**
 * 映射预览相关类型
 */
export interface MappingMatchedModel {
  allowed_model: string
  mapping_pattern: string
}

export interface MappingMatchingGlobalModel {
  global_model_id: string
  global_model_name: string
  display_name: string
  is_active: boolean
  matched_models: MappingMatchedModel[]
}

export interface MappingMatchingKey {
  key_id: string
  key_name: string
  masked_key: string
  is_active: boolean
  allowed_models: string[]
  matching_global_models: MappingMatchingGlobalModel[]
}

export interface ProviderMappingPreviewResponse {
  provider_id: string
  provider_name: string
  keys: MappingMatchingKey[]
  total_keys: number
  total_matches: number
  // 截断提示
  truncated: boolean
  truncated_keys: number
  truncated_models: number
}

function mappingPreviewRecord(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {}
}

function mappingPreviewString(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback
}

function mappingPreviewCount(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0
    ? value
    : fallback
}

function normalizeProviderMappingPreview(
  value: unknown,
  providerId: string,
): ProviderMappingPreviewResponse {
  const source = mappingPreviewRecord(value)
  const rawKeys = Array.isArray(source.keys) ? source.keys : []
  const keys = rawKeys.map((rawKey) => {
    const key = mappingPreviewRecord(rawKey)
    const rawGlobalModels = Array.isArray(key.matching_global_models)
      ? key.matching_global_models
      : []

    return {
      key_id: mappingPreviewString(key.key_id),
      key_name: mappingPreviewString(key.key_name),
      masked_key: mappingPreviewString(key.masked_key, '***'),
      is_active: key.is_active === true,
      allowed_models: Array.isArray(key.allowed_models)
        ? key.allowed_models.filter((item): item is string => typeof item === 'string')
        : [],
      matching_global_models: rawGlobalModels.map((rawGlobalModel) => {
        const globalModel = mappingPreviewRecord(rawGlobalModel)
        const rawMatchedModels = Array.isArray(globalModel.matched_models)
          ? globalModel.matched_models
          : []

        return {
          global_model_id: mappingPreviewString(globalModel.global_model_id),
          global_model_name: mappingPreviewString(globalModel.global_model_name),
          display_name: mappingPreviewString(
            globalModel.display_name,
            mappingPreviewString(globalModel.global_model_name),
          ),
          is_active: globalModel.is_active === true,
          matched_models: rawMatchedModels.map((rawMatchedModel) => {
            const matchedModel = mappingPreviewRecord(rawMatchedModel)
            return {
              allowed_model: mappingPreviewString(matchedModel.allowed_model),
              mapping_pattern: mappingPreviewString(matchedModel.mapping_pattern),
            }
          }),
        }
      }),
    }
  })
  const inferredMatches = keys.reduce(
    (total, key) => total + key.matching_global_models.length,
    0,
  )

  return {
    provider_id: mappingPreviewString(source.provider_id, providerId),
    provider_name: mappingPreviewString(source.provider_name),
    keys,
    total_keys: mappingPreviewCount(source.total_keys, keys.length),
    total_matches: mappingPreviewCount(source.total_matches, inferredMatches),
    truncated: source.truncated === true,
    truncated_keys: mappingPreviewCount(source.truncated_keys, 0),
    truncated_models: mappingPreviewCount(source.truncated_models, 0),
  }
}

/**
 * 获取 Provider 映射预览
 */
export async function getProviderMappingPreview(
  providerId: string
): Promise<ProviderMappingPreviewResponse> {
  return dedupedRequest(`providers:mapping-preview:${providerId}`, async () => {
    const response = await client.get<ProviderMappingPreviewResponse>(`/api/admin/providers/${providerId}/mapping-preview`)
    return normalizeProviderMappingPreview(response.data, providerId)
  })
}
