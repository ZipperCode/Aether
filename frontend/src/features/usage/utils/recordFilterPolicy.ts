import type { FilterStatusValue, UsageRecord } from '../types'

export function isUserLocalOnlyRecordStatus(status: FilterStatusValue): boolean {
  // 跳过候选已由用户接口先筛选再分页，重试和转移保持现有的本地筛选。
  return status === 'has_retry' || status === 'has_fallback'
}

export function shouldUseServerUserRecordFilters(input: {
  search: string
  apiFormat: string
  status: FilterStatusValue
}): boolean {
  if (isUserLocalOnlyRecordStatus(input.status)) return false

  return input.search.trim().length > 0
    || input.apiFormat !== '__all__'
    || input.status !== '__all__'
}

export function isUnknownUsageLabel(value: unknown): boolean {
  if (typeof value !== 'string') return false
  const normalized = value.trim().toLowerCase()
  return normalized === 'unknown' || normalized === 'unknow'
}

export function hasUnknownModelOrProvider(record: UsageRecord): boolean {
  if (isUnknownUsageLabel(record.model)) return true
  if (isUnknownUsageLabel(record.provider)) return true
  return false
}
