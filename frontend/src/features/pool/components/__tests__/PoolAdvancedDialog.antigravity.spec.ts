import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue'
import type { PoolAdvancedConfig } from '@/api/endpoints/types/provider'
import PoolAdvancedDialog from '../PoolAdvancedDialog.vue'

const api = vi.hoisted(() => ({ getProvider: vi.fn(), updateProvider: vi.fn() }))
vi.mock('@/api/endpoints', () => api)
vi.mock('@/composables/useToast', () => ({ useToast: () => ({ success: vi.fn(), error: vi.fn() }) }))
const mounted: Array<{ app: App, root: HTMLElement }> = []
async function settle() {
  for (let i = 0; i < 4; i++) { await Promise.resolve(); await nextTick() }
}
function mountDialog(providerType = 'antigravity', currentConfig: PoolAdvancedConfig | null = null) {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const open = ref(false)
  const app = createApp(defineComponent({
    setup() {
      void nextTick(() => { open.value = true })
      return () => h(PoolAdvancedDialog, {
        modelValue: open.value, providerId: 'provider-1', providerType, currentConfig,
        'onUpdate:modelValue': (v: boolean) => { open.value = v },
      })
    },
  }))
  app.mount(root)
  mounted.push({ app, root })
}
function button(text: string) {
  const result = [...document.body.querySelectorAll<HTMLButtonElement>('button')]
    .find(el => el.textContent?.trim() === text)
  expect(result).toBeDefined()
  return result!
}
beforeEach(() => {
  api.getProvider.mockReset().mockResolvedValue({ id: 'provider-1', pool_advanced: { scheduling_presets: ['lru'], antigravity: { future_option: true } } })
  api.updateProvider.mockReset().mockResolvedValue({ id: 'provider-1' })
})
afterEach(() => {
  for (const { app, root } of mounted.splice(0)) { app.unmount(); root.remove() }
  document.body.innerHTML = ''
})
describe('Antigravity advanced settings', () => {
  it('renders opt-in defaults, edits exact rules and preserves latest unrelated configuration', async () => {
    mountDialog()
    await settle()
    for (const id of ['antigravity-tos-pause', 'antigravity-short-retry', 'antigravity-neutralization']) {
      const toggle = document.getElementById(id)!
      expect(toggle.getAttribute('aria-checked')).toBe('false')
      toggle.click()
    }
    await settle()
    expect((document.getElementById('antigravity-pause-seconds') as HTMLInputElement).value).toBe('900')
    expect((document.getElementById('antigravity-retry-ms') as HTMLInputElement).value).toBe('2000')
    button('添加规则').click()
    await settle()
    const match = document.getElementById('antigravity-rule-match-0') as HTMLTextAreaElement
    match.value = 'Exact system text'
    match.dispatchEvent(new Event('input', { bubbles: true }))
    button('保存').click()
    await settle()
    expect(api.updateProvider).toHaveBeenCalledOnce()
    const saved = api.updateProvider.mock.calls[0]![1].pool_advanced
    expect(saved.scheduling_presets).toEqual(['lru'])
    expect(saved.antigravity).toEqual({
      future_option: true, tos_pause_enabled: true, tos_pause_seconds: 900,
      short_retry_enabled: true, short_retry_max_delay_ms: 2000,
      identity_neutralization_enabled: true,
      identity_neutralization_mode: 'exact',
      identity_neutralization_rules: [{ match: 'Exact system text', replacement: '' }],
    })
  })
  it('does not expose or rewrite Antigravity settings on other providers', async () => {
    mountDialog('codex')
    await settle()
    expect(document.getElementById('antigravity-tos-pause')).toBeNull()
    button('保存').click()
    await settle()
    expect(api.updateProvider.mock.calls[0]![1].pool_advanced.antigravity).toEqual({ future_option: true })
  })
  it('loads saved rules without mutating the input configuration', async () => {
    const config = { antigravity: { identity_neutralization_enabled: true, identity_neutralization_rules: [{ match: 'x', replacement: 'y' }] } }
    mountDialog('antigravity', config)
    await settle()
    expect((document.getElementById('antigravity-rule-match-0') as HTMLTextAreaElement).value).toBe('x')
    button('移除规则').click()
    await settle()
    expect(config.antigravity.identity_neutralization_rules).toEqual([{ match: 'x', replacement: 'y' }])
    expect(document.getElementById('antigravity-rule-match-0')).toBeNull()
  })
  it('shows and saves opening identity mode while retaining inactive exact rules', async () => {
    const config: PoolAdvancedConfig = { antigravity: {
      identity_neutralization_enabled: true,
      identity_neutralization_rules: [{ match: 'keep exact rule', replacement: 'replacement' }],
    } }
    mountDialog('antigravity', config)
    await settle()
    const mode = document.getElementById('antigravity-neutralization-mode') as HTMLSelectElement
    expect(mode.value).toBe('exact')
    mode.value = 'opening_identity'
    mode.dispatchEvent(new Event('change', { bubbles: true }))
    await settle()
    expect(document.getElementById('antigravity-rule-match-0')).toBeNull()
    expect(document.body.textContent).toContain('不保证上游接受或避免账号限制')
    button('保存').click()
    await settle()
    const saved = api.updateProvider.mock.calls[0]![1].pool_advanced.antigravity
    expect(saved.identity_neutralization_mode).toBe('opening_identity')
    expect(saved.identity_neutralization_enabled).toBe(true)
    expect(saved.identity_neutralization_rules).toEqual(config.antigravity!.identity_neutralization_rules)
    expect(config.antigravity!.identity_neutralization_mode).toBeUndefined()
  })
  it('loads saved opening identity mode', async () => {
    mountDialog('antigravity', { antigravity: {
      identity_neutralization_enabled: true,
      identity_neutralization_mode: 'opening_identity',
    } })
    await settle()
    expect((document.getElementById('antigravity-neutralization-mode') as HTMLSelectElement).value).toBe('opening_identity')
    expect(document.getElementById('antigravity-rule-match-0')).toBeNull()
  })
  it('edits and saves literal rules without changing inactive configuration', async () => {
    const config: PoolAdvancedConfig = { antigravity: {
      identity_neutralization_enabled: true,
      identity_neutralization_mode: 'opening_identity',
      identity_neutralization_rules: [{ match: 'old', replacement: 'new' }],
    } }
    mountDialog('antigravity', config)
    await settle()
    const mode = document.getElementById('antigravity-neutralization-mode') as HTMLSelectElement
    mode.value = 'literal'
    mode.dispatchEvent(new Event('change', { bubbles: true }))
    await settle()
    expect(document.body.textContent).toContain('字面匹配片段（区分大小写）')
    const match = document.getElementById('antigravity-rule-match-0') as HTMLTextAreaElement
    match.value = 'Read xd://<tool> for full docs + JSON schema before first use.'
    match.dispatchEvent(new Event('input', { bubbles: true }))
    const replacement = document.getElementById('antigravity-rule-replacement-0') as HTMLTextAreaElement
    replacement.value = '工具首次使用前，先读取对应的 xd:// 文档，确认使用说明和参数结构。'
    replacement.dispatchEvent(new Event('input', { bubbles: true }))
    button('保存').click()
    await settle()
    expect(api.updateProvider.mock.calls[0]![1].pool_advanced.antigravity).toMatchObject({
      identity_neutralization_enabled: true,
      identity_neutralization_mode: 'literal',
      identity_neutralization_rules: [{ match: match.value, replacement: replacement.value }],
      future_option: true,
    })
    expect(config.antigravity!.identity_neutralization_mode).toBe('opening_identity')
    expect(config.antigravity!.identity_neutralization_rules![0]!.match).toBe('old')
  })
})
