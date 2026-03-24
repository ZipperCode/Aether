import { describe, expect, it } from 'vitest'

import { cleanAccountBlockReason, isRefreshFailedReason } from '@/utils/accountBlock'

describe('accountBlock helpers', () => {
  it('detects refresh failure markers even when account block is also present', () => {
    expect(
      isRefreshFailedReason(
        '[ACCOUNT_BLOCK] 工作区已停用 (deactivated_workspace)\n[REFRESH_FAILED] Token 续期失败',
      ),
    ).toBe(true)
  })

  it('keeps account block reason clean when refresh failure is appended', () => {
    expect(
      cleanAccountBlockReason(
        '[ACCOUNT_BLOCK] 工作区已停用 (deactivated_workspace)\n[REFRESH_FAILED] Token 续期失败',
      ),
    ).toBe('工作区已停用 (deactivated_workspace)')
  })
})
