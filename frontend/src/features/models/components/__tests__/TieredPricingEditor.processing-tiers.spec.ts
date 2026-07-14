import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, type App, type ComponentPublicInstance } from 'vue'

import type { TieredPricingConfig } from '@/api/endpoints/types'
import TieredPricingEditor from '../TieredPricingEditor.vue'

interface TieredPricingEditorExposed extends ComponentPublicInstance {
  getFinalPricing: () => TieredPricingConfig
  getValidationError: () => string | null
}

const mountedApps: Array<{ app: App, root: HTMLElement }> = []

function mountEditor(
  modelValue: TieredPricingConfig,
  options: {
    showCache1h?: boolean
    showImagePricing?: boolean
    showTokenPricing?: boolean
    showImageEditor?: boolean
  } = {},
) {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const onUpdate = vi.fn()
  let editor: TieredPricingEditorExposed | null = null

  const app = createApp(defineComponent({
    setup() {
      return () => h(TieredPricingEditor, {
        ref: (instance: unknown) => {
          editor = instance as TieredPricingEditorExposed | null
        },
        modelValue,
        showCache1h: options.showCache1h,
        showImagePricing: options.showImagePricing,
        showTokenPricing: options.showTokenPricing,
        showImageEditor: options.showImageEditor,
        'onUpdate:modelValue': onUpdate,
      })
    },
  }))

  app.mount(root)
  mountedApps.push({ app, root })

  return {
    root,
    onUpdate,
    getFinalPricing: () => {
      if (!editor) throw new Error('TieredPricingEditor ref was not mounted')
      return editor.getFinalPricing()
    },
    getValidationError: () => {
      if (!editor) throw new Error('TieredPricingEditor ref was not mounted')
      return editor.getValidationError()
    },
  }
}

function click(element: Element | null) {
  if (!(element instanceof HTMLButtonElement)) {
    throw new Error('Expected a button')
  }
  element.click()
}

afterEach(() => {
  for (const { app, root } of mountedApps.splice(0)) {
    app.unmount()
    root.remove()
  }
})

describe('TieredPricingEditor processing tiers', () => {
  it('round-trips root, overlay and pricing-tier extension fields', () => {
    const pricing = {
      tiers: [{
        up_to: null,
        input_price_per_1m: 5,
        output_price_per_1m: 30,
        vendor_tier_note: 'keep-standard-tier',
      }],
      future_root_option: { enabled: true },
      processing_tiers: {
        priority: {
          tiers: [{
            up_to: null,
            input_price_per_1m: 10,
            output_price_per_1m: 60,
            vendor_tier_note: 'keep-priority-tier',
          }],
          contract_reference: 'priority-2026',
        },
        hyperlane: {
          tiers: [{
            up_to: null,
            input_price_per_1m: 7.5,
            output_price_per_1m: 42,
          }],
          future_overlay_option: { mode: 'reserved' },
        },
      },
    } as TieredPricingConfig

    const { getFinalPricing } = mountEditor(pricing)
    const result = getFinalPricing()

    expect(result.future_root_option).toEqual({ enabled: true })
    expect(result.tiers[0].vendor_tier_note).toBe('keep-standard-tier')
    expect(result.processing_tiers?.priority.contract_reference).toBe('priority-2026')
    expect(result.processing_tiers?.priority.tiers?.[0].vendor_tier_note).toBe('keep-priority-tier')
    expect(result.processing_tiers?.hyperlane.future_overlay_option).toEqual({ mode: 'reserved' })
  })

  it('shows known and discovered tiers and edits a discovered tier through the shared rate controls', async () => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: 5, output_price_per_1m: 30 }],
      future_root_option: 'keep-root',
      processing_tiers: {
        hyperlane: {
          tiers: [{ up_to: null, input_price_per_1m: 7.5, output_price_per_1m: 42 }],
          future_overlay_option: 'keep-overlay',
        },
      },
    } as TieredPricingConfig
    const { root, onUpdate } = mountEditor(pricing)

    expect(root.querySelectorAll('[data-processing-tier]')).toHaveLength(5)
    expect(root.textContent).toContain('Standard')
    expect(root.textContent).toContain('Priority')
    expect(root.textContent).toContain('Flex')
    expect(root.textContent).toContain('Batch')
    expect(root.textContent).toContain('hyperlane')

    click(root.querySelector('[data-processing-tier="hyperlane"]'))
    await nextTick()

    const input = root.querySelector('[data-testid="tier-input-price"]') as HTMLInputElement | null
    if (!input) throw new Error('Expected the shared input-price control')
    input.value = '9.75'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await nextTick()

    const emitted = onUpdate.mock.lastCall?.[0] as TieredPricingConfig
    expect(emitted.processing_tiers?.hyperlane.tiers?.[0].input_price_per_1m).toBe(9.75)
    expect(emitted.processing_tiers?.hyperlane.future_overlay_option).toBe('keep-overlay')
    expect(emitted.future_root_option).toBe('keep-root')
  })

  it('adds and removes an explicit known-tier overlay without changing Standard', async () => {
    const pricing = {
      tiers: [{
        up_to: null,
        input_price_per_1m: 5,
        output_price_per_1m: 30,
        future_tier_option: 'keep-on-clone',
      }],
    } as TieredPricingConfig
    const { root, onUpdate } = mountEditor(pricing)

    click(root.querySelector('[data-processing-tier="priority"]'))
    await nextTick()
    expect(root.querySelector('[data-testid="processing-tier-empty"]')).not.toBeNull()

    click(root.querySelector('[data-testid="processing-tier-add"]'))
    await nextTick()

    let emitted = onUpdate.mock.lastCall?.[0] as TieredPricingConfig
    expect(emitted.tiers[0].input_price_per_1m).toBe(5)
    expect(emitted.processing_tiers?.priority.tiers?.[0]).toMatchObject(pricing.tiers[0])
    expect(emitted.processing_tiers?.priority.tiers?.[0].cache_creation_price_per_1m).toBe(6.25)
    expect(emitted.processing_tiers?.priority.tiers?.[0].cache_read_price_per_1m).toBe(0.5)

    expect(root.querySelector('[data-testid="processing-tier-remove"]'), root.innerHTML).not.toBeNull()
    click(root.querySelector('[data-testid="processing-tier-remove"]'))
    await nextTick()

    emitted = onUpdate.mock.lastCall?.[0] as TieredPricingConfig
    expect(emitted.processing_tiers).toBeUndefined()
    expect(emitted.tiers[0]).toMatchObject(pricing.tiers[0])
    expect(emitted.tiers[0].cache_creation_price_per_1m).toBe(6.25)
    expect(emitted.tiers[0].cache_read_price_per_1m).toBe(0.5)
  })

  it('keeps an unconfigured tier tab outside the persisted pricing contract', async () => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: 5, output_price_per_1m: 30 }],
      image_output_price_default: 0.01,
    } as TieredPricingConfig
    const { getFinalPricing, getValidationError, root } = mountEditor(pricing, {
      showImagePricing: true,
    })

    click(root.querySelector('[data-processing-tier="priority"]'))
    await nextTick()

    expect(root.querySelector('[data-testid="processing-tier-empty"]')).not.toBeNull()
    expect(getValidationError()).toBeNull()
    const result = getFinalPricing()
    expect(result.tiers[0].input_price_per_1m).toBe(5)
    expect(result.image_output_price_default).toBe(0.01)
    expect(result.processing_tiers).toBeUndefined()
  })

  it.each([
    ['absent', undefined],
    ['null', null],
    ['empty object', {}],
  ] as const)('preserves an unedited %s processing_tiers value', (_, processingTiers) => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: 5, output_price_per_1m: 30 }],
      ...(processingTiers === undefined ? {} : { processing_tiers: processingTiers }),
    } as TieredPricingConfig

    const { getFinalPricing } = mountEditor(pricing)
    const result = getFinalPricing()

    expect(Object.prototype.hasOwnProperty.call(result, 'processing_tiers'))
      .toBe(processingTiers !== undefined)
    expect(result.processing_tiers).toEqual(processingTiers)
  })

  it('edits every configured official processing tier through the same controls', async () => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: 5, output_price_per_1m: 30 }],
      processing_tiers: Object.fromEntries(['priority', 'flex', 'batch'].map((key, index) => [
        key,
        {
          tiers: [{
            up_to: null,
            input_price_per_1m: index + 1,
            output_price_per_1m: (index + 1) * 6,
          }],
        },
      ])),
    } as TieredPricingConfig
    const { root, getFinalPricing } = mountEditor(pricing)

    for (const [index, key] of ['priority', 'flex', 'batch'].entries()) {
      click(root.querySelector(`[data-processing-tier="${key}"]`))
      await nextTick()

      const input = root.querySelector('[data-testid="tier-input-price"]') as HTMLInputElement | null
      if (!input) throw new Error(`Expected input-price control for ${key}`)
      input.value = String(11 + index)
      input.dispatchEvent(new Event('input', { bubbles: true }))
      await nextTick()
    }

    const result = getFinalPricing()
    expect(result.tiers[0].input_price_per_1m).toBe(5)
    expect(result.processing_tiers?.priority.tiers?.[0].input_price_per_1m).toBe(11)
    expect(result.processing_tiers?.flex.tiers?.[0].input_price_per_1m).toBe(12)
    expect(result.processing_tiers?.batch.tiers?.[0].input_price_per_1m).toBe(13)
  })

  it('keeps cache multiplier drafts isolated by processing scope', async () => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: 5, output_price_per_1m: 30 }],
      processing_tiers: {
        priority: {
          tiers: [{ up_to: 272000, input_price_per_1m: 10, output_price_per_1m: 60 }],
        },
      },
    } as TieredPricingConfig
    const { root, getFinalPricing } = mountEditor(pricing)

    click(root.querySelector('[data-processing-tier="priority"]'))
    await nextTick()
    const multiplier = root.querySelector(
      'input[aria-label="Priority 阶梯 1 缓存创建倍率"]',
    ) as HTMLInputElement
    multiplier.value = '2'
    multiplier.dispatchEvent(new Event('input', { bubbles: true }))
    await nextTick()

    const result = getFinalPricing()
    expect(result.tiers[0].cache_creation_price_per_1m).toBe(6.25)
    expect(result.processing_tiers?.priority.tiers?.[0].cache_creation_price_per_1m).toBe(20)
  })

  it('keeps processing image catalogs editable when token controls are hidden', async () => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: 5, output_price_per_1m: 30 }],
      processing_tiers: {
        priority: { image_output_price_default: 0.05 },
      },
    } as TieredPricingConfig
    const { root } = mountEditor(pricing, {
      showTokenPricing: false,
      showImagePricing: true,
      showImageEditor: true,
    })

    click(root.querySelector('[data-processing-tier="priority"]'))
    await nextTick()

    expect(root.querySelector('[data-testid="tier-input-price"]')).toBeNull()
    expect(root.querySelector('input[aria-label="Priority 图像输出默认价格"]')).not.toBeNull()
  })

  it('accepts a finite terminal tier for any processing overlay', () => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: 5, output_price_per_1m: 30 }],
      processing_tiers: {
        priority: {
          tiers: [{ up_to: 272000, input_price_per_1m: 10, output_price_per_1m: 60 }],
        },
        hyperlane: {
          tiers: [{ up_to: 180000, input_price_per_1m: 7, output_price_per_1m: 42 }],
        },
      },
    } as TieredPricingConfig

    const { getFinalPricing, getValidationError } = mountEditor(pricing)

    expect(getValidationError()).toBeNull()
    expect(getFinalPricing().processing_tiers?.priority.tiers?.[0].up_to).toBe(272000)
    expect(getFinalPricing().processing_tiers?.hyperlane.tiers?.[0].up_to).toBe(180000)
  })

  it('keeps Standard terminal coverage unbounded', () => {
    const pricing = {
      tiers: [{ up_to: 272000, input_price_per_1m: 5, output_price_per_1m: 30 }],
    } as TieredPricingConfig

    const { getValidationError } = mountEditor(pricing)

    expect(getValidationError()).toBe('Standard: 最后一个阶梯必须是无上限的')
  })

  it('switches a processing terminal tier between finite and unbounded coverage', async () => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: 5, output_price_per_1m: 30 }],
      processing_tiers: {
        priority: {
          tiers: [{ up_to: 272000, input_price_per_1m: 10, output_price_per_1m: 60 }],
        },
      },
    } as TieredPricingConfig
    const { root, getFinalPricing } = mountEditor(pricing)
    click(root.querySelector('[data-processing-tier="priority"]'))
    await nextTick()

    const terminal = root.querySelector(
      'select[aria-label="Priority 阶梯 1 上限"]',
    ) as HTMLSelectElement
    expect(terminal.value).toBe('272000')

    terminal.value = '-2'
    terminal.dispatchEvent(new Event('change', { bubbles: true }))
    await nextTick()
    expect(getFinalPricing().processing_tiers?.priority.tiers?.[0].up_to).toBeNull()

    terminal.value = '272000'
    terminal.dispatchEvent(new Event('change', { bubbles: true }))
    await nextTick()
    expect(getFinalPricing().processing_tiers?.priority.tiers?.[0].up_to).toBe(272000)
  })

  it('preserves processing coverage when tiers are added and removed', async () => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: 5, output_price_per_1m: 30 }],
      processing_tiers: {
        priority: {
          tiers: [{ up_to: 272000, input_price_per_1m: 10, output_price_per_1m: 60 }],
        },
      },
    } as TieredPricingConfig
    const { root, getFinalPricing } = mountEditor(pricing)
    click(root.querySelector('[data-processing-tier="priority"]'))
    await nextTick()

    const addButton = [...root.querySelectorAll('button')]
      .find(button => button.textContent?.includes('添加价格阶梯'))
    click(addButton ?? null)
    await nextTick()
    expect(getFinalPricing().processing_tiers?.priority.tiers?.map(tier => tier.up_to))
      .toEqual([272000, null])

    click(root.querySelector('button[aria-label="删除 Priority 阶梯 2"]'))
    await nextTick()
    expect(getFinalPricing().processing_tiers?.priority.tiers?.map(tier => tier.up_to))
      .toEqual([272000])
  })

  it('preserves a special unknown processing tier key without prototype coercion', () => {
    const pricing = JSON.parse(`{
      "tiers": [{"up_to": null, "input_price_per_1m": 5, "output_price_per_1m": 30}],
      "processing_tiers": {
        "__proto__": {
          "tiers": [{"up_to": null, "input_price_per_1m": 7, "output_price_per_1m": 42}],
          "future_overlay_option": "keep"
        }
      }
    }`) as TieredPricingConfig

    const { root, getFinalPricing } = mountEditor(pricing)
    expect(root.textContent).toContain('__proto__')

    const result = getFinalPricing()
    expect(Object.prototype.hasOwnProperty.call(result.processing_tiers, '__proto__')).toBe(true)
    expect(result.processing_tiers?.__proto__.future_overlay_option).toBe('keep')
    expect(result.processing_tiers?.__proto__.tiers?.[0].input_price_per_1m).toBe(7)
  })

  it('preserves future image pricing fields when image pricing is enabled', () => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: 5, output_price_per_1m: 30 }],
      image_output_prices: {
        '1024x1024': { low: 0.01, ultra: 0.09 },
      },
      image_output_price_ranges: [{
        up_to_pixels: 1_048_576,
        prices: { low: 0.01, ultra: 0.09 },
        future_range_option: { billing_unit: 'image' },
      }],
    } as TieredPricingConfig

    const { getFinalPricing } = mountEditor(pricing, { showImagePricing: true })
    const result = getFinalPricing()

    expect(result.image_output_prices?.['1024x1024'].ultra).toBe(0.09)
    expect(result.image_output_price_ranges?.[0].prices.ultra).toBe(0.09)
    expect(result.image_output_price_ranges?.[0].future_range_option)
      .toEqual({ billing_unit: 'image' })
  })

  it('rejects fractional image pixel limits without coercing them to integers', async () => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: 5, output_price_per_1m: 30 }],
      image_output_price_ranges: [{
        up_to_pixels: 1_048_576,
        prices: { high: 0.07 },
      }],
    } as TieredPricingConfig
    const { getValidationError, root } = mountEditor(pricing, { showImagePricing: true })
    const limit = root.querySelector(
      'input[aria-label="图像像素区间 1 上限"]',
    ) as HTMLInputElement
    limit.value = '1.5'
    limit.dispatchEvent(new Event('input', { bubbles: true }))
    await nextTick()

    expect(getValidationError()).toBe('Standard: 图像像素区间 1 的上限必须是正整数')
  })

  it('treats an image-only processing overlay as a valid tier configuration', async () => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: 5, output_price_per_1m: 30 }],
      processing_tiers: {
        hyperlane: {
          image_output_price_default: 0.08,
          future_overlay_option: { billing_unit: 'image' },
        },
      },
    } as TieredPricingConfig
    const { getFinalPricing, root } = mountEditor(pricing, { showImagePricing: true })

    click(root.querySelector('[data-processing-tier="hyperlane"]'))
    await nextTick()

    expect(root.textContent).not.toContain('至少需要一个价格阶梯')
    expect(getFinalPricing().processing_tiers?.hyperlane).toEqual(
      pricing.processing_tiers?.hyperlane,
    )
  })

  it('edits image pricing through the active processing-tier scope', async () => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: 5, output_price_per_1m: 30 }],
      image_output_price_default: 0.01,
      processing_tiers: {
        priority: {
          image_output_price_default: 0.05,
          image_output_prices: {
            '1024x1024': { high: 0.08, ultra: 0.12 },
          },
          image_output_price_ranges: [{
            up_to_pixels: 1_048_576,
            prices: { high: 0.07, ultra: 0.11 },
            future_range_option: 'keep-priority',
          }],
        },
        flex: {
          image_output_price_default: 0.02,
        },
      },
    } as TieredPricingConfig
    const { getFinalPricing, root } = mountEditor(pricing, { showImagePricing: true })

    click(root.querySelector('[data-processing-tier="priority"]'))
    await nextTick()

    const priorityDefault = root.querySelector(
      'input[aria-label="Priority 图像输出默认价格"]',
    ) as HTMLInputElement
    const priorityHigh = root.querySelector(
      'input[aria-label="1024x1024 high 图像输出价格"]',
    ) as HTMLInputElement
    expect(priorityDefault.value).toBe('0.05')
    expect(priorityHigh.value).toBe('0.08')

    priorityDefault.value = '0.06'
    priorityDefault.dispatchEvent(new Event('input', { bubbles: true }))
    priorityHigh.value = '0.09'
    priorityHigh.dispatchEvent(new Event('input', { bubbles: true }))
    await nextTick()

    click(root.querySelector('[data-processing-tier="flex"]'))
    await nextTick()
    const flexDefault = root.querySelector(
      'input[aria-label="Flex 图像输出默认价格"]',
    ) as HTMLInputElement
    expect(flexDefault.value).toBe('0.02')

    const result = getFinalPricing()
    expect(result.image_output_price_default).toBe(0.01)
    expect(result.processing_tiers?.priority.image_output_price_default).toBe(0.06)
    expect(result.processing_tiers?.priority.image_output_prices?.['1024x1024'].high).toBe(0.09)
    expect(result.processing_tiers?.priority.image_output_prices?.['1024x1024'].ultra).toBe(0.12)
    expect(result.processing_tiers?.priority.image_output_price_ranges?.[0].future_range_option)
      .toBe('keep-priority')
    expect(result.processing_tiers?.flex.image_output_price_default).toBe(0.02)
  })

  it('clears threshold editing state when removing and then adding tiers', async () => {
    const pricing = {
      tiers: [
        { up_to: 64_000, input_price_per_1m: 5, output_price_per_1m: 30 },
        { up_to: 128_000, input_price_per_1m: 7, output_price_per_1m: 42 },
        { up_to: null, input_price_per_1m: 9, output_price_per_1m: 54 },
      ],
    } as TieredPricingConfig
    const { root } = mountEditor(pricing)
    const thresholdSelects = root.querySelectorAll('select')
    const secondThreshold = thresholdSelects.item(1) as HTMLSelectElement

    secondThreshold.value = '-1'
    secondThreshold.dispatchEvent(new Event('change', { bubbles: true }))
    await nextTick()
    expect(root.querySelectorAll('input[placeholder="K"]')).toHaveLength(1)

    const tierRemoveButtons = Array.from(root.querySelectorAll('button'))
      .filter(button => button.querySelector('.lucide-x'))
    click(tierRemoveButtons[0] ?? null)
    await nextTick()

    const addTierButton = Array.from(root.querySelectorAll('button'))
      .find(button => button.textContent?.includes('添加价格阶梯'))
    click(addTierButton ?? null)
    await nextTick()

    expect(root.querySelectorAll('input[placeholder="K"]')).toHaveLength(0)
  })

  it('gives every compact pricing control an accessible name', () => {
    const pricing = {
      tiers: [
        { up_to: 64_000, input_price_per_1m: 5, output_price_per_1m: 30 },
        { up_to: null, input_price_per_1m: 7, output_price_per_1m: 42 },
      ],
    } as TieredPricingConfig
    const { root } = mountEditor(pricing, {
      showCache1h: true,
      showImagePricing: true,
    })

    for (const control of root.querySelectorAll('input, select')) {
      expect(control.getAttribute('aria-label'), control.outerHTML).toBeTruthy()
    }
    const iconOnlyButtons = Array.from(root.querySelectorAll('button'))
      .filter(button => button.textContent?.trim() === '')
    for (const button of iconOnlyButtons) {
      expect(button.getAttribute('aria-label'), button.outerHTML).toBeTruthy()
    }
  })

  it('blocks serialization when an inactive processing tier is invalid', async () => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: 5, output_price_per_1m: 30 }],
      processing_tiers: {
        priority: {
          tiers: [
            { up_to: 128_000, input_price_per_1m: 10, output_price_per_1m: 60 },
            { up_to: 64_000, input_price_per_1m: 11, output_price_per_1m: 66 },
            { up_to: null, input_price_per_1m: 12, output_price_per_1m: 72 },
          ],
        },
      },
    } as TieredPricingConfig
    const { getFinalPricing, getValidationError, onUpdate, root } = mountEditor(pricing)
    const standardInput = root.querySelector('[data-testid="tier-input-price"]') as HTMLInputElement

    standardInput.value = '6'
    standardInput.dispatchEvent(new Event('input', { bubbles: true }))
    await nextTick()

    expect(onUpdate).not.toHaveBeenCalled()
    expect(getValidationError()).toContain('Priority')
    expect(getValidationError()).toContain('上限必须大于前一个阶梯')
    expect(() => getFinalPricing()).toThrow('Priority')
  })

  it('rejects negative known prices before they reach the billing contract', () => {
    const pricing = {
      tiers: [{ up_to: null, input_price_per_1m: -1, output_price_per_1m: 30 }],
    } as TieredPricingConfig
    const { getFinalPricing, getValidationError } = mountEditor(pricing)

    expect(getValidationError()).toBe('Standard: 阶梯 1 的输入价格必须是非负有限数值')
    expect(() => getFinalPricing()).toThrow('输入价格必须是非负有限数值')
  })
})
