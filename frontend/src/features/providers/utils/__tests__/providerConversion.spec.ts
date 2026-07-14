import { describe, expect, it } from 'vitest'

import {
  canConvertExistingProvider,
  formatProviderConversionConfirmation,
} from '../providerConversion'

describe('provider conversion', () => {
  it('offers conversion only for an existing custom provider', () => {
    expect(canConvertExistingProvider({ providerType: 'custom', isEditMode: true })).toBe(true)
    expect(canConvertExistingProvider({ providerType: 'deepseek', isEditMode: true })).toBe(false)
    expect(canConvertExistingProvider({ providerType: 'custom', isEditMode: false })).toBe(false)
  })

  it('names the target type and affected endpoint and key counts', () => {
    const message = formatProviderConversionConfirmation({
      targetType: 'OpenRouter', endpointCount: 3, keyCount: 7,
    })
    expect(message).toContain('OpenRouter')
    expect(message).toContain('3 个端点和 7 个密钥')
  })
})
