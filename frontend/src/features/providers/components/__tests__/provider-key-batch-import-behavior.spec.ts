import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia } from 'pinia'
import { createApp, h, nextTick, ref, type App } from 'vue'
import ProviderKeyBatchImportDialog from '@/features/providers/components/ProviderKeyBatchImportDialog.vue'
import type { PoolBatchImportRequest } from '@/api/endpoints/pool'
import { parsePatternText } from '@/utils/form'

const poolMocks = vi.hoisted(() => ({
  batchImportPoolKeys: vi.fn(),
}))

const toastMocks = vi.hoisted(() => ({
  success: vi.fn(),
  warning: vi.fn(),
  error: vi.fn(),
}))

vi.mock('@/api/endpoints/pool', async () => {
  const actual = await vi.importActual<typeof import('@/api/endpoints/pool')>('@/api/endpoints/pool')
  return {
    ...actual,
    batchImportPoolKeys: poolMocks.batchImportPoolKeys,
  }
})

vi.mock('@/composables/useToast', () => ({
  useToast: () => toastMocks,
}))

vi.mock('@/stores/proxy-nodes', () => ({
  useProxyNodesStore: () => ({
    loading: false,
    nodes: [],
    onlineNodes: [],
    ensureLoaded: () => Promise.resolve(),
  }),
}))

interface MountedHarness {
  app: App
  root: HTMLElement
  open: { value: boolean }
}

const mounted: MountedHarness[] = []

function mountDialog(): MountedHarness {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const open = ref(true)
  const closeSpy = vi.fn()
  const savedSpy = vi.fn()
  const app = createApp({
    setup() {
      return () => h(ProviderKeyBatchImportDialog, {
        open: open.value,
        providerId: 'provider-1',
        providerName: '测试提供商',
        availableApiFormats: ['openai:chat'],
        onClose: () => {
          closeSpy()
          open.value = false
        },
        onSaved: savedSpy,
      })
    },
  })
  app.use(createPinia())
  app.mount(root)
  const harness = { app, root, open }
  mounted.push(harness)
  return harness
}

function dialogLayer(): HTMLElement | null {
  // Dialog teleport 到 body，标题用于区分批量导入弹窗与其他浮层
  const layer = [...document.querySelectorAll<HTMLElement>('body > div')]
    .find(el => el.querySelector('[data-dialog-initial-focus], textarea, button') !== null
      && el.textContent?.includes('批量导入 Key'))
  return layer ?? null
}

function findTextarea(): HTMLTextAreaElement | null {
  return dialogLayer()?.querySelector('textarea') ?? null
}

async function settle(): Promise<void> {
  await nextTick()
  await nextTick()
  // 项目 lib 为 ES2021，无 Promise.withResolvers
  await new Promise<void>(resolve => setTimeout(resolve, 0))
  await nextTick()
}

async function clickPrimary(expectedText: string): Promise<void> {
  const button = [...(dialogLayer()?.querySelectorAll('button') ?? [])]
    .find(b => b.textContent?.trim() === expectedText)
  expect(button, `primary action "${expectedText}" should exist`).toBeTruthy()
  button!.click()
  await settle()
}

async function fillInputText(value: string): Promise<void> {
  const textarea = findTextarea()
  expect(textarea).not.toBeNull()
  textarea!.value = value
  textarea!.dispatchEvent(new Event('input', { bubbles: true }))
  await settle()
}

function importSwitch(): HTMLElement | null {
  const sw = [...(dialogLayer()?.querySelectorAll<HTMLElement>('[role="switch"]') ?? [])]
    .find(el => el.closest('div')?.textContent?.includes('自动获取上游可用模型'))
  return sw ?? null
}

async function toggleImportSwitch(): Promise<void> {
  const sw = importSwitch()
  expect(sw, '自动获取模型开关应存在').not.toBeNull()
  sw!.click()
  await settle()
}

beforeEach(() => {
  vi.clearAllMocks()
  poolMocks.batchImportPoolKeys.mockReset()
})

afterEach(() => {
  for (const { app, root } of mounted.splice(0)) {
    app.unmount()
    root.remove()
  }
  document.body.innerHTML = ''
})

async function walkToStepThree(): Promise<void> {
  await fillInputText('alpha----sk-alpha\nbeta----sk-beta')
  await clickPrimary('下一步：统一配置')
  await clickPrimary('下一步：逐项确认')
}

describe('parsePatternText', () => {
  it('解析逗号分隔规则为去空去重的数组', () => {
    expect(parsePatternText(' gpt-*, claude-* ,gpt-*, ,')).toEqual(['gpt-*', 'claude-*'])
    expect(parsePatternText('  ')).toEqual([])
  })
})

describe('ProviderKeyBatchImportDialog 批量导入', () => {
  it('全部成功后关闭弹窗并通知刷新', async () => {
    poolMocks.batchImportPoolKeys.mockResolvedValue({
      imported: 2,
      skipped: 0,
      errors: [],
      model_sync: null,
    })

    mountDialog()
    await settle()
    expect(dialogLayer()).not.toBeNull()

    await walkToStepThree()
    await clickPrimary('导入 2 个 Key')

    expect(poolMocks.batchImportPoolKeys).toHaveBeenCalledTimes(1)
    await settle()

    // 弹窗随父级 open=false 卸载，toast 提示成功
    expect(dialogLayer()).toBeNull()
    expect(toastMocks.success).toHaveBeenCalledWith('已导入 2 个 Key')
    expect(toastMocks.warning).not.toHaveBeenCalled()
  })

  it('部分失败时保留失败项与原因，重试仅重新提交失败项', async () => {
    poolMocks.batchImportPoolKeys
      .mockResolvedValueOnce({
        imported: 1,
        skipped: 0,
        errors: [{ index: 1, reason: '该名称已存在于当前 Provider 或本次导入中' }],
        model_sync: null,
      })
      .mockResolvedValueOnce({
        imported: 1,
        skipped: 0,
        errors: [],
        model_sync: null,
      })

    mountDialog()
    await settle()
    await walkToStepThree()
    await clickPrimary('导入 2 个 Key')

    // 部分失败：弹窗保持打开，失败行展示原因
    expect(dialogLayer()).not.toBeNull()
    expect(dialogLayer()!.textContent).toContain('导入失败：该名称已存在于当前 Provider 或本次导入中')
    expect(toastMocks.warning).toHaveBeenCalledWith('已导入 1 个，1 个失败，可修正后重试')

    // 成功项（alpha）已从重试列表移除，只剩失败项（beta）
    const primary = [...(dialogLayer()?.querySelectorAll('button') ?? [])]
      .find(b => b.textContent?.includes('重试导入'))
    expect(primary?.textContent).toContain('1 个 Key')
    expect(dialogLayer()!.textContent).not.toContain('alpha')

    await clickPrimary('重试导入 1 个 Key')
    expect(poolMocks.batchImportPoolKeys).toHaveBeenCalledTimes(2)
    const retryBody = poolMocks.batchImportPoolKeys.mock.calls[1][1] as PoolBatchImportRequest
    expect(retryBody.keys).toHaveLength(1)
    expect(retryBody.keys[0].name).toBe('beta')

    // 重试全部成功后关闭
    await settle()
    expect(dialogLayer()).toBeNull()
  })

  it('开启自动获取上游模型后，设置随导入 payload 持久化', async () => {
    poolMocks.batchImportPoolKeys.mockResolvedValue({
      imported: 1,
      skipped: 0,
      errors: [],
      model_sync: { requested: 1, attempted: 1, succeeded: 1, failed: 0, skipped: 0 },
    })

    mountDialog()
    await settle()
    await fillInputText('alpha----sk-alpha')
    await clickPrimary('下一步：统一配置')

    await toggleImportSwitch()

    // 开关通过 label[for] 与文本关联（可访问性）
    const switchEl = importSwitch()
    expect(switchEl).not.toBeNull()
    const label = dialogLayer()!.querySelector(`label[for="${switchEl!.id}"]`)
    expect(label?.textContent).toContain('自动获取上游可用模型')

    const includeInput = [...(dialogLayer()?.querySelectorAll('input') ?? [])]
      .find(input => input.placeholder.includes('gpt-*'))
    expect(includeInput).toBeDefined()
    includeInput!.value = 'gpt-*, gpt-4*'
    includeInput!.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()

    await clickPrimary('下一步：逐项确认')
    await clickPrimary('导入 1 个 Key')

    const body = poolMocks.batchImportPoolKeys.mock.calls[0][1] as PoolBatchImportRequest
    expect(body.settings?.auto_fetch_models).toBe(true)
    expect(body.settings?.model_include_patterns).toEqual(['gpt-*', 'gpt-4*'])
    expect(body.settings?.model_exclude_patterns).toEqual([])
  })
})