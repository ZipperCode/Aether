export interface ServiceTierFacts {
  requested: string | null
  actual: string | null
  billing: string | null
}

export interface ServiceTierFactSource {
  service_tier?: unknown
  actual_service_tier?: unknown
  settlement?: unknown
}

export function resolveServiceTierFacts(
  source: ServiceTierFactSource | null | undefined,
): ServiceTierFacts {
  const settlement = asRecord(source?.settlement)
  const settlementSnapshot = asRecord(settlement?.settlement_snapshot)
  const pricingSnapshot = asRecord(settlementSnapshot?.pricing_snapshot)
  return {
    requested: normalizeServiceTierFact(source?.service_tier),
    actual: normalizeServiceTierFact(source?.actual_service_tier),
    billing: normalizeServiceTierFact(pricingSnapshot?.billing_processing_tier),
  }
}

export function hasServiceTierFact(facts: ServiceTierFacts): boolean {
  return facts.requested !== null || facts.actual !== null || facts.billing !== null
}

export function normalizeServiceTierFact(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const normalized = value.trim()
  return normalized || null
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
}
