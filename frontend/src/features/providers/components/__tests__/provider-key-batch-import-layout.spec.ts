import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

function readSource(path: string): string {
  return readFileSync(resolve(process.cwd(), path), 'utf8')
}

describe('provider key batch import UI contract', () => {
  it('shows the entry for every key-managed provider', () => {
    const source = readSource('src/features/providers/components/ProviderDetailDrawer.vue')

    // 按钮和弹窗共用密钥型判断；弹窗先确认 provider 非空，不依赖可选链写法。
    expect(source).toContain('v-if="endpoints.length > 0 && isKeyManagedProviderType(provider.provider_type)"')
    expect(source).toContain('@click="keyBatchImportDialogOpen = true"')
    expect(source).toContain('v-if="open && keyBatchImportDialogOpen && provider && isKeyManagedProviderType(provider.provider_type)"')
    expect(source).not.toContain("provider.provider_type === 'custom'")
    expect(source).toContain('<ProviderKeyBatchImportDialog')
    expect(source).toContain('keyBatchImportDialogOpen')
  })

  it('uses a three-step flow with per-key review overrides', () => {
    const source = readSource('src/features/providers/components/ProviderKeyBatchImportDialog.vue')

    expect(source).toContain('parseProviderKeyBatchImport')
    expect(source).toContain("{ id: 3, label: '逐项确认' }")
    expect(source).toContain('ProviderKeyImportSettingsFields')
    expect(source).toContain('REVIEW_PAGE_SIZE')
    expect(source).toContain('item.customized')
    expect(source).toContain('api_formats: item.apiFormats')
    expect(source).toContain('batchImportPoolKeys')
    expect(source).not.toContain('MAX_BATCH_IMPORT_KEYS')

    const fieldsSource = readSource('src/features/providers/components/ProviderKeyImportSettingsFields.vue')
    expect(fieldsSource).toContain('settings.max_probe_interval_minutes')
    expect(fieldsSource).toContain('settings.proxy_node_id')
  })

  it('uses selective update_settings in pool batch management', () => {
    const source = readSource('src/features/pool/components/PoolAccountBatchDialog.vue')

    expect(source).toContain("selectedAction === 'update_settings'")
    expect(source).toContain('buildPoolKeySettingsPatch')
    expect(source).toContain('confirmAndExecuteAction(selectedAction)')
    expect(source).toContain('仅更新已勾选字段')
    expect(source).not.toContain('v-for="item in ACTION_OPTIONS"')
  })
})
