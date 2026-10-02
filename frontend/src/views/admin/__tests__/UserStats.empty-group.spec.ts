import { afterEach, expect, it, vi } from 'vitest'
import { createApp, nextTick, type App } from 'vue'
import UserStats from '../UserStats.vue'

const api = vi.hoisted(() => ({
  getAllUsers: vi.fn().mockResolvedValue([{ id: 'user-1', username: 'Alice', groups: [] }]),
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

it('shows the synthetic ungrouped group when no groups exist and ignores the stale user response', async () => {
  vi.useFakeTimers()
  const resolveSummary: Array<(summary: { total_requests: number }) => void> = []
  // 项目 lib 为 ES2021,无 Promise.withResolvers;按调用顺序记录各次摘要请求的 resolver。
  api.getUsageStats.mockImplementation(() => new Promise(resolve => { resolveSummary.push(resolve) }))
  root = document.createElement('div')
  document.body.appendChild(root)
  app = createApp(UserStats)
  app.mount(root)
  await settle()
  expect(root.querySelectorAll('[data-loading]')).toHaveLength(2)

  root.querySelector<HTMLButtonElement>('[data-switch-group]')?.click()
  await nextTick()
  await vi.advanceTimersByTimeAsync(120)
  await settle()
  // 上游特性:没有真实分组时组作用域仍合成 __ungrouped__ 并以其为选中实体重新加载面板。
  expect(api.getUsageStats).toHaveBeenLastCalledWith(expect.objectContaining({ user_group_id: '__ungrouped__' }))
  expect(root.querySelectorAll('[data-loading]')).toHaveLength(2)

  resolveSummary[1]({ total_requests: 42 })
  await settle()
  expect(root.querySelectorAll('[data-loading]')).toHaveLength(0)
  expect(root.textContent).toContain('42')

  resolveSummary[0]({ total_requests: 777 })
  await settle()
  // 旧的用户维度响应迟到,不得覆盖未分组结果,也不得重启加载态。
  expect(root.querySelectorAll('[data-loading]')).toHaveLength(0)
  expect(root.textContent).toContain('42')
  expect(root.textContent).not.toContain('777')
})
