<template>
  <a
    v-if="recovery"
    :href="recovery.url"
    target="_blank"
    rel="noopener noreferrer"
    referrerpolicy="no-referrer"
    class="inline-flex shrink-0 items-center gap-1 text-xs text-primary underline-offset-2 hover:underline"
    @click.stop
  >
    {{ t(recovery.label) }}
  </a>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from '@/i18n'
import type { AccountStatusSnapshot } from '@/api/endpoints/types/statusSnapshot'

const props = defineProps<{ account?: AccountStatusSnapshot | null }>()
const { t } = useI18n()

const recovery = computed(() => {
  const account = props.account
  if (!account?.blocked) return null
  const verification = account.code === 'account_verification'
  const appeal = account.code === 'account_banned' || account.code === 'account_suspended'
  if (!verification && !appeal) return null
  const raw = verification ? account.validation_url : account.appeal_url
  if (!raw || /[\u0000-\u0020\u007f]/.test(raw)) return null
  try {
    const url = new URL(raw)
    // 只打开上游明确提供的 Google 恢复入口，不把诊断文字或任意 URL 变成链接。
    const host = verification ? 'accounts.google.com' : 'support.google.com'
    if (url.protocol !== 'https:' || url.hostname !== host || url.username || url.password || url.port) return null
    return {
      url: url.href,
      label: verification ? 'providers.local.verifyAccount' : 'providers.local.appealAccount',
    } as const
  } catch {
    return null
  }
})
</script>
