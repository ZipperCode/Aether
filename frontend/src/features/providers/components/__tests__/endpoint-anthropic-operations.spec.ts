import { describe, expect, it } from 'vitest'

import {
  endpointConfigWithClaudeCountTokensSupport,
  endpointSupportsClaudeCountTokens,
  isClaudeCountTokensSupportLocked,
  isClaudeMessagesEndpoint,
} from '../endpoint-anthropic-operations'

describe('endpoint Anthropic operations', () => {
  it('matches Claude Messages aliases and keeps legacy endpoints enabled by default', () => {
    expect(isClaudeMessagesEndpoint('claude:messages')).toBe(true)
    expect(isClaudeMessagesEndpoint('CLAUDE_MESSAGES')).toBe(true)
    expect(isClaudeMessagesEndpoint('openai:chat')).toBe(false)
    expect(endpointSupportsClaudeCountTokens(null)).toBe(true)
    expect(endpointSupportsClaudeCountTokens({ anthropic: {} })).toBe(true)
  })

  it('reads explicit operation support and locks private adapters', () => {
    expect(endpointSupportsClaudeCountTokens({
      anthropic: { supported_operations: ['messages'] },
    })).toBe(false)
    expect(endpointSupportsClaudeCountTokens({
      anthropic: { supported_operations: ['messages', 'COUNT_TOKENS'] },
    })).toBe(true)
    expect(isClaudeCountTokensSupportLocked('kiro')).toBe(true)
    expect(endpointSupportsClaudeCountTokens({
      anthropic: { supported_operations: ['messages', 'count_tokens'] },
    }, 'grok')).toBe(false)
  })

  it('disables count_tokens without replacing unrelated endpoint config', () => {
    const result = endpointConfigWithClaudeCountTokensSupport({
      upstream_stream_policy: 'force_stream',
      anthropic: {
        compatibility_profile: 'native_transparent',
        count_tokens_path: '/custom/count',
        supported_operations: ['messages', 'count_tokens', 'batch'],
      },
    }, false)

    expect(result).toEqual({
      upstream_stream_policy: 'force_stream',
      anthropic: {
        compatibility_profile: 'native_transparent',
        count_tokens_path: '/custom/count',
        supported_operations: ['messages', 'batch'],
      },
    })
  })

  it('enables count_tokens with canonical operations and preserves extensions', () => {
    expect(endpointConfigWithClaudeCountTokensSupport({
      anthropic: {
        supported_operations: ['MESSAGES', 'batch', 'BATCH', 'COUNT_TOKENS'],
      },
    }, true)).toEqual({
      anthropic: {
        supported_operations: ['messages', 'batch', 'count_tokens'],
      },
    })
  })
})
