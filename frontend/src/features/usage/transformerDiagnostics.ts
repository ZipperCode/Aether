export interface TransformerDiagnostic {
  stage: string
  transformer: string
  code: string
  message: string
  severity: string
  details?: Record<string, unknown>
}

export interface TransformerDiagnosticSummary {
  count: number
  by_code: Record<string, number>
  by_transformer: Record<string, number>
}

type CountEntry = {
  key: string
  count: number
}

function sortCountEntries(items: Record<string, number> | undefined): CountEntry[] {
  return Object.entries(items || {})
    .map(([key, count]) => ({ key, count }))
    .sort((left, right) => {
      if (right.count !== left.count) return right.count - left.count
      return left.key.localeCompare(right.key)
    })
}

export function buildTransformerDiagnosticSummary(
  diagnostics: TransformerDiagnostic[] | null | undefined,
): TransformerDiagnosticSummary | null {
  if (!diagnostics || diagnostics.length === 0) return null

  const byCode: Record<string, number> = {}
  const byTransformer: Record<string, number> = {}

  for (const item of diagnostics) {
    byCode[item.code] = (byCode[item.code] || 0) + 1
    byTransformer[item.transformer] = (byTransformer[item.transformer] || 0) + 1
  }

  return {
    count: diagnostics.length,
    by_code: byCode,
    by_transformer: byTransformer,
  }
}

export function buildTransformerDiagnosticPreview(
  summary: TransformerDiagnosticSummary | null | undefined,
  limit = 3,
): string[] {
  if (!summary || summary.count <= 0) return []
  return sortCountEntries(summary.by_code)
    .slice(0, Math.max(0, limit))
    .map(({ key, count }) => `${key} x${count}`)
}

export function buildTransformerDiagnosticTransformerPreview(
  summary: TransformerDiagnosticSummary | null | undefined,
  limit = 3,
): string[] {
  if (!summary || summary.count <= 0) return []
  return sortCountEntries(summary.by_transformer)
    .slice(0, Math.max(0, limit))
    .map(({ key, count }) => `${key} x${count}`)
}
