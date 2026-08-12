<template>
  <Card class="overflow-hidden">
    <!-- 标题栏 -->
    <div class="px-4 py-3 border-b border-border/60">
      <div class="flex items-center justify-between gap-4">
        <div class="flex items-baseline gap-2">
          <h4 class="text-sm font-semibold">
            链路预览
          </h4>
          <template v-if="routingData">
            <span class="text-xs text-muted-foreground">·</span>
            <span class="text-xs text-muted-foreground">
              {{ getSchedulingModeLabel(routingData.scheduling_mode) }}
            </span>
            <span class="text-xs text-muted-foreground">·</span>
            <span class="text-xs text-muted-foreground">
              {{ getPriorityModeLabel(routingData.priority_mode) }}
            </span>
          </template>
        </div>
        <div class="flex items-center gap-2">
          <Button
            variant="ghost"
            size="icon"
            class="h-8 w-8"
            title="关联提供商"
            @click="$emit('addProvider')"
          >
            <Link class="w-3.5 h-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            class="h-8 w-8"
            title="刷新"
            @click="loadRoutingData"
          >
            <RefreshCw
              class="w-3.5 h-3.5"
              :class="loading ? 'animate-spin' : ''"
            />
          </Button>
        </div>
      </div>
    </div>

    <!-- 加载状态 -->
    <div
      v-if="loading"
      class="flex items-center justify-center py-12"
    >
      <Loader2 class="w-6 h-6 animate-spin text-primary" />
    </div>

    <template v-else-if="routingData">
      <div class="p-4 space-y-4">
        <!-- 空状态 -->
        <div
          v-if="apiFormatGroups.length === 0"
          class="text-center py-8"
        >
          <Route class="w-10 h-10 mx-auto text-muted-foreground/30 mb-3" />
          <p class="text-sm text-muted-foreground">
            暂无关联提供商
          </p>
          <p class="text-xs text-muted-foreground mt-1">
            请先为此模型添加提供商关联
          </p>
        </div>

        <!-- API 格式分组 -->
        <div
          v-for="formatGroup in apiFormatGroups"
          :key="formatGroup.api_format"
          data-testid="routing-format-group"
          :data-api-format="formatGroup.api_format"
          class="border border-border/60 rounded-lg overflow-hidden"
        >
          <!-- 格式标题栏 -->
          <div
            data-testid="routing-format-header"
            class="px-4 py-3 bg-muted/30 flex items-center justify-between cursor-pointer hover:bg-muted/50 transition-colors"
            @click="toggleFormat(formatGroup.api_format)"
          >
            <div class="flex items-center gap-3">
              <Badge
                variant="secondary"
                class="text-xs font-semibold px-2.5 py-1"
              >
                {{ formatApiFormat(formatGroup.api_format) }}
              </Badge>
            </div>
            <div class="flex items-center gap-3">
              <span class="text-sm text-muted-foreground">
                {{ formatGroup.active_keys }}/{{ formatGroup.total_keys }} Keys
                <span class="mx-1.5">·</span>
                {{ formatGroup.active_providers }}/{{ formatGroup.total_providers }} 提供商
              </span>
              <ChevronDown
                class="w-4 h-4 text-muted-foreground transition-transform"
                :class="isFormatExpanded(formatGroup.api_format) ? 'rotate-180' : ''"
              />
            </div>
          </div>

          <!-- 展开的内容 -->
          <Transition name="collapse">
            <div v-if="isFormatExpanded(formatGroup.api_format)">
              <!-- ========== 全局 Key 优先模式 ========== -->
              <template v-if="isGlobalKeyMode">
                <div class="px-3 pt-2 space-y-1.5">
                  <template
                    v-for="providerEntry in formatGroup.providers"
                    :key="`${providerEntry.provider.id}-${providerEntry.provider.model_id}-${providerEntry.endpoint?.id || 'endpoint'}`"
                  >
                    <div
                      v-if="providerEntry.endpoint"
                      data-testid="endpoint-routing-card"
                      :data-endpoint-id="providerEntry.endpoint.id"
                      class="flex items-start gap-3 rounded-lg border px-3 py-2"
                      :class="isEndpointBindingActive(providerEntry.provider, providerEntry.endpoint)
                        ? 'border-border/50 bg-muted/15'
                        : 'border-border/40 bg-muted/30 opacity-70'"
                    >
                      <div class="min-w-0 flex-1 space-y-0.5">
                        <div class="flex flex-wrap items-center gap-1.5">
                          <span class="text-xs font-medium truncate">{{ providerEntry.provider.name }}</span>
                          <Badge
                            variant="outline"
                            class="text-[9px] px-1.5 py-0 shrink-0"
                          >
                            {{ formatApiFormat(providerEntry.endpoint.api_format) }}
                          </Badge>
                          <span
                            v-if="providerEntry.is_cross_format"
                            class="text-[9px] text-amber-600 dark:text-amber-400"
                          >
                            格式转换链路
                          </span>
                          <span
                            v-if="!isEndpointBindingActive(providerEntry.provider, providerEntry.endpoint)"
                            class="text-[9px] text-muted-foreground"
                          >
                            当前模型已禁用
                          </span>
                        </div>
                        <div class="text-[10px] text-muted-foreground break-all">
                          {{ formatEndpointUrl(providerEntry.endpoint) }}
                        </div>
                        <div class="text-[9px] text-muted-foreground/70 break-all">
                          Endpoint ID · {{ providerEntry.endpoint.id }} · 绑定来源 · {{ getBindingSourceLabel(providerEntry.provider, providerEntry.endpoint) }}
                        </div>
                        <div
                          v-if="(providerEntry.endpoint.runtime_capability_quarantines || []).length > 0"
                          class="text-[9px] text-amber-600 dark:text-amber-400"
                        >
                          运行时能力暂不可用 · {{ formatRuntimeCapabilityQuarantines(providerEntry.endpoint) }}
                        </div>
                      </div>
                      <div
                        data-testid="endpoint-binding-control"
                        class="flex items-center gap-2 shrink-0"
                        title="仅控制当前模型与此 Endpoint 的绑定"
                        @click.stop
                      >
                        <span class="text-[10px] font-medium whitespace-nowrap">Endpoint 链路</span>
                        <Switch
                          :model-value="isEndpointBindingActive(providerEntry.provider, providerEntry.endpoint)"
                          :disabled="isBindingSaving(providerEntry.provider, providerEntry.endpoint)"
                          :aria-label="`允许当前模型使用 ${providerEntry.provider.name} 的 Endpoint ${providerEntry.endpoint.id}`"
                          @update:model-value="handleEndpointBindingToggle(providerEntry.provider, providerEntry.endpoint, $event)"
                        />
                      </div>
                    </div>
                  </template>
                </div>
                <div class="py-2 pl-3">
                  <template
                    v-for="(keyGroup, groupIndex) in formatGroup.keyGroups"
                    :key="groupIndex"
                  >
                    <!-- 第一组且有多个 key 时显示调度行为标签 -->
                    <div
                      v-if="groupIndex === 0 && keyGroup.keys.length > 1"
                      class="ml-6 mr-3 mb-1 flex items-center gap-1 text-[10px] text-muted-foreground/60"
                    >
                      <span>{{ samePriorityLabel }}</span>
                    </div>

                    <!-- 该优先级组内的 Keys -->
                    <div
                      v-for="(keyEntry, keyIndex) in keyGroup.keys"
                      :key="keyEntry.key.id"
                      class="flex py-1"
                    >
                      <!-- 左侧：节点 + 连线 -->
                      <div class="w-6 flex flex-col items-center shrink-0">
                        <!-- 上半段连线 -->
                        <div
                          class="w-0.5 flex-1"
                          :class="groupIndex === 0 && keyIndex === 0 ? 'bg-transparent' : 'bg-border'"
                        />
                        <!-- 节点圆点 -->
                        <div
                          class="w-3 h-3 rounded-full border-2 shrink-0"
                          :class="getGlobalKeyNodeClass(keyEntry, groupIndex, keyIndex)"
                        />
                        <!-- 下半段连线 -->
                        <div
                          class="w-0.5 flex-1"
                          :class="isLastKeyInFormat(formatGroup, groupIndex, keyIndex) ? 'bg-transparent' : 'bg-border'"
                        />
                      </div>

                      <!-- Key 卡片 -->
                      <div
                        class="flex-1 mr-3"
                        :class="!keyEntry.key.is_active ? 'opacity-50' : ''"
                      >
                        <div
                          class="group rounded-lg transition-all px-3 py-2"
                          :class="getGlobalKeyCardClass(keyEntry, groupIndex, keyIndex)"
                        >
                          <div class="flex items-center gap-3">
                            <!-- 优先级标签 -->
                            <div
                              v-if="keyEntry.key.is_active"
                              class="px-1.5 py-0.5 rounded text-[10px] font-medium shrink-0"
                              :class="groupIndex === 0 && keyIndex === 0
                                ? 'bg-primary text-primary-foreground'
                                : 'bg-muted-foreground/20 text-muted-foreground'"
                            >
                              <span v-if="groupIndex === 0 && keyIndex === 0">首选</span>
                              <span v-else>P{{ keyGroup.priority ?? '?' }}</span>
                            </div>

                            <!-- Key 信息：两行 -->
                            <div class="min-w-0 flex-1">
                              <!-- 第一行：Key 名称 -->
                              <div
                                class="text-sm font-medium truncate"
                                :class="keyEntry.key.circuit_breaker_open ? 'text-destructive' : ''"
                              >
                                {{ keyEntry.key.name }}
                              </div>
                              <!-- 第二行：提供商名 · sk · 模型映射 -->
                              <div class="flex items-center gap-1 text-[10px] text-muted-foreground">
                                <span>{{ keyEntry.provider.name }}</span>
                                <span>·</span>
                                <code class="text-muted-foreground/60">{{ keyEntry.key.masked_key }}</code>
                                <!-- Key 白名单映射显示 -->
                                <template v-if="getKeyMatchedModels(keyEntry.key).length > 0">
                                  <span>·</span>
                                  <span
                                    class="text-primary/70"
                                    :title="getKeyMatchedModels(keyEntry.key).join(', ')"
                                  >{{ formatMatchedModels(getKeyMatchedModels(keyEntry.key)) }}</span>
                                </template>
                                <!-- Provider 模型映射显示 -->
                                <template v-else-if="hasModelMapping(keyEntry.provider)">
                                  <span>·</span>
                                  <span class="text-primary/70">{{ keyEntry.provider.provider_model_name }}</span>
                                </template>
                              </div>
                            </div>

                            <!-- 熔断徽章 -->
                            <Badge
                              v-if="keyEntry.key.circuit_breaker_open"
                              variant="destructive"
                              class="text-[10px] px-1.5 py-0 shrink-0 tabular-nums"
                            >
                              熔断{{ getKeyProbeCountdown(keyEntry.key) }}
                            </Badge>

                            <!-- 健康度 -->
                            <div class="flex items-center gap-1 shrink-0">
                              <div class="w-10 h-1.5 bg-muted/80 rounded-full overflow-hidden">
                                <div
                                  class="h-full transition-all duration-300"
                                  :class="getHealthScoreBarColor(keyEntry.key.health_score)"
                                  :style="{ width: `${(keyEntry.key.health_score || 0) * 100}%` }"
                                />
                              </div>
                              <span
                                class="text-[10px] font-medium tabular-nums w-7 text-right"
                                :class="getHealthScoreTextColor(keyEntry.key.health_score)"
                              >
                                {{ ((keyEntry.key.health_score || 0) * 100).toFixed(0) }}%
                              </span>
                            </div>

                            <!-- 操作按钮 -->
                            <div class="flex items-center shrink-0">
                              <Button
                                v-if="keyEntry.key.circuit_breaker_open || (keyEntry.key.health_score ?? 1) < 0.5"
                                variant="ghost"
                                size="icon"
                                class="h-6 w-6 text-green-600"
                                title="刷新健康状态"
                                @click.stop="handleRecoverKey(keyEntry.key.id, keyEntry.endpoint?.api_format || formatGroup.api_format)"
                              >
                                <RefreshCw class="w-3 h-3" />
                              </Button>
                              <Button
                                variant="ghost"
                                size="icon"
                                class="h-6 w-6"
                                :title="keyEntry.provider.model_is_active ? '停用此关联' : '启用此关联'"
                                @click.stop="$emit('toggleProviderStatus', keyEntry.provider)"
                              >
                                <Power class="w-3 h-3" />
                              </Button>
                            </div>
                          </div>
                        </div>
                      </div>
                    </div>

                    <!-- 降级标记 -->
                    <div
                      v-if="groupIndex < formatGroup.keyGroups.length - 1"
                      class="flex py-0.5"
                    >
                      <div class="w-6 flex justify-center shrink-0">
                        <div class="w-0.5 h-full bg-border" />
                      </div>
                      <div class="flex items-center gap-1 text-[10px] text-muted-foreground/50">
                        <ArrowDown class="w-3 h-3" />
                        <span>
                          {{ getDemoteLabel(formatGroup.keyGroups[groupIndex + 1].keys.length) }}
                        </span>
                      </div>
                    </div>
                  </template>
                </div>
              </template>

              <!-- ========== 提供商优先模式 ========== -->
              <template v-else>
                <div class="py-2 pl-3">
                  <div
                    v-for="(providerEntry, providerIndex) in formatGroup.providers"
                    :key="`${providerEntry.provider.id}-${providerEntry.provider.model_id}-${providerEntry.endpoint?.id || providerIndex}`"
                  >
                    <!-- 提供商行 -->
                    <div class="flex py-1">
                      <!-- 左侧：节点 + 连线 -->
                      <div class="w-6 flex flex-col items-center shrink-0 self-stretch">
                        <!-- 上半段连线 -->
                        <div
                          class="w-0.5 h-5"
                          :class="providerIndex === 0 ? 'bg-transparent' : 'bg-border'"
                        />
                        <!-- 节点圆点 -->
                        <div
                          class="w-3 h-3 rounded-full border-2 shrink-0"
                          :class="getFormatProviderNodeClass(providerEntry, providerIndex)"
                        />
                        <!-- 下半段连线（延伸到底部） -->
                        <div
                          class="w-0.5 flex-1"
                          :class="providerIndex === formatGroup.providers.length - 1 ? 'bg-transparent' : 'bg-border'"
                        />
                      </div>

                      <!-- 提供商卡片 -->
                      <div
                        class="flex-1 mr-3"
                        :class="!isFormatProviderEntryActive(providerEntry) ? 'opacity-50' : ''"
                      >
                        <div
                          data-testid="endpoint-routing-card"
                          :data-endpoint-id="providerEntry.endpoint?.id"
                          class="group rounded-lg transition-all"
                          :class="getFormatProviderCardClass(providerEntry, providerIndex)"
                        >
                          <!-- 卡片头部 -->
                          <div
                            data-testid="endpoint-routing-card-header"
                            class="p-2.5 cursor-pointer"
                            @click="toggleProviderInFormat(formatGroup.api_format, providerEntry.provider.id, providerEntry.provider.model_id, providerEntry.endpoint?.id)"
                          >
                            <div class="flex items-center gap-2">
                              <!-- 第一列：优先级标签（固定宽度，用于对齐） -->
                              <div
                                v-if="isFormatProviderEntryActive(providerEntry)"
                                data-testid="endpoint-priority-badge"
                                class="min-w-8 px-1.5 py-0.5 rounded-full text-[10px] font-medium shrink-0 text-center"
                                :class="providerIndex === 0
                                  ? 'bg-primary text-primary-foreground'
                                  : 'bg-muted-foreground/20 text-muted-foreground'"
                              >
                                <span v-if="providerIndex === 0">首选</span>
                                <span v-else>P{{ providerEntry.provider.provider_priority }}</span>
                              </div>

                              <!-- 第二列：状态指示灯 -->
                              <span
                                data-testid="endpoint-status-dot"
                                class="w-1.5 h-1.5 rounded-full shrink-0"
                                :class="getFormatProviderStatusClass(providerEntry)"
                              />

                              <!-- 第三列：名称(第一行) + URL(第二行) -->
                              <div class="min-w-0 flex-1">
                                <!-- 第一行：提供商名称 + 模型映射 -->
                                <div class="flex items-center gap-1">
                                  <span class="text-sm font-medium truncate">{{ providerEntry.provider.name }}</span>
                                  <span
                                    v-if="hasModelMapping(providerEntry.provider)"
                                    class="text-[10px] text-muted-foreground shrink-0"
                                  >
                                    ({{ providerEntry.provider.provider_model_name }})
                                  </span>
                                </div>
                                <!-- 第二行：Endpoint URL -->
                                <div
                                  v-if="providerEntry.endpoint"
                                  class="text-[10px] text-muted-foreground truncate"
                                >
                                  {{ formatEndpointUrl(providerEntry.endpoint) }}
                                </div>
                                <div
                                  v-if="providerEntry.endpoint"
                                  class="flex flex-wrap items-center gap-1 text-[9px] text-muted-foreground/70"
                                >
                                  <span>{{ formatApiFormat(providerEntry.endpoint.api_format) }}</span>
                                  <span>·</span>
                                  <span>Endpoint ID · {{ providerEntry.endpoint.id }}</span>
                                  <span>·</span>
                                  <span>绑定来源 · {{ getBindingSourceLabel(providerEntry.provider, providerEntry.endpoint) }}</span>
                                  <template v-if="!isEndpointBindingActive(providerEntry.provider, providerEntry.endpoint)">
                                    <span>·</span>
                                    <span>当前模型已禁用</span>
                                  </template>
                                </div>
                                <div
                                  v-if="providerEntry.endpoint && (providerEntry.endpoint.runtime_capability_quarantines || []).length > 0"
                                  class="text-[9px] text-amber-600 dark:text-amber-400"
                                >
                                  运行时能力暂不可用 · {{ formatRuntimeCapabilityQuarantines(providerEntry.endpoint) }}
                                </div>
                              </div>

                              <!-- 第四列：操作区域 -->
                              <div class="flex items-center gap-1 shrink-0">
                                <div
                                  v-if="providerEntry.endpoint"
                                  data-testid="endpoint-binding-control"
                                  class="flex items-center gap-1.5 rounded-md border border-border/50 bg-background/60 px-1.5 py-0.5 mr-1"
                                  title="仅控制当前模型与此 Endpoint 的绑定"
                                  @click.stop
                                >
                                  <span class="text-[9px] font-medium whitespace-nowrap">Endpoint 链路</span>
                                  <Switch
                                    :model-value="isEndpointBindingActive(providerEntry.provider, providerEntry.endpoint)"
                                    :disabled="isBindingSaving(providerEntry.provider, providerEntry.endpoint)"
                                    :aria-label="`允许当前模型使用 ${providerEntry.provider.name} 的 Endpoint ${providerEntry.endpoint.id}`"
                                    @update:model-value="handleEndpointBindingToggle(providerEntry.provider, providerEntry.endpoint, $event)"
                                  />
                                </div>
                                <!-- 计费标签 -->
                                <span
                                  v-if="providerEntry.provider.billing_type"
                                  class="text-[10px] text-muted-foreground mr-1"
                                >
                                  {{ getBillingLabel(providerEntry.provider) }}
                                </span>
                                <!-- Keys 统计 -->
                                <span class="text-[10px] text-muted-foreground">
                                  {{ providerEntry.active_keys }}/{{ providerEntry.keys.length }} Keys
                                </span>
                                <!-- 操作按钮 -->
                                <Button
                                  variant="ghost"
                                  size="icon"
                                  class="h-6 w-6"
                                  :title="providerEntry.provider.model_is_active ? '停用此关联' : '启用此关联'"
                                  @click.stop="$emit('toggleProviderStatus', providerEntry.provider)"
                                >
                                  <Power class="w-3 h-3" />
                                </Button>
                                <!-- 展开图标 -->
                                <ChevronDown
                                  class="w-3.5 h-3.5 text-muted-foreground transition-transform"
                                  :class="isProviderInFormatExpanded(formatGroup.api_format, providerEntry.provider.id, providerEntry.provider.model_id, providerEntry.endpoint?.id) ? 'rotate-180' : ''"
                                />
                              </div>
                            </div>
                          </div>

                          <!-- 展开的 Keys 详情 -->
                          <Transition name="collapse">
                            <div
                              v-if="isProviderInFormatExpanded(formatGroup.api_format, providerEntry.provider.id, providerEntry.provider.model_id, providerEntry.endpoint?.id)"
                              data-testid="endpoint-routing-key-details"
                              class="border-t border-border/30 p-2.5"
                            >
                              <!-- 模型映射显示 -->
                              <div
                                v-if="hasModelMapping(providerEntry.provider)"
                                class="flex items-center gap-1.5 text-[10px] text-muted-foreground mb-2 px-1"
                              >
                                <span class="text-muted-foreground/60">映射:</span>
                                <span class="text-primary/70 font-medium">{{ providerEntry.provider.provider_model_name }}</span>
                              </div>
                              <!-- Keys 列表 -->
                              <div
                                v-if="providerEntry.keys.length > 0"
                                class="relative"
                              >
                                <div
                                  v-for="(group, groupIndex) in getKeyPriorityGroups(providerEntry.keys)"
                                  :key="groupIndex"
                                >
                                  <!-- 第一组且有多个 key 时显示调度行为标签 -->
                                  <div
                                    v-if="groupIndex === 0 && group.keys.length > 1"
                                    class="flex items-center gap-1 text-[10px] text-muted-foreground/60 mb-0.5 ml-4"
                                  >
                                    <span>{{ samePriorityLabel }}</span>
                                  </div>

                                  <!-- 该优先级组内的 Keys -->
                                  <div class="relative">
                                    <!-- 垂直主干线 -->
                                    <div
                                      v-if="group.keys.length > 1"
                                      class="absolute left-1 top-2 w-px bg-border"
                                      :style="{ height: `calc(100% - 1rem)` }"
                                    />

                                    <div
                                      v-for="(key, keyIndex) in group.keys"
                                      :key="key.id"
                                      class="relative flex items-center gap-2"
                                    >
                                      <!-- 第一列：节点（与优先级标签对齐，min-w-8） -->
                                      <div class="min-w-8 flex items-center justify-center shrink-0">
                                        <div
                                          class="w-2 h-2 rounded-full border-2 z-10"
                                          :class="groupIndex === 0 && keyIndex === 0
                                            ? 'bg-primary border-primary'
                                            : 'bg-background border-muted-foreground/40'"
                                        />
                                      </div>

                                      <!-- 第二列：状态灯（与提供商状态灯对齐） -->
                                      <span
                                        class="w-1.5 h-1.5 rounded-full shrink-0"
                                        :class="getKeyStatusClass(key)"
                                      />

                                      <!-- 第三列：Key 信息 -->
                                      <div
                                        class="flex-1 min-w-0 flex items-center gap-1.5 px-2 py-1 my-0.5 rounded text-xs"
                                        :class="[
                                          groupIndex === 0 ? 'bg-primary/5' : 'bg-muted/30',
                                          !key.is_active ? 'opacity-50' : ''
                                        ]"
                                        :title="getKeyTooltip(key)"
                                      >
                                        <!-- 名称 + sk + 映射（垂直堆叠） -->
                                        <div class="min-w-0 flex flex-col">
                                          <span
                                            class="font-medium truncate"
                                            :class="key.circuit_breaker_open ? 'text-destructive' : ''"
                                          >
                                            {{ key.name }}
                                          </span>
                                          <div class="flex items-center gap-1">
                                            <code class="font-mono text-[10px] text-muted-foreground/60">
                                              {{ key.masked_key }}
                                            </code>
                                            <template v-if="getKeyMatchedModels(key).length > 0">
                                              <span class="text-[10px] text-muted-foreground/40">·</span>
                                              <span
                                                class="text-[10px] text-primary/70"
                                                :title="getKeyMatchedModels(key).join(', ')"
                                              >{{ formatMatchedModels(getKeyMatchedModels(key)) }}</span>
                                            </template>
                                          </div>
                                        </div>
                                        <span class="flex-1" />
                                        <!-- 熔断徽章（带倒计时）- 靠右 -->
                                        <Badge
                                          v-if="key.circuit_breaker_open"
                                          variant="destructive"
                                          class="text-[9px] px-1 py-0 h-4 shrink-0 tabular-nums"
                                        >
                                          熔断{{ getKeyProbeCountdown(key) }}
                                        </Badge>
                                        <!-- 健康度（进度条 + 百分比） -->
                                        <div class="flex items-center gap-1 shrink-0">
                                          <div class="w-8 h-1 bg-muted/80 rounded-full overflow-hidden">
                                            <div
                                              class="h-full transition-all duration-300"
                                              :class="getHealthScoreBarColor(key.health_score)"
                                              :style="{ width: `${(key.health_score || 0) * 100}%` }"
                                            />
                                          </div>
                                          <span
                                            class="text-[10px] font-medium tabular-nums"
                                            :class="getHealthScoreTextColor(key.health_score)"
                                          >
                                            {{ ((key.health_score || 0) * 100).toFixed(0) }}%
                                          </span>
                                        </div>
                                        <!-- 刷新健康按钮 -->
                                        <button
                                          v-if="key.circuit_breaker_open || (key.health_score ?? 1) < 0.5"
                                          class="p-0.5 rounded hover:bg-muted/50 text-green-600 shrink-0"
                                          title="刷新健康状态"
                                          @click.stop="handleRecoverKey(key.id, providerEntry.endpoint?.api_format || formatGroup.api_format)"
                                        >
                                          <RefreshCw class="w-3 h-3" />
                                        </button>
                                      </div>
                                    </div>
                                  </div>

                                  <!-- 优先级组之间的降级标记 -->
                                  <div
                                    v-if="groupIndex < getKeyPriorityGroups(providerEntry.keys).length - 1"
                                    class="flex items-center my-0.5 text-[10px] text-muted-foreground/50"
                                  >
                                    <!-- 箭头居中于节点列 -->
                                    <div class="min-w-8 flex items-center justify-center shrink-0">
                                      <ArrowDown class="w-3 h-3" />
                                    </div>
                                    <span>
                                      {{ getDemoteLabel(getKeyPriorityGroups(providerEntry.keys)[groupIndex + 1].keys.length) }}
                                    </span>
                                  </div>
                                </div>
                              </div>

                              <div
                                v-else
                                class="text-[10px] text-muted-foreground"
                              >
                                暂无可用 Key
                              </div>
                            </div>
                          </Transition>
                        </div>
                      </div>
                    </div>

                    <!-- 降级标记 -->
                    <div
                      v-if="providerIndex < formatGroup.providers.length - 1"
                      class="flex py-0.5"
                    >
                      <div class="w-6 flex justify-center shrink-0">
                        <div class="w-0.5 h-full bg-border" />
                      </div>
                      <div class="flex items-center gap-1 text-[10px] text-muted-foreground/50">
                        <ArrowDown class="w-3 h-3" />
                        <span>降级</span>
                      </div>
                    </div>
                  </div>
                </div>
              </template>
            </div>
          </Transition>
        </div>
      </div>
    </template>

    <!-- 错误状态 -->
    <div
      v-else-if="error"
      class="p-8 text-center"
    >
      <AlertCircle class="w-12 h-12 mx-auto text-destructive/50 mb-3" />
      <p class="text-sm text-destructive">
        {{ error }}
      </p>
      <Button
        variant="outline"
        size="sm"
        class="mt-4"
        @click="loadRoutingData"
      >
        重试
      </Button>
    </div>
  </Card>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import {
  RefreshCw,
  Loader2,
  ArrowDown,
  ChevronDown,
  Route,
  AlertCircle,
  Power,
  Link
} from 'lucide-vue-next'
import Card from '@/components/ui/card.vue'
import Badge from '@/components/ui/badge.vue'
import Button from '@/components/ui/button.vue'
import Switch from '@/components/ui/switch.vue'
import {
  getGlobalModelRoutingPreview,
  type ModelRoutingPreviewResponse,
  type RoutingProviderInfo,
  type RoutingKeyInfo,
  type RoutingEndpointInfo
} from '@/api/global-models'
import { API_FORMAT_ORDER } from '@/api/endpoints/types'
import { formatApiFormat } from '@/api/endpoints/types/api-format'
import { recoverKeyHealth } from '@/api/endpoints/health'
import { updateModel } from '@/api/endpoints/models'
import { parseApiError } from '@/utils/errorParser'
import { useToast } from '@/composables/useToast'
import { useCountdownTimer, getProbeCountdown } from '@/composables/useCountdownTimer'

const props = defineProps<{
  globalModelId: string
  routingData?: ModelRoutingPreviewResponse | null
  loading?: boolean
  error?: string | null
}>()

const emit = defineEmits<{
  editProvider: [provider: RoutingProviderInfo]
  toggleProviderStatus: [provider: RoutingProviderInfo]
  deleteProvider: [provider: RoutingProviderInfo]
  addProvider: []
  refresh: []
}>()

const { success: showSuccess, error: showError } = useToast()
const { tick: countdownTick, start: startCountdownTimer } = useCountdownTimer()

// 使用外部传入的数据或内部状态
const internalRoutingData = ref<ModelRoutingPreviewResponse | null>(null)
const internalLoading = ref(false)
const internalError = ref<string | null>(null)

// 计算属性：优先使用外部传入的数据
const routingData = computed(() => props.routingData ?? internalRoutingData.value)
const loading = computed(() => props.loading ?? internalLoading.value)
const error = computed(() => props.error ?? internalError.value)

// 是否为全局 Key 优先模式
const isGlobalKeyMode = computed(() => routingData.value?.priority_mode === 'global_key')

// ========== 数据结构定义 ==========

interface FormatProviderEntry {
  provider: RoutingProviderInfo
  endpoint: RoutingEndpointInfo | null
  keys: RoutingKeyInfo[]
  active_keys: number
  is_cross_format: boolean
  priority_api_format: string
}

// 全局 Key 模式下的 Key 条目（包含 Provider 信息）
interface GlobalKeyEntry {
  key: RoutingKeyInfo
  provider: RoutingProviderInfo
  endpoint: RoutingEndpointInfo | null
  is_cross_format: boolean
  priority_api_format: string
}

// 全局 Key 模式下的优先级分组
interface GlobalKeyGroup {
  priority: number | null
  demote_cross_format: boolean
  keys: GlobalKeyEntry[]
}

interface ApiFormatGroup {
  api_format: string
  // 提供商优先模式使用
  providers: FormatProviderEntry[]
  // 全局 Key 优先模式使用
  keyGroups: GlobalKeyGroup[]
  total_providers: number
  active_providers: number
  total_keys: number
  active_keys: number
}

function formatEndpointUrl(endpoint: RoutingEndpointInfo): string {
  const baseUrl = endpoint.base_url.replace(/\/$/, '')
  const customPath = endpoint.custom_path?.trim()
  if (!customPath) return baseUrl
  return `${baseUrl}/${customPath.replace(/^\//, '')}`
}

const savingBindings = ref<Set<string>>(new Set())
const endpointBindingOverrides = ref<Map<string, boolean>>(new Map())

function bindingKey(provider: RoutingProviderInfo, endpoint: RoutingEndpointInfo): string {
  return `${provider.model_id}:${endpoint.id}`
}

function isBindingSaving(provider: RoutingProviderInfo, endpoint: RoutingEndpointInfo): boolean {
  return savingBindings.value.has(bindingKey(provider, endpoint))
}

function isEndpointBindingActive(
  provider: RoutingProviderInfo,
  endpoint: RoutingEndpointInfo
): boolean {
  const override = endpointBindingOverrides.value.get(bindingKey(provider, endpoint))
  if (override !== undefined) return override
  return endpoint.model_binding?.is_active === true
}

function effectiveEndpointActiveKeys(
  provider: RoutingProviderInfo,
  endpoint: RoutingEndpointInfo
): number {
  if (!provider.is_active || !provider.model_is_active || !provider.model_is_available || !endpoint.is_active || !isEndpointBindingActive(provider, endpoint)) return 0
  return (endpoint.keys || []).filter(key => key.is_active).length
}

function getBindingSourceLabel(
  provider: RoutingProviderInfo,
  endpoint: RoutingEndpointInfo
): string {
  if (endpointBindingOverrides.value.has(bindingKey(provider, endpoint))) return '人工设置'
  switch (endpoint.model_binding?.source) {
    case 'manual':
      return '人工设置'
    case 'discovered':
      return '上游发现'
    case 'mapping':
      return '模型映射'
    case 'migration':
      return '兼容迁移'
    default:
      return '未绑定'
  }
}

function formatRuntimeCapabilityQuarantines(endpoint: RoutingEndpointInfo): string {
  return (endpoint.runtime_capability_quarantines || [])
    .map(entry => [
      `Key ${entry.key_id}`,
      formatApiFormat(entry.client_api_format),
      entry.request_mode === 'stream' ? '流式' : '非流式',
      entry.request_operation || null,
    ].filter(Boolean).join(' / '))
    .join('、')
}

async function handleEndpointBindingToggle(
  provider: RoutingProviderInfo,
  endpoint: RoutingEndpointInfo,
  isActive: boolean
) {
  const key = bindingKey(provider, endpoint)
  if (savingBindings.value.has(key)) return

  const hadOverride = endpointBindingOverrides.value.has(key)
  const previousOverride = endpointBindingOverrides.value.get(key)
  endpointBindingOverrides.value.set(key, isActive)
  savingBindings.value.add(key)
  try {
    await updateModel(provider.id, provider.model_id, {
      endpoint_bindings: [{ endpoint_id: endpoint.id, is_active: isActive }]
    })
    showSuccess(isActive ? '已启用模型 Endpoint 链路' : '已禁用模型 Endpoint 链路')
  } catch (err: unknown) {
    if (hadOverride && previousOverride !== undefined) {
      endpointBindingOverrides.value.set(key, previousOverride)
    } else {
      endpointBindingOverrides.value.delete(key)
    }
    showError(parseApiError(err, '链路设置失败'), '错误')
  } finally {
    savingBindings.value.delete(key)
  }
}

const STANDARD_ROUTING_API_FORMATS = [
  'openai:chat',
  'openai:responses',
  'claude:messages',
  'gemini:generate_content'
]

function normalizeLegacyOpenAIFormatAlias(apiFormat: string): string {
  switch (apiFormat.trim().toLowerCase()) {
    default:
      return apiFormat.trim().toLowerCase()
  }
}

function apiDataFormatId(apiFormat: string): string | null {
  switch (normalizeLegacyOpenAIFormatAlias(apiFormat)) {
    case 'claude:messages':
      return 'claude'
    case 'gemini:generate_content':
      return 'gemini'
    case 'openai:chat':
      return 'openai_chat'
    case 'openai:responses':
    case 'openai:responses:compact':
      return 'openai_responses'
    default:
      return null
  }
}

function requestConversionKind(clientApiFormat: string, providerApiFormat: string): string | null {
  const client = normalizeLegacyOpenAIFormatAlias(clientApiFormat)
  const provider = normalizeLegacyOpenAIFormatAlias(providerApiFormat)
  if (client === provider) return null
  if (!STANDARD_ROUTING_API_FORMATS.includes(client) || !STANDARD_ROUTING_API_FORMATS.includes(provider)) {
    return null
  }

  switch (provider) {
    case 'openai:chat':
      return 'to_openai_chat'
    case 'openai:responses':
      return 'to_openai_responses'
    case 'claude:messages':
      return 'to_claude'
    case 'gemini:generate_content':
      return 'to_gemini'
    default:
      return null
  }
}

function requestConversionRequiresEnableFlag(clientApiFormat: string, providerApiFormat: string): boolean {
  const clientDataFormat = apiDataFormatId(clientApiFormat)
  const providerDataFormat = apiDataFormatId(providerApiFormat)
  if (!clientDataFormat || !providerDataFormat) return true
  return clientDataFormat !== providerDataFormat
}

function endpointFormatAcceptanceEnabled(endpoint: RoutingEndpointInfo): boolean {
  return endpoint.format_acceptance_config?.enabled === true
}

function endpointFormatListContains(formats: string[] | null | undefined, apiFormat: string): boolean {
  if (!Array.isArray(formats)) return false
  return formats.some(value => value.trim().toLowerCase() === apiFormat.trim().toLowerCase())
}

function endpointAcceptsClientFormat(endpoint: RoutingEndpointInfo, clientApiFormat: string): boolean {
  if (!endpointFormatAcceptanceEnabled(endpoint)) return false

  const config = endpoint.format_acceptance_config
  if (endpointFormatListContains(config?.reject_formats, clientApiFormat)) {
    return false
  }
  if (config?.accept_formats) {
    return endpointFormatListContains(config.accept_formats, clientApiFormat)
  }
  return true
}

function endpointSupportsClientFormat(
  provider: RoutingProviderInfo,
  endpoint: RoutingEndpointInfo,
  clientApiFormat: string,
  providerApiFormat: string
): boolean {
  const clientFormat = clientApiFormat.trim().toLowerCase()
  const providerFormat = providerApiFormat.trim().toLowerCase()
  if (clientFormat === providerFormat) return true
  if (!requestConversionKind(clientFormat, providerFormat)) return false
  if (!requestConversionRequiresEnableFlag(clientFormat, providerFormat)) return true
  return !!provider.enable_format_conversion || endpointAcceptsClientFormat(endpoint, clientFormat)
}

function targetFormatsForEndpoint(
  provider: RoutingProviderInfo,
  endpoint: RoutingEndpointInfo
): string[] {
  const endpointFormat = normalizeLegacyOpenAIFormatAlias(endpoint.api_format)
  if (!isEndpointBindingActive(provider, endpoint)) return [endpointFormat]
  const candidateFormats = Array.from(new Set([...STANDARD_ROUTING_API_FORMATS, endpointFormat]))
  return candidateFormats.filter(format =>
    endpointSupportsClientFormat(provider, endpoint, format, endpoint.api_format)
  )
}

function keepPriorityOnConversion(provider: RoutingProviderInfo): boolean {
  return !!(routingData.value?.keep_priority_on_conversion || provider.keep_priority_on_conversion)
}

function shouldDemoteCrossFormat(
  targetApiFormat: string,
  entryApiFormat: string,
  provider: RoutingProviderInfo
): boolean {
  return targetApiFormat !== entryApiFormat && !keepPriorityOnConversion(provider)
}

function resolvedGlobalKeyPriority(keyEntry: GlobalKeyEntry): number {
  const priorityByFormat = keyEntry.key.global_priority_by_format
  if (!priorityByFormat) return 999
  const value = priorityByFormat[keyEntry.priority_api_format]
  return typeof value === 'number' ? value : 999
}

// 按 API 格式分组的计算属性
const apiFormatGroups = computed<ApiFormatGroup[]>(() => {
  if (!routingData.value) return []

  const formatMap = new Map<string, {
    providers: FormatProviderEntry[]
    allKeys: GlobalKeyEntry[]
  }>()

  // 遍历所有提供商和它们的 endpoints
  for (const provider of routingData.value.providers) {
    for (const endpoint of provider.endpoints || []) {
      for (const format of targetFormatsForEndpoint(provider, endpoint)) {
        if (!formatMap.has(format)) {
          formatMap.set(format, { providers: [], allKeys: [] })
        }

        const data = formatMap.get(format)
        if (!data) continue

        data.providers.push({
          provider,
          endpoint,
          keys: endpoint.keys || [],
          active_keys: effectiveEndpointActiveKeys(provider, endpoint),
          is_cross_format: endpoint.api_format !== format,
          priority_api_format: endpoint.api_format
        })

        if (provider.is_active && provider.model_is_active && provider.model_is_available && endpoint.is_active && isEndpointBindingActive(provider, endpoint)) {
          for (const key of endpoint.keys || []) {
            data.allKeys.push({
              key,
              provider,
              endpoint,
              is_cross_format: endpoint.api_format !== format,
              priority_api_format: endpoint.api_format
            })
          }
        }
      }
    }
  }

  // 转换为数组并计算统计
  const groups: ApiFormatGroup[] = []
  for (const [format, data] of formatMap) {
    // Provider 排序（提供商优先模式）
    const sortedProviders = [...data.providers].sort((a, b) => {
      const aActive = isFormatProviderEntryActive(a)
      const bActive = isFormatProviderEntryActive(b)
      if (aActive !== bActive) return bActive ? 1 : -1
      const aDemoted = shouldDemoteCrossFormat(format, a.priority_api_format, a.provider)
      const bDemoted = shouldDemoteCrossFormat(format, b.priority_api_format, b.provider)
      if (aDemoted !== bDemoted) return aDemoted ? 1 : -1
      return a.provider.provider_priority - b.provider.provider_priority
    })

    // Key 按全局优先级分组排序（全局 Key 优先模式）
    const keyGroupMap = new Map<string, GlobalKeyGroup>()
    for (const keyEntry of data.allKeys) {
      const priority = resolvedGlobalKeyPriority(keyEntry)
      const demoteCrossFormat = shouldDemoteCrossFormat(format, keyEntry.priority_api_format, keyEntry.provider)
      const groupKey = `${demoteCrossFormat ? 1 : 0}:${priority}`
      if (!keyGroupMap.has(groupKey)) {
        keyGroupMap.set(groupKey, {
          priority: priority === 999 ? null : priority,
          demote_cross_format: demoteCrossFormat,
          keys: []
        })
      }
      keyGroupMap.get(groupKey)?.keys.push(keyEntry)
    }

    // 转换为分组数组并排序
    const keyGroups: GlobalKeyGroup[] = Array.from(keyGroupMap.values())
      .sort((a, b) => {
        if (a.demote_cross_format !== b.demote_cross_format) {
          return a.demote_cross_format ? 1 : -1
        }
        return (a.priority ?? 999) - (b.priority ?? 999)
      })
      .map(group => ({
        ...group,
        keys: group.keys.sort((a, b) => {
          // 同优先级内按活跃状态和健康度排序
          const aActive = a.key.is_active && isGlobalKeyEntryActive(a)
          const bActive = b.key.is_active && isGlobalKeyEntryActive(b)
          if (aActive !== bActive) return bActive ? 1 : -1
          return b.key.health_score - a.key.health_score
        })
      }))

    const providerModelActivity = new Map<string, boolean>()
    for (const entry of sortedProviders) {
      const isActive = isFormatProviderEntryActive(entry) && entry.active_keys > 0
      providerModelActivity.set(
        entry.provider.model_id,
        (providerModelActivity.get(entry.provider.model_id) ?? false) || isActive
      )
    }
    const activeProviders = Array.from(providerModelActivity.values()).filter(Boolean).length
    const totalKeys = sortedProviders.reduce((sum, e) => sum + e.keys.length, 0)
    const activeKeys = sortedProviders.reduce((sum, entry) => sum + entry.active_keys, 0)

    groups.push({
      api_format: format,
      providers: sortedProviders,
      keyGroups,
      total_providers: providerModelActivity.size,
      active_providers: activeProviders,
      total_keys: totalKeys,
      active_keys: activeKeys
    })
  }

  // 按 API 格式排序
  groups.sort((a, b) => {
    const aIndex = API_FORMAT_ORDER.indexOf(a.api_format)
    const bIndex = API_FORMAT_ORDER.indexOf(b.api_format)
    if (aIndex === -1 && bIndex === -1) return a.api_format.localeCompare(b.api_format)
    if (aIndex === -1) return 1
    if (bIndex === -1) return -1
    return aIndex - bIndex
  })

  return groups
})

// ========== 展开状态管理 ==========

const expandedFormats = ref<Set<string>>(new Set())

function isFormatExpanded(format: string): boolean {
  return expandedFormats.value.has(format)
}

function toggleFormat(format: string) {
  if (expandedFormats.value.has(format)) {
    expandedFormats.value.delete(format)
  } else {
    expandedFormats.value.add(format)
  }
}

// 提供商模式：格式内提供商级别的展开状态
const expandedProvidersInFormat = ref<Set<string>>(new Set())

function providerExpansionKey(
  format: string,
  providerId: string,
  modelId: string,
  endpointId?: string
): string {
  return endpointId
    ? `${format}:${providerId}:${modelId}:${endpointId}`
    : `${format}:${providerId}:${modelId}`
}

function isProviderInFormatExpanded(
  format: string,
  providerId: string,
  modelId: string,
  endpointId?: string
): boolean {
  const key = providerExpansionKey(format, providerId, modelId, endpointId)
  return expandedProvidersInFormat.value.has(key)
}

function toggleProviderInFormat(
  format: string,
  providerId: string,
  modelId: string,
  endpointId?: string
) {
  const key = providerExpansionKey(format, providerId, modelId, endpointId)
  if (expandedProvidersInFormat.value.has(key)) {
    expandedProvidersInFormat.value.delete(key)
  } else {
    expandedProvidersInFormat.value.add(key)
  }
}

// 加载数据（仅在没有外部数据时使用）
async function loadRoutingData() {
  // 如果有外部传入的数据，通知父组件刷新
  if (props.routingData !== undefined) {
    emit('refresh')
    return
  }

  if (!props.globalModelId) return

  internalLoading.value = true
  internalError.value = null

  try {
    internalRoutingData.value = await getGlobalModelRoutingPreview(props.globalModelId)
  } catch (err: unknown) {
    internalError.value = parseApiError(err, '加载失败')
  } finally {
    internalLoading.value = false
  }
}

// 获取调度模式标签
function getSchedulingModeLabel(mode: string): string {
  const labels: Record<string, string> = {
    cache_affinity: '缓存亲和',
    fixed_order: '固定顺序',
    load_balance: '负载均衡'
  }
  return labels[mode] || mode
}

// 当前调度模式的显示标签（复用 getSchedulingModeLabel，computed 缓存避免重复计算）
const samePriorityLabel = computed(() =>
  getSchedulingModeLabel(routingData.value?.scheduling_mode || 'cache_affinity')
)

// 获取降级标签（含同优先级调度行为）
function getDemoteLabel(nextGroupKeyCount: number): string {
  if (nextGroupKeyCount > 1) {
    return `降级 · ${samePriorityLabel.value}`
  }
  return '降级'
}

// 获取优先级模式标签
function getPriorityModeLabel(mode: string): string {
  const labels: Record<string, string> = {
    provider: '提供商优先',
    global_key: '全局 Key 优先'
  }
  return labels[mode] || mode
}

// 判断是否存在模型映射（始终显示 provider_model_name）
function hasModelMapping(provider: RoutingProviderInfo): boolean {
  return !!provider.provider_model_name
}

// 获取 Key 的 allowed_models 中匹配当前 GlobalModel 的所有模型名
// 逻辑：用 GlobalModel 的 model_mappings（正则模式）去匹配 Key 的 allowed_models 中的值
function getKeyMatchedModels(key: RoutingKeyInfo): string[] {
  return key.matched_models || []
}

// 格式化匹配的模型显示文本
function formatMatchedModels(models: string[]): string {
  if (models.length === 0) return ''
  if (models.length === 1) return models[0]
  return `${models[0]} +${models.length - 1}`
}

// 按优先级分组 Keys（提供商优先模式下使用）
interface KeyPriorityGroup {
  priority: number | null
  keys: RoutingKeyInfo[]
}

function getKeyPriorityGroups(keys: RoutingKeyInfo[]): KeyPriorityGroup[] {
  const groups = new Map<number, KeyPriorityGroup>()

  for (const key of keys) {
    // 提供商优先模式：按 internal_priority 分组
    const priority = key.internal_priority ?? 999

    if (!groups.has(priority)) {
      groups.set(priority, {
        priority: priority === 999 ? null : priority,
        keys: []
      })
    }
    groups.get(priority)?.keys.push(key)
  }

  return Array.from(groups.values()).sort((a, b) => {
    const pa = a.priority ?? 999
    const pb = b.priority ?? 999
    return pa - pb
  })
}

function isFormatProviderEntryActive(entry: FormatProviderEntry): boolean {
  return !!entry.endpoint
    && entry.provider.is_active
    && entry.provider.model_is_active
    && entry.provider.model_is_available
    && entry.endpoint.is_active
    && isEndpointBindingActive(entry.provider, entry.endpoint)
}

function isGlobalKeyEntryActive(entry: GlobalKeyEntry): boolean {
  return !!entry.endpoint
    && entry.provider.is_active
    && entry.provider.model_is_active
    && entry.provider.model_is_available
    && entry.endpoint.is_active
    && isEndpointBindingActive(entry.provider, entry.endpoint)
}

// Endpoint 卡片状态同时反映 Provider、模型关联、Endpoint 与模型绑定。
function getFormatProviderStatusClass(entry: FormatProviderEntry): string {
  if (!entry.provider.is_active || !entry.endpoint?.is_active || !isEndpointBindingActive(entry.provider, entry.endpoint)) {
    return 'bg-gray-400'
  }
  if (!entry.provider.model_is_active || !entry.provider.model_is_available) {
    return 'bg-yellow-500'
  }
  return 'bg-green-500'
}

// 获取格式分组中提供商节点圆点样式（提供商优先模式）
function getFormatProviderNodeClass(entry: FormatProviderEntry, index: number): string {
  if (!isFormatProviderEntryActive(entry)) {
    return 'bg-muted border-muted-foreground/30'
  }
  if (index === 0) {
    return 'bg-primary border-primary'
  }
  return 'bg-background border-border'
}

// 获取格式分组中提供商卡片样式（提供商优先模式）
function getFormatProviderCardClass(entry: FormatProviderEntry, index: number): string {
  if (!isFormatProviderEntryActive(entry)) {
    return 'bg-muted/30 border border-border/40'
  }
  if (index === 0) {
    return 'bg-primary/5 border border-primary/30 shadow-sm'
  }
  return 'bg-muted/20 border border-border/50 hover:border-border'
}

// 获取全局 Key 节点圆点样式（全局 Key 优先模式）
function getGlobalKeyNodeClass(entry: GlobalKeyEntry, groupIndex: number, keyIndex: number): string {
  if (!entry.key.is_active || !isGlobalKeyEntryActive(entry)) {
    return 'bg-muted border-muted-foreground/30'
  }
  if (groupIndex === 0 && keyIndex === 0) {
    return 'bg-primary border-primary'
  }
  return 'bg-background border-border'
}

// 判断是否为格式组中的最后一个 Key
function isLastKeyInFormat(formatGroup: ApiFormatGroup, groupIndex: number, keyIndex: number): boolean {
  const isLastGroup = groupIndex === formatGroup.keyGroups.length - 1
  const isLastKeyInGroup = keyIndex === formatGroup.keyGroups[groupIndex].keys.length - 1
  return isLastGroup && isLastKeyInGroup
}

// 获取全局 Key 卡片样式（全局 Key 优先模式）
function getGlobalKeyCardClass(entry: GlobalKeyEntry, groupIndex: number, keyIndex: number): string {
  if (!entry.key.is_active || !isGlobalKeyEntryActive(entry)) {
    return 'bg-muted/30 border border-border/40'
  }
  if (groupIndex === 0 && keyIndex === 0) {
    return 'bg-primary/5 border border-primary/30 shadow-sm'
  }
  return 'bg-muted/20 border border-border/50 hover:border-border'
}

// 健康度进度条颜色（score 为 0-1 小数格式）
function getHealthScoreBarColor(score: number): string {
  if (score >= 0.8) return 'bg-green-500 dark:bg-green-400'
  if (score >= 0.5) return 'bg-yellow-500 dark:bg-yellow-400'
  return 'bg-red-500 dark:bg-red-400'
}

// 健康度文字颜色（score 为 0-1 小数格式）
function getHealthScoreTextColor(score: number): string {
  if (score >= 0.8) return 'text-green-600 dark:text-green-400'
  if (score >= 0.5) return 'text-yellow-600 dark:text-yellow-400'
  return 'text-red-600 dark:text-red-400'
}

// 获取计费标签
function getBillingLabel(provider: RoutingProviderInfo): string {
  if (provider.billing_type === 'monthly_quota') {
    const used = provider.monthly_used_usd || 0
    const quota = provider.monthly_quota_usd
    return quota ? `$${used.toFixed(0)}/$${quota.toFixed(0)}` : '月卡'
  }
  if (provider.billing_type === 'pay_as_you_go') {
    return '按量'
  }
  return '免费'
}

// 获取 Key 状态样式（score 为 0-1 小数格式）
function getKeyStatusClass(key: RoutingKeyInfo): string {
  if (!key.is_active) {
    return 'bg-gray-400'
  }
  if (key.circuit_breaker_open) {
    return 'bg-red-500'
  }
  const score = key.health_score ?? 1
  if (score < 0.5) {
    return 'bg-red-500'
  }
  if (score < 0.8) {
    return 'bg-yellow-500'
  }
  return 'bg-green-500'
}

// 获取 Key 提示信息
function getKeyTooltip(key: RoutingKeyInfo): string {
  const parts: string[] = []
  parts.push(`名称: ${key.name}`)
  parts.push(`健康度: ${((key.health_score || 0) * 100).toFixed(0)}%`)
  if (!key.is_active) {
    parts.push('状态: 禁用')
  } else if (key.circuit_breaker_open) {
    parts.push(`熔断中: ${key.circuit_breaker_formats.join(', ')}`)
  }
  return parts.join('\n')
}

// 获取 Key 探测倒计时（用于 RoutingKeyInfo）
function getKeyProbeCountdown(key: RoutingKeyInfo): string {
  if (!key.circuit_breaker_open) return ''
  const countdown = getProbeCountdown(key.next_probe_at, countdownTick.value)
  return countdown ? ` · ${countdown}` : ''
}

// 恢复 Key 健康状态（仅恢复指定 API 格式）
async function handleRecoverKey(keyId: string, apiFormat: string) {
  try {
    const result = await recoverKeyHealth(keyId, apiFormat)
    // 通知父组件刷新数据
    emit('refresh')
    showSuccess(result.message || 'Key 已恢复')
  } catch (err: unknown) {
    showError(parseApiError(err, 'Key 恢复失败'), '错误')
  }
}

// 监听 globalModelId 变化
watch(() => props.globalModelId, () => {
  expandedFormats.value.clear()
  expandedProvidersInFormat.value.clear()
  endpointBindingOverrides.value.clear()
  // 如果没有外部数据，才自己加载
  if (props.routingData === undefined) {
    loadRoutingData()
  }
}, { immediate: false })

watch(() => props.routingData, (current, previous) => {
  if (current !== previous) endpointBindingOverrides.value.clear()
})

onMounted(() => {
  // 如果没有外部数据，才自己加载
  if (props.routingData === undefined) {
    loadRoutingData()
  }
  startCountdownTimer()
})

// 暴露方法给父组件
defineExpose({
  loadRoutingData
})
</script>

<style scoped>
.collapse-enter-active,
.collapse-leave-active {
  transition: all 0.2s ease;
  overflow: hidden;
}

.collapse-enter-from,
.collapse-leave-to {
  opacity: 0;
  max-height: 0;
}

.collapse-enter-to,
.collapse-leave-from {
  opacity: 1;
  max-height: 500px;
}
</style>
