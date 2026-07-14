import { afterEach, describe, expect, it } from 'vitest'
import { createApp, h, type App } from 'vue'

import ServiceTierFacts from '../ServiceTierFacts.vue'

const mountedApps: Array<{ app: App, root: HTMLElement }> = []

afterEach(() => {
  for (const { app, root } of mountedApps.splice(0)) {
    app.unmount()
    root.remove()
  }
})

describe('ServiceTierFacts', () => {
  it('renders all three facts and marks a missing actual tier explicitly', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp({
      render: () => h(ServiceTierFacts, {
        requested: 'priority',
        actual: null,
        billing: 'flex',
      }),
    })
    app.mount(root)
    mountedApps.push({ app, root })

    expect(root.querySelector('[data-testid="service-tier-facts"]')).not.toBeNull()
    expect([...root.querySelectorAll('dt')].map(node => node.textContent?.trim())).toEqual([
      '请求层级',
      '实际层级',
      '计费层级',
    ])
    expect([...root.querySelectorAll('dd')].map(node => node.textContent?.trim())).toEqual([
      'priority',
      '-',
      'flex',
    ])
  })
})
