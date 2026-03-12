import { describe, expect, it, vi } from 'vitest'

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
  }),
}))

vi.mock('@/composables/useSiteInfo', () => ({
  useSiteInfo: () => ({
    refreshSiteInfo: vi.fn(),
  }),
}))

vi.mock('@/api/admin', () => ({
  adminApi: {
    getSystemConfig: vi.fn(),
    getSystemVersion: vi.fn(),
    updateSystemConfig: vi.fn(),
  },
}))

vi.mock('@/utils/logger', () => ({
  log: {
    error: vi.fn(),
  },
}))

import { useSystemConfig } from '../composables/useSystemConfig'

describe('useSystemConfig', () => {
  it('does not expose legacy all-api-hub fields in default config', () => {
    const { systemConfig } = useSystemConfig()
    const configKeys = new Set(Object.keys(systemConfig.value))

    expect(configKeys.has('enable_all_api_hub_sync')).toBe(false)
    expect(configKeys.has('all_api_hub_sync_time')).toBe(false)
    expect(configKeys.has('all_api_hub_webdav_url')).toBe(false)
    expect(configKeys.has('all_api_hub_webdav_username')).toBe(false)
    expect(configKeys.has('all_api_hub_webdav_password')).toBe(false)
    expect(configKeys.has('enable_all_api_hub_auto_create_provider_ops')).toBe(false)
  })
})
