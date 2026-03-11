import type { OAuthOrganizationInfo } from '@/api/endpoints/types/provider'

export function formatOAuthIdentityShort(
  value: string | null | undefined,
  head = 8,
  tail = 6,
): string {
  const normalized = String(value || '').trim()
  if (!normalized) return ''
  if (normalized.length <= head + tail + 3) return normalized
  return `${normalized.slice(0, head)}...${normalized.slice(-tail)}`
}

export function getPrimaryOAuthOrganizationTitle(
  value: { oauth_organizations?: OAuthOrganizationInfo[] | null } | null | undefined,
): string | null {
  const organizations = Array.isArray(value?.oauth_organizations) ? value.oauth_organizations : []
  const defaultOrg = organizations.find(
    (org) => org?.is_default && typeof org?.title === 'string' && org.title.trim(),
  )
  if (defaultOrg?.title) return defaultOrg.title.trim()
  const firstWithTitle = organizations.find(
    (org) => typeof org?.title === 'string' && org.title.trim(),
  )
  return firstWithTitle?.title?.trim() || null
}

export function getOAuthOrganizationsTooltip(
  value: { oauth_organizations?: OAuthOrganizationInfo[] | null } | null | undefined,
): string {
  const organizations = Array.isArray(value?.oauth_organizations) ? value.oauth_organizations : []
  if (organizations.length === 0) return ''
  return organizations
    .map((org) => {
      const title =
        typeof org?.title === 'string' && org.title.trim() ? org.title.trim() : '未命名组织'
      const role =
        typeof org?.role === 'string' && org.role.trim() ? ` (${org.role.trim()})` : ''
      const suffix = org?.is_default ? ' [default]' : ''
      return `${title}${role}${suffix}`
    })
    .join('\n')
}
