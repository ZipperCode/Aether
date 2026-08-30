import {
  apiFormatPermissionCovers,
  normalizeApiFormatAlias,
} from '@/api/endpoints/types/api-format'
import type { ModelTestCapabilities, OpenAiImageModelTestCapability } from '@/api/endpoints/types'

export type ModelTestEndpointSource = {
  api_format: string
  is_active?: boolean | null
}

export type ModelTestImageSource = {
  effective_supports_image_generation?: boolean | null
  supports_image_generation?: boolean | null
  model_test_capabilities?: ModelTestCapabilities | null
}

/** 能力检测判断所需的最小模型元数据，兼容 Provider 与合并后的 Global config。 */
export type ModelCapabilitySource = {
  /** ProviderModel 原始开放配置。 */
  config?: Record<string, unknown> | null
  /** 合并 GlobalModel 默认值后的有效配置。 */
  effective_config?: Record<string, unknown> | null
}

export type ModelTestKeySource = {
  api_formats?: string[] | null
  is_active?: boolean | null
  auth_type?: string | null
  credential_kind?: string | null
  oauth_managed?: boolean | null
}

const MODEL_TEST_UNSUPPORTED_API_FORMATS = new Set([
  'openai:realtime',
  'codex:live',
  'openai:video',
  'gemini:video',
  'gemini:files',
])

const MODEL_TEST_OAUTH_INHERITS_PROVIDER_FORMATS = new Set([
  'claude_code',
  'codex',
  'chatgpt_web',
  'gemini_cli',
  'vertex_ai',
  'antigravity',
  'kiro',
])

const MODEL_TEST_BEARER_INHERITS_PROVIDER_FORMATS = new Set([
  'chatgpt_web',
])

const MODEL_CAPABILITY_TEXT_API_FORMATS = new Set([
  'openai:chat',
  'openai:responses',
  'claude:messages',
  'gemini:generate_content',
])

const MODEL_CAPABILITY_TEXT_TYPES = new Set(['generation', 'chat', 'responses', 'text_generation'])
const MODEL_CAPABILITY_NON_TEXT_TYPES = new Set([
  'embedding',
  'rerank',
  'image',
  'video',
  'audio',
  'realtime',
  'files',
])

const MODEL_TEST_DIAGNOSTIC_LABELS: Record<string, string> = {
  key_model_not_allowed: 'Key 未允许当前模型，已跳过',
  pool_account_blocked: '账号已失效，需重新授权',
}

export function normalizeModelTestStringList(values: string[] | null | undefined): string[] {
  return (values ?? [])
    .map(value => value.trim())
    .filter(Boolean)
}

export function isModelTestableApiFormat(apiFormat: string | null | undefined): boolean {
  const normalized = normalizeApiFormatAlias(apiFormat ?? '')
  return Boolean(normalized) && !MODEL_TEST_UNSUPPORTED_API_FORMATS.has(normalized)
}

/** 判断协议是否属于首版可稳定聚合文本的能力检测格式。 */
export function isModelCapabilityApiFormat(apiFormat: string | null | undefined): boolean {
  return MODEL_CAPABILITY_TEXT_API_FORMATS.has(normalizeApiFormatAlias(apiFormat ?? ''))
}

/** 根据显式格式、能力和模型类型判断是否为文本生成模型；无声明历史模型保持兼容。 */
export function modelSupportsCapabilityDetection(
  model: ModelCapabilitySource | null | undefined,
): boolean {
  if (!model) return false
  const config = model.effective_config ?? model.config ?? {}
  const apiFormats = Array.isArray(config.api_formats)
    ? config.api_formats.filter((value): value is string => typeof value === 'string')
    : []
  if (apiFormats.length > 0) return apiFormats.some(isModelCapabilityApiFormat)

  const capabilities = [config.capabilities, config.supported_capabilities]
    .filter(Array.isArray)
    .flat()
    .filter((value): value is string => typeof value === 'string')
    .map(value => value.trim().toLowerCase())
  if (capabilities.some(value => MODEL_CAPABILITY_TEXT_TYPES.has(value))) return true
  if (capabilities.some(value => MODEL_CAPABILITY_NON_TEXT_TYPES.has(value))) return false

  const modelType = typeof config.model_type === 'string'
    ? config.model_type.trim().toLowerCase()
    : ''
  if (MODEL_CAPABILITY_TEXT_TYPES.has(modelType)) return true
  if (MODEL_CAPABILITY_NON_TEXT_TYPES.has(modelType)) return false
  return true
}

export function modelTestKeySupportsEndpoint(
  key: ModelTestKeySource,
  endpoint: ModelTestEndpointSource,
  providerType?: string | null,
): boolean {
  if (key.is_active === false) return false

  const endpointFormat = normalizeApiFormatAlias(endpoint.api_format)
  if (!isModelTestableApiFormat(endpointFormat)) return false

  if (modelTestKeyInheritsProviderFormats(key, providerType)) return true

  const keyFormats = normalizeModelTestStringList(key.api_formats)
  if (keyFormats.length === 0) return true

  return keyFormats.some(format => apiFormatPermissionCovers(format, endpointFormat))
}

export function isModelTestableEndpoint(
  endpoint: ModelTestEndpointSource,
  keys: ModelTestKeySource[],
  providerType?: string | null,
): boolean {
  return endpoint.is_active !== false
    && isModelTestableApiFormat(endpoint.api_format)
    && keys.some(key => modelTestKeySupportsEndpoint(key, endpoint, providerType))
}

function modelTestKeyInheritsProviderFormats(
  key: ModelTestKeySource,
  providerType: string | null | undefined,
): boolean {
  const normalizedProviderType = providerType?.trim().toLowerCase()
  if (!normalizedProviderType) return false

  const authType = key.auth_type?.trim().toLowerCase()
  const credentialKind = key.credential_kind?.trim().toLowerCase()
  const oauthManaged = key.oauth_managed === true
    || credentialKind === 'oauth_session'
    || authType === 'oauth'

  if (oauthManaged && MODEL_TEST_OAUTH_INHERITS_PROVIDER_FORMATS.has(normalizedProviderType)) {
    return true
  }

  return authType === 'bearer'
    && MODEL_TEST_BEARER_INHERITS_PROVIDER_FORMATS.has(normalizedProviderType)
}

export function selectPreferredModelTestEndpoint<T extends ModelTestEndpointSource>(
  model: ModelTestImageSource | null | undefined,
  endpoints: T[],
): T | null {
  if (modelSupportsImageGeneration(model)) {
    const imageEndpoint = endpoints.find(
      endpoint => normalizeApiFormatAlias(endpoint.api_format) === 'openai:image',
    )
    if (imageEndpoint) return imageEndpoint
  }

  return endpoints[0] ?? null
}

export function getOpenAiImageModelTestCapability(
  model: ModelTestImageSource | null | undefined,
): OpenAiImageModelTestCapability | null {
  const capability = model?.model_test_capabilities?.['openai:image']
  return capability && typeof capability === 'object'
    ? capability as OpenAiImageModelTestCapability
    : null
}

export function getOpenAiImageModelTestMaxGenerationCount(
  model: ModelTestImageSource | null | undefined,
): number | null {
  const maxGenerationCount = getOpenAiImageModelTestCapability(model)?.max_generation_count
  return typeof maxGenerationCount === 'number' && Number.isFinite(maxGenerationCount)
    ? Math.max(1, Math.floor(maxGenerationCount))
    : null
}

export function formatModelTestDiagnostic(value: string | null | undefined): string {
  const normalized = value?.trim()
  if (!normalized) return ''
  return MODEL_TEST_DIAGNOSTIC_LABELS[normalized] ?? normalized
}

export function modelSupportsImageGeneration(model: ModelTestImageSource | null | undefined): boolean {
  const imageCapability = getOpenAiImageModelTestCapability(model)
  if (imageCapability) {
    return imageCapability.supports_generation !== false
  }
  return Boolean(
    model?.effective_supports_image_generation ?? model?.supports_image_generation,
  )
}
