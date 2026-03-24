import { describe, expect, it } from 'vitest'

import { getOAuthExpiresCountdown } from '@/composables/useCountdownTimer'

describe('getOAuthExpiresCountdown', () => {
  it('treats invalid reason as invalid even without invalid timestamp', () => {
    expect(
      getOAuthExpiresCountdown(
        Math.floor(Date.now() / 1000) + 3600,
        0,
        null,
        '[REFRESH_FAILED] Token 续期失败 (400): refresh_token_reused',
      ),
    ).toMatchObject({
      text: '已失效',
      isInvalid: true,
      invalidReason: '[REFRESH_FAILED] Token 续期失败 (400): refresh_token_reused',
    })
  })
})
