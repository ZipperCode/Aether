import { describe, expect, it } from 'vitest'

import {
  buildTransformerDiagnosticPreview,
  buildTransformerDiagnosticSummary,
  buildTransformerDiagnosticTransformerPreview,
  type TransformerDiagnostic,
  type TransformerDiagnosticSummary,
} from '../transformerDiagnostics'

describe('transformerDiagnostics', () => {
  it('builds sorted preview entries from summary', () => {
    const summary: TransformerDiagnosticSummary = {
      count: 5,
      by_code: {
        sampling_clamped: 3,
        tool_result_linked: 1,
        cache_hint_removed: 1,
      },
      by_transformer: {
        sampling: 3,
        tooluse: 1,
        cleancache: 1,
      },
    }

    expect(buildTransformerDiagnosticPreview(summary, 2)).toEqual([
      'sampling_clamped x3',
      'cache_hint_removed x1',
    ])
  })

  it('returns empty preview when summary is missing or empty', () => {
    expect(buildTransformerDiagnosticPreview(null)).toEqual([])
    expect(
      buildTransformerDiagnosticPreview({
        count: 0,
        by_code: {},
        by_transformer: {},
      }),
    ).toEqual([])
  })

  it('builds summary from diagnostics when backend summary is absent', () => {
    const diagnostics: TransformerDiagnostic[] = [
      {
        stage: 'request',
        transformer: 'sampling',
        code: 'sampling_clamped',
        message: 'temperature clamped',
        severity: 'info',
      },
      {
        stage: 'request',
        transformer: 'tooluse',
        code: 'tool_id_generated',
        message: 'generated missing id',
        severity: 'info',
      },
      {
        stage: 'response',
        transformer: 'sampling',
        code: 'sampling_clamped',
        message: 'top_p dropped',
        severity: 'warning',
      },
    ]

    expect(buildTransformerDiagnosticSummary(diagnostics)).toEqual({
      count: 3,
      by_code: {
        sampling_clamped: 2,
        tool_id_generated: 1,
      },
      by_transformer: {
        sampling: 2,
        tooluse: 1,
      },
    })
    expect(buildTransformerDiagnosticPreview(buildTransformerDiagnosticSummary(diagnostics))).toEqual([
      'sampling_clamped x2',
      'tool_id_generated x1',
    ])
    expect(buildTransformerDiagnosticTransformerPreview(buildTransformerDiagnosticSummary(diagnostics))).toEqual([
      'sampling x2',
      'tooluse x1',
    ])
  })
})
