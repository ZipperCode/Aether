import { describe, expect, it } from 'vitest'

import { selectOpenProviderSnapshot } from '../providerOpenState'

const oldProvider = { id: 'provider-1', name: 'Old' }
const refreshedProvider = { id: 'provider-1', name: 'Refreshed' }

describe('open provider state', () => {
  it('updates the open provider from a refreshed list snapshot without resetting the drawer', () => {
    expect(selectOpenProviderSnapshot({
      open: true,
      providerId: 'provider-1',
      current: oldProvider,
      incoming: refreshedProvider,
    })).toBe(refreshedProvider)
  })

  it('ignores stale refreshes after close or provider selection changes', () => {
    expect(selectOpenProviderSnapshot({
      open: false,
      providerId: 'provider-1',
      current: oldProvider,
      incoming: refreshedProvider,
    })).toBe(oldProvider)
    expect(selectOpenProviderSnapshot({
      open: true,
      providerId: 'provider-2',
      current: oldProvider,
      incoming: refreshedProvider,
    })).toBe(oldProvider)
  })
})
