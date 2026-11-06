import { describe, expect, it } from 'vitest'
import type { QuotaStatusSnapshot } from '@/api/endpoints/types'

import {
  getGenericQuotaGroups,
  getGenericQuotaSections,
  getGeminiCliAccountCreditsText,
  getGenericQuotaStatusTitle,
  getQuotaDisplayText,
  getQuotaQueryStatusLabel,
  getQuotaStatusBadgeInfo,
} from '../providerKeyQuota'

describe('providerKeyQuota', () => {
  it('prefers provider-neutral balances and keeps quota categories separate', () => {
    const quota = {
      kind: 'balance',
      code: 'ok',
      exhausted: false,
      freshness: 'stale',
      balances: [{ unit: 'USD', available: '12.50', total: 20 }],
      windows: [{ code: 'monthly', label: '月度', remaining_value: '800', limit_value: '1000' }],
      rate_limits: { rpm: 60, tpm: 100000, kind: 'configured_limits' },
      refresh_state: { error: '上游暂时不可用' },
    } satisfies QuotaStatusSnapshot

    expect(getGenericQuotaSections(quota)).toEqual({
      balances: ['可用余额 $12.50 · 总额 $20'],
      windows: ['月度 剩余 80.0% · 800 / 1,000 单位未知'],
      rateLimits: ['RPM 60', 'TPM 100,000'],
      status: ['查询失败'],
    })
  })

  it('keeps the previous DeepSeek balance with a stale query status after refresh failure', () => {
    const quota = {
      provider_type: 'deepseek',
      kind: 'balance',
      code: 'http_server_error',
      exhausted: false,
      freshness: 'stale',
      balances: [{ unit: 'CNY', available: '88.5', total: '88.5' }],
      refresh_state: {
        last_attempt_at: 1_700_000_100,
        last_success_at: 1_700_000_000,
        error: 'http_server_error: quota upstream returned an error',
      },
    } satisfies QuotaStatusSnapshot

    const sections = getGenericQuotaSections(quota)
    expect(sections.balances[0]).toContain('88.5 CNY')
    expect(sections.status).toContain('查询失败')
    // stale 事实仍保留在分组投影中，只是不再生成过期文案
    expect(getGenericQuotaGroups(quota)[0].stale).toBe(true)
  })

  it('keeps query authentication rejection separate from key validity', () => {
    for (const providerType of ['openrouter', 'moonshot', 'kimi_coding', 'siliconflow', 'zhipu', 'zai']) {
      const quota = {
        provider_type: providerType,
        kind: 'balance',
        code: 'http_unauthorized',
        exhausted: false,
        freshness: 'stale',
        balances: [{ unit: 'CNY', available: '88.5' }],
        refresh_state: {
          error: 'http_unauthorized: quota upstream rejected authentication',
        },
      } satisfies QuotaStatusSnapshot

      const sections = getGenericQuotaSections(quota, providerType)
      expect(sections.balances[0]).toContain('88.5 CNY')
      expect(sections.status).toContain('查询失败')
      const text = getQuotaDisplayText({ status_snapshot: { quota } } as never, providerType)
      expect(text).toContain('查询失败')
      expect(text).not.toContain('不可用')
      expect(text).not.toContain('Expired')
    }
  })

  it('preserves decimal strings beyond Number precision', () => {
    const quota = {
      code: 'ok',
      exhausted: false,
      balances: [{
        unit: 'CNY',
        available: '9007199254740993.123456789012345678',
        total: '9007199254740994.000000000000000001',
      }],
    } satisfies QuotaStatusSnapshot

    expect(getGenericQuotaSections(quota).balances).toEqual([
      '可用余额 9,007,199,254,740,993.123456789012345678 CNY · 总额 9,007,199,254,740,994.000000000000000001 CNY',
    ])
  })

  it('renders the Zhipu informational balance fallback at zero', () => {
    const quota = {
      provider_type: 'zhipu',
      kind: 'balance',
      code: 'ok',
      exhausted: false,
      balance_insufficient: true,
      token_plan_status: 'query_failed',
      token_plan_scheduling_blocked: false,
      token_plan_error: 'upstream business code 500: quota upstream returned a business error',
      balances: [{ unit: 'CNY', available: '0' }],
    } satisfies QuotaStatusSnapshot

    expect(getGenericQuotaSections(quota).balances[0]).toContain('0 CNY')
  })

  it('keeps a stale Zhipu query error separate from a balance fact', () => {
    const quota = {
      provider_type: 'zhipu',
      code: 'http_client_error',
      exhausted: false,
      freshness: 'stale',
      refresh_state: {
        error: 'http_client_error: upstream business code 1113: account balance is insufficient',
      },
    } satisfies QuotaStatusSnapshot

    expect(getGenericQuotaSections(quota).balances).toEqual([])
  })

  it('includes Codex Spark quota windows in display text', () => {
    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: {
          code: 'valid',
        },
        account: {
          code: 'ok',
          blocked: false,
        },
        quota: {
          provider_type: 'codex',
          code: 'ok',
          exhausted: false,
          windows: [
            {
              code: 'weekly',
              remaining_ratio: 0.9,
            },
            {
              code: '5h',
              remaining_ratio: 0.8,
            },
            {
              code: 'spark_5h',
              remaining_ratio: 0.6,
            },
            {
              code: 'spark_weekly',
              remaining_ratio: 0.95,
            },
          ],
        },
      },
    }, 'codex')).toBe('周剩余 90.0% | 5H剩余 80.0% | Spark5H剩余 60.0% | Spark周剩余 95.0%')
  })

  it('uses actual Codex window durations and ignores zero placeholders', () => {
    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          provider_type: 'codex',
          code: 'ok',
          exhausted: false,
          windows: [
            {
              code: 'weekly',
              label: '周',
              window_minutes: 0,
              remaining_ratio: 1,
            },
            {
              code: '5h',
              label: '5H',
              window_minutes: 43_800,
              remaining_ratio: 0.86,
            },
          ],
        },
      },
    }, 'codex')).toBe('月剩余 86.0%')
  })

  it('keeps account and model Codex weekly quotas distinct in display text', () => {
    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          provider_type: 'codex',
          code: 'ok',
          exhausted: false,
          windows: [
            {
              code: 'weekly',
              label: '周',
              scope: 'account',
              window_minutes: 10_080,
              remaining_ratio: 0.9,
            },
            {
              code: 'additional_0_primary',
              label: 'gpt-reserve',
              scope: 'model',
              model: 'gpt-reserve',
              window_minutes: 10_080,
              remaining_ratio: 0.4,
            },
          ],
        },
      },
    }, 'codex')).toBe('周剩余 90.0% | gpt-reserve 周剩余 40.0%')
  })

  it('formats Grok account quota from structured quota windows', () => {
    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: {
          code: 'valid',
        },
        account: {
          code: 'ok',
          blocked: false,
        },
        quota: {
          provider_type: 'grok',
          code: 'ok',
          exhausted: false,
          windows: [
            {
              code: 'account',
              scope: 'account',
              used_value: 2,
              limit_value: 10,
              remaining_ratio: 0.8,
            },
          ],
        },
      },
    }, 'grok')).toBe('剩余 80.0% (8/10)')
  })

  it('formats Grok mode quota from model-scoped windows', () => {
    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: {
          code: 'valid',
        },
        account: {
          code: 'ok',
          blocked: false,
        },
        quota: {
          provider_type: 'grok',
          code: 'ok',
          exhausted: false,
          plan_type: 'heavy',
          windows: [
            {
              code: 'model:quota_auto',
              label: 'auto',
              scope: 'model',
              remaining_ratio: 0.4,
              used_value: 90,
              limit_value: 150,
            },
            {
              code: 'model:quota_heavy',
              label: 'heavy',
              scope: 'model',
              remaining_ratio: 0,
              used_value: 20,
              limit_value: 20,
            },
          ],
        },
      },
    }, 'grok')).toBe('Auto剩余 40.0% (60/150) | Heavy剩余 0.0% (0/20)')
  })

  it('formats Gemini CLI AI credits from status snapshot and upstream metadata', () => {
    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: { code: 'none' },
        account: { code: 'ok', blocked: false },
        quota: {
          provider_type: 'gemini_cli',
          code: 'ok',
          exhausted: false,
          credits: {
            remaining: 123.5,
            consumed: 7,
          },
        },
      },
    }, 'gemini_cli')).toBe('AI Credits 剩余 123.5')

    expect(getGeminiCliAccountCreditsText({
      status_snapshot: {
        oauth: { code: 'none' },
        account: { code: 'ok', blocked: false },
        quota: {
          provider_type: 'gemini_cli',
          code: 'ok',
          exhausted: false,
        },
      },
      upstream_metadata: {
        gemini_cli: {
          paidTier: {
            id: 'g1-pro-tier',
            availableCredits: '41.5',
          },
        },
      },
    }, 'gemini_cli')).toBe('AI Credits 剩余 41.5')
  })

  it('formats ChatGPT Web image quota as remaining count', () => {
    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: { code: 'none' },
        account: { code: 'ok', blocked: false },
        quota: {
          provider_type: 'chatgpt_web',
          code: 'ok',
          exhausted: false,
          windows: [
            {
              code: 'image_gen',
              scope: 'account',
              remaining_ratio: 0.96,
              used_value: 1,
              remaining_value: 24,
              limit_value: 25,
            },
          ],
        },
      },
    }, 'chatgpt_web')).toBe('生图剩余 24/25')
  })

  it('surfaces Windsurf hard account states', () => {
    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: { code: 'none' },
        account: { code: 'ok', blocked: false },
        quota: {
          provider_type: 'windsurf',
          code: 'quarantined',
          label: '账号隔离中',
          exhausted: false,
        },
      },
    }, 'windsurf')).toBe('账号隔离中')

    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: { code: 'none' },
        account: { code: 'ok', blocked: false },
        quota: {
          provider_type: 'windsurf',
          code: 'cooldown',
          label: '冷却中',
          exhausted: false,
        },
      },
    }, 'windsurf')).toBe('冷却中')

    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: { code: 'none' },
        account: { code: 'ok', blocked: false },
        quota: {
          provider_type: 'windsurf',
          code: 'cooldown',
          exhausted: false,
        },
      },
    }, 'windsurf')).toBe('冷却中')
  })

  it('includes Windsurf quota windows in display text', () => {
    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: { code: 'none' },
        account: { code: 'ok', blocked: false },
        quota: {
          provider_type: 'windsurf',
          code: 'ok',
          exhausted: false,
          allowed_models_count: 7,
          windows: [
            {
              code: 'daily',
              remaining_ratio: 0.75,
            },
            {
              code: 'weekly',
              remaining_ratio: 0.5,
            },
            {
              code: 'prompt',
              remaining_value: 12,
              limit_value: 20,
            },
            {
              code: 'flex',
              used_value: 2,
              limit_value: 5,
            },
          ],
        },
      },
    }, 'windsurf')).toBe('日剩余 75.0% | 周剩余 50.0% | Prompt 剩余 12/20 | Flex 剩余 3/5 | 可用模型 7 个')

    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: { code: 'none' },
        account: { code: 'ok', blocked: false },
        quota: {
          provider_type: 'windsurf',
          code: 'cooldown',
          label: '冷却中',
          exhausted: false,
          rate_limit: {
            limited: true,
            has_capacity: true,
            messages_remaining: -1,
            max_messages: -1,
          },
          allowed_models_count: 118,
          windows: [
            {
              code: 'daily',
              remaining_ratio: 0.99,
            },
            {
              code: 'weekly',
              remaining_ratio: 1,
            },
            {
              code: 'prompt',
              remaining_value: 100,
              limit_value: 100,
            },
            {
              code: 'rate_limit',
              reset_seconds: null,
              is_exhausted: false,
            },
          ],
        },
      },
    }, 'windsurf')).toBe('日剩余 99.0% | 周剩余 100.0% | Prompt 剩余 100/100 | 可用模型 118 个')
  })

  it('uses Windsurf allowed model count when no quota window is present', () => {
    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: { code: 'none' },
        account: { code: 'ok', blocked: false },
        quota: {
          provider_type: 'windsurf',
          code: 'ok',
          exhausted: false,
          allowed_models_count: 3,
        },
      },
    }, 'windsurf')).toBe('可用模型 3 个')
  })

  it('formats xAI weekly credits as remaining percent', () => {
    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: {
          provider_type: 'xai',
          code: 'ok',
          exhausted: false,
          windows: [
            {
              code: 'usage',
              scope: 'account',
              used_ratio: 0.46,
              remaining_ratio: 0.54,
            },
            {
              code: 'prepaid',
              remaining_value: 12.5,
            },
          ],
        },
      },
    }, 'xai')).toBe('剩余 54.0% | 预付剩余 12.5')
  })
  it('assigns neutral outline to capability conclusions and reserves red for real blocks and errors', () => {
    // 流程或能力结论应为中性 outline，绝不渲染成红色
    expect(getQuotaStatusBadgeInfo('不支持查询')?.tone).toBe('neutral')
    expect(getQuotaStatusBadgeInfo('不支持查询')?.variant).toBe('outline')
    expect(getQuotaStatusBadgeInfo('不支持查询')?.class).toContain('text-muted-foreground')

    expect(getQuotaStatusBadgeInfo('不适用')?.tone).toBe('neutral')
    expect(getQuotaStatusBadgeInfo('未查询')?.tone).toBe('neutral')
    expect(getQuotaStatusBadgeInfo('暂无可查询额度')?.tone).toBe('neutral')

    // 真正阻断或查询失败为红色 destructive
    expect(getQuotaStatusBadgeInfo('额度耗尽·需人工恢复')?.tone).toBe('destructive')
    expect(getQuotaStatusBadgeInfo('额度耗尽·需人工恢复')?.variant).toBe('destructive')
    expect(getQuotaStatusBadgeInfo('额度查询失败')?.tone).toBe('destructive')

    // 警告类（疑似不足、部分更新）为琥珀色 outline
    expect(getQuotaStatusBadgeInfo('疑似额度不足')?.tone).toBe('warning')
    expect(getQuotaStatusBadgeInfo('额度部分更新')?.tone).toBe('warning')
  })

  it('generates explanatory tooltips confirming query capability does not block model calls', () => {
    const titleUnsupported = getGenericQuotaStatusTitle({
      quota: {
        sources: [{ id: 'b1', label: '余额', product: 'account_balance', scope: 'account', query_status: 'unsupported', freshness: 'fresh', refresh_state: {} }],
        code: 'ok',
        exhausted: false,
      },
      providerType: 'siliconflow',
    })
    expect(titleUnsupported).toContain('不影响实际模型调用')
  })

  it('renders OpenRouter free requests window with request counts and model scope without implying account exhaustion', () => {
    const quota = {
      provider_type: 'openrouter',
      code: 'ok',
      exhausted: false,
      sources: [
        {
          id: 'key_spending',
          label: 'Key 消费限额',
          product: 'key_spending_limit',
          scope: 'key',
          query_status: 'ok' as const,
          freshness: 'fresh',
          refresh_state: {},
        },
        {
          id: 'free_requests',
          label: '免费模型每日请求',
          product: 'model_requests',
          scope: 'model',
          query_status: 'ok' as const,
          freshness: 'fresh',
          refresh_state: {},
        },
      ],
      windows: [
        {
          source_id: 'free_requests',
          code: 'free_model_daily_requests',
          scope: 'model',
          unit: 'requests',
          remaining_value: '48',
          limit_value: '50',
          used_value: '2',
          remaining_ratio: 0.96,
        },
      ],
    } satisfies QuotaStatusSnapshot

    const groups = getGenericQuotaGroups(quota, 'openrouter')
    expect(groups).toHaveLength(2)

    // Key 消费限额组
    expect(groups[0].label).toBe('Key 消费限额')
    expect(groups[0].notes).toContain('账户余额未知')

    // 免费模型每日请求组
    expect(groups[1].label).toBe('免费模型每日请求')
    // 免费来源不得包含账户余额未知或限制 note，不暗示账号耗尽
    expect(groups[1].notes).not.toContain('账户余额未知')
    expect(groups[1].metadata).toContain('免费模型')

    // 窗口必须显示次数而不是仅百分比
    expect(groups[1].windows[0].meterText).toBe('48 / 50 次 · 剩余 96.0%')
  })

  it('omits expired and historical hints in shared projections while keeping values', () => {
    const quota = {
      provider_type: 'deepseek',
      kind: 'balance',
      code: 'ok',
      exhausted: false,
      freshness: 'stale',
      observed_at: 1_700_000_000,
      balances: [{ unit: 'CNY', available: '25.00' }],
    } satisfies QuotaStatusSnapshot

    // 旧快照不再生成“历史快照/数据已过期”提示，但 stale 事实与数值保留
    expect(getQuotaQueryStatusLabel(quota, 'deepseek')).toBeNull()
    const groups = getGenericQuotaGroups(quota, 'deepseek')
    expect(groups[0].notes).toEqual([])
    expect(groups[0].stale).toBe(true)
    expect(groups[0].balances[0].available).toBe('25.00 CNY')
    expect(getQuotaDisplayText({ status_snapshot: { quota } } as never, 'deepseek')).toBe('可用余额 25.00 CNY')
  })
})
