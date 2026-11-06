import { translate as t } from '@/i18n'

export function canConvertExistingProvider(params: {
  readonly providerType?: string | null
  readonly isEditMode: boolean
}): boolean {
  return params.isEditMode && params.providerType?.trim().toLowerCase() === 'custom'
}

export function formatProviderConversionConfirmation(params: {
  readonly targetType: string
  readonly endpointCount: number
  readonly keyCount: number
}): string {
  return t('providers.local.conversionConfirmation', params)
}
