<template>
  <div class="space-y-3 rounded-lg border p-3">
    <div class="space-y-1.5">
      <div class="flex items-center justify-between gap-2">
        <Label class="text-xs font-medium">{{ t('providers.local.targetAPIFormat') }}</Label>
        <span class="text-xs text-muted-foreground">{{ selectedFormats.length }}/{{ formatOptions.length }}</span>
      </div>
      <MultiSelect
        :model-value="selectedFormats"
        :options="formatOptions"
        :placeholder="t('providers.local.selectTargetFormat')"
        :empty-text="t('providers.local.theSelectedProvidersHaveNoEndpoints')"
        no-results-text="未找到格式"
        trigger-class="h-9 rounded-md"
        dropdown-min-width="18rem"
        :disabled="disabled || loading"
        :search-threshold="4"
        @update:model-value="emit('update:selectedFormats', $event)"
      />
    </div>

    <div class="grid gap-3 md:grid-cols-2">
      <div class="space-y-2 rounded-md border bg-muted/10 p-3">
        <label class="flex items-center gap-2 text-sm font-medium">
          <Checkbox
            :checked="state.proxy.include"
            :disabled="disabled"
            @update:checked="patchProxy({ include: $event })"
          />
          <span>{{ t('ui.proxyNodes23f1b6a') }}</span>
        </label>
        <div
          class="flex items-center gap-2"
          :class="state.proxy.include ? '' : 'pointer-events-none opacity-45'"
        >
          <ProxyNodeSelect
            ref="proxySelectRef"
            :model-value="state.proxy.nodeId"
            trigger-class="h-9 flex-1"
            @update:model-value="patchProxy({ nodeId: $event })"
          />
          <Button
            variant="outline"
            size="sm"
            class="h-9"
            @click="patchProxy({ nodeId: '' })"
          >
            {{ t('ui.clear') }}
          </Button>
        </div>
      </div>

      <div class="space-y-2 rounded-md border bg-muted/10 p-3">
        <label class="flex items-center gap-2 text-sm font-medium">
          <Checkbox
            :checked="state.formatConversion.include"
            :disabled="disabled"
            @update:checked="patchFormatConversion({ include: $event })"
          />
          <span>{{ t('guide.nav.advanced.formatConversion') }}</span>
        </label>
        <div
          class="flex items-center justify-between"
          :class="state.formatConversion.include ? '' : 'pointer-events-none opacity-45'"
        >
          <span class="text-xs text-muted-foreground">{{ t('providers.local.toggleFormatConversionForAll') }}</span>
          <Switch
            :model-value="state.formatConversion.enabled"
            @update:model-value="patchFormatConversion({ enabled: $event })"
          />
        </div>
      </div>

      <div class="space-y-2 rounded-md border bg-muted/10 p-3">
        <label class="flex items-center gap-2 text-sm font-medium">
          <Checkbox
            :checked="state.upstreamPolicy.include"
            :disabled="disabled"
            @update:checked="patchUpstreamPolicy({ include: $event })"
          />
          <span>{{ t('providers.local.upstreamStreamingPolicy') }}</span>
        </label>
        <Select
          :model-value="state.upstreamPolicy.value"
          :disabled="!state.upstreamPolicy.include || disabled"
          @update:model-value="patchUpstreamPolicy({ value: $event as EndpointBatchState['upstreamPolicy']['value'] })"
        >
          <SelectTrigger class="h-9">
            <SelectValue :placeholder="t('providers.local.selectPolicy')" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="auto">
              {{ t('ui.automatic') }}
            </SelectItem>
            <SelectItem value="force_stream">
              {{ t('providers.local.forceStreaming') }}
            </SelectItem>
            <SelectItem value="force_non_stream">
              {{ t('providers.local.forceNonStreaming') }}
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      <div class="space-y-2 rounded-md border bg-muted/10 p-3">
        <label class="flex items-center gap-2 text-sm font-medium">
          <Checkbox
            :checked="state.active.include"
            :disabled="disabled"
            @update:checked="patchActive({ include: $event })"
          />
          <span>{{ t('feedback.error.field.isActive') }}</span>
        </label>
        <div
          class="flex items-center justify-between"
          :class="state.active.include ? '' : 'pointer-events-none opacity-45'"
        >
          <span class="text-xs text-muted-foreground">{{ t('providers.local.enableOrDisableAllEndpoints') }}</span>
          <Switch
            :model-value="state.active.value"
            @update:model-value="patchActive({ value: $event })"
          />
        </div>
      </div>
    </div>

    <div class="space-y-2 rounded-md border bg-muted/10 p-3">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <label class="flex items-center gap-2 text-sm font-medium">
          <Checkbox
            :checked="state.rules.include"
            :disabled="disabled"
            @update:checked="patchRules({ include: $event })"
          />
          <span>{{ t('providers.local.headerBodyRules') }}</span>
        </label>
        <div class="flex items-center gap-3 text-xs">
          <label class="flex items-center gap-1.5">
            <input
              type="radio"
              value="append"
              :checked="state.rules.mode === 'append'"
              :disabled="disabled || !state.rules.include"
              @change="patchRules({ mode: 'append' })"
            >
            <span>{{ t('ui.append') }}</span>
          </label>
          <label class="flex items-center gap-1.5">
            <input
              type="radio"
              value="overwrite"
              :checked="state.rules.mode === 'overwrite'"
              :disabled="disabled || !state.rules.include"
              @change="patchRules({ mode: 'overwrite' })"
            >
            <span>{{ t('providers.local.overwrite') }}</span>
          </label>
        </div>
      </div>
      <Textarea
        :model-value="state.rules.json"
        class="min-h-[140px] font-mono text-xs"
        spellcheck="false"
        placeholder="{ &quot;header_rules&quot;: [], &quot;body_rules&quot;: [] }"
        :disabled="disabled || !state.rules.include"
        @update:model-value="patchRules({ json: $event })"
      />
      <p
        v-if="state.rules.error"
        class="text-xs text-destructive"
      >
        {{ state.rules.error }}
      </p>
      <p class="text-xs text-muted-foreground">
        {{ t('providers.local.responseHeaderRulesAreNotIncludedInThis') }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from '@/i18n'

import { ref, watch } from 'vue'
import {
  Button,
  Checkbox,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Switch,
  Textarea,
} from '@/components/ui'
import MultiSelect from '@/components/common/MultiSelect.vue'
import ProxyNodeSelect from '@/features/providers/components/ProxyNodeSelect.vue'
import type { EndpointBatchState } from '@/features/providers/utils/batchEdit'

const props = defineProps<{
  state: EndpointBatchState
  formatOptions: Array<{ value: string; label: string }>
  selectedFormats: string[]
  disabled: boolean
  loading: boolean
}>()

const emit = defineEmits<{
  'update:state': [state: EndpointBatchState]
  'update:selectedFormats': [formats: string[]]
}>()
const { t } = useI18n()


const proxySelectRef = ref<InstanceType<typeof ProxyNodeSelect> | null>(null)

watch(
  () => props.state.proxy.include,
  (include) => {
    if (include) proxySelectRef.value?.ensureLoaded()
  },
)

function patchState(patch: Partial<EndpointBatchState>) {
  emit('update:state', { ...props.state, ...patch })
}

function patchProxy(patch: Partial<EndpointBatchState['proxy']>) {
  patchState({ proxy: { ...props.state.proxy, ...patch } })
}

function patchFormatConversion(patch: Partial<EndpointBatchState['formatConversion']>) {
  patchState({ formatConversion: { ...props.state.formatConversion, ...patch } })
}

function patchUpstreamPolicy(patch: Partial<EndpointBatchState['upstreamPolicy']>) {
  patchState({ upstreamPolicy: { ...props.state.upstreamPolicy, ...patch } })
}

function patchActive(patch: Partial<EndpointBatchState['active']>) {
  patchState({ active: { ...props.state.active, ...patch } })
}

function patchRules(patch: Partial<EndpointBatchState['rules']>) {
  patchState({
    rules: {
      ...props.state.rules,
      ...patch,
      error: patch.json !== undefined ? null : props.state.rules.error,
    },
  })
}
</script>
