import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue'
import type { PoolAdvancedConfig } from '@/api/endpoints/types/provider'
import PoolAdvancedDialog from '../PoolAdvancedDialog.vue'

const api = vi.hoisted(() => ({ getProvider: vi.fn(), updateProvider: vi.fn() }))
vi.mock('@/api/endpoints', () => api)
vi.mock('@/composables/useToast', () => ({ useToast: () => ({ success: vi.fn(), error: vi.fn() }) }))

const mounted: Array<{ app: App, root: HTMLElement }> = []

async function settle() {
  for (let i = 0; i < 4; i++) {
    await Promise.resolve()
    await nextTick()
  }
}

function mountDialog(currentConfig: PoolAdvancedConfig | null = null) {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const open = ref(false)
  const configRef = ref<PoolAdvancedConfig | null>(currentConfig)
  const app = createApp(defineComponent({
    setup() {
      void nextTick(() => { open.value = true })
      return () => h(PoolAdvancedDialog, {
        modelValue: open.value,
        providerId: 'provider-1',
        providerType: 'codex',
        currentConfig: configRef.value,
        'onUpdate:modelValue': (v: boolean) => { open.value = v },
      })
    },
  }))
  app.mount(root)
  mounted.push({ app, root })
  return { open, configRef }
}

function button(text: string) {
  const result = [...document.body.querySelectorAll<HTMLButtonElement>('button')]
    .find(el => el.textContent?.trim() === text)
  expect(result).toBeDefined()
  return result!
}

beforeEach(() => {
  api.getProvider.mockReset().mockResolvedValue({
    id: 'provider-1',
    pool_advanced: { scheduling_presets: ['lru'] },
  })
  api.updateProvider.mockReset().mockResolvedValue({ id: 'provider-1' })
})

afterEach(() => {
  for (const { app, root } of mounted.splice(0)) {
    app.unmount()
    root.remove()
  }
  document.body.innerHTML = ''
})

describe('PoolAdvancedDialog auto_publish_discovered_models', () => {
  it('defaults to false, renders a single control, and saves explicit false when untouched', async () => {
    mountDialog(null)
    await settle()

    const toggles = document.body.querySelectorAll('[data-testid="pool-auto-publish-discovered-models"]')
    expect(toggles).toHaveLength(1)
    expect(toggles[0]!.getAttribute('aria-checked')).toBe('false')

    button('保存').click()
    await settle()

    expect(api.updateProvider).toHaveBeenCalledOnce()
    const saved = api.updateProvider.mock.calls[0]![1].pool_advanced
    expect(saved.auto_publish_discovered_models).toBe(false)
    expect(saved.scheduling_presets).toEqual(['lru'])
  })

  it('opts in to true, rehydrates true, and saves/rehydrates explicit false when turned back off', async () => {
    const { open, configRef } = mountDialog({ auto_publish_discovered_models: false })
    await settle()

    const toggle = document.getElementById('pool-auto-publish-discovered-models')!
    expect(toggle.getAttribute('aria-checked')).toBe('false')

    toggle.click()
    await settle()
    expect(toggle.getAttribute('aria-checked')).toBe('true')

    button('保存').click()
    await settle()
    expect(api.updateProvider.mock.calls[0]![1].pool_advanced.auto_publish_discovered_models).toBe(true)

    configRef.value = { auto_publish_discovered_models: true }
    open.value = true
    await settle()
    const reopenedToggle = document.getElementById('pool-auto-publish-discovered-models')!
    expect(reopenedToggle.getAttribute('aria-checked')).toBe('true')

    reopenedToggle.click()
    await settle()
    expect(reopenedToggle.getAttribute('aria-checked')).toBe('false')

    button('保存').click()
    await settle()
    expect(api.updateProvider.mock.calls[1]![1].pool_advanced.auto_publish_discovered_models).toBe(false)

    configRef.value = { auto_publish_discovered_models: false }
    open.value = true
    await settle()
    expect(document.getElementById('pool-auto-publish-discovered-models')!.getAttribute('aria-checked')).toBe('false')
  })
})
