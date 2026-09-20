import { afterEach, expect, it, vi } from 'vitest'
import { createApp, nextTick, type App } from 'vue'
import UserStats from '../UserStats.vue'

const api = vi.hoisted(() => ({
  getAllUsers: vi.fn().mockResolvedValue([{ id: 'user-1', username: 'Alice' }]),
  listUserGroups: vi.fn().mockResolvedValue({ items: [] }),
  listUserGroupMembers: vi.fn(),
  getLeaderboardUsers: vi.fn().mockResolvedValue({ items: [], total: 0 }),
  getLeaderboardUserGroups: vi.fn().mockResolvedValue({ items: [], total: 0 }),
  getTimeSeries: vi.fn().mockResolvedValue([]),
  getUsageStats: vi.fn(),
}))

vi.mock('@/api/users', () => ({ usersApi: api }))
vi.mock('@/api/admin', () => ({ adminApi: api }))
vi.mock('@/api/usage', () => ({ usageApi: api }))
vi.mock('@/features/usage/composables', () => ({
  getDateRangeFromPeriod: () => ({ preset: 'last7days' }),
}))
vi.mock('@/components/ui', async () => {
  const { defineComponent, h } = await import('vue')
  const passthrough = defineComponent({
    inheritAttrs: false,
    setup: (_props, { slots }) => () => slots.default?.(),
  })
  return {
    Button: passthrough,
    Card: passthrough,
    SelectContent: passthrough,
    SelectItem: passthrough,
    SelectTrigger: passthrough,
    SelectValue: passthrough,
    Select: defineComponent({
      props: { modelValue: { type: String, default: '' } },
      emits: ['update:modelValue'],
      // 只替换控件交互，保留真实页面的作用域监听及异步加载链。
      setup: (props, { emit }) => () => props.modelValue === 'user'
        ? h('button', {
          'data-switch-group': '',
          onClick: () => emit('update:modelValue', 'user_group'),
        }, '切换用户组')
        : null,
    }),
  }
})
vi.mock('@/components/common', async () => {
  const { defineComponent, h } = await import('vue')
  return {
    LoadingState: defineComponent({
      setup: () => () => h('span', { 'data-loading': '' }),
    }),
    TimeRangePicker: defineComponent({ setup: () => () => null }),
  }
})
vi.mock('@/components/stats', async () => {
  const { defineComponent } = await import('vue')
  return { LeaderboardTable: defineComponent({ setup: () => () => null }) }
})
vi.mock('@/components/charts/LineChart.vue', async () => {
  const { defineComponent } = await import('vue')
  return { default: defineComponent({ setup: () => () => null }) }
})

let app: App | undefined
let root: HTMLDivElement | undefined

afterEach(() => {
  app?.unmount()
  root?.remove()
  vi.useRealTimers()
})

async function settle() {
  for (let index = 0; index < 5; index += 1) {
    await Promise.resolve()
    await nextTick()
  }
}

it('clears pending panels when switching to an empty group scope and ignores the old response', async () => {
  vi.useFakeTimers()
  let resolveSummary: ((summary: { total_requests: number }) => void) | undefined
  api.getUsageStats.mockReturnValue(new Promise(resolve => { resolveSummary = resolve }))
  root = document.createElement('div')
  document.body.appendChild(root)
  app = createApp(UserStats)
  app.mount(root)
  await settle()
  expect(api.getUsageStats).toHaveBeenCalledOnce()
  expect(root.querySelectorAll('[data-loading]')).toHaveLength(2)

  root.querySelector<HTMLButtonElement>('[data-switch-group]')?.click()
  await nextTick()
  await vi.advanceTimersByTimeAsync(120)
  await settle()
  expect(api.getLeaderboardUserGroups).toHaveBeenCalledOnce()
  expect(api.getUsageStats).toHaveBeenCalledOnce()
  expect(root.querySelectorAll('[data-loading]')).toHaveLength(0)

  resolveSummary?.({ total_requests: 777 })
  await settle()
  expect(root.querySelectorAll('[data-loading]')).toHaveLength(0)
  expect(root.textContent).not.toContain('777')
})
