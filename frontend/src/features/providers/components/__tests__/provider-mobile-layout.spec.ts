import { describe, expect, it } from 'vitest'
import { createApp } from 'vue'

import ProviderMobileCard from '@/features/providers/components/ProviderMobileCard.vue'
import ProviderTableHeader from '@/features/providers/components/ProviderTableHeader.vue'
import { createI18n } from '@/i18n'

function mount(component: Parameters<typeof createApp>[0], props: Record<string, unknown>) {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(component, props)
  app.use(createI18n())
  app.mount(root)
  return { root, unmount: () => { app.unmount(); root.remove() } }
}

describe('provider mobile layout', () => {
  it('gives each mobile filter enough width to keep its identity visible', () => {
    const { root, unmount } = mount(ProviderTableHeader, {
      searchQuery: '', filterStatus: 'all-status', filterApiFormat: 'all-format', filterModel: 'all-model',
      statusFilters: [{ value: 'all-status', label: '全部状态' }],
      apiFormatFilters: [{ value: 'all-format', label: '全部格式' }],
      modelFilters: [{ value: 'all-model', label: '全部模型' }],
      hasActiveFilters: false, priorityModeLabel: '轮询', loading: false,
    })

    const filters = root.querySelector('[data-testid="provider-mobile-filters"]')
    expect(filters?.classList).toContain('grid-cols-3')
    const triggers = [...(filters?.querySelectorAll('button') || [])]
    expect(triggers).toHaveLength(3)
    expect(triggers.every(trigger => trigger.classList.contains('w-full'))).toBe(true)
    expect(triggers.every(trigger => !trigger.classList.contains('w-20'))).toBe(true)
    unmount()
  })

  it('keeps the full provider name separate from the mobile action row', () => {
    const { root, unmount } = mount(ProviderMobileCard, {
      provider: {
        id: 1, name: 'SiliconFlow', is_active: true, billing_type: 'pay_as_you_go',
        active_endpoints: 0, total_endpoints: 0, active_keys: 0, total_keys: 0,
      },
      editingDescriptionId: null,
      descriptionValue: '',
      isBalanceLoading: () => false,
      getProviderBalance: () => null,
      getProviderBalanceError: () => null,
      getProviderCheckin: () => null,
      getProviderCookieExpired: () => null,
      formatBalanceDisplay: () => '',
      getQuotaUsedColorClass: () => '',
    })

    const name = root.querySelector('[data-testid="provider-mobile-name"]')
    expect(name?.textContent).toBe('SiliconFlow')
    expect(name?.classList).not.toContain('truncate')
    expect(root.querySelector('[data-testid="provider-mobile-actions"]')?.classList).toContain('justify-end')
    unmount()
  })
})
