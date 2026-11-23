import { onBeforeUnmount, reactive } from 'vue'
import { isAxiosError } from 'axios'
import { adminApi, type ProviderModelsQueryResponse } from '@/api/admin'
import {
  testModel,
  type TestAttemptDetail,
  type TestCandidateSummary,
  type TestModelFailoverResponse,
  type TestModelResponse,
} from '@/api/endpoints/providers'
import type { PoolKeyDetail } from '@/api/endpoints/pool'
import type { ProviderEndpoint } from '@/api/endpoints/types'
import {
  buildDefaultModelTestRequestBody,
  extractModelTestResponsePreview,
  isModelTestableApiFormat,
  modelTestKeySupportsEndpoint,
  parseModelTestRequestBodyDraft,
  selectPreferredModelTestEndpoint,
} from '@/features/providers/components/provider-tabs/model-test-request'
import { parseApiError } from '@/utils/errorParser'

export type PoolAccountModelItem = ProviderModelsQueryResponse['data']['models'][number]

export interface PoolAccountModelsState {
  loading: boolean
  loaded: boolean
  models: PoolAccountModelItem[]
  fromCache: boolean
  error: string | null
  warning: string | null
}

export interface PoolAccountModelTestResult {
  keyId: string
  modelName: string
  endpointId: string | null
  apiFormat: string | null
  endpointBaseUrl: string | null
  testing: boolean
  status: 'idle' | 'success' | 'failed'
  statusCode: number | null
  latencyMs: number | null
  errorMessage: string | null
  responsePreview: string | null
  rawResult: TestModelResponse | null
  requestHeaders: Record<string, unknown> | null
  requestBody: Record<string, unknown> | null
  testedAt: number | null
}

export interface RunPoolAccountModelTestParams {
  keyId: string
  modelName: string
  endpoint?: ProviderEndpoint | null
  modelMeta?: PoolAccountModelItem | null
}

export interface UsePoolKeyModelTestOptions {
  providerId: () => string | null
  providerName?: () => string
}

const AUTO_ENDPOINT_KEY = '__auto__'

function isAbortError(err: unknown): boolean {
  if (isAxiosError(err) && err.code === 'ERR_CANCELED') {
    return true
  }
  return err instanceof Error && err.name === 'AbortError'
}

export function buildPoolModelTestResultKey(
  keyId: string,
  modelName: string,
  endpointId?: string | null,
): string {
  const normalizedEndpoint = endpointId?.trim() || AUTO_ENDPOINT_KEY
  return `${keyId}::${normalizedEndpoint}::${modelName.trim()}`
}

/**
 * 判断某模型是否在账号的 allowed_models 白名单内。
 * null/undefined 表示账号未设置白名单（继承提供商权限）；显式空数组表示全部未放行。
 */
export function isModelAllowedByPoolKey(key: PoolKeyDetail, modelId: string): boolean {
  if (key.allowed_models == null) {
    return true
  }
  const target = modelId.trim().toLowerCase()
  if (!target) return true
  return key.allowed_models.some(item => item.trim().toLowerCase() === target)
}

/**
 * 筛选出该账号可用于连通性直测的端点列表，允许已停用账号与端点按显式单号测试规则进入。
 */
export function getCompatiblePoolKeyEndpoints(
  key: PoolKeyDetail,
  endpoints: ProviderEndpoint[],
  providerType?: string | null,
): ProviderEndpoint[] {
  const normalizedKey = {
    ...key,
    is_active: true,
  }
  return endpoints
    .filter(endpoint => isModelTestableApiFormat(endpoint.api_format))
    .filter(endpoint => modelTestKeySupportsEndpoint(normalizedKey, endpoint, providerType))
    .sort((left, right) => {
      if (left.is_active !== right.is_active) {
        return left.is_active ? -1 : 1
      }
      return left.api_format.localeCompare(right.api_format)
    })
}

/**
 * 为单账号和指定模型挑选默认测试端点：优先匹配模型返回的 endpoint_ids / api_format，再回退到通用优先级。
 */
export function selectDefaultPoolKeyEndpoint(
  endpoints: ProviderEndpoint[],
  modelMeta?: PoolAccountModelItem | null,
): ProviderEndpoint | null {
  if (endpoints.length === 0) return null

  const activeEndpoints = endpoints.filter(endpoint => endpoint.is_active)
  const pool = activeEndpoints.length > 0 ? activeEndpoints : endpoints

  if (modelMeta?.endpoint_ids?.length) {
    const matchedById = pool.find(endpoint => modelMeta.endpoint_ids?.includes(endpoint.id))
    if (matchedById) return matchedById
  }

  if (modelMeta?.api_format) {
    const matchedByFormat = pool.find(
      endpoint => endpoint.api_format.toLowerCase() === modelMeta.api_format?.toLowerCase(),
    )
    if (matchedByFormat) return matchedByFormat
  }

  return selectPreferredModelTestEndpoint(modelMeta ?? null, pool) ?? pool[0] ?? null
}

function extractFailureMessage(
  response: TestModelResponse,
  primaryAttempt: TestAttemptDetail | null,
): string | null {
  if (primaryAttempt?.error_message?.trim()) {
    return primaryAttempt.error_message.trim()
  }
  if (typeof response.error === 'string' && response.error.trim()) {
    return response.error.trim()
  }
  const nestedError = response.data?.response?.error
  if (typeof nestedError === 'string' && nestedError.trim()) {
    return nestedError.trim()
  }
  if (nestedError && typeof nestedError === 'object' && typeof nestedError.message === 'string' && nestedError.message.trim()) {
    return nestedError.message.trim()
  }
  return null
}

function extractResponsePreview(
  response: TestModelResponse,
  primaryAttempt: TestAttemptDetail | null,
): string | null {
  if (typeof response.data?.content_preview === 'string' && response.data.content_preview.trim()) {
    return response.data.content_preview.trim()
  }
  const fromAttempt = extractModelTestResponsePreview(primaryAttempt?.response_body)
  if (fromAttempt) return fromAttempt
  return extractModelTestResponsePreview(response.data?.response ?? response.data)
}

/**
 * 将行内直测结果转换为 ModelTestDialog 可直接检视的完整结果结构，避免打开详情时重复发请求。
 */
export function normalizePoolModelTestResultForDialog(
  result: PoolAccountModelTestResult,
  providerId: string,
  providerName: string,
  key: PoolKeyDetail,
): TestModelFailoverResponse {
  const raw = result.rawResult
  const fallbackAttempt: TestAttemptDetail = {
    candidate_index: 0,
    endpoint_api_format: result.apiFormat || '-',
    endpoint_base_url: result.endpointBaseUrl || '',
    key_name: key.key_name || key.key_id,
    key_id: key.key_id,
    auth_type: key.auth_type || '',
    effective_model: raw?.model || result.modelName,
    status: result.status === 'success' ? 'success' : 'failed',
    skip_reason: null,
    error_message: result.status === 'success' ? null : result.errorMessage,
    status_code: result.statusCode,
    latency_ms: result.latencyMs,
    request_url: null,
    request_headers: result.requestHeaders,
    request_body: result.requestBody,
    response_headers: null,
    response_body: (raw?.data?.response as Record<string, unknown> | undefined)
      ?? (raw?.data as Record<string, unknown> | undefined)
      ?? null,
  }
  const attempts = Array.isArray(raw?.attempts) && raw.attempts.length > 0
    ? raw.attempts
    : [fallbackAttempt]
  const totalCandidates = typeof raw?.total_candidates === 'number'
    ? raw.total_candidates
    : attempts.length
  const totalAttempts = typeof raw?.total_attempts === 'number'
    ? raw.total_attempts
    : attempts.filter(attempt => !['skipped', 'available', 'unused'].includes(attempt.status)).length
  const candidateSummary: TestCandidateSummary = raw?.candidate_summary ?? {
    total_candidates: totalCandidates,
    attempted: totalAttempts,
    success: result.status === 'success' ? 1 : 0,
    failed: result.status === 'success' ? 0 : 1,
    skipped: 0,
    unused: 0,
    pending: 0,
    available: 0,
    completed: totalAttempts,
    stop_reason: result.status === 'success' ? 'first_success' : 'exhausted',
    winning_candidate_index: result.status === 'success' ? 0 : null,
    winning_key_name: result.status === 'success' ? (key.key_name || key.key_id) : null,
    winning_key_id: result.status === 'success' ? key.key_id : null,
    winning_auth_type: result.status === 'success' ? (key.auth_type || null) : null,
    winning_effective_model: result.status === 'success' ? (raw?.model || result.modelName) : null,
    winning_endpoint_api_format: result.apiFormat,
    winning_endpoint_base_url: result.endpointBaseUrl,
    winning_latency_ms: result.latencyMs,
    winning_status_code: result.statusCode,
  }

  return {
    success: result.status === 'success',
    model: raw?.model || result.modelName,
    provider: raw?.provider || { id: providerId, name: providerName || providerId },
    attempts,
    total_candidates: totalCandidates,
    total_attempts: totalAttempts,
    candidate_summary: candidateSummary,
    error: result.status === 'success' ? undefined : (result.errorMessage ?? undefined),
  }
}

/**
 * 号池单账号模型拉取与连通性直测状态管理。
 * 按 key_id 隔离模型目录缓存，按 key_id + endpoint_id + modelName 隔离测试结果，
 * 并在切换提供商或卸载时取消在途请求，防止过期响应写回。
 */
export function usePoolKeyModelTest(options: UsePoolKeyModelTestOptions) {
  const modelsByKeyId = reactive<Record<string, PoolAccountModelsState>>({})
  const resultsByKey = reactive<Record<string, PoolAccountModelTestResult>>({})
  const customHistoryByKeyId = reactive<Record<string, string[]>>({})

  const fetchRequestSeqByKeyId = new Map<string, number>()
  const testRequestSeqByKey = new Map<string, number>()
  const abortControllersByKey = new Map<string, AbortController>()

  function getModelsState(keyId: string): PoolAccountModelsState {
    if (!modelsByKeyId[keyId]) {
      modelsByKeyId[keyId] = {
        loading: false,
        loaded: false,
        models: [],
        fromCache: false,
        error: null,
        warning: null,
      }
    }
    return modelsByKeyId[keyId]
  }

  function getModelResult(
    keyId: string,
    modelName: string,
    endpointId?: string | null,
  ): PoolAccountModelTestResult | null {
    const resultKey = buildPoolModelTestResultKey(keyId, modelName, endpointId)
    return resultsByKey[resultKey] ?? null
  }

  function getCustomTestedModels(keyId: string): string[] {
    return customHistoryByKeyId[keyId] ?? []
  }

  function rememberCustomModel(keyId: string, modelName: string) {
    const normalized = modelName.trim()
    if (!normalized) return
    const existing = customHistoryByKeyId[keyId] ?? []
    const next = [normalized, ...existing.filter(item => item !== normalized)].slice(0, 10)
    customHistoryByKeyId[keyId] = next
  }

  async function fetchModelsForKey(
    keyId: string,
    fetchOptions: { forceRefresh?: boolean } = {},
  ): Promise<void> {
    const providerId = options.providerId()
    if (!providerId || !keyId) return

    const state = getModelsState(keyId)
    const forceRefresh = fetchOptions.forceRefresh === true
    if (!forceRefresh && state.loaded && !state.error) {
      return
    }
    if (state.loading && !forceRefresh) {
      return
    }

    const requestSeq = (fetchRequestSeqByKeyId.get(keyId) ?? 0) + 1
    fetchRequestSeqByKeyId.set(keyId, requestSeq)

    state.loading = true
    state.error = null
    state.warning = null

    try {
      const response = await adminApi.queryProviderModels(providerId, keyId, forceRefresh)
      if (
        fetchRequestSeqByKeyId.get(keyId) !== requestSeq
        || options.providerId() !== providerId
      ) {
        return
      }

      const rawModels = Array.isArray(response.data?.models) ? response.data.models : []
      const seenIds = new Set<string>()
      const uniqueModels: PoolAccountModelItem[] = []
      for (const item of rawModels) {
        const id = item?.id?.trim()
        if (!id || seenIds.has(id)) continue
        seenIds.add(id)
        uniqueModels.push({
          ...item,
          id,
        })
      }

      state.models = uniqueModels
      state.fromCache = Boolean(response.data?.from_cache)
      state.warning = response.data?.warning?.trim() || null
      state.error = !response.success
        ? (response.data?.error?.trim() || null)
        : null
      state.loaded = true
    } catch (err: unknown) {
      if (
        fetchRequestSeqByKeyId.get(keyId) !== requestSeq
        || options.providerId() !== providerId
      ) {
        return
      }
      state.error = parseApiError(err, '获取模型失败')
      state.loaded = true
    } finally {
      if (fetchRequestSeqByKeyId.get(keyId) === requestSeq) {
        state.loading = false
      }
    }
  }

  function cancelModelTest(
    keyId: string,
    modelName: string,
    endpointId?: string | null,
  ): void {
    const resultKey = buildPoolModelTestResultKey(keyId, modelName, endpointId)
    const controller = abortControllersByKey.get(resultKey)
    if (controller) {
      abortControllersByKey.delete(resultKey)
      controller.abort()
    }
    const existing = resultsByKey[resultKey]
    if (existing && existing.testing) {
      existing.testing = false
    }
  }

  async function runModelTest(
    params: RunPoolAccountModelTestParams,
  ): Promise<PoolAccountModelTestResult | null> {
    const providerId = options.providerId()
    const modelName = params.modelName.trim()
    if (!providerId || !params.keyId || !modelName) return null

    const endpoint = params.endpoint ?? null
    const resultKey = buildPoolModelTestResultKey(params.keyId, modelName, endpoint?.id)

    // 若同模型正在测试则先中止旧请求，避免并发结果互写。
    const previousController = abortControllersByKey.get(resultKey)
    if (previousController) {
      previousController.abort()
      abortControllersByKey.delete(resultKey)
    }

    const requestSeq = (testRequestSeqByKey.get(resultKey) ?? 0) + 1
    testRequestSeqByKey.set(resultKey, requestSeq)

    const controller = new AbortController()
    abortControllersByKey.set(resultKey, controller)

    let requestBody: Record<string, unknown> | undefined
    if (endpoint?.api_format) {
      const defaultBodyDraft = buildDefaultModelTestRequestBody(
        modelName,
        endpoint.api_format,
        params.modelMeta ?? null,
      )
      const parsedBody = parseModelTestRequestBodyDraft(defaultBodyDraft)
      if (parsedBody.value && !parsedBody.error) {
        requestBody = parsedBody.value
      }
    }

    const nextEntry: PoolAccountModelTestResult = {
      keyId: params.keyId,
      modelName,
      endpointId: endpoint?.id ?? null,
      apiFormat: endpoint?.api_format ?? null,
      endpointBaseUrl: endpoint?.base_url ?? null,
      testing: true,
      status: resultsByKey[resultKey]?.status ?? 'idle',
      statusCode: resultsByKey[resultKey]?.statusCode ?? null,
      latencyMs: resultsByKey[resultKey]?.latencyMs ?? null,
      errorMessage: null,
      responsePreview: resultsByKey[resultKey]?.responsePreview ?? null,
      rawResult: resultsByKey[resultKey]?.rawResult ?? null,
      requestHeaders: {},
      requestBody: requestBody ?? null,
      testedAt: resultsByKey[resultKey]?.testedAt ?? null,
    }
    resultsByKey[resultKey] = nextEntry

    try {
      const response = await testModel(
        {
          provider_id: providerId,
          api_key_id: params.keyId,
          model_name: modelName,
          endpoint_id: endpoint?.id,
          api_format: endpoint?.api_format,
          mode: 'direct',
          apply_model_mapping: false,
          ...(requestBody ? { request_body: requestBody } : {}),
        },
        { signal: controller.signal },
      )

      if (
        testRequestSeqByKey.get(resultKey) !== requestSeq
        || options.providerId() !== providerId
      ) {
        return null
      }

      const attempts = Array.isArray(response.attempts) ? response.attempts : []
      const primaryAttempt = attempts.find(attempt => attempt.status === 'success')
        ?? attempts.find(attempt => attempt.status === 'failed')
        ?? attempts[0]
        ?? null

      const entry = resultsByKey[resultKey]
      if (!entry) return null

      entry.testing = false
      entry.status = response.success ? 'success' : 'failed'
      entry.statusCode = primaryAttempt?.status_code
        ?? response.candidate_summary?.winning_status_code
        ?? response.data?.response?.status_code
        ?? null
      entry.latencyMs = primaryAttempt?.latency_ms
        ?? response.candidate_summary?.winning_latency_ms
        ?? null
      entry.apiFormat = primaryAttempt?.endpoint_api_format
        ?? response.candidate_summary?.winning_endpoint_api_format
        ?? endpoint?.api_format
        ?? null
      entry.endpointBaseUrl = primaryAttempt?.endpoint_base_url
        ?? response.candidate_summary?.winning_endpoint_base_url
        ?? endpoint?.base_url
        ?? null
      entry.errorMessage = response.success
        ? null
        : extractFailureMessage(response, primaryAttempt)
      entry.responsePreview = response.success
        ? extractResponsePreview(response, primaryAttempt)
        : null
      entry.rawResult = response
      entry.testedAt = Date.now()

      return entry
    } catch (err: unknown) {
      if (
        testRequestSeqByKey.get(resultKey) !== requestSeq
        || options.providerId() !== providerId
      ) {
        return null
      }
      const entry = resultsByKey[resultKey]
      if (!entry) return null

      entry.testing = false
      if (isAbortError(err)) {
        return entry
      }

      entry.status = 'failed'
      entry.statusCode = isAxiosError(err) ? (err.response?.status ?? null) : null
      entry.latencyMs = null
      entry.errorMessage = parseApiError(err, '测试请求失败')
      entry.responsePreview = null
      entry.rawResult = null
      entry.testedAt = Date.now()
      return entry
    } finally {
      if (abortControllersByKey.get(resultKey) === controller) {
        abortControllersByKey.delete(resultKey)
      }
    }
  }

  function resetAll(): void {
    for (const controller of abortControllersByKey.values()) {
      controller.abort()
    }
    abortControllersByKey.clear()
    fetchRequestSeqByKeyId.clear()
    testRequestSeqByKey.clear()

    for (const key of Object.keys(modelsByKeyId)) {
      delete modelsByKeyId[key]
    }
    for (const key of Object.keys(resultsByKey)) {
      delete resultsByKey[key]
    }
    for (const key of Object.keys(customHistoryByKeyId)) {
      delete customHistoryByKeyId[key]
    }
  }

  onBeforeUnmount(() => {
    resetAll()
  })

  return {
    modelsByKeyId,
    resultsByKey,
    customHistoryByKeyId,
    getModelsState,
    getModelResult,
    getCustomTestedModels,
    rememberCustomModel,
    fetchModelsForKey,
    runModelTest,
    cancelModelTest,
    resetAll,
  }
}
