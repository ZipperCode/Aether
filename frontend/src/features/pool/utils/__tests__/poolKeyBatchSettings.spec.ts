import { describe, expect, it } from 'vitest'

import {
  buildPoolKeySettingsPatch,
  createPoolKeyBatchSettingSelection,
  createPoolKeyBatchSettingsDraft,
  validatePoolKeyBatchSettings,
} from '../poolKeyBatchSettings'

describe('pool key batch settings', () => {
  it('only includes explicitly selected fields', () => {
    const selection = createPoolKeyBatchSettingSelection()
    const draft = createPoolKeyBatchSettingsDraft()
    selection.rpm_limit = true
    selection.proxy_node_id = true
    draft.rpm_limit = 800
    draft.proxy_mode = 'clear'

    expect(buildPoolKeySettingsPatch(selection, draft)).toEqual({
      rpm_limit: 800,
      proxy_node_id: null,
    })
  })

  it('supports adaptive RPM and unlimited concurrency', () => {
    const selection = createPoolKeyBatchSettingSelection()
    const draft = createPoolKeyBatchSettingsDraft()
    selection.rpm_limit = true
    selection.concurrent_limit = true

    expect(validatePoolKeyBatchSettings(selection, draft)).toEqual([])
    expect(buildPoolKeySettingsPatch(selection, draft)).toEqual({
      rpm_limit: null,
      concurrent_limit: null,
    })
  })

  /** 验证批量设置只把已选择的正数并发上限写入补丁。 */
  it('serializes a positive concurrency limit as a number', () => {
    const selection = createPoolKeyBatchSettingSelection()
    const draft = createPoolKeyBatchSettingsDraft()
    selection.concurrent_limit = true
    draft.concurrent_limit = 6

    expect(validatePoolKeyBatchSettings(selection, draft)).toEqual([])
    expect(buildPoolKeySettingsPatch(selection, draft)).toEqual({
      concurrent_limit: 6,
    })
  })

  it('requires a selected field and a proxy node for set mode', () => {
    const selection = createPoolKeyBatchSettingSelection()
    const draft = createPoolKeyBatchSettingsDraft()

    expect(validatePoolKeyBatchSettings(selection, draft)).toEqual(['请至少选择一个要修改的设置'])

    selection.proxy_node_id = true
    expect(validatePoolKeyBatchSettings(selection, draft)).toEqual(['请选择要设置的代理节点'])
  })

  it('validates and serializes weekly_quota_reserve_percent within 0..99', () => {
    const selection = createPoolKeyBatchSettingSelection()
    const draft = createPoolKeyBatchSettingsDraft()
    selection.weekly_quota_reserve_percent = true
    draft.weekly_quota_reserve_percent = 10

    expect(validatePoolKeyBatchSettings(selection, draft)).toEqual([])
    expect(buildPoolKeySettingsPatch(selection, draft)).toEqual({
      weekly_quota_reserve_percent: 10,
    })

    draft.weekly_quota_reserve_percent = 0
    expect(validatePoolKeyBatchSettings(selection, draft)).toEqual([])
    expect(buildPoolKeySettingsPatch(selection, draft)).toEqual({
      weekly_quota_reserve_percent: 0,
    })

    draft.weekly_quota_reserve_percent = 100
    expect(validatePoolKeyBatchSettings(selection, draft)).toEqual(['周限预留必须是 0 到 99 之间的整数'])
  })
})
