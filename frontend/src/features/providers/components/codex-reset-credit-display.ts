import type {
  QuotaResetCreditSnapshot,
  QuotaResetCreditsSnapshot,
} from '@/api/endpoints/types'

export interface CodexResetCreditDisplayItem {
  id?: string | null
  displayKey: string
  expiresAt?: number | null
  remainingSeconds: number
  title: string
}

interface CodexResetCreditDisplayCandidate {
  id?: string | null
  expiresAt?: number | null
  remainingSeconds: number
}

export function getCodexResetCreditAvailableCount(
  snapshot: QuotaResetCreditsSnapshot | null | undefined,
): number | null {
  const count = snapshot?.available_count
  return typeof count === 'number' && Number.isFinite(count) && count >= 0 ? count : null
}

export function formatCodexResetCreditCount(count: number | null | undefined): string {
  return `共 ${count ?? 0} 次机会`
}

function codexResetCreditRemainingSeconds(
  item: QuotaResetCreditSnapshot,
  snapshot: QuotaResetCreditsSnapshot,
  nowUnixSecs: number,
): number | null {
  if (typeof item.expires_at === 'number' && Number.isFinite(item.expires_at)) {
    return Math.max(item.expires_at - nowUnixSecs, 0)
  }
  if (typeof item.remaining_seconds === 'number' && Number.isFinite(item.remaining_seconds)) {
    const updatedAt = snapshot.updated_at
    const elapsed = typeof updatedAt === 'number' && Number.isFinite(updatedAt)
      ? Math.max(nowUnixSecs - updatedAt, 0)
      : 0
    return Math.max(item.remaining_seconds - elapsed, 0)
  }
  return null
}

function codexResetCreditStatusIsDisplayable(item: QuotaResetCreditSnapshot): boolean {
  const status = item.status?.trim().toLowerCase()
  return !status || status === 'available' || status === 'active'
}

export function getVisibleCodexResetCreditItems(
  snapshot: QuotaResetCreditsSnapshot | null | undefined,
  nowUnixSecs = Math.floor(Date.now() / 1000),
  limit = 5,
): CodexResetCreditDisplayItem[] {
  const credits = snapshot?.credits
  if (!snapshot || !Array.isArray(credits)) return []

  return credits
    .map((item) => {
      if (!codexResetCreditStatusIsDisplayable(item)) return null
      const remainingSeconds = codexResetCreditRemainingSeconds(item, snapshot, nowUnixSecs)
      if (remainingSeconds === null || remainingSeconds <= 0) return null
      return {
        id: item.id,
        expiresAt: nowUnixSecs + remainingSeconds,
        remainingSeconds,
      } satisfies CodexResetCreditDisplayCandidate
    })
    .filter((item): item is CodexResetCreditDisplayCandidate => item !== null)
    .sort((a, b) => a.remainingSeconds - b.remainingSeconds)
    .slice(0, limit)
    .map((item, index) => {
      const displayKey = `Key-${index + 1}`
      return {
        ...item,
        displayKey,
        title: `Codex 重置机会 ${displayKey}`,
      } satisfies CodexResetCreditDisplayItem
    })
}

export function formatCodexResetCreditExpiresAt(expiresAt: number | null | undefined): string {
  if (typeof expiresAt !== 'number' || !Number.isFinite(expiresAt)) return '-'
  const date = new Date(expiresAt * 1000)
  if (Number.isNaN(date.getTime())) return '-'

  const pad = (value: number) => String(value).padStart(2, '0')
  return `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
}
