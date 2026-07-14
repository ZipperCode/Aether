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
  return `确认将此提供商转换为 ${params.targetType}？转换会影响 ${params.endpointCount} 个端点和 ${params.keyCount} 个密钥。取消后将保持当前自定义类型不变。`
}
