import { describe, expect, it } from 'vitest'
import { createApp, defineComponent, h } from 'vue'

import PoolKeyQuotaPanel from '@/features/pool/components/PoolKeyQuotaPanel.vue'
import PoolKeyStatsPanel from '@/features/pool/components/PoolKeyStatsPanel.vue'
import PoolKeyHealthIndicator from '@/features/pool/components/PoolKeyHealthIndicator.vue'
import { createI18n } from '@/i18n'

describe('pool key display panels', () => {
  it('renders the provider-compatible key health percentage and progress', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(PoolKeyHealthIndicator, { score: 0.73 })
    app.mount(root)

    expect(root.querySelector('[data-testid="pool-key-health"]')?.textContent).toContain('73%')
    expect((root.querySelector('[data-testid="pool-key-health-bar"]') as HTMLElement).style.width).toBe('73%')

    app.unmount()
    root.remove()
  })

  it('renders codex cycle stats with stable test hooks', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(PoolKeyStatsPanel, {
      cycle: true,
      cycleGroups: [
        {
          code: '5h',
          label: '5H',
          metrics: [{ key: 'request_count', label: '请求', value: '12', missing: false, numericValue: 12 }],
        },
        {
          code: 'weekly',
          label: '周',
          metrics: [{ key: 'request_count', label: '请求', value: '88', missing: false, numericValue: 88 }],
        },
      ],
      accountMetrics: [],
    })
    app.use(createI18n())
    app.mount(root)

    const stats = root.querySelector('[data-testid="pool-stats-cycle-text"]')
    const requestValue = root.querySelector('[data-testid="pool-stats-cycle-request_count"]')
    expect(stats).toBeTruthy()
    expect(stats?.className).toContain('w-full')
    expect(stats?.className).toContain('max-w-[168px]')
    expect(requestValue?.textContent?.trim()).toBe('12/88')
    expect(requestValue?.previousElementSibling?.textContent?.trim()).toBe('请求')
    expect(requestValue?.parentElement?.className).toContain('justify-between')
    expect(requestValue?.className).toContain('grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)]')
    expect(requestValue?.className).toContain('w-[112px]')
    expect(requestValue?.children[0]?.className).toContain('text-right')
    expect(requestValue?.children[1]?.textContent).toBe('/')
    expect(requestValue?.children[2]?.className).toContain('text-left')
    expect(root.querySelectorAll('[data-cycle-stat-part="divider"]')).toHaveLength(3)
    expect(root.querySelector('[data-testid="pool-stats-cycle-small-overlay"]')).toBeNull()
    expect(root.querySelector('[data-testid="pool-stats-cycle-large-base"]')).toBeNull()
    expect(root.textContent).not.toContain('5H')
    expect(root.textContent).not.toContain('周')

    app.unmount()
    root.remove()
  })

  it('renders quota progress rows and fallback quota text', () => {
    const Probe = defineComponent({
      setup() {
        return () => h('div', [
          h(PoolKeyQuotaPanel, {
            items: [{
              label: '5H',
              remainingPercent: 42,
              resetText: '1h 后重置',
              meterText: '42.0%',
              barClass: 'bg-amber-500',
              meterClass: 'text-amber-600',
            }],
          }),
          h(PoolKeyQuotaPanel, {
            items: [],
            accountQuotaText: null,
            fallbackText: '额度未知',
            textClass: 'text-muted-foreground',
            variant: 'mobile',
          }),
        ])
      },
    })

    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(Probe)
    app.use(createI18n())
    app.mount(root)

    expect(root.querySelector('[data-testid="pool-quota-reset-text"]')?.textContent).toBe('1h 后重置')
    expect(root.querySelector('[data-testid="pool-quota-meter-text"]')?.textContent).toBe('42.0%')
    expect(root.textContent).toContain('额度未知')

    app.unmount()
    root.remove()
  })

  it('renders structured provider balances instead of plain fallback text', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(PoolKeyQuotaPanel, {
      items: [],
      providerType: 'deepseek',
      quota: {
        code: 'ok', exhausted: false, kind: 'balance',
        balances: [{ unit: 'CNY', available: '47.73', granted: '0', topped_up: '47.73' }],
      },
    })
    app.use(createI18n())
    app.mount(root)

    expect(root.querySelector('[data-testid="pool-quota-balance"]')?.textContent).toContain('47.73 CNY')
    expect(root.textContent).not.toContain('可用 47.73 CNY')
    app.unmount()
    root.remove()
  })

  it('renders every currency with exact decimal strings', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(PoolKeyQuotaPanel, {
      items: [],
      providerType: 'siliconflow',
      quota: {
        code: 'ok', exhausted: false, kind: 'balance',
        balances: [
          { unit: 'CNY', available: '9007199254740993.123456789' },
          { unit: 'USD', available: '0.000000000000000001' },
        ],
      },
    })
    app.use(createI18n())
    app.mount(root)

    const balances = [...root.querySelectorAll('[data-testid="pool-quota-balance"]')]
      .map(element => element.textContent)
    expect(balances).toHaveLength(2)
    expect(balances[0]).toContain('9,007,199,254,740,993.123456789 CNY')
    expect(balances[1]).toContain('$0.000000000000000001')
    expect(root.querySelector('[data-testid="pool-quota-available"]')?.classList).toContain('break-all')
    expect(root.querySelector('[data-testid="pool-quota-available"]')?.classList).not.toContain('whitespace-nowrap')
    app.unmount()
    root.remove()
  })

  it('uses an alert tone for a negative available balance', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(PoolKeyQuotaPanel, {
      items: [], providerType: 'siliconflow',
      quota: { code: 'ok', exhausted: true, balances: [{ unit: 'CNY', available: '-33.6545' }] },
    })
    app.use(createI18n())
    app.mount(root)

    const available = root.querySelector('[data-testid="pool-quota-available"]')
    expect(available?.classList).toContain('text-red-600')
    expect(available?.classList).toContain('whitespace-nowrap')
    expect(available?.classList).not.toContain('break-all')
    app.unmount()
    root.remove()
  })

  it('shows DeepSeek quota failures as unavailable on desktop and mobile', () => {
    for (const variant of ['desktop', 'mobile'] as const) {
      const root = document.createElement('div')
      document.body.appendChild(root)
      const app = createApp(PoolKeyQuotaPanel, {
        items: [], providerType: 'deepseek', variant,
        fallbackText: '可用 9.25 USD',
        quota: {
          provider_type: 'deepseek', kind: 'balance', code: 'http_server_error',
          exhausted: false, freshness: 'stale',
          balances: [{ unit: 'USD', available: '9.25' }],
          refresh_state: { error: 'http_server_error: quota upstream returned an error' },
        },
      })
      app.use(createI18n())
      app.mount(root)

      const unavailable = root.querySelector('[data-testid="pool-quota-unavailable"]')
      expect(unavailable?.textContent?.trim()).toBe('不可用')
      expect(unavailable?.classList).toContain('text-red-700')
      expect(root.querySelector('[data-testid="pool-quota-balance"]')).toBeNull()
      expect(root.textContent).not.toContain('9.25')
      app.unmount()
      root.remove()
    }
  })

  it('hides retained balances after generic quota authentication rejection', () => {
    for (const variant of ['desktop', 'mobile'] as const) {
      const root = document.createElement('div')
      document.body.appendChild(root)
      const app = createApp(PoolKeyQuotaPanel, {
        items: [], providerType: 'openrouter', variant,
        fallbackText: '可用 18.50 CNY',
        quota: {
          provider_type: 'openrouter', kind: 'balance', code: 'http_unauthorized',
          exhausted: false, freshness: 'stale',
          balances: [{ unit: 'CNY', available: '18.50' }],
          refresh_state: { error: 'http_unauthorized: quota upstream rejected authentication' },
        },
      })
      app.use(createI18n())
      app.mount(root)

      expect(root.querySelector('[data-testid="pool-quota-unavailable"]')?.textContent?.trim())
        .toBe('不可用')
      expect(root.querySelector('[data-testid="pool-quota-balance"]')).toBeNull()
      expect(root.textContent).not.toContain('http_unauthorized')
      expect(root.textContent).not.toContain('18.50')
      app.unmount()
      root.remove()
    }
  })

  it('labels a zero Zhipu balance as insufficient', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(PoolKeyQuotaPanel, {
      items: [], providerType: 'zhipu',
      quota: {
        code: 'ok', exhausted: false, balance_insufficient: true,
        balances: [{ unit: 'CNY', available: '0' }],
      },
    })
    app.use(createI18n())
    app.mount(root)

    expect(root.querySelector('[data-testid="pool-quota-balance"]')?.textContent).toContain('余额不足')
    expect(root.querySelector('[data-testid="pool-quota-available"]')?.classList).toContain('text-red-600')
    app.unmount()
    root.remove()
  })

  it('hides an ambiguous Zhipu zero balance when no model probe exists', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(PoolKeyQuotaPanel, {
      items: [], providerType: 'zhipu',
      quota: {
        kind: 'balance', code: 'ok', exhausted: false, balance_insufficient: true,
        token_plan_status: 'query_failed', token_plan_scheduling_blocked: false,
        balances: [{ unit: 'CNY', available: '0' }],
      },
    })
    app.use(createI18n())
    app.mount(root)

    expect(root.querySelector('[data-testid="pool-quota-balance"]')).toBeNull()
    expect(root.querySelector('[data-testid="pool-model-availability"]')?.textContent)
      .toContain('额度未知，继续参与模型调度')
    expect(root.querySelector('[data-testid="pool-model-availability"]')?.classList).toContain('text-amber-700')
    app.unmount()
    root.remove()
  })

  /** 验证最终 Antigravity 家族摘要使用进度条、百分号范围和重置时间。 */
  it('renders Antigravity quota summaries with progress tracks', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(PoolKeyQuotaPanel, {
      items: [
        {
          label: 'Gemini额度',
          remainingPercent: 90.6,
          resetText: '1h 后重置',
          meterText: '90.6%–100%',
          barClass: 'bg-emerald-500',
          meterClass: 'text-emerald-600',
        },
        {
          label: 'Claude & ChatGPT',
          remainingPercent: 100,
          resetText: '1h 后重置',
          meterText: '100%',
          barClass: 'bg-emerald-500',
          meterClass: 'text-emerald-600',
        },
      ],
    })
    app.use(createI18n())
    app.mount(root)

    expect(root.querySelector('[data-testid="pool-quota-rows"]')?.className).toContain('space-y-2')
    expect(Array.from(root.querySelectorAll('[data-testid="pool-quota-period-label"]')).map(node => node.textContent)).toEqual([
      'Gemini额度',
      'Claude & ChatGPT',
    ])
    expect(Array.from(root.querySelectorAll('[data-testid="pool-quota-meter-text"]')).map(node => node.textContent)).toEqual(['90.6%–100%', '100%'])
    expect(root.querySelectorAll('[data-testid="pool-quota-progress-track"]')).toHaveLength(2)
    expect(Array.from(root.querySelectorAll('[data-testid="pool-quota-reset-text"]')).map(node => node.textContent)).toEqual([
      '1h 后重置',
      '1h 后重置',
    ])

    app.unmount()
    root.remove()
  })

  it('shows successful model-probe evidence for an ambiguous Zhipu quota', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(PoolKeyQuotaPanel, {
      items: [], providerType: 'zhipu',
      modelProbe: { status: 'ok', model: 'glm-5', status_code: 200 },
      quota: {
        kind: 'balance', code: 'ok', exhausted: false, balance_insufficient: true,
        token_plan_status: 'query_failed', token_plan_scheduling_blocked: false,
        balances: [{ unit: 'CNY', available: '0' }],
      },
    })
    app.use(createI18n())
    app.mount(root)

    const availability = root.querySelector('[data-testid="pool-model-availability"]')
    expect(availability?.textContent).toContain('模型调用已验证可用')
    expect(availability?.textContent).toContain('额度查询失败，额度未知')
    expect(availability?.classList).toContain('text-emerald-700')
    app.unmount()
    root.remove()
  })

  it('does not render a percentage meter for unlimited balances', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(PoolKeyQuotaPanel, {
      items: [],
      providerType: 'openrouter',
      quota: {
        code: 'ok', exhausted: false, kind: 'balance', unlimited: true,
        balances: [{ unit: 'USD', available: null, used: '45.421278308', total: null }],
      },
    })
    app.use(createI18n())
    app.mount(root)

    expect(root.textContent).toContain('无限制')
    expect(root.textContent).not.toContain('100.0%')
    expect(root.querySelector('[style*="width: 100%"]')).toBeFalsy()
    app.unmount()
    root.remove()
  })

  it('renders structured subscription windows when the pool has no legacy items', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(PoolKeyQuotaPanel, {
      items: [],
      providerType: 'kimi_coding',
      quota: {
        code: 'ok', exhausted: false, kind: 'subscription',
        windows: [{ code: 'cycle', label: '周期配额', remaining_ratio: '0.99', remaining_value: '99', limit_value: '100' }],
      },
    })
    app.use(createI18n())
    app.mount(root)

    expect(root.textContent).toContain('周期配额')
    expect(root.querySelector('[data-testid="pool-quota-meter-text"]')?.textContent).toBe('99 / 100')
    app.unmount()
    root.remove()
  })

  it('preserves exact decimal strings in subscription window values', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(PoolKeyQuotaPanel, {
      items: [],
      providerType: 'kimi_coding',
      quota: {
        code: 'ok', exhausted: false, kind: 'subscription',
        windows: [{
          code: 'cycle', label: '周期配额', remaining_ratio: '0.5',
          remaining_value: '9007199254740993.123456789012345678',
          limit_value: '18014398509481986.246913578024691356',
        }],
      },
    })
    app.use(createI18n())
    app.mount(root)

    const meter = root.querySelector('[data-testid="pool-quota-meter-text"]')
    expect(meter?.textContent).toBe(
      '9,007,199,254,740,993.123456789012345678 / 18,014,398,509,481,986.246913578024691356',
    )
    expect(meter?.classList).toContain('break-all')
    app.unmount()
    root.remove()
  })

  for (const providerType of ['deepseek', 'openrouter', 'moonshot', 'kimi_coding', 'siliconflow', 'zhipu', 'zai']) {
    it(`keeps the pool quota column visible for ${providerType} before a snapshot exists`, () => {
      const root = document.createElement('div')
      document.body.appendChild(root)
      const app = createApp(PoolKeyQuotaPanel, {
        items: [],
        providerType,
        quota: null,
      })
      app.use(createI18n())
      app.mount(root)

      expect(root.textContent).toContain('待刷新')
      app.unmount()
      root.remove()
    })
  }

  for (const providerType of ['codex', 'gemini_cli', 'kiro', 'windsurf', 'grok', 'nous', 'antigravity', 'chatgpt_web']) {
    it(`does not route legacy ${providerType} snapshots through the new generic pool UI`, () => {
      const root = document.createElement('div')
      document.body.appendChild(root)
      const app = createApp(PoolKeyQuotaPanel, {
        items: [],
        providerType,
        quota: {
          code: 'ok', exhausted: false,
          balances: [{ unit: 'USD', available: '99' }],
          windows: [{ code: 'legacy', label: '不应显示', remaining_ratio: 0.5 }],
        },
      })
      app.use(createI18n())
      app.mount(root)

      expect(root.querySelector('[data-testid="pool-quota-balance"]')).toBeFalsy()
      expect(root.querySelector('[data-testid="pool-quota-meter-text"]')).toBeFalsy()
      expect(root.textContent).not.toContain('不应显示')
      expect(root.textContent?.trim()).toBe('-')
      app.unmount()
      root.remove()
    })
  }
  it('renders single-cycle stats as plain text', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(PoolKeyStatsPanel, {
      cycle: true,
      cycleGroups: [{
        code: 'monthly',
        label: '月',
        metrics: [
          { key: 'request_count', label: '请求', value: '31', missing: false, numericValue: 31 },
          { key: 'total_tokens', label: 'Token', value: '38.8K', missing: false, numericValue: 38_800 },
          { key: 'total_cost_usd', label: '费用', value: '$0.077', missing: false, numericValue: 0.077 },
        ],
      }],
      accountMetrics: [],
    })
    app.use(createI18n())
    app.mount(root)

    const requestValue = root.querySelector('[data-testid="pool-stats-cycle-request_count"]')
    expect(requestValue?.textContent?.trim()).toBe('-/31')
    expect(requestValue?.previousElementSibling?.textContent?.trim()).toBe('请求')
    expect(requestValue?.className).toContain('grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)]')
    expect(requestValue?.children[0]?.textContent).toBe('-')
    expect(requestValue?.children[1]?.textContent).toBe('/')
    expect(requestValue?.children[1]?.className).toContain('w-1.5')
    expect(requestValue?.children[2]?.textContent).toBe('31')
    expect(requestValue?.children[2]?.className).toContain('text-left')
    expect(root.querySelector('[data-testid="pool-stats-cycle-single-marker"]')).toBeNull()
    expect(root.querySelector('[data-testid="pool-stats-cycle-bar-request_count"]')).toBeNull()
    expect(root.textContent).not.toContain('月')

    app.unmount()
    root.remove()
  })

})
