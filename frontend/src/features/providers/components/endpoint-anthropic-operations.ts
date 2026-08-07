import { normalizeApiFormatAlias } from '@/api/endpoints/types/api-format'

const CLAUDE_MESSAGES_API_FORMAT = 'claude:messages'
const CLAUDE_COUNT_TOKENS_OPERATION = 'count_tokens'
const CLAUDE_MESSAGES_OPERATION = 'messages'
const CLAUDE_COUNT_TOKENS_LOCKED_PROVIDER_TYPES = new Set(['kiro', 'grok'])

function objectValue(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null
  return value as Record<string, unknown>
}

function configuredAnthropicOperations(config: Record<string, unknown> | null | undefined): string[] | null {
  const anthropic = objectValue(config?.anthropic)
  if (!anthropic || !Object.prototype.hasOwnProperty.call(anthropic, 'supported_operations')) {
    return null
  }
  const operations = anthropic.supported_operations
  if (!Array.isArray(operations)) return []
  return operations.filter((operation): operation is string => typeof operation === 'string')
}

export function isClaudeMessagesEndpoint(apiFormat: string): boolean {
  return normalizeApiFormatAlias(apiFormat) === CLAUDE_MESSAGES_API_FORMAT
}

export function isClaudeCountTokensSupportLocked(providerType: string | null | undefined): boolean {
  return CLAUDE_COUNT_TOKENS_LOCKED_PROVIDER_TYPES.has(providerType?.trim().toLowerCase() || '')
}

export function endpointSupportsClaudeCountTokens(
  config: Record<string, unknown> | null | undefined,
  providerType?: string | null,
): boolean {
  if (isClaudeCountTokensSupportLocked(providerType)) return false

  const operations = configuredAnthropicOperations(config)
  // 后端为兼容旧配置，在 supported_operations 缺失时默认支持 Token 计数。
  if (operations === null) return true
  return operations.some(operation => operation.trim().toLowerCase() === CLAUDE_COUNT_TOKENS_OPERATION)
}

export function endpointConfigWithClaudeCountTokensSupport(
  config: Record<string, unknown> | null | undefined,
  enabled: boolean,
): Record<string, unknown> {
  const merged: Record<string, unknown> = { ...(objectValue(config) || {}) }
  const anthropic: Record<string, unknown> = { ...(objectValue(merged.anthropic) || {}) }
  const configured = Array.isArray(anthropic.supported_operations)
    ? anthropic.supported_operations
    : []
  const extraOperations: string[] = []
  const seen = new Set<string>()

  for (const operation of configured) {
    if (typeof operation !== 'string') continue
    const normalized = operation.trim().toLowerCase()
    if (!normalized
      || normalized === CLAUDE_MESSAGES_OPERATION
      || normalized === CLAUDE_COUNT_TOKENS_OPERATION
      || seen.has(normalized)) {
      continue
    }
    seen.add(normalized)
    extraOperations.push(operation.trim())
  }

  anthropic.supported_operations = [
    CLAUDE_MESSAGES_OPERATION,
    ...extraOperations,
    ...(enabled ? [CLAUDE_COUNT_TOKENS_OPERATION] : []),
  ]
  merged.anthropic = anthropic
  return merged
}
