import { ref, computed, type Ref } from 'vue'
import { CalendarCheck, RotateCcw, RefreshCw, KeyRound, Cloud } from 'lucide-vue-next'
import { useToast } from '@/composables/useToast'
import { adminApi } from '@/api/admin'
import { log } from '@/utils/logger'
import type { SystemConfig } from './useSystemConfig'

export function useScheduledTasks(systemConfig: Ref<SystemConfig>) {
  const { success, error } = useToast()

  const checkinConfigLoading = ref(false)
  const allApiHubSyncConfigLoading = ref(false)
  const quotaResetConfigLoading = ref(false)
  const standaloneKeyResetConfigLoading = ref(false)

  // 签到时间的原始值（用于回滚）
  const previousCheckinTime = ref('')
  const previousAllApiHubSyncTime = ref('')
  const previousAllApiHubWebdavUrl = ref('')
  const previousAllApiHubWebdavUsername = ref('')
  const previousAllApiHubAutoCreateProviderOps = ref(true)
  // 用户配额重置时间的原始值
  const previousUserQuotaResetTime = ref('')
  const previousUserQuotaResetIntervalDays = ref(1)
  // 独立密钥额度重置的原始值
  const previousStandaloneKeyResetTime = ref('')
  const previousStandaloneKeyResetIntervalDays = ref(1)

  // 初始化原始值（在配置加载完成后调用）
  function initPreviousValues() {
    previousCheckinTime.value = systemConfig.value.provider_checkin_time
    previousAllApiHubSyncTime.value = systemConfig.value.all_api_hub_sync_time
    previousAllApiHubWebdavUrl.value = systemConfig.value.all_api_hub_webdav_url
    previousAllApiHubWebdavUsername.value = systemConfig.value.all_api_hub_webdav_username
    previousAllApiHubAutoCreateProviderOps.value =
      systemConfig.value.enable_all_api_hub_auto_create_provider_ops
    previousUserQuotaResetTime.value = systemConfig.value.user_quota_reset_time
    previousUserQuotaResetIntervalDays.value = systemConfig.value.user_quota_reset_interval_days
    previousStandaloneKeyResetTime.value = systemConfig.value.standalone_key_quota_reset_time
    previousStandaloneKeyResetIntervalDays.value = systemConfig.value.standalone_key_quota_reset_interval_days
  }

  // 签到时间
  const checkinHour = computed(() => {
    const time = systemConfig.value.provider_checkin_time
    if (!time || !time.includes(':')) return '01'
    return time.split(':')[0]
  })

  const checkinMinute = computed(() => {
    const time = systemConfig.value.provider_checkin_time
    if (!time || !time.includes(':')) return '05'
    return time.split(':')[1]
  })

  function updateCheckinTime(hour: string, minute: string) {
    systemConfig.value.provider_checkin_time = `${hour}:${minute}`
  }

  const hasCheckinTimeChanged = computed(() => {
    return systemConfig.value.provider_checkin_time !== previousCheckinTime.value
  })

  // all-api-hub 同步时间
  const allApiHubSyncHour = computed(() => {
    const time = systemConfig.value.all_api_hub_sync_time
    if (!time || !time.includes(':')) return '01'
    return time.split(':')[0]
  })

  const allApiHubSyncMinute = computed(() => {
    const time = systemConfig.value.all_api_hub_sync_time
    if (!time || !time.includes(':')) return '35'
    return time.split(':')[1]
  })

  function updateAllApiHubSyncTime(hour: string, minute: string) {
    systemConfig.value.all_api_hub_sync_time = `${hour}:${minute}`
  }

  const hasAllApiHubSyncConfigChanged = computed(() => {
    return (
      systemConfig.value.all_api_hub_sync_time !== previousAllApiHubSyncTime.value ||
      systemConfig.value.all_api_hub_webdav_url !== previousAllApiHubWebdavUrl.value ||
      systemConfig.value.all_api_hub_webdav_username !== previousAllApiHubWebdavUsername.value ||
      systemConfig.value.enable_all_api_hub_auto_create_provider_ops !==
      previousAllApiHubAutoCreateProviderOps.value ||
      !!systemConfig.value.all_api_hub_webdav_password
    )
  })

  // 用户配额重置时间
  const userQuotaResetHour = computed(() => {
    const time = systemConfig.value.user_quota_reset_time
    if (!time || !time.includes(':')) return '05'
    return time.split(':')[0]
  })

  const userQuotaResetMinute = computed(() => {
    const time = systemConfig.value.user_quota_reset_time
    if (!time || !time.includes(':')) return '00'
    return time.split(':')[1]
  })

  function updateUserQuotaResetTime(hour: string, minute: string) {
    systemConfig.value.user_quota_reset_time = `${hour}:${minute}`
  }

  const hasUserQuotaResetTimeChanged = computed(() => {
    return systemConfig.value.user_quota_reset_time !== previousUserQuotaResetTime.value
  })

  const hasUserQuotaResetIntervalChanged = computed(() => {
    return (
      systemConfig.value.user_quota_reset_interval_days !==
      previousUserQuotaResetIntervalDays.value
    )
  })

  const hasQuotaResetConfigChanged = computed(() => {
    return hasUserQuotaResetTimeChanged.value || hasUserQuotaResetIntervalChanged.value
  })

  // 独立密钥额度重置时间
  const standaloneKeyResetHour = computed(() => {
    const time = systemConfig.value.standalone_key_quota_reset_time
    if (!time || !time.includes(':')) return '05'
    return time.split(':')[0]
  })

  const standaloneKeyResetMinute = computed(() => {
    const time = systemConfig.value.standalone_key_quota_reset_time
    if (!time || !time.includes(':')) return '00'
    return time.split(':')[1]
  })

  function updateStandaloneKeyResetTime(hour: string, minute: string) {
    systemConfig.value.standalone_key_quota_reset_time = `${hour}:${minute}`
  }

  const hasStandaloneKeyResetTimeChanged = computed(() => {
    return systemConfig.value.standalone_key_quota_reset_time !== previousStandaloneKeyResetTime.value
  })

  const hasStandaloneKeyResetIntervalChanged = computed(() => {
    return (
      systemConfig.value.standalone_key_quota_reset_interval_days !==
      previousStandaloneKeyResetIntervalDays.value
    )
  })

  const hasStandaloneKeyResetConfigChanged = computed(() => {
    return hasStandaloneKeyResetTimeChanged.value || hasStandaloneKeyResetIntervalChanged.value
  })

  // Toggle handlers
  async function handleProviderCheckinToggle(enabled: boolean) {
    const previousValue = systemConfig.value.enable_provider_checkin
    systemConfig.value.enable_provider_checkin = enabled
    try {
      await adminApi.updateSystemConfig(
        'enable_provider_checkin',
        enabled,
        '是否启用 Provider 自动签到任务'
      )
      success(enabled ? '已启用自动签到' : '已禁用自动签到')
    } catch (err) {
      error('保存配置失败')
      log.error('保存自动签到配置失败:', err)
      systemConfig.value.enable_provider_checkin = previousValue
    }
  }

  async function handleUserQuotaResetToggle(enabled: boolean) {
    const previousValue = systemConfig.value.enable_user_quota_reset
    systemConfig.value.enable_user_quota_reset = enabled
    try {
      await adminApi.updateSystemConfig(
        'enable_user_quota_reset',
        enabled,
        '是否启用用户配额自动重置任务'
      )
      success(enabled ? '已启用用户配额自动重置' : '已禁用用户配额自动重置')
    } catch (err) {
      error('保存配置失败')
      log.error('保存用户配额自动重置配置失败:', err)
      systemConfig.value.enable_user_quota_reset = previousValue
    }
  }

  async function handleAllApiHubSyncToggle(enabled: boolean) {
    const previousValue = systemConfig.value.enable_all_api_hub_sync
    systemConfig.value.enable_all_api_hub_sync = enabled
    try {
      await adminApi.updateSystemConfig(
        'enable_all_api_hub_sync',
        enabled,
        '是否启用 all-api-hub WebDAV Cookie 自动同步任务'
      )
      success(enabled ? '已启用 all-api-hub 自动同步' : '已禁用 all-api-hub 自动同步')
    } catch (err) {
      error('保存配置失败')
      log.error('保存 all-api-hub 自动同步配置失败:', err)
      systemConfig.value.enable_all_api_hub_sync = previousValue
    }
  }

  async function handleOAuthTokenRefreshToggle(enabled: boolean) {
    const previousValue = systemConfig.value.enable_oauth_token_refresh
    systemConfig.value.enable_oauth_token_refresh = enabled
    try {
      await adminApi.updateSystemConfig(
        'enable_oauth_token_refresh',
        enabled,
        '是否启用 OAuth Token 自动刷新任务'
      )
      success(enabled ? '已启用 OAuth Token 自动刷新' : '已禁用 OAuth Token 自动刷新')
    } catch (err) {
      error('保存配置失败')
      log.error('保存 OAuth Token 自动刷新配置失败:', err)
      systemConfig.value.enable_oauth_token_refresh = previousValue
    }
  }

  async function handleStandaloneKeyResetToggle(enabled: boolean) {
    const previousValue = systemConfig.value.enable_standalone_key_quota_reset
    systemConfig.value.enable_standalone_key_quota_reset = enabled
    try {
      await adminApi.updateSystemConfig(
        'enable_standalone_key_quota_reset',
        enabled,
        '是否启用独立密钥额度自动重置任务'
      )
      success(enabled ? '已启用独立密钥额度自动重置' : '已禁用独立密钥额度自动重置')
    } catch (err) {
      error('保存配置失败')
      log.error('保存独立密钥额度自动重置配置失败:', err)
      systemConfig.value.enable_standalone_key_quota_reset = previousValue
    }
  }

  // Cancel handlers
  function handleCheckinTimeCancel() {
    systemConfig.value.provider_checkin_time = previousCheckinTime.value
  }

  function handleQuotaResetConfigCancel() {
    systemConfig.value.user_quota_reset_time = previousUserQuotaResetTime.value
    systemConfig.value.user_quota_reset_interval_days = previousUserQuotaResetIntervalDays.value
  }

  function handleAllApiHubSyncConfigCancel() {
    systemConfig.value.all_api_hub_sync_time = previousAllApiHubSyncTime.value
    systemConfig.value.all_api_hub_webdav_url = previousAllApiHubWebdavUrl.value
    systemConfig.value.all_api_hub_webdav_username = previousAllApiHubWebdavUsername.value
    systemConfig.value.enable_all_api_hub_auto_create_provider_ops =
      previousAllApiHubAutoCreateProviderOps.value
    systemConfig.value.all_api_hub_webdav_password = ''
  }

  function handleStandaloneKeyResetConfigCancel() {
    systemConfig.value.standalone_key_quota_reset_time = previousStandaloneKeyResetTime.value
    systemConfig.value.standalone_key_quota_reset_interval_days = previousStandaloneKeyResetIntervalDays.value
  }

  // Save handlers
  async function handleCheckinTimeSave() {
    const newTime = systemConfig.value.provider_checkin_time
    if (!newTime || !/^\d{2}:\d{2}$/.test(newTime)) {
      error('请输入有效的时间格式 (HH:MM)')
      return
    }

    checkinConfigLoading.value = true
    try {
      await adminApi.updateSystemConfig(
        'provider_checkin_time',
        newTime,
        'Provider 自动签到执行时间（HH:MM 格式）'
      )
      previousCheckinTime.value = newTime
      success(`签到时间已设置为 ${newTime}`)
    } catch (err) {
      error('保存签到时间失败')
      log.error('保存签到时间失败:', err)
    } finally {
      checkinConfigLoading.value = false
    }
  }

  async function handleQuotaResetConfigSave() {
    const configItems: Array<{
      key: string
      value: unknown
      description: string
      onSuccess: () => void
    }> = []

    if (hasUserQuotaResetTimeChanged.value) {
      const newTime = systemConfig.value.user_quota_reset_time
      if (!newTime || !/^\d{2}:\d{2}$/.test(newTime)) {
        error('请输入有效的时间格式 (HH:MM)')
        return
      }
      configItems.push({
        key: 'user_quota_reset_time',
        value: newTime,
        description: '用户配额自动重置执行时间（HH:MM 格式）',
        onSuccess: () => {
          previousUserQuotaResetTime.value = newTime
        },
      })
    }

    if (hasUserQuotaResetIntervalChanged.value) {
      let intervalDays = Number(systemConfig.value.user_quota_reset_interval_days)
      if (!Number.isFinite(intervalDays) || intervalDays < 1) intervalDays = 1
      intervalDays = Math.trunc(intervalDays)

      systemConfig.value.user_quota_reset_interval_days = intervalDays

      configItems.push({
        key: 'user_quota_reset_interval_days',
        value: intervalDays,
        description: '用户配额重置周期（天数），滚动计算',
        onSuccess: () => {
          previousUserQuotaResetIntervalDays.value = intervalDays
        },
      })
    }

    if (configItems.length === 0) return

    quotaResetConfigLoading.value = true
    const failedKeys: string[] = []

    try {
      for (const item of configItems) {
        try {
          await adminApi.updateSystemConfig(item.key, item.value, item.description)
          item.onSuccess()
        } catch (err) {
          failedKeys.push(item.key)
          log.error(`保存配额重置配置失败: ${item.key}`, err)
        }
      }

      if (failedKeys.length > 0) {
        error(`部分配置保存失败: ${failedKeys.join(', ')}`)
        return
      }

      success('配额重置配置已保存')
    } finally {
      quotaResetConfigLoading.value = false
    }
  }

  async function handleAllApiHubSyncConfigSave() {
    const configItems: Array<{
      key: string
      value: unknown
      description: string
      onSuccess: () => void
    }> = []

    if (systemConfig.value.all_api_hub_sync_time !== previousAllApiHubSyncTime.value) {
      const newTime = systemConfig.value.all_api_hub_sync_time
      if (!newTime || !/^\d{2}:\d{2}$/.test(newTime)) {
        error('请输入有效的时间格式 (HH:MM)')
        return
      }
      configItems.push({
        key: 'all_api_hub_sync_time',
        value: newTime,
        description: 'all-api-hub WebDAV Cookie 自动同步执行时间（HH:MM 格式）',
        onSuccess: () => {
          previousAllApiHubSyncTime.value = newTime
        },
      })
    }

    if (systemConfig.value.all_api_hub_webdav_url !== previousAllApiHubWebdavUrl.value) {
      const url = systemConfig.value.all_api_hub_webdav_url.trim()
      systemConfig.value.all_api_hub_webdav_url = url
      configItems.push({
        key: 'all_api_hub_webdav_url',
        value: url || null,
        description: 'all-api-hub WebDAV 备份文件 URL',
        onSuccess: () => {
          previousAllApiHubWebdavUrl.value = url
        },
      })
    }

    if (
      systemConfig.value.all_api_hub_webdav_username !== previousAllApiHubWebdavUsername.value
    ) {
      const username = systemConfig.value.all_api_hub_webdav_username.trim()
      systemConfig.value.all_api_hub_webdav_username = username
      configItems.push({
        key: 'all_api_hub_webdav_username',
        value: username || null,
        description: 'all-api-hub WebDAV 用户名',
        onSuccess: () => {
          previousAllApiHubWebdavUsername.value = username
        },
      })
    }

    if (systemConfig.value.all_api_hub_webdav_password) {
      configItems.push({
        key: 'all_api_hub_webdav_password',
        value: systemConfig.value.all_api_hub_webdav_password,
        description: 'all-api-hub WebDAV 密码（加密存储）',
        onSuccess: () => {
          systemConfig.value.all_api_hub_webdav_password = ''
        },
      })
    }

    if (
      systemConfig.value.enable_all_api_hub_auto_create_provider_ops !==
      previousAllApiHubAutoCreateProviderOps.value
    ) {
      const enabled = systemConfig.value.enable_all_api_hub_auto_create_provider_ops
      configItems.push({
        key: 'enable_all_api_hub_auto_create_provider_ops',
        value: enabled,
        description: 'all-api-hub 同步时是否自动创建缺失 provider_ops 配置',
        onSuccess: () => {
          previousAllApiHubAutoCreateProviderOps.value = enabled
        },
      })
    }

    if (configItems.length === 0) return

    allApiHubSyncConfigLoading.value = true
    const failedKeys: string[] = []

    try {
      for (const item of configItems) {
        try {
          await adminApi.updateSystemConfig(item.key, item.value, item.description)
          item.onSuccess()
        } catch (err) {
          failedKeys.push(item.key)
          log.error(`保存 all-api-hub 同步配置失败: ${item.key}`, err)
        }
      }

      if (failedKeys.length > 0) {
        error(`部分配置保存失败: ${failedKeys.join(', ')}`)
        return
      }

      success('all-api-hub 同步配置已保存')
    } finally {
      allApiHubSyncConfigLoading.value = false
    }
  }

  async function handleStandaloneKeyResetConfigSave() {
    const configItems: Array<{
      key: string
      value: unknown
      description: string
      onSuccess: () => void
    }> = []

    if (hasStandaloneKeyResetTimeChanged.value) {
      const newTime = systemConfig.value.standalone_key_quota_reset_time
      if (!newTime || !/^\d{2}:\d{2}$/.test(newTime)) {
        error('请输入有效的时间格式 (HH:MM)')
        return
      }
      configItems.push({
        key: 'standalone_key_quota_reset_time',
        value: newTime,
        description: '独立密钥额度自动重置执行时间（HH:MM 格式）',
        onSuccess: () => {
          previousStandaloneKeyResetTime.value = newTime
        },
      })
    }

    if (hasStandaloneKeyResetIntervalChanged.value) {
      let intervalDays = Number(systemConfig.value.standalone_key_quota_reset_interval_days)
      if (!Number.isFinite(intervalDays) || intervalDays < 1) intervalDays = 1
      intervalDays = Math.trunc(intervalDays)

      systemConfig.value.standalone_key_quota_reset_interval_days = intervalDays

      configItems.push({
        key: 'standalone_key_quota_reset_interval_days',
        value: intervalDays,
        description: '独立密钥额度重置周期（天数），滚动计算',
        onSuccess: () => {
          previousStandaloneKeyResetIntervalDays.value = intervalDays
        },
      })
    }

    if (configItems.length === 0) return

    standaloneKeyResetConfigLoading.value = true
    const failedKeys: string[] = []

    try {
      for (const item of configItems) {
        try {
          await adminApi.updateSystemConfig(item.key, item.value, item.description)
          item.onSuccess()
        } catch (err) {
          failedKeys.push(item.key)
          log.error(`保存独立密钥额度重置配置失败: ${item.key}`, err)
        }
      }

      if (failedKeys.length > 0) {
        error(`部分配置保存失败: ${failedKeys.join(', ')}`)
        return
      }

      success('独立密钥额度重置配置已保存')
    } finally {
      standaloneKeyResetConfigLoading.value = false
    }
  }

  // 保存独立密钥额度重置模式和选中密钥
  async function saveStandaloneKeyResetMode(mode: string) {
    try {
      await adminApi.updateSystemConfig(
        'standalone_key_quota_reset_mode',
        mode,
        '独立密钥额度重置模式'
      )
      success('重置模式已保存')
    } catch (err) {
      error('保存重置模式失败')
      log.error('保存独立密钥额度重置模式失败:', err)
    }
  }

  async function saveStandaloneKeyResetKeyIds(keyIds: string[]) {
    try {
      await adminApi.updateSystemConfig(
        'standalone_key_quota_reset_key_ids',
        keyIds,
        '独立密钥额度重置指定的密钥 ID 列表'
      )
      success('已保存选中密钥')
    } catch (err) {
      error('保存选中密钥失败')
      log.error('保存独立密钥额度重置密钥列表失败:', err)
    }
  }

  // 定时任务配置列表
  const scheduledTasks = computed(() => [
    {
      id: 'provider-checkin',
      icon: CalendarCheck,
      title: 'Provider 自动签到',
      description: '自动执行已配置 Provider 的签到任务',
      enabled: systemConfig.value.enable_provider_checkin,
      hasTimeConfig: true,
      hour: checkinHour.value,
      minute: checkinMinute.value,
      updateTime: updateCheckinTime,
      hasChanges: hasCheckinTimeChanged.value,
      loading: checkinConfigLoading.value,
      onToggle: handleProviderCheckinToggle,
      onSave: handleCheckinTimeSave,
      onCancel: handleCheckinTimeCancel,
    },
    {
      id: 'all-api-hub-sync',
      icon: Cloud,
      title: 'all-api-hub Cookie 自动同步',
      description: '定时从 WebDAV 拉取备份并按域名匹配更新 Cookie',
      enabled: systemConfig.value.enable_all_api_hub_sync,
      hasTimeConfig: true,
      hour: allApiHubSyncHour.value,
      minute: allApiHubSyncMinute.value,
      updateTime: updateAllApiHubSyncTime,
      hasChanges: hasAllApiHubSyncConfigChanged.value,
      loading: allApiHubSyncConfigLoading.value,
      onToggle: handleAllApiHubSyncToggle,
      onSave: handleAllApiHubSyncConfigSave,
      onCancel: handleAllApiHubSyncConfigCancel,
    },
    {
      id: 'user-quota-reset',
      icon: RotateCcw,
      title: '用户配额自动重置',
      description: '定时将用户已使用配额重置为零',
      enabled: systemConfig.value.enable_user_quota_reset,
      hasTimeConfig: true,
      hour: userQuotaResetHour.value,
      minute: userQuotaResetMinute.value,
      updateTime: updateUserQuotaResetTime,
      hasChanges: hasQuotaResetConfigChanged.value,
      loading: quotaResetConfigLoading.value,
      onToggle: handleUserQuotaResetToggle,
      onSave: handleQuotaResetConfigSave,
      onCancel: handleQuotaResetConfigCancel,
    },
    {
      id: 'oauth-token-refresh',
      icon: RefreshCw,
      title: 'OAuth Token 自动刷新',
      description: '主动刷新即将过期的 OAuth Token（动态调度）',
      enabled: systemConfig.value.enable_oauth_token_refresh,
      hasTimeConfig: false,
      hour: '',
      minute: '',
      updateTime: () => {},
      hasChanges: false,
      loading: false,
      onToggle: handleOAuthTokenRefreshToggle,
      onSave: () => {},
      onCancel: () => {},
    },
    {
      id: 'standalone-key-quota-reset',
      icon: KeyRound,
      title: '独立密钥额度自动重置',
      description: '定时将独立密钥已使用额度重置为零',
      enabled: systemConfig.value.enable_standalone_key_quota_reset,
      hasTimeConfig: true,
      hour: standaloneKeyResetHour.value,
      minute: standaloneKeyResetMinute.value,
      updateTime: updateStandaloneKeyResetTime,
      hasChanges: hasStandaloneKeyResetConfigChanged.value,
      loading: standaloneKeyResetConfigLoading.value,
      onToggle: handleStandaloneKeyResetToggle,
      onSave: handleStandaloneKeyResetConfigSave,
      onCancel: handleStandaloneKeyResetConfigCancel,
    },
  ])

  return {
    checkinConfigLoading,
    allApiHubSyncConfigLoading,
    quotaResetConfigLoading,
    standaloneKeyResetConfigLoading,
    scheduledTasks,
    initPreviousValues,
    saveStandaloneKeyResetMode,
    saveStandaloneKeyResetKeyIds,
  }
}
