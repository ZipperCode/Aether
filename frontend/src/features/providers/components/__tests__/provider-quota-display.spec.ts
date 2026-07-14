import { describe, expect, it } from 'vitest'
import { createApp, defineComponent, h } from 'vue'

import ProviderMonthlyQuotaCard from '@/features/providers/components/ProviderMonthlyQuotaCard.vue'
import ProviderQuotaProgressRow from '@/features/providers/components/ProviderQuotaProgressRow.vue'
import ProviderQuotaSectionHeader from '@/features/providers/components/ProviderQuotaSectionHeader.vue'
import ProviderGenericQuotaCard from '@/features/providers/components/ProviderGenericQuotaCard.vue'
import { createI18n } from '@/i18n'

function mount(component: Parameters<typeof createApp>[0], props?: Record<string, unknown>) {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(component, props)
  app.use(createI18n())
  app.mount(root)

  return {
    root,
    unmount: () => {
      app.unmount()
      root.remove()
    },
  }
}

describe('provider quota display components', () => {
  it('renders balance, subscription, rate limit and stale error separately', () => {
    const { root, unmount } = mount(ProviderGenericQuotaCard, {
      quota: {
        kind: 'balance', code: 'ok', exhausted: false, freshness: 'stale', observed_at: 1_700_000_000,
        balances: [{ unit: 'USD', available: '9.25' }],
        windows: [{ code: 'monthly', label: 'Monthly', remaining_ratio: 0.75 }],
        rate_limits: { rpm: 20 },
        refresh_state: { error: 'sanitized upstream error' },
      },
      loading: true,
    })

    expect(root.querySelector('[data-testid="provider-balance-panel"]')?.textContent).toContain('可用余额$9.25')
    expect(root.querySelector('[data-testid="provider-subscription-panel"]')?.textContent).toContain('Monthly75.0%')
    expect(root.querySelector('[data-testid="provider-quota-kind-badge"]')).toBeNull()
    expect(root.textContent).toContain('RPM 20')
    expect(root.querySelector('[data-testid="provider-generic-quota-status"]')?.textContent).toContain('数据已过期')
    expect(root.textContent).toContain('sanitized upstream error')
    expect(root.querySelector('[data-testid="provider-quota-header-loading"]')).toBeNull()
    unmount()
  })

  it('renders provider balance composition as compact visual metrics', () => {
    const { root, unmount } = mount(ProviderGenericQuotaCard, {
      providerType: 'siliconflow',
      quota: {
        kind: 'balance', code: 'ok', exhausted: false,
        balances: [{ unit: 'CNY', available: '88.88', granted: '0.88', topped_up: '88' }],
      },
    })

    expect(root.textContent).not.toContain('账户余额')
    expect(root.querySelector('[data-testid="provider-quota-kind-badge"]')).toBeNull()
    expect(root.querySelector('[data-testid="provider-balance-panel"]')?.textContent).toContain('88.88 CNY')
    expect(root.textContent).toContain('赠送 0.88 CNY')
    expect(root.textContent).toContain('充值 88 CNY')
    unmount()
  })

  it('renders exact high precision decimal strings and rejects malformed percentages', () => {
    const { root, unmount } = mount(ProviderGenericQuotaCard, {
      providerType: 'openrouter',
      quota: {
        code: 'ok', exhausted: false,
        balances: [{
          unit: 'USD',
          available: '9007199254740993.123456789012345678',
          total: 'not-a-decimal',
        }],
        windows: [{ code: 'bad', label: 'Malformed', remaining_ratio: '12oops' }],
      },
    })

    expect(root.textContent).toContain('$9,007,199,254,740,993.123456789012345678')
    expect(root.querySelector('[data-testid="provider-quota-available"]')?.classList).toContain('break-all')
    expect(root.querySelector('[data-testid="provider-quota-progress-bar"]')).toBeFalsy()
    unmount()
  })

  it('retains last-good balance while reporting a refresh failure', () => {
    const { root, unmount } = mount(ProviderGenericQuotaCard, {
      providerType: 'deepseek',
      quota: {
        code: 'ok', exhausted: false, freshness: 'stale',
        balances: [{ unit: 'CNY', available: '47.730000000000000001' }],
        refresh_state: { error: 'temporary upstream failure', failure_count: 2 },
      },
    })

    expect(root.textContent).toContain('47.730000000000000001 CNY')
    expect(root.textContent).toContain('temporary upstream failure')
    unmount()
  })

  it('renders GLM subscription quota with progress and reset metadata', () => {
    const { root, unmount } = mount(ProviderGenericQuotaCard, {
      providerType: 'zhipu',
      quota: {
        kind: 'subscription', code: 'ok', exhausted: false,
        windows: [{
          code: 'tokens_limit', label: '5小时 Token 配额',
          remaining_value: 3_800_000, limit_value: 5_000_000,
          reset_at_text: '2026-07-13T20:00:00Z',
        }],
      },
    })

    expect(root.textContent).not.toContain('订阅配额')
    expect(root.querySelector('[data-testid="provider-quota-kind-badge"]')).toBeNull()
    expect(root.querySelector('[data-testid="provider-quota-progress-meter"]')?.textContent).toContain('76.0%')
    expect(root.textContent).toContain('3,800,000 / 5,000,000')
    expect(root.textContent).toContain('重置 2026-07-13T20:00:00Z')
    unmount()
  })

  it('renders Kimi Coding plan, concurrency and both quota windows', () => {
    const { root, unmount } = mount(ProviderGenericQuotaCard, {
      providerType: 'kimi_coding',
      quota: {
        kind: 'subscription', code: 'ok', exhausted: false,
        membership_level: 'LEVEL_ADVANCED', parallel_limit: '30',
        windows: [
          { code: 'cycle', label: '周期配额', remaining_value: '99', limit_value: '100', remaining_ratio: 0.99 },
          { code: 'window_0', label: '5小时配额', remaining_value: '99', limit_value: '100', remaining_ratio: 0.99 },
        ],
      },
    })

    expect(root.textContent).toContain('ADVANCED')
    expect(root.textContent).toContain('并发 30')
    expect(root.textContent).toContain('周期配额99.0%')
    expect(root.textContent).toContain('5小时配额99.0%')
    expect(root.querySelectorAll('[data-testid="provider-quota-progress-row"]')).toHaveLength(2)
    unmount()
  })

  it('renders DeepSeek balance with the unified project card hierarchy', () => {
    const { root, unmount } = mount(ProviderGenericQuotaCard, {
      providerType: 'deepseek',
      quota: {
        kind: 'balance',
        balances: [{ unit: 'USD', available: '9.25' }],
      },
    })

    expect(root.textContent).not.toContain('账户余额')
    expect(root.querySelector('[data-testid="provider-generic-quota"] > template')).toBeNull()
    expect(root.querySelector('[data-testid="provider-balance-panel"]')?.textContent).toContain('可用余额$9.25')
    expect(root.querySelector('[data-testid="provider-quota-progress-row"]')).toBeFalsy()
    unmount()
  })

  for (const providerType of ['deepseek', 'openrouter', 'moonshot', 'kimi_coding', 'siliconflow', 'zhipu', 'zai']) {
    it(`keeps a visible quota area for ${providerType} before a snapshot exists`, () => {
      const { root, unmount } = mount(ProviderGenericQuotaCard, {
        providerType,
        quota: null,
        loading: true,
      })

      expect(root.querySelector('[data-testid="provider-generic-quota"]')).toBeTruthy()
      expect(root.querySelector('[data-testid="provider-quota-empty"]')?.textContent).toContain('正在查询额度')
      unmount()
    })
  }

  it('renders an unusable-key error without balance data', () => {
    const { root, unmount } = mount(ProviderGenericQuotaCard, {
      providerType: 'openrouter',
      quota: {
        code: 'unknown',
        exhausted: false,
        freshness: 'error',
        refresh_state: { error: 'quota request failed (401): User not found' },
      },
    })

    expect(root.querySelector('[data-testid="provider-generic-quota"]')).toBeTruthy()
    expect(root.querySelector('[data-testid="provider-generic-quota-status"]')?.textContent)
      .toContain('quota request failed (401): User not found')
    unmount()
  })

  it('renders OpenRouter paid tier, limit, usage and expiry compactly', () => {
    const { root, unmount } = mount(ProviderGenericQuotaCard, {
      providerType: 'openrouter',
      quota: {
        code: 'ok', exhausted: false, is_free_tier: false,
        expires_at: '2030-01-01T00:00:00Z',
        balances: [{ unit: 'USD', available: 5, total: 20, used: 15 }],
      },
    })

    expect(root.textContent).not.toContain('OpenRouter Key')
    expect(root.textContent).not.toContain('Paid')
    expect(root.textContent).toContain('$5')
    expect(root.textContent).toContain('$20')
    expect(root.textContent).toContain('$15')
    expect(root.textContent).toContain('过期')
    unmount()
  })

  it('hides misleading usage progress for an unlimited Free OpenRouter key', () => {
    const { root, unmount } = mount(ProviderGenericQuotaCard, {
      providerType: 'openrouter',
      quota: {
        code: 'ok', exhausted: false, is_free_tier: true, unlimited: true,
        balances: [{ unit: 'USD', used: 45.23 }],
      },
    })

    expect(root.querySelector('[data-testid="provider-generic-quota"]')).toBeTruthy()
    expect(root.querySelector('[data-testid="provider-quota-progress-row"]')).toBeFalsy()
    expect(root.textContent).not.toContain('Free')
    expect(root.textContent).toContain('无限制')
    unmount()
  })

  it('shows an unlimited label without a misleading percentage bar for a Paid key', () => {
    const { root, unmount } = mount(ProviderGenericQuotaCard, {
      providerType: 'openrouter',
      quota: {
        code: 'ok', exhausted: false, is_free_tier: false, unlimited: true,
        balances: [{ unit: 'USD', used: 45.23 }],
      },
    })

    expect(root.textContent).toContain('无限制')
    expect(root.textContent).toContain('累计已用 $45.23')
    expect(root.querySelector('[data-testid="provider-quota-progress-bar"]')).toBeFalsy()
    unmount()
  })

  it('renders monthly quota usage and reset day', () => {
    const { root, unmount } = mount(ProviderMonthlyQuotaCard, {
      used: 25,
      quota: 100,
      resetDay: 15,
    })

    expect(root.querySelector('[data-testid="provider-monthly-quota-card"]')).toBeTruthy()
    expect(root.querySelector('[data-testid="provider-monthly-quota-percent"]')?.textContent).toContain('25.0%')
    expect(root.querySelector('[data-testid="provider-monthly-quota-amount"]')?.textContent).toContain('$25.00 / $100.00')
    expect(root.querySelector('[data-testid="provider-monthly-quota-reset"]')?.textContent).toContain('15')

    unmount()
  })

  it('normalizes quota progress and renders fallback footer text', () => {
    const { root, unmount } = mount(ProviderQuotaProgressRow, {
      label: 'Daily',
      remainingPercent: 120,
      meterClass: 'text-green-600',
      barClass: 'bg-green-500',
      resetText: '2h reset',
    })

    expect(root.querySelector('[data-testid="provider-quota-progress-meter"]')?.textContent?.trim()).toBe('100.0%')
    expect((root.querySelector('[data-testid="provider-quota-progress-bar"]') as HTMLElement).style.width).toBe('100%')
    expect(root.querySelector('[data-testid="provider-quota-progress-reset"]')?.textContent).toBe('2h reset')

    unmount()
  })

  it('renders section loading and updated state', () => {
    const Probe = defineComponent({
      setup() {
        return () => h(ProviderQuotaSectionHeader, {
          title: 'Account quota',
          loading: true,
          updatedText: '10:30',
        })
      },
    })

    const { root, unmount } = mount(Probe)

    expect(root.textContent).toContain('Account quota')
    expect(root.querySelector('[data-testid="provider-quota-header-loading"]')).toBeTruthy()
    expect(root.querySelector('[data-testid="provider-quota-header-updated"]')?.textContent).toBe('10:30')

    unmount()
  })
})
