/**
 * Provider 类型判断工具函数。
 *
 * 区分"密钥型"和"OAuth 账号型"两类 Provider，影响前端显示标签和操作入口。
 */

const oauthAccountProviderTypes = new Set([
  'claude_code',
  'codex',
  'chatgpt_web',
  'gemini_cli',
  'antigravity',
  'kiro',
  'grok',
  'nous',
  'xai',
  'windsurf',
])

export const isOAuthAccountProviderType = (providerType?: string | null): boolean =>
  oauthAccountProviderTypes.has((providerType || '').toLowerCase())

export const isKeyManagedProviderType = (providerType?: string | null): boolean =>
  !isOAuthAccountProviderType(providerType)

/** 官方 API Key 额度查询的统一入口，供抽屉、列表和号池共同判断。 */
const officialQuotaProviderTypes = new Set([
  'deepseek', 'openrouter', 'moonshot', 'kimi_coding', 'siliconflow', 'zhipu', 'zai', 'minimax',
])

export const isOfficialQuotaProviderType = (providerType?: string | null): boolean =>
  officialQuotaProviderTypes.has((providerType || '').trim().toLowerCase())
