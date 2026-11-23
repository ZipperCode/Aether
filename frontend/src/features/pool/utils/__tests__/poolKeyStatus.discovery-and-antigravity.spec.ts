import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import type { PoolKeyDetail } from '@/api/endpoints/pool'
import { setI18nLocale } from '@/i18n'
import {
  formatUnixSecondsWithSeconds,
  getAntigravityInferenceRejectionDisplay,
  getPoolKeyModelFetchDisplay,
  getSchedulingBadgeLabel,
  getSchedulingBadgeVariant,
  getSchedulingStatus,
  getSchedulingTitle,
} from '../poolKeyStatus'

function poolKey(overrides: Partial<PoolKeyDetail> = {}): PoolKeyDetail {
  return {
    key_id: 'key-1',
    key_name: 'Account 1',
    is_active: true,
    auth_type: 'oauth',
    account_quota: null,
    cooldown_reason: null,
    cooldown_ttl_seconds: null,
    cost_window_usage: 0,
    cost_limit: null,
    request_count: 0,
    total_tokens: 0,
    total_cost_usd: '0',
    sticky_sessions: 0,
    lru_score: null,
    created_at: null,
    last_used_at: null,
    ...overrides,
  }
}

beforeEach(() => setI18nLocale('zh-CN'))
afterEach(() => setI18nLocale('zh-CN'))

describe('Codex OAuth discovery status', () => {
  it('shows pending, failed with actionable guidance, and ready from fetch result and catalog presence without using allowed_models', () => {
    const pendingManual = poolKey({
      auto_fetch_models: false,
      codex_model_catalog_ready: false,
      allowed_models: ['gpt-5'],
    })
    expect(getPoolKeyModelFetchDisplay(pendingManual, 'codex')).toMatchObject({
      modelFetchLabel: '模型待发现',
      modelFetchHint: '未开启自动获取，请在模型权限中手动刷新或编辑账号开启自动获取',
    })

    const pendingAuto = poolKey({
      auto_fetch_models: true,
      codex_model_catalog_ready: false,
      last_models_fetch_at: 1760000000,
      allowed_models: ['gpt-5'],
    })
    expect(getPoolKeyModelFetchDisplay(pendingAuto, 'codex')).toMatchObject({
      modelFetchLabel: '模型待发现',
      modelFetchHint: '等待建立 Codex 模型目录，可在模型权限中手动刷新',
    })

    const failedNoCatalog = poolKey({
      auto_fetch_models: true,
      codex_model_catalog_ready: false,
      last_models_fetch_error: '401 unauthorized',
      last_models_fetch_at: 1760000100,
      allowed_models: ['gpt-5'],
    })
    const failedDisplay = getPoolKeyModelFetchDisplay(failedNoCatalog, 'codex')
    expect(failedDisplay.modelFetchLabel).toBe('模型发现失败')
    expect(failedDisplay.modelFetchHint).toBe('请检查授权或代理后在模型权限中刷新模型')
    expect(failedDisplay.modelFetchTitle).not.toContain('401 unauthorized')
    expect(failedDisplay.modelFetchTitle).toContain('请检查授权或代理后在模型权限中刷新模型')
    expect(failedDisplay.modelFetchTitle).toContain(formatUnixSecondsWithSeconds(1760000100))

    const failedWithCatalog = poolKey({
      auto_fetch_models: true,
      codex_model_catalog_ready: true,
      last_models_fetch_error: 'upstream timeout',
    })
    expect(getPoolKeyModelFetchDisplay(failedWithCatalog, 'codex')).toMatchObject({
      modelFetchLabel: '模型发现失败',
      modelFetchHint: '已有缓存目录；请检查授权或代理后在模型权限中重试刷新',
    })

    const readyWithoutAutoFetch = poolKey({
      auto_fetch_models: false,
      codex_model_catalog_ready: true,
      last_models_fetch_at: 1760000200,
      allowed_models: null,
    })
    const readyDisplay = getPoolKeyModelFetchDisplay(readyWithoutAutoFetch, 'codex')
    expect(readyDisplay.modelFetchLabel).toBe('模型目录就绪')
    expect(readyDisplay.modelFetchHint).toBe('')
    expect(readyDisplay.modelFetchTitle).toContain(formatUnixSecondsWithSeconds(1760000200))
  })

  it('never lets Codex OAuth discovery readiness override hard blocks', () => {
    const bannedReady = poolKey({
      codex_model_catalog_ready: true,
      scheduling_status: 'available',
      scheduling_reason: 'available',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'account_banned', label: '账号封禁', blocked: true },
        quota: { code: 'ok', exhausted: false },
      },
    })
    expect(getSchedulingStatus(bannedReady, 0)).toBe('blocked')
    expect(getSchedulingBadgeLabel(bannedReady, 0, 'codex')).toBe('账号封禁')
    expect(getSchedulingBadgeVariant(bannedReady, 0, 'codex')).toBe('destructive')

    const disabledReady = poolKey({
      is_active: false,
      codex_model_catalog_ready: true,
      scheduling_status: 'blocked',
      scheduling_reason: 'manual_disabled',
    })
    expect(getSchedulingStatus(disabledReady, 0)).toBe('blocked')
    expect(getSchedulingBadgeLabel(disabledReady, 0, 'codex')).toBe('禁用')
  })

  it('preserves existing auto_fetch_models sync display for non-Codex-OAuth keys', () => {
    const apiKey = poolKey({
      auth_type: 'api_key',
      auto_fetch_models: false,
      codex_model_catalog_ready: false,
    })
    expect(getPoolKeyModelFetchDisplay(apiKey, 'codex').modelFetchLabel).toBe('')

    const syncedApiKey = poolKey({
      auth_type: 'api_key',
      auto_fetch_models: true,
      last_models_fetch_at: 1760000300,
    })
    expect(getPoolKeyModelFetchDisplay(syncedApiKey, 'openai').modelFetchLabel).toBe('模型同步成功')
  })
})

describe('Antigravity generic 403 inference rejection status', () => {
  const observedAt = 1760000000
  const retryAt = 1760000300

  it('shows 推理被拒, last occurrence, and retry/check time during cooldown', () => {
    const coolingKey = poolKey({
      cooldown_reason: 'forbidden_403',
      cooldown_ttl_seconds: 300,
      scheduling_status: 'degraded',
      scheduling_reason: 'cooldown',
      scheduling_label: '冷却中',
      last_inference_rejection: { observed_at: observedAt, retry_at: retryAt },
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: { code: 'ok', exhausted: false },
      },
    })

    expect(getSchedulingStatus(coolingKey, 0)).toBe('degraded')
    expect(getSchedulingBadgeLabel(coolingKey, 0, 'antigravity')).toBe('推理被拒')
    expect(getSchedulingBadgeVariant(coolingKey, 0, 'antigravity')).toBe('warning')

    const display = getAntigravityInferenceRejectionDisplay(coolingKey, 'antigravity', 0)
    expect(display).toMatchObject({
      mode: 'cooldown',
      lastOccurredText: `最近发生：${formatUnixSecondsWithSeconds(observedAt)}`,
      retryAtText: `重试检查：${formatUnixSecondsWithSeconds(retryAt)}`,
      remainingText: '5m 0s',
    })
    expect(getSchedulingTitle(coolingKey, 0, 'antigravity')).toContain('推理被拒 (5m 0s)')
    expect(getSchedulingTitle(coolingKey, 0, 'antigravity')).toContain(formatUnixSecondsWithSeconds(observedAt))
    expect(getSchedulingTitle(coolingKey, 0, 'antigravity')).toContain(formatUnixSecondsWithSeconds(retryAt))
  })

  it('shows historical reason after eligible again while keeping available status', () => {
    const recoveredKey = poolKey({
      cooldown_reason: null,
      cooldown_ttl_seconds: null,
      scheduling_status: 'available',
      scheduling_reason: 'available',
      scheduling_label: '可用',
      last_inference_rejection: { observed_at: observedAt, retry_at: retryAt },
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: { code: 'ok', exhausted: false },
      },
    })

    expect(getSchedulingStatus(recoveredKey, 0)).toBe('available')
    expect(getSchedulingBadgeLabel(recoveredKey, 0, 'antigravity')).toBe('可用')
    expect(getSchedulingBadgeVariant(recoveredKey, 0, 'antigravity')).toBe('default')

    const display = getAntigravityInferenceRejectionDisplay(recoveredKey, 'antigravity', 0)
    expect(display).toMatchObject({
      mode: 'history',
      label: '历史：推理被拒',
      lastOccurredText: `最近发生：${formatUnixSecondsWithSeconds(observedAt)}`,
      retryAtText: `重试检查：${formatUnixSecondsWithSeconds(retryAt)}`,
    })
    expect(getSchedulingTitle(recoveredKey, 0, 'antigravity')).toContain('历史原因：推理被拒')
  })

  it('respects manual disable and explicitly distinguishes known verification and ban', () => {
    const manuallyDisabled = poolKey({
      is_active: false,
      cooldown_reason: 'forbidden_403',
      cooldown_ttl_seconds: 120,
      scheduling_status: 'blocked',
      scheduling_reason: 'manual_disabled',
      last_inference_rejection: { observed_at: observedAt, retry_at: retryAt },
    })
    expect(getSchedulingStatus(manuallyDisabled, 0)).toBe('blocked')
    expect(getSchedulingBadgeLabel(manuallyDisabled, 0, 'antigravity')).toBe('禁用')
    expect(getAntigravityInferenceRejectionDisplay(manuallyDisabled, 'antigravity', 0)?.mode).toBe('history')

    const verificationKey = poolKey({
      cooldown_reason: 'forbidden_403',
      last_inference_rejection: { observed_at: observedAt, retry_at: retryAt },
      status_snapshot: {
        oauth: { code: 'valid' },
        account: {
          code: 'account_verification',
          label: '需要验证',
          reason: 'Verify account',
          blocked: true,
          validation_url: 'https://accounts.google.com/ServiceLogin',
        },
        quota: { code: 'ok', exhausted: false },
      },
    })
    expect(getSchedulingStatus(verificationKey, 0)).toBe('blocked')
    expect(getSchedulingBadgeLabel(verificationKey, 0, 'antigravity')).toBe('需要验证')
    expect(getSchedulingBadgeVariant(verificationKey, 0, 'antigravity')).toBe('destructive')
    expect(getAntigravityInferenceRejectionDisplay(verificationKey, 'antigravity', 0)).toMatchObject({
      mode: 'history',
      label: '历史：推理被拒',
    })

    const bannedKey = poolKey({
      status_snapshot: {
        oauth: { code: 'valid' },
        account: {
          code: 'account_banned',
          label: '账号封禁',
          reason: 'Account suspended',
          blocked: true,
          appeal_url: 'https://support.google.com/accounts/appeal',
        },
        quota: { code: 'ok', exhausted: false },
      },
    })
    expect(getSchedulingBadgeLabel(bannedKey, 0, 'antigravity')).toBe('账号封禁')
    expect(getSchedulingBadgeVariant(bannedKey, 0, 'antigravity')).toBe('destructive')
    expect(getAntigravityInferenceRejectionDisplay(bannedKey, 'antigravity', 0)).toBeNull()

    setI18nLocale('en-US')
    expect(getSchedulingBadgeLabel(poolKey({
      cooldown_reason: 'forbidden_403',
      last_inference_rejection: { observed_at: observedAt, retry_at: retryAt },
    }), 0, 'antigravity')).toBe('Inference rejected')
  })
})

describe('Recoverable runtime quota_exhausted hold badge and tooltip', () => {
  it('shows neutral 额度耗尽 and 可刷新额度核对 for active Codex and Antigravity runtime holds', () => {
    const codexHold = poolKey({
      scheduling_status: 'blocked',
      scheduling_reason: 'key_quota_exhausted',
      scheduling_label: '额度耗尽·需人工恢复',
      scheduling_reasons: [
        { code: 'key_quota_exhausted', label: '额度耗尽·需人工恢复', source: 'pool', blocking: true, detail: '5h window exhausted' },
      ],
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: { code: 'ok', exhausted: false, provider_type: 'codex' },
        scheduling: { code: 'quota_exhausted', blocked: true, requires_manual_recovery: true },
      },
    })
    expect(getSchedulingBadgeLabel(codexHold, 0, 'codex')).toBe('额度耗尽')
    expect(getSchedulingBadgeVariant(codexHold, 0, 'codex')).toBe('destructive')
    const codexTitle = getSchedulingTitle(codexHold, 0, 'codex')
    expect(codexTitle).toContain('额度耗尽 - 5h window exhausted')
    expect(codexTitle).toContain('可刷新额度核对')
    expect(codexTitle).not.toContain('需人工恢复')

    const agHold = poolKey({
      scheduling_status: 'blocked',
      scheduling_reason: 'key_quota_exhausted',
      scheduling_label: '额度耗尽·需人工恢复',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: { code: 'ok', exhausted: false, provider_type: 'antigravity' },
        scheduling: { code: 'quota_exhausted', blocked: true, requires_manual_recovery: true },
      },
    })
    expect(getSchedulingBadgeLabel(agHold, 0, 'antigravity')).toBe('额度耗尽')
    expect(getSchedulingBadgeVariant(agHold, 0, 'antigravity')).toBe('destructive')
    const agTitle = getSchedulingTitle(agHold, 0, 'antigravity')
    expect(agTitle).toContain('额度耗尽')
    expect(agTitle).toContain('可刷新额度核对')
    expect(agTitle).not.toContain('需人工恢复')

    setI18nLocale('en-US')
    expect(getSchedulingBadgeLabel(agHold, 0, 'antigravity')).toBe('Quota exhausted')
    expect(getSchedulingTitle(agHold, 0, 'antigravity')).toContain('Refresh quota to verify')
  })

  it('preserves manual recovery label for other providers and preserves real bans/manual disable', () => {
    const claudeHold = poolKey({
      scheduling_status: 'blocked',
      scheduling_reason: 'key_quota_exhausted',
      scheduling_label: '额度耗尽·需人工恢复',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: { code: 'ok', exhausted: false, provider_type: 'claude_code' },
        scheduling: { code: 'quota_exhausted', blocked: true, requires_manual_recovery: true },
      },
    })
    expect(getSchedulingBadgeLabel(claudeHold, 0, 'claude_code')).toBe('额度耗尽·需人工恢复')
    expect(getSchedulingTitle(claudeHold, 0, 'claude_code')).toBe('额度耗尽·需人工恢复')

    const bannedCodex = poolKey({
      scheduling_status: 'blocked',
      scheduling_reason: 'key_quota_exhausted',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'account_banned', label: '账号封禁', blocked: true },
        quota: { code: 'ok', exhausted: false, provider_type: 'codex' },
        scheduling: { code: 'quota_exhausted', blocked: true, requires_manual_recovery: true },
      },
    })
    expect(getSchedulingBadgeLabel(bannedCodex, 0, 'codex')).toBe('账号封禁')

    const disabledCodex = poolKey({
      is_active: false,
      scheduling_status: 'blocked',
      scheduling_reason: 'manual_disabled',
      status_snapshot: {
        oauth: { code: 'valid' },
        account: { code: 'ok', blocked: false },
        quota: { code: 'ok', exhausted: false, provider_type: 'codex' },
        scheduling: { code: 'quota_exhausted', blocked: true, requires_manual_recovery: true },
      },
    })
    expect(getSchedulingBadgeLabel(disabledCodex, 0, 'codex')).toBe('禁用')
  })
})
