<template>
  <ProviderGenericQuotaCard
    v-if="supportsStructuredQuota"
    :quota="quota"
    :model-probe="modelProbe"
    :provider-type="providerType"
    compact
  />
  <div
    v-else-if="variant === 'mobile'"
    class="rounded-xl border border-border/50 bg-muted/30 px-3 py-2 text-xs"
  >
    <div class="text-muted-foreground mb-1">
      {{ legacyT('配额') }}
    </div>
    <div
      v-if="items.length"
      :class="hasNumericOnlyItems ? '' : 'space-y-2'"
    >
      <QuotaProgressRows
        :items="items"
        mobile
      />
      <div
        v-if="accountQuotaText"
        class="text-[10px] leading-none text-muted-foreground tabular-nums"
      >
        {{ accountQuotaText }}
      </div>
      <ResetCredits />
    </div>
    <div
      v-else-if="accountQuotaText || fallbackText"
      :class="textClass"
    >
      {{ accountQuotaText || fallbackText }}
    </div>
    <div
      v-else
      class="text-muted-foreground"
    >
      -
    </div>
  </div>
  <template v-else>
    <div
      v-if="items.length"
      class="w-full max-w-[208px]"
      :class="hasNumericOnlyItems ? '' : 'space-y-2'"
    >
      <QuotaProgressRows :items="items" />
      <div
        v-if="accountQuotaText"
        class="text-[10px] leading-none text-muted-foreground tabular-nums"
      >
        {{ accountQuotaText }}
      </div>
      <ResetCredits />
    </div>
    <span
      v-else-if="accountQuotaText || fallbackText"
      :class="textClass"
    >
      {{ accountQuotaText || fallbackText }}
    </span>
    <span
      v-else
      class="text-xs text-muted-foreground"
    >-</span>
  </template>
</template>

<script setup lang="ts">
import { computed, defineComponent, h, type PropType } from 'vue'
import { useI18n } from '@/i18n'
import type { ModelProbeStatusSnapshot, QuotaStatusSnapshot } from '@/api/endpoints/types'
import ProviderGenericQuotaCard from '@/features/providers/components/ProviderGenericQuotaCard.vue'
import { isOfficialQuotaProviderType } from '@/features/providers/utils/providerTypeUtils'

/** Pool 额度行的展示模型；百分比控制进度宽度，文字可保留精确数值或范围。 */
export interface PoolQuotaProgressDisplayItem {
  label: string
  remainingPercent: number
  resetText: string
  meterText: string
  barClass: string
  meterClass: string
  wrapMeter?: boolean
  /** 为真时只显示紧凑数字，不绘制进度条；最终 Antigravity 摘要不使用该模式。 */
  numericOnly?: boolean
}

const props = withDefaults(defineProps<{
  items: PoolQuotaProgressDisplayItem[]
  quota?: QuotaStatusSnapshot | null
  modelProbe?: ModelProbeStatusSnapshot | null
  providerType?: string | null
  accountQuotaText?: string | null
  fallbackText?: string | null
  textClass?: string
  variant?: 'desktop' | 'mobile'
  /** Codex 可用重置机会的汇总文字；为空时不渲染重置区。 */
  resetCreditText?: string | null
  /** 最多若干条 Codex 重置机会的到期说明。 */
  resetCreditItems?: string[]
  /** 当前凭据代际与额度状态是否允许发起一次幂等重置。 */
  canConsumeResetCredit?: boolean
  /** 当前 Key 是否正在消耗重置机会，用于禁止重复提交。 */
  consumingResetCredit?: boolean
}>(), {
  accountQuotaText: null,
  quota: null,
  modelProbe: null,
  providerType: null,
  fallbackText: null,
  textClass: '',
  variant: 'desktop',
  resetCreditText: null,
  resetCreditItems: () => [],
  canConsumeResetCredit: false,
  consumingResetCredit: false,
})

/** 将重置动作交给页面级幂等流程处理，展示组件不直接访问接口。 */
const emit = defineEmits<{
  'consume-reset-credit': []
}>()


const { legacyT } = useI18n()
// 官方 API Key 额度与提供商抽屉共享完整来源分组，避免维护两套金额解析。
const supportsStructuredQuota = computed(() => isOfficialQuotaProviderType(
  props.quota?.provider_type || props.providerType,
))
const hasNumericOnlyItems = computed(() => (
  props.items.length > 0 && props.items.every(item => item.numericOnly)
))

/** 渲染 Codex 重置机会摘要与单一消费入口。 */
const ResetCredits = defineComponent({
  name: 'PoolQuotaResetCredits',
  /** 返回依赖当前 props 的轻量渲染函数；无重置摘要时不占布局。 */
  setup() {
    return () => props.resetCreditText ? h('div', {
      'data-testid': 'pool-quota-reset-credits',
      class: 'mt-2 border-t border-border/50 pt-1.5 text-[10px] leading-4 text-muted-foreground',
    }, [
      h('div', { class: 'flex flex-wrap items-center gap-x-1' }, [
        props.canConsumeResetCredit
          ? h('button', {
            type: 'button',
            disabled: props.consumingResetCredit,
            class: 'font-medium text-primary hover:underline disabled:pointer-events-none disabled:opacity-60',
            onClick: () => emit('consume-reset-credit'),
          }, props.consumingResetCredit ? legacyT('重置中...') : legacyT('点击以进行重置'))
          : null,
        h('span', props.resetCreditText),
      ]),
      props.resetCreditItems.length
        ? h('div', { class: 'truncate tabular-nums', title: props.resetCreditItems.join(' · ') }, props.resetCreditItems.join(' · '))
        : null,
    ]) : null
  },
})

/** 统一渲染普通进度条与可选的紧凑数字额度行。 */
const QuotaProgressRows = defineComponent({
  name: 'QuotaProgressRows',
  props: {
    items: {
      type: Array as PropType<PoolQuotaProgressDisplayItem[]>,
      required: true,
    },
    mobile: {
      type: Boolean,
      default: false,
    },
  },
  /** 根据行级模式选择网格或进度条布局，并保留精确数值换行能力。 */
  setup(props) {
    return () => h('div', {
      'data-testid': 'pool-quota-rows',
      class: props.items.every(item => item.numericOnly)
        ? 'grid grid-cols-2 gap-x-3 gap-y-1.5 min-w-0'
        : 'space-y-2',
    }, props.items.map((item, idx) => h('div', {
      key: `${item.label}-${idx}`,
      class: item.numericOnly
        ? 'flex min-w-0 items-baseline justify-between gap-2 text-[10px] leading-4'
        : props.mobile
          ? 'flex flex-col gap-1 min-w-0'
          : 'flex flex-col gap-1 min-w-[140px] max-w-[208px]',
    }, [
      h('div', { class: item.numericOnly ? 'contents' : 'flex items-center justify-between text-[10px] leading-none' }, [
        h('span', {
          'data-testid': 'pool-quota-period-label',
          class: item.numericOnly
            ? 'min-w-0 truncate text-muted-foreground'
            : 'text-muted-foreground font-medium shrink-0',
          title: item.numericOnly ? item.label : undefined,
        }, item.label),
        item.resetText && !item.numericOnly
          ? h('span', {
            'data-testid': 'pool-quota-reset-text',
            class: 'text-muted-foreground/80 tabular-nums truncate',
            title: item.resetText,
          }, item.resetText)
          : null,
      ]),
      h('div', { class: item.numericOnly ? 'contents' : 'flex items-center gap-1.5' }, [
        item.numericOnly
          ? null
          : h('div', {
            'data-testid': 'pool-quota-progress-track',
            class: 'relative flex-1 h-1.5 rounded-full bg-border overflow-hidden',
          }, [
            h('div', {
              class: ['absolute left-0 top-0 h-full rounded-full transition-all duration-300', item.barClass],
              style: { width: `${item.remainingPercent}%` },
            }),
          ]),
        h('span', {
          'data-testid': 'pool-quota-meter-text',
          class: [
            'text-[10px] font-medium tabular-nums leading-tight',
            item.wrapMeter ? 'min-w-0 break-all text-right' : 'shrink-0',
            item.numericOnly ? 'text-right' : '',
            item.meterClass,
          ],
        }, item.meterText),
      ]),
    ])))
  },
})
</script>
