import { describe, expect, it } from 'vitest'

import { getOAuthIcon, OAUTH_ICONS } from '../oauth-icons'

describe('Nous OAuth icon', () => {
  it('uses a built-in Nous icon instead of the fallback icon', () => {
    expect(OAUTH_ICONS.nous).toBeTruthy()
    expect(getOAuthIcon('Nous')).toBe(OAUTH_ICONS.nous)
    expect(getOAuthIcon('nous')).not.toBe(OAUTH_ICONS.github)
  })
})
