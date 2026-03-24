import {
  getAccountStatusDisplay,
  type ProviderKeyStatusCarrier,
} from './providerKeyStatus'

export interface OAuthAccountBlockDisplay {
  label: string | null
  reason: string | null
}

export interface OAuthRefreshFeedbackInput {
  accountStateRecheckAttempted?: boolean | null
  accountStateRecheckError?: string | null
  snapshot?: ProviderKeyStatusCarrier | null
}

function normalizeText(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const text = value.trim()
  return text || null
}

export function resolveOAuthAccountBlockDisplay(
  snapshot: ProviderKeyStatusCarrier,
): OAuthAccountBlockDisplay {
  const account = getAccountStatusDisplay(snapshot)
  if (!account.blocked || !account.label) {
    return { label: null, reason: null }
  }
  return {
    label: account.label,
    reason: account.reason,
  }
}

export function getOAuthRefreshFeedback(
  input: OAuthRefreshFeedbackInput,
): { tone: 'success' | 'warning'; message: string } {
  const blockedLabel = normalizeText(
    input.snapshot ? getAccountStatusDisplay(input.snapshot).label : null,
  )
  const recheckError = normalizeText(input.accountStateRecheckError)

  if (input.accountStateRecheckAttempted) {
    if (recheckError) {
      return {
        tone: 'warning',
        message: 'Token 刷新成功，但额度/账号状态复检失败',
      }
    }
    if (blockedLabel) {
      return {
        tone: 'warning',
        message: `Token 刷新成功，已重新检查额度/账号状态；当前状态仍是${blockedLabel}`,
      }
    }
    return {
      tone: 'success',
      message: 'Token 刷新成功，已重新检查额度/账号状态',
    }
  }

  if (blockedLabel) {
    return {
      tone: 'warning',
      message: `Token 刷新成功，但当前状态仍是${blockedLabel}`,
    }
  }

  return {
    tone: 'success',
    message: 'Token 刷新成功',
  }
}
