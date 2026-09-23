import { afterEach, describe, expect, it } from 'vitest'
import { createApp, nextTick, type App } from 'vue'
import { createPinia } from 'pinia'
import { createI18n } from '@/i18n'
import ProviderFormDialog from '../ProviderFormDialog.vue'

let app: App | undefined
let root: HTMLDivElement | undefined

afterEach(() => {
  app?.unmount()
  root?.remove()
  app = undefined
  root = undefined
})

describe('提供商编辑表单', () => {
  it('立即打开推荐类型时保留名称，并允许取消而不保存', async () => {
    root = document.createElement('div')
    document.body.appendChild(root)
    let closed = false
    app = createApp(ProviderFormDialog, {
      modelValue: true,
      suggestedType: 'moonshot',
      provider: {
        id: 'form-regression', name: '测试站点', provider_type: 'custom',
        description: '原有描述', is_active: true, provider_priority: 10,
      },
      'onUpdate:modelValue': (value: boolean) => { closed = !value },
    })
    app.use(createPinia()).use(createI18n()).mount(root)
    await nextTick()
    const dialog = document.querySelector('[role="dialog"]')
    expect(dialog?.querySelector('input')?.value).toBe('测试站点')
    expect(dialog?.querySelector('[role="combobox"]')?.textContent).toContain('Moonshot')
    expect(dialog?.querySelector('[role="switch"][aria-label="Responses WebSocket 模式"]')).not.toBeNull()
    const cancel = Array.from(dialog?.querySelectorAll('button') ?? [])
      .find(button => button.textContent?.trim() === '取消')
    cancel?.click()
    await nextTick()
    expect(closed).toBe(true)
  })
})
