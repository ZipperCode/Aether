import { describe, expect, it } from 'vitest'

import {
  getGenericQuotaSections,
  getGeminiCliAccountCreditsText,
  getQuotaDisplayText,
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
    } as const

    expect(getGenericQuotaSections(quota)).toEqual({
      balances: ['可用 $12.50 / 总额 $20'],
      windows: ['月度 剩余 800/1000'],
      rateLimits: ['RPM 60', 'TPM 100000'],
      status: ['数据已过期', '上游暂时不可用'],
    })
    expect(getQuotaDisplayText({ status_snapshot: { quota } } as never, 'siliconflow'))
      .toBe('可用 $12.50 / 总额 $20 | 月度 剩余 800/1000 | RPM 60 | TPM 100000')
  })

  it('shows DeepSeek as unavailable without retained balance when quota refresh fails', () => {
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
    } as const

    expect(getGenericQuotaSections(quota)).toEqual({
      balances: [],
      windows: [],
      rateLimits: [],
      status: ['不可用'],
    })
    expect(getQuotaDisplayText({ status_snapshot: { quota } } as never, 'deepseek'))
      .toBe('不可用')
  })

  it('treats quota authentication rejection as expired for every generic provider', () => {
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
      } as const

      expect(getGenericQuotaSections(quota, providerType)).toEqual({
        balances: [],
        windows: [],
        rateLimits: [],
        status: ['不可用'],
      })
      expect(getQuotaDisplayText({ status_snapshot: { quota } } as never, providerType))
        .toBe('不可用')
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
    } as const

    expect(getGenericQuotaSections(quota).balances).toEqual([
      '可用 9,007,199,254,740,993.123456789012345678 CNY / 总额 9,007,199,254,740,994.000000000000000001 CNY',
    ])
  })

  it('treats a Zhipu zero-balance fallback as unknown without model-probe evidence', () => {
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
    } as const

    expect(getGenericQuotaSections(quota)).toEqual({
      balances: [],
      windows: [],
      rateLimits: [],
      status: ['额度查询失败，额度未知'],
    })
    expect(getQuotaDisplayText({ status_snapshot: { quota } } as never, 'zhipu'))
      .toBe('额度未知，继续参与模型调度')
  })

  it('uses a successful model probe to distinguish a callable Zhipu key', () => {
    const quota = {
      provider_type: 'zhipu', kind: 'balance', code: 'ok', exhausted: false,
      balance_insufficient: true, balance_status: 'insufficient',
      token_plan_status: 'query_failed', token_plan_scheduling_blocked: false,
      balances: [{ unit: 'CNY', available: '0' }],
    } as const

    expect(getQuotaDisplayText({
      status_snapshot: {
        quota,
        model_probe: {
          status: 'ok', model: 'glm-5', tested_at: 1_700_000_000,
          status_code: 200, source: 'admin_model_test',
        },
      },
    } as never, 'zhipu')).toBe('模型调用已验证可用 · 额度查询失败，额度未知')
  })

  it('uses a failed model probe to distinguish a truly unavailable Zhipu key', () => {
    const quota = {
      provider_type: 'zhipu', kind: 'balance', code: 'ok', exhausted: false,
      balance_insufficient: true, balance_status: 'insufficient',
      token_plan_status: 'query_failed', token_plan_scheduling_blocked: false,
      balances: [{ unit: 'CNY', available: '0' }],
    } as const

    expect(getQuotaDisplayText({
      status_snapshot: {
        quota,
        model_probe: {
          status: 'failed', model: 'glm-5', tested_at: 1_700_000_001,
          status_code: 429, error: '余额不足或无可用资源包', source: 'admin_model_test',
        },
      },
    } as never, 'zhipu')).toBe('模型调用验证失败：余额不足或无可用资源包')
  })

  it('translates a stale Zhipu 1113 error into an actionable balance status', () => {
    const quota = {
      provider_type: 'zhipu',
      code: 'http_client_error',
      exhausted: false,
      freshness: 'stale',
      refresh_state: {
        error: 'http_client_error: upstream business code 1113: account balance is insufficient',
      },
    } as const

    expect(getGenericQuotaSections(quota).status).toEqual(['数据已过期', '余额不足'])
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
        quota: {
          provider_type: 'windsurf',
          code: 'cooldown',
          exhausted: false,
        },
      },
    }, 'windsurf')).toBe('冷却中')
  })

  it('includes Windsurf quota windows and model availability in display text', () => {
    expect(getQuotaDisplayText({
      status_snapshot: {
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

  it('uses Windsurf model availability when no quota window is present', () => {
    expect(getQuotaDisplayText({
      status_snapshot: {
        quota: {
          provider_type: 'windsurf',
          code: 'ok',
          exhausted: false,
          allowed_models_count: 3,
        },
      },
    }, 'windsurf')).toBe('可用模型 3 个')
  })
})
