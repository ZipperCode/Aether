import { describe, expect, it } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref } from 'vue'
import type { AccountStatusSnapshot } from '@/api/endpoints/types/statusSnapshot'
import ProviderAccountRecoveryLink from '../ProviderAccountRecoveryLink.vue'
import { createI18n } from '@/i18n'

function mount(initial: AccountStatusSnapshot) {
  const account = ref(initial)
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(defineComponent({
    setup: () => () => h(ProviderAccountRecoveryLink, { account: account.value }),
  }))
  app.use(createI18n())
  app.mount(root)
  return { root, account, unmount: () => { app.unmount(); root.remove() } }
}

describe('ProviderAccountRecoveryLink', () => {
  it.each([
    ['account_verification', 'validation_url', 'https://accounts.google.com/signin/continue?token=opaque', '验证账号'],
    ['account_suspended', 'appeal_url', 'https://support.google.com/antigravity/contact/appeal?case=opaque', '提交申诉'],
  ])('renders an actionable recovery link for %s', (code, field, url, label) => {
    const { root, unmount } = mount({ code, blocked: true, [field]: url })
    try {
      const link = root.querySelector('a')
      expect(link?.textContent).toContain(label)
      expect(link?.getAttribute('href')).toBe(url)
      expect(link?.getAttribute('target')).toBe('_blank')
      expect(link?.getAttribute('rel')).toBe('noopener noreferrer')
      expect(link?.getAttribute('referrerpolicy')).toBe('no-referrer')
    } finally { unmount() }
  })

  it.each([
    'javascript:alert(1)',
    'http://accounts.google.com/verify',
    'https://accounts.google.com.evil.test/verify',
    'https://evil.test/verify',
    'https://user:pass@accounts.google.com/verify',
    'https://accounts.google.com:8443/verify',
    'https://accounts.google.com/verify\n',
    'not a URL',
  ])('rejects unsafe or unrelated verification URL %s', (validation_url) => {
    const { root, unmount } = mount({ code: 'account_verification', blocked: true, validation_url })
    try { expect(root.querySelector('a')).toBeNull() } finally { unmount() }
  })

  it('hides stale links when the block is cleared or its classification changes', async () => {
    const { root, account, unmount } = mount({
      code: 'account_verification', blocked: true, validation_url: 'https://accounts.google.com/verify',
    })
    try {
      expect(root.querySelector('a')).not.toBeNull()
      account.value.blocked = false
      await nextTick()
      expect(root.querySelector('a')).toBeNull()
      account.value = { ...account.value, blocked: true, code: 'oauth_token_invalid' }
      await nextTick()
      expect(root.querySelector('a')).toBeNull()
      account.value = { ...account.value, code: 'account_suspended' }
      await nextTick()
      expect(root.querySelector('a')).toBeNull()
    } finally { unmount() }
  })
})
