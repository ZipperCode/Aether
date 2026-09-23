import type { ProviderEndpoint, ProviderType } from '@/api/endpoints'

/**
 * 官方提供商精确 host 白名单。
 * 必须使用 HTTPS、默认 443 端口且无凭据；绝不把用户自建反代域名猜成官方。
 * MiniMax 仅限现行官方 allowlist，旧 .chat 域名不作跨域猜测。
 */
export const OFFICIAL_PROVIDER_HOSTS: Readonly<Record<string, readonly string[]>> = {
  deepseek: ['api.deepseek.com'],
  openrouter: ['openrouter.ai'],
  moonshot: ['api.moonshot.cn', 'api.moonshot.ai'],
  kimi_coding: ['api.kimi.com', 'api.kimi.ai'],
  siliconflow: ['api.siliconflow.cn', 'api.siliconflow.com'],
  zhipu: ['bigmodel.cn', 'open.bigmodel.cn'],
  zai: ['api.z.ai'],
  minimax: ['api.minimaxi.com', 'api.minimax.io'],
}

/** 官方提供商的人类可读中文名称。 */
export const OFFICIAL_PROVIDER_NAMES: Readonly<Record<string, string>> = {
  deepseek: 'DeepSeek',
  openrouter: 'OpenRouter',
  moonshot: 'Moonshot (月之暗面)',
  kimi_coding: 'Kimi Coding',
  siliconflow: 'SiliconFlow (硅基流动)',
  zhipu: '智谱开放平台',
  zai: 'Z.ai',
  minimax: 'MiniMax',
}

export interface OfficialEndpointDetectionResult {
  /** 建议切换的目标官方提供商类型。 */
  suggestedType: ProviderType
  /** 匹配到的官方 host。 */
  officialHost: string
  /** 匹配到的官方类型中文展示名称。 */
  label: string
}

/**
 * 判断 endpoint 的 URL 是否严格匹配指定的官方 origin。
 * 规则：HTTPS 协议、默认端口 443、无用户名和密码、精确 host 匹配、非 IP。
 */
export function endpointMatchesOfficialOrigin(baseUrl: string | null | undefined, officialHost: string): boolean {
  if (!baseUrl) return false
  const trimmed = baseUrl.trim()
  if (!trimmed) return false

  try {
    const url = new URL(trimmed)
    if (url.protocol !== 'https:') return false
    if (url.username || url.password) return false
    // 默认 443 端口在 URL 对象中通常为空字符串或 '443'
    if (url.port && url.port !== '443') return false

    const hostname = url.hostname.toLowerCase()
    // 排除纯 IP 地址
    if (/^(?:\d{1,3}\.){3}\d{1,3}$/.test(hostname) || hostname.includes(':')) {
      return false
    }

    return hostname === officialHost.toLowerCase()
  } catch {
    return false
  }
}

/**
 * 针对 provider_type 为 custom 的提供商端点进行官方域名识别。
 * 若所有命中官方域名的端点指向唯一的官方类型，返回建议配置结果；
 * 若未命中官方域名、或混合了不同提供商的官方域名，不提供单一建议，返回 null。
 */
export function detectOfficialProviderFromEndpoints(
  providerType: string | null | undefined,
  endpoints: readonly ProviderEndpoint[] | null | undefined,
): OfficialEndpointDetectionResult | null {
  if (providerType?.trim().toLowerCase() !== 'custom') return null
  if (!endpoints || endpoints.length === 0) return null

  const matched = new Map<string, string>()

  for (const endpoint of endpoints) {
    if (!endpoint.is_active) continue
    for (const [type, hosts] of Object.entries(OFFICIAL_PROVIDER_HOSTS)) {
      for (const host of hosts) {
        if (endpointMatchesOfficialOrigin(endpoint.base_url, host)) {
          matched.set(type, host)
        }
      }
    }
  }

  // 混合了多个不同官方提供商的 host 时，不能武断猜测单一类型
  if (matched.size !== 1) {
    return null
  }

  const [[suggestedType, officialHost]] = [...matched.entries()]
  return {
    suggestedType: suggestedType as ProviderType,
    officialHost,
    label: OFFICIAL_PROVIDER_NAMES[suggestedType] || suggestedType,
  }
}
