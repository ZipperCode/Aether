export function selectOpenProviderSnapshot<T extends { readonly id: string }>(params: {
  readonly open: boolean
  readonly providerId?: string | null
  readonly current: T | null
  readonly incoming: T | null
}): T | null {
  if (!params.open || !params.incoming || params.incoming.id !== params.providerId) {
    return params.current
  }
  return params.incoming
}
