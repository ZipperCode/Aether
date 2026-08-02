import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

const source = readFileSync(
  resolve(process.cwd(), 'src/features/providers/components/ProviderDetailDrawer.vue'),
  'utf8',
)

describe('ProviderDetailDrawer loading priorities', () => {
  it('plays the drawer transition when the lazily mounted component first appears', () => {
    const transitionTemplate = source
      .split('<Transition')[1]
      ?.split('>')[0]

    expect(transitionTemplate).toContain('name="drawer"')
    expect(transitionTemplate).toContain('appear')
    expect(source).toContain('class="drawer-panel relative')
    expect(source).toContain('.drawer-enter-from .drawer-panel')
    expect(source).not.toContain('.drawer-enter-from .relative')
  })

  it('loads mapping preview after first-screen provider data', () => {
    const openWatcher = source
      .split('// 合并监听 providerId 和 open')[1]
      ?.split('} else if (!newOpen && oldOpen)')[0]

    expect(openWatcher).toBeTruthy()
    expect(openWatcher).toContain('const endpointsPromise = loadEndpoints()')
    expect(openWatcher).toContain('endpointsPromise.then(() => {')
    expect(openWatcher).toContain('if (!props.open || props.providerId !== newId) return')
    expect(openWatcher).toContain('void loadMappingPreview()')

    const beforeEndpoints = openWatcher?.split('const endpointsPromise = loadEndpoints()')[0]
    expect(beforeEndpoints).not.toContain('loadMappingPreview()')
  })

  it('keeps model and mapping loading states independent', () => {
    expect(source).toContain(':loading="loadingProviderModels"')
    expect(source).toContain(':loading="loadingProviderMappingPreview"')
    expect(source).not.toContain(':loading="loadingProviderModels || loadingProviderKeys"')
    expect(source).not.toContain(':loading="loadingProviderEndpoints || loadingProviderKeys || loadingProviderModels || loadingProviderMappingPreview"')
  })

  it('does not mount closed child dialogs or blur the page behind the drawer', () => {
    expect(source).toContain('class="absolute inset-0 bg-black/30"')
    expect(source).not.toContain('backdrop-blur-sm')
    expect(source).toContain('v-if="provider && open && endpointDialogOpen"')
    expect(source).toContain('v-if="open && keyFormDialogOpen"')
    expect(source).toContain('v-if="open && oauthAccountDialogOpen && provider"')
    expect(source).toContain('v-if="open && oauthKeyEditDialogOpen"')
    expect(source).toContain('v-if="open && keyPermissionsDialogOpen"')
    expect(source).toContain('v-if="open && modelFormDialogOpen && provider"')
    expect(source).toContain('v-if="open && batchAssignDialogOpen && provider"')
    expect(source).toContain('v-if="open && failoverRulesDialogOpen"')
  })

  it('marks a transport-error stop policy as a configured failover rule', () => {
    expect(source).toContain('rules.stop_on_transport_errors === true')
  })

  it('wires generic quota cards to a single-key manual refresh', () => {
    expect(source).toContain(':loading="isQuotaRefreshingForKey(key)"')
    expect(source).toContain(':refreshable="key.is_active"')
    expect(source).toContain(':model-probe="key.status_snapshot?.model_probe"')
    expect(source).toContain('@refresh="handleManualQuotaRefresh(key)"')

    const handler = source
      .split('async function handleManualQuotaRefresh(key: EndpointAPIKey): Promise<void> {')[1]
      ?.split('// 通用的自动刷新配额函数')[0]
    expect(handler).toBeTruthy()
    expect(handler).toContain('refreshProviderQuota(providerId, [key.id])')
    expect(handler).toContain('applyQuotaResults(result.results)')
    expect(source).toContain('model_probe: target.status_snapshot?.model_probe ?? null')
  })

  it('moves generic quota-unavailable markers beside the key name', () => {
    expect(source).toContain(':quota-status-label="getGenericQuotaStatusLabel(key)"')

    const helper = source
      .split('function getGenericQuotaStatusLabel(key: EndpointAPIKey): string | null {')[1]
      ?.split('function getQuotaSnapshotForProvider')[0]
    expect(helper).toBeTruthy()
    expect(helper).toContain('isGenericQuotaUnavailable(')
    expect(helper).toContain("? 'Expired' : null")
  })
})
