import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h } from 'vue'

import ClaudeEndpointOperationSupport from '../ClaudeEndpointOperationSupport.vue'

const mounted: Array<() => void> = []

function mountControl(props: { modelValue: boolean, disabled?: boolean, locked?: boolean }, onUpdate = vi.fn()) {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(defineComponent({
    setup() {
      return () => h(ClaudeEndpointOperationSupport, {
        ...props,
        'onUpdate:modelValue': onUpdate,
      })
    },
  }))
  app.mount(root)
  mounted.push(() => {
    app.unmount()
    root.remove()
  })
  return { root, onUpdate }
}

afterEach(() => {
  mounted.splice(0).forEach(unmount => unmount())
})

describe('ClaudeEndpointOperationSupport', () => {
  it('keeps messages fixed and emits Token count changes', () => {
    const { root, onUpdate } = mountControl({ modelValue: true })
    const switches = root.querySelectorAll<HTMLButtonElement>('[role="switch"]')

    expect(switches).toHaveLength(2)
    expect(switches[0].disabled).toBe(true)
    expect(switches[1].getAttribute('aria-checked')).toBe('true')

    switches[1].click()
    expect(onUpdate).toHaveBeenCalledWith(false)
  })

  it('locks Token count for unsupported provider adapters', () => {
    const { root, onUpdate } = mountControl({ modelValue: false, locked: true })
    const countTokensSwitch = root.querySelector<HTMLButtonElement>('[data-testid="claude-count-tokens-switch"]')

    expect(countTokensSwitch?.disabled).toBe(true)
    countTokensSwitch?.click()
    expect(onUpdate).not.toHaveBeenCalled()
  })
})
