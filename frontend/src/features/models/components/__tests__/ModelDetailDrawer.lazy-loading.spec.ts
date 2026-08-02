import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

const drawerSource = readFileSync(
  resolve(process.cwd(), 'src/features/models/components/ModelDetailDrawer.vue'),
  'utf8',
)
const mappingsSource = readFileSync(
  resolve(process.cwd(), 'src/features/models/components/ModelMappingsTab.vue'),
  'utf8',
)

describe('ModelDetailDrawer lazy loading', () => {
  it('does not load routing when the drawer opens and loads it on the routing tab', () => {
    const openWatcher = drawerSource.split('watch(() => props.open')[1]?.split('watch(detailTab')[0]
    const tabWatcher = drawerSource.split('watch(detailTab')[1]?.split('watch(() => props.model?.id')[0]

    expect(openWatcher).toBeTruthy()
    expect(openWatcher).not.toContain('loadRoutingData()')
    expect(tabWatcher).toContain("tab === 'routing'")
    expect(tabWatcher).toContain('void loadRoutingData()')
    expect(drawerSource).toContain(':active="detailTab === \'mappings\'"')
  })

  it('keeps preview errors distinct from empty matches and exposes pagination', () => {
    expect(mappingsSource).toContain('v-else-if="previewError"')
    expect(mappingsSource).toContain('预览失败')
    expect(mappingsSource).toContain('expandedTotalPages > 1')
    expect(mappingsSource).toContain('sequence !== previewSequence')
  })

  it('shows key-model mapping occurrences instead of collapsing duplicate model names', () => {
    expect(mappingsSource).toContain('matched_mapping_count')
    expect(mappingsSource).toContain('{{ mappingMatchCounts[index] }} 匹配')
    expect(mappingsSource).toContain('个上游模型名')
  })
})
