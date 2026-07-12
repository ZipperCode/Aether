import { describe, expect, it } from 'vitest'
import { createApp } from 'vue'
import NousQuotaCard from '../NousQuotaCard.vue'
import { createI18n } from '@/i18n'

describe('NousQuotaCard', () => {
  it('renders decimal-string credits, billing, plan, ceilings and recovery state', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(NousQuotaCard, {
      quota: {
        code: 'cooldown',
        exhausted: false,
        provider_type: 'nous',
        plan_type: 'Free',
        updated_at: 1_780_000_000,
        current_period_end: 1_780_086_400,
        total_usable_credits: '0.1000',
        purchased_credits_remaining: '0.025',
        balance_usd: '12.50',
        billing_available: true,
        billing_stale: true,
        rate_limits: { rpm: 50, tpm: 500000, rph: 2100, tph: 6000000, kind: 'configured_limits' },
        windows: [
          { code: 'subscription_credits', remaining_value: '0.1', limit_value: '0.2', remaining_ratio: '0.5' },
          { code: 'monthly_spend', used_value: '25.5', limit_value: '1000', used_ratio: '0.0255' },
          { code: 'rate_limit', is_exhausted: true, reset_seconds: 42 },
        ],
      },
    })
    app.use(createI18n())
    app.mount(root)

    expect(root.textContent).toContain('套餐：Free')
    expect(root.textContent).toContain('订阅 Credits')
    expect(root.textContent).toContain('0.1 / 0.2')
    expect(root.textContent).toContain('本月消费')
    expect(root.textContent).toContain('$25.5 / $1000')
    expect(root.textContent).toContain('总可用 Credits 0.1')
    expect(root.textContent).toContain('购买 Credits 0.025')
    expect(root.textContent).toContain('账户余额 $12.5')
    expect(root.textContent).toContain('RPM 50')
    expect(root.textContent).toContain('账单数据已过期')
    expect(root.textContent).toContain('42 秒后恢复')

    app.unmount()
    root.remove()
  })

  it('shows unusable credits as exhausted instead of a full remaining meter', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(NousQuotaCard, {
      quota: {
        exhausted: true,
        exhausted_reason: 'no_usable_credits',
        provider_type: 'nous',
        windows: [
          {
            code: 'subscription_credits',
            is_exhausted: true,
            remaining_value: '0.1',
            limit_value: '0.1',
            remaining_ratio: '1.0',
          },
        ],
      },
    })
    app.use(createI18n())
    app.mount(root)

    expect(root.textContent).toContain('无可用推理 Credits')
    expect(root.querySelector('[data-testid="provider-quota-progress-meter"]')?.textContent).toContain('0.0%')

    app.unmount()
    root.remove()
  })
})
