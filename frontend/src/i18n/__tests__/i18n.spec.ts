import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref } from 'vue'

import { createI18n, getI18nLocale, normalizeLocale, setI18nLocale, useI18n, useLocaleOptions } from '@/i18n'
import { formatDate, formatRelativeTime } from '@/utils/format'
import { translateLegacyText } from '@/i18n/messages'
import { transformLegacyTemplateI18n } from '@/i18n/legacy-template-transform'

describe('i18n infrastructure', () => {
  beforeEach(() => {
    localStorage.clear()
    document.documentElement.lang = ''
  })

  it('installs a single app-level translator and updates document language', () => {
    const app = createApp(defineComponent({ setup: () => () => h('div') }))

    app.use(createI18n())

    expect(['zh-CN', 'en-US']).toContain(document.documentElement.lang)
    expect(app.config.globalProperties.$t('site.home.login')).toBeTruthy()
    expect(app.config.globalProperties.$legacyT('保存')).toBeTruthy()
    expect(localStorage.getItem('aether_locale')).toBe(document.documentElement.lang)
  })

  it('switches locale, persists it, and interpolates params', async () => {
    const Probe = defineComponent({
      setup() {
        const { t, setLocale } = useI18n()
        const { currentLocaleLabel } = useLocaleOptions()

        return { t, setLocale, currentLocaleLabel }
      },
      render() {
        return h('div', [
          h('span', { id: 'label' }, this.currentLocaleLabel),
          h('span', { id: 'message' }, this.t('site.privacy.currentVersion', { version: '2' })),
        ])
      },
    })

    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(Probe)
    app.use(createI18n())
    const vm = app.mount(root) as InstanceType<typeof Probe> & {
      setLocale: (locale: 'zh-CN' | 'en-US') => void
    }

    vm.setLocale('en-US')
    await nextTick()

    expect(document.documentElement.lang).toBe('en-US')
    expect(localStorage.getItem('aether_locale')).toBe('en-US')
    expect(root.textContent).toContain('English')
    expect(root.textContent).toContain('Current version: 2')

    app.unmount()
    root.remove()
  })

  it('exposes legacy text translation through the app-level helper', async () => {
    const Probe = defineComponent({
      setup() {
        const { legacyT, setLocale } = useI18n()
        const source = ref('保存')

        return { legacyT, setLocale, source }
      },
      render() {
        return h('button', { title: this.legacyT('关闭') }, this.legacyT(this.source))
      },
    })

    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(Probe)
    app.use(createI18n())
    const vm = app.mount(root) as InstanceType<typeof Probe> & {
      setLocale: (locale: 'zh-CN' | 'en-US') => void
      source: string
    }

    vm.setLocale('en-US')
    await nextTick()

    expect(root.querySelector('button')?.textContent).toBe('Save')
    expect(root.querySelector('button')?.getAttribute('title')).toBe('Close')

    vm.source = '保存中...'
    await nextTick()

    expect(root.querySelector('button')?.textContent).toBe('Saving...')

    app.unmount()
    root.remove()
  })

  it('rewrites template text and static attributes without touching code blocks', () => {
    const result = transformLegacyTemplateI18n(`
      <button title="关闭">取消</button>
      <input placeholder="全部状态">
      <code>复制 配置</code>
    `)

    expect(result.changed).toBe(true)
    expect(result.needsHelper).toBe(true)
    expect(result.code).toContain(`:title='__aetherLegacyT("关闭")'`)
    expect(result.code).toContain(`{{ __aetherLegacyT("取消") }}`)
    expect(result.code).toContain(`:placeholder='__aetherLegacyT("全部状态")'`)
    expect(result.code).toContain('<code>复制 配置</code>')
  })

  it('translates common legacy phrases without adding new message entry points', () => {
    expect(translateLegacyText('请求记录清理策略', 'en-US')).toBe('Request log cleanup policy')
    expect(translateLegacyText('点击上方"添加映射"按钮为模型创建名称映射', 'en-US')).toBe(
      'Click Add mapping to create a model name mapping.',
    )
    expect(translateLegacyText('添加映射', 'en-US')).toBe('Add mapping')
    expect(translateLegacyText('转换提供商类型', 'en-US')).toBe('Convert provider type')
    expect(translateLegacyText('转换会保留现有端点和密钥，保存前需要再次确认', 'en-US')).toBe(
      'Conversion preserves existing endpoints and keys and requires confirmation before saving.',
    )
    expect(translateLegacyText('数据已过期', 'en-US')).toBe('Data is stale')
    expect(translateLegacyText('刷新失败', 'en-US')).toBe('Refresh failed')
    expect(translateLegacyText('更新时间未知', 'en-US')).toBe('Update time unknown')
    expect(
      translateLegacyText(
        '确认将此提供商转换为 OpenRouter？转换会影响 3 个端点和 7 个密钥。取消后将保持当前自定义类型不变。',
        'en-US',
      ),
    ).toBe(
      'Convert this provider to OpenRouter? This conversion affects 3 endpoints and 7 keys. Cancel keeps the current Custom type unchanged.',
    )
    expect(translateLegacyText('最大转移次数', 'en-US')).toBe('Max transfers')
    expect(translateLegacyText('最大转移超时', 'en-US')).toBe('Max transfer timeout')
    expect(translateLegacyText('0 (不限制)', 'en-US')).toBe('0 (unlimited)')
    expect(translateLegacyText('  发布于 2026-01-01  ', 'en-US')).toBe('  Published at 2026-01-01  ')
    expect(translateLegacyText('git clone https://github.com/ZipperCode/Aether.git', 'en-US')).toBe('git clone https://github.com/ZipperCode/Aether.git')
  })

  it('normalizes saved language aliases without accepting unrelated language names', () => {
    expect(normalizeLocale('en')).toBe('en-US')
    expect(normalizeLocale('en_GB')).toBe('en-US')
    expect(normalizeLocale(' ZH-cn ')).toBe('zh-CN')
    expect(normalizeLocale('english')).toBeUndefined()
    expect(normalizeLocale('fr-FR')).toBeUndefined()
  })

  it('continues switching language when browser storage is unavailable', () => {
    const write = vi.spyOn(localStorage, 'setItem').mockImplementation(() => {
      throw new DOMException('Storage blocked', 'SecurityError')
    })
    try {
      expect(() => setI18nLocale('en-US')).not.toThrow()
      expect(getI18nLocale()).toBe('en-US')
      expect(document.documentElement.lang).toBe('en-US')
    } finally {
      write.mockRestore()
    }
  })

  it('updates date and relative-time formatting with the selected language', () => {
    const date = '2026-09-07T12:30:00'
    setI18nLocale('zh-CN')
    const chineseDate = formatDate(date)
    expect(formatRelativeTime(-1, 'day')).toBe('昨天')
    setI18nLocale('en-US')
    expect(formatRelativeTime(-1, 'day')).toBe('yesterday')
    expect(formatRelativeTime(-1, 'minute')).toBe('1 minute ago')
    expect(formatRelativeTime(-2, 'minute')).toBe('2 minutes ago')
    expect(formatDate(date)).not.toBe(chineseDate)
    setI18nLocale('zh-CN')
    expect(formatDate(date)).toBe(chineseDate)
  })
})
