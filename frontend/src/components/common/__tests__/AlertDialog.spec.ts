import { afterEach, describe, expect, it } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref } from 'vue'

import AlertDialog from '@/components/common/AlertDialog.vue'
import { createI18n } from '@/i18n'

afterEach(() => {
  document.body.innerHTML = ''
})

describe('AlertDialog accessibility', () => {
  it('opens as a labelled alertdialog, focuses cancel, traps focus, and restores focus', async () => {
    const trigger = document.createElement('button')
    document.body.appendChild(trigger)
    trigger.focus()
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(AlertDialog, {
      modelValue: true,
      title: '转换提供商类型',
      description: '确认转换？',
    })
    app.use(createI18n())
    app.mount(root)
    await nextTick()
    await nextTick()

    const dialog = document.querySelector('[role="alertdialog"]') as HTMLElement
    const buttons = [...dialog.querySelectorAll('button')] as HTMLButtonElement[]
    expect(dialog.getAttribute('aria-modal')).toBe('true')
    expect(dialog.getAttribute('aria-label')).toBe('转换提供商类型')
    expect(document.activeElement).toBe(buttons[0])

    buttons.at(-1)?.focus()
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }))
    expect(document.activeElement).toBe(buttons[0])

    app.unmount()
    await nextTick()
    expect(document.activeElement).toBe(trigger)
  })

  it('supports Shift+Tab and restores focus across repeated close and reopen cycles', async () => {
    const Harness = defineComponent({
      setup() {
        const open = ref(false)
        return () => h('div', [
          h('button', { id: 'repeat-trigger', onClick: () => { open.value = true } }, 'Open confirmation'),
          h(AlertDialog, {
            modelValue: open.value,
            title: '重复确认',
            description: '再次确认？',
            'onUpdate:modelValue': (value: boolean) => { open.value = value },
          }),
        ])
      },
    })
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(Harness)
    app.use(createI18n())
    app.mount(root)
    const trigger = root.querySelector('#repeat-trigger') as HTMLButtonElement

    for (let cycle = 0; cycle < 2; cycle += 1) {
      trigger.focus()
      trigger.click()
      await nextTick()
      await nextTick()
      const dialog = document.querySelector('[role="alertdialog"]') as HTMLElement
      const buttons = [...dialog.querySelectorAll('button')] as HTMLButtonElement[]
      expect(document.activeElement).toBe(buttons[0])
      dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true }))
      expect(document.activeElement).toBe(buttons.at(-1))
      buttons[0].click()
      await nextTick()
      await nextTick()
      expect(document.activeElement).toBe(trigger)
    }

    app.unmount()
    root.remove()
  })
})
