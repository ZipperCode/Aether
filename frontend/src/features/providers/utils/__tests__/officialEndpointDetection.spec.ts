import { describe, expect, it } from 'vitest'
import {
  detectOfficialProviderFromEndpoints,
  endpointMatchesOfficialOrigin,
} from '../officialEndpointDetection'
import type { ProviderEndpoint } from '@/api/endpoints'

describe('officialEndpointDetection', () => {
  describe('endpointMatchesOfficialOrigin', () => {
    it('matches exact official https 443 origins without credentials', () => {
      expect(endpointMatchesOfficialOrigin('https://api.deepseek.com', 'api.deepseek.com')).toBe(true)
      expect(endpointMatchesOfficialOrigin('https://api.deepseek.com/v1', 'api.deepseek.com')).toBe(true)
      expect(endpointMatchesOfficialOrigin('https://openrouter.ai/api/v1', 'openrouter.ai')).toBe(true)
      expect(endpointMatchesOfficialOrigin('https://api.siliconflow.cn', 'api.siliconflow.cn')).toBe(true)
      expect(endpointMatchesOfficialOrigin('https://api.minimaxi.com/v1', 'api.minimaxi.com')).toBe(true)
      expect(endpointMatchesOfficialOrigin('https://api.minimax.io', 'api.minimax.io')).toBe(true)
    })

    it('rejects non-https schemes', () => {
      expect(endpointMatchesOfficialOrigin('http://api.deepseek.com', 'api.deepseek.com')).toBe(false)
      expect(endpointMatchesOfficialOrigin('ftp://api.deepseek.com', 'api.deepseek.com')).toBe(false)
    })

    it('rejects custom ports', () => {
      expect(endpointMatchesOfficialOrigin('https://api.deepseek.com:8443', 'api.deepseek.com')).toBe(false)
      expect(endpointMatchesOfficialOrigin('https://api.deepseek.com:443', 'api.deepseek.com')).toBe(true)
    })

    it('rejects credentials in url', () => {
      expect(endpointMatchesOfficialOrigin('https://user:pass@api.deepseek.com', 'api.deepseek.com')).toBe(false)
      expect(endpointMatchesOfficialOrigin('https://user@api.deepseek.com', 'api.deepseek.com')).toBe(false)
    })

    it('rejects user reverse proxy domains and does not guess loosely', () => {
      expect(endpointMatchesOfficialOrigin('https://deepseek.myproxy.com', 'api.deepseek.com')).toBe(false)
      expect(endpointMatchesOfficialOrigin('https://my-openrouter.ai', 'openrouter.ai')).toBe(false)
      expect(endpointMatchesOfficialOrigin('https://sub.openrouter.ai', 'openrouter.ai')).toBe(false)
    })

    it('rejects IP addresses', () => {
      expect(endpointMatchesOfficialOrigin('https://192.168.1.1', 'api.deepseek.com')).toBe(false)
      expect(endpointMatchesOfficialOrigin('https://10.0.0.1:443', 'openrouter.ai')).toBe(false)
    })

    it('rejects MiniMax .chat domains because it is not in the official query allowlist', () => {
      expect(endpointMatchesOfficialOrigin('https://api.minimax.chat', 'api.minimaxi.com')).toBe(false)
    })
  })

  describe('detectOfficialProviderFromEndpoints', () => {
    function makeEndpoint(baseUrl: string): ProviderEndpoint {
      return {
        id: 'ep-1',
        provider_id: 'p-1',
        base_url: baseUrl,
        api_format: 'openai',
        is_active: true,
        provider_name: '测试提供商',
        max_retries: 0,
        active_keys: 1,
        total_keys: 1,
        created_at: '2026-09-23T00:00:00Z',
        updated_at: '2026-09-23T00:00:00Z',
      }
    }

    it('returns null if provider_type is not custom', () => {
      const endpoints = [makeEndpoint('https://api.deepseek.com')]
      expect(detectOfficialProviderFromEndpoints('deepseek', endpoints)).toBeNull()
      expect(detectOfficialProviderFromEndpoints('openai', endpoints)).toBeNull()
      expect(detectOfficialProviderFromEndpoints(null, endpoints)).toBeNull()
    })

    it('suggests deepseek when custom provider endpoints match api.deepseek.com', () => {
      const endpoints = [makeEndpoint('https://api.deepseek.com/v1')]
      const result = detectOfficialProviderFromEndpoints('custom', endpoints)
      expect(result).toEqual({
        suggestedType: 'deepseek',
        officialHost: 'api.deepseek.com',
        label: 'DeepSeek',
      })
    })

    it('suggests openrouter when custom provider endpoints match openrouter.ai', () => {
      const endpoints = [makeEndpoint('https://openrouter.ai/api/v1')]
      const result = detectOfficialProviderFromEndpoints('custom', endpoints)
      expect(result).toEqual({
        suggestedType: 'openrouter',
        officialHost: 'openrouter.ai',
        label: 'OpenRouter',
      })
    })

    it('returns null when custom provider endpoints do not match any official host', () => {
      const endpoints = [makeEndpoint('https://api.mycustomai.com/v1')]
      expect(detectOfficialProviderFromEndpoints('custom', endpoints)).toBeNull()
    })

    it('returns null when endpoints mix hosts from different official providers', () => {
      const endpoints = [
        makeEndpoint('https://api.deepseek.com/v1'),
        makeEndpoint('https://openrouter.ai/api/v1'),
      ]
      // 混合多个官方 host 时不给单一建议
      expect(detectOfficialProviderFromEndpoints('custom', endpoints)).toBeNull()
    })
  })
})
