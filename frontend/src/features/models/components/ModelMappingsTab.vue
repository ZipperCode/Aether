<template>
  <Card class="overflow-hidden">
    <div class="px-4 py-3 border-b border-border/60">
      <div class="flex items-center justify-between">
        <div class="flex items-baseline gap-2">
          <h4 class="text-sm font-semibold">映射规则</h4>
          <span class="text-xs text-muted-foreground">
            支持正则表达式 ({{ localMappings.length }}/{{
              MAX_MAPPINGS_PER_MODEL
            }})
          </span>
        </div>
        <div class="flex items-center gap-1">
          <Button
            variant="ghost"
            size="icon"
            class="h-7 w-7"
            title="添加规则"
            :disabled="localMappings.length >= MAX_MAPPINGS_PER_MODEL"
            @click="addMapping"
          >
            <Plus class="w-4 h-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            class="h-7 w-7"
            title="刷新"
            :disabled="loadingPreview"
            @click="loadMatchPreview"
          >
            <RefreshCw
              class="w-4 h-4"
              :class="{ 'animate-spin': loadingPreview }"
            />
          </Button>
        </div>
      </div>
    </div>

    <div v-if="localMappings.length > 0" class="divide-y">
      <div v-for="(mapping, index) in localMappings" :key="index">
        <div
          class="px-4 py-3 flex items-center gap-3 cursor-pointer hover:bg-muted/30 transition-colors"
          @click="toggleExpand(index)"
        >
          <ChevronRight
            class="w-4 h-4 text-muted-foreground transition-transform flex-shrink-0"
            :class="{ 'rotate-90': expandedIndex === index }"
          />
          <div class="flex-1 min-w-0">
            <Input
              v-model="localMappings[index]"
              placeholder="例如: claude-haiku-.*"
              :class="`font-mono text-sm ${normalizedMappings[index] && !mappingValidations[index].valid ? 'border-destructive' : ''}`"
              @click.stop
              @input="markDirty"
            />
            <div
              v-if="
                normalizedMappings[index] && !mappingValidations[index].valid
              "
              class="flex items-center gap-1 mt-1 text-xs text-destructive"
            >
              <AlertCircle class="w-3 h-3" />
              <span>无效正则表达式</span>
            </div>
          </div>

          <Badge
            v-if="loadingPreview"
            variant="outline"
            class="text-xs text-muted-foreground flex-shrink-0 h-6 leading-none"
            >计算中</Badge
          >
          <Badge
            v-else-if="previewError"
            variant="destructive"
            class="text-xs flex-shrink-0 h-6 leading-none"
            >预览失败</Badge
          >
          <Badge
            v-else-if="
              mappingValidations[index].valid && mappingMatchCounts[index] > 0
            "
            variant="secondary"
            class="text-xs flex-shrink-0 h-6 leading-none"
            :title="getMappingMatchTitle(index)"
            >{{ mappingMatchCounts[index] }} 匹配</Badge
          >
          <Badge
            v-else-if="
              normalizedMappings[index] && mappingValidations[index].valid
            "
            variant="outline"
            class="text-xs text-muted-foreground flex-shrink-0 h-6 leading-none"
            >无匹配</Badge
          >

          <div class="flex items-center gap-1 flex-shrink-0">
            <Button
              v-if="isDirty"
              variant="ghost"
              size="icon"
              class="h-7 w-7 text-muted-foreground hover:text-primary"
              title="保存"
              :disabled="saving || loadingPreview || hasValidationErrors"
              @click.stop="saveMappings"
            >
              <Save v-if="!saving" class="w-4 h-4" />
              <RefreshCw v-else class="w-4 h-4 animate-spin" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              class="h-7 w-7 text-muted-foreground hover:text-destructive"
              title="删除"
              :disabled="saving"
              @click.stop="removeMapping(index)"
            >
              <Trash2 class="w-4 h-4" />
            </Button>
          </div>
        </div>

        <div
          v-if="expandedIndex === index"
          class="border-t bg-muted/10 px-4 py-3"
        >
          <div
            v-if="loadingPreview"
            class="flex items-center justify-center py-4"
          >
            <RefreshCw class="w-4 h-4 animate-spin text-muted-foreground" />
          </div>
          <div v-else-if="previewError" class="text-center py-4">
            <p class="text-sm text-destructive">{{ previewError }}</p>
          </div>
          <div v-else-if="expandedGroups.length === 0" class="text-center py-4">
            <p class="text-sm text-muted-foreground">
              {{
                normalizedMappings[index]
                  ? "此规则暂无匹配的 Key 白名单"
                  : "请输入映射规则"
              }}
            </p>
          </div>
          <div v-else class="space-y-3">
            <div
              v-for="group in expandedGroups"
              :key="group.providerId"
              class="bg-background rounded-md border overflow-hidden"
            >
              <div
                class="px-3 py-2 bg-muted/30 border-b flex items-center justify-between"
              >
                <div>
                  <span class="text-sm font-medium">{{
                    group.providerName
                  }}</span>
                  <span class="text-xs text-muted-foreground ml-2"
                    >({{ group.keys.length }} Key)</span
                  >
                </div>
                <Badge v-if="group.isLinked" variant="secondary" class="text-xs"
                  >已关联</Badge
                >
                <Button
                  v-else
                  variant="ghost"
                  size="icon"
                  class="h-7 w-7"
                  title="关联到当前模型"
                  @click="$emit('linkProvider', group.providerId)"
                >
                  <Link class="w-3.5 h-3.5" />
                </Button>
              </div>
              <div class="divide-y divide-border/50">
                <div
                  v-for="keyItem in group.keys"
                  :key="keyItem.keyId"
                  class="px-3 py-2"
                >
                  <div class="flex items-center gap-1.5 text-sm mb-1.5">
                    <span class="font-medium">{{ keyItem.keyName }}</span>
                    <code class="text-xs text-muted-foreground/70">{{
                      keyItem.maskedKey
                    }}</code>
                  </div>
                  <div class="flex flex-wrap gap-1">
                    <Badge
                      v-for="model in keyItem.matchedModels"
                      :key="model"
                      variant="secondary"
                      class="text-xs font-mono"
                      >{{ model }}</Badge
                    >
                  </div>
                </div>
              </div>
            </div>

            <div
              v-if="expandedTotalPages > 1"
              class="flex items-center justify-center gap-3 pt-2"
            >
              <Button
                variant="outline"
                size="sm"
                :disabled="expandedPage <= 1 || loadingPreview"
                @click="changeExpandedPage(expandedPage - 1)"
                >上一页</Button
              >
              <span class="text-xs text-muted-foreground">
                {{ expandedPage }} / {{ expandedTotalPages }}（{{
                  previewData?.expanded?.total_keys || 0
                }}
                Key）
              </span>
              <Button
                variant="outline"
                size="sm"
                :disabled="expandedPage >= expandedTotalPages || loadingPreview"
                @click="changeExpandedPage(expandedPage + 1)"
                >下一页</Button
              >
            </div>
          </div>
        </div>
      </div>
    </div>

    <div v-else class="text-center py-32">
      <GitMerge class="w-10 h-10 mx-auto text-muted-foreground/30 mb-3" />
      <p class="text-sm text-muted-foreground">暂无映射规则</p>
    </div>
  </Card>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue";
import {
  AlertCircle,
  ChevronRight,
  GitMerge,
  Link,
  Plus,
  RefreshCw,
  Save,
  Trash2,
} from "lucide-vue-next";
import { Badge, Button, Card, Input } from "@/components/ui";
import {
  getGlobalModel,
  getGlobalModelMappingPreview,
  updateGlobalModel,
} from "@/api/global-models";
import type {
  ModelMappingPreviewResponse,
  ModelMappingPreviewRule,
} from "@/api/endpoints/types";
import { useToast } from "@/composables/useToast";
import { parseApiError } from "@/utils/errorParser";
import { log } from "@/utils/logger";
import { MAX_MAPPINGS_PER_MODEL } from "@/features/models/utils/model-mapping-regex";

const props = withDefaults(
  defineProps<{
    globalModelId: string;
    modelName: string;
    mappings: string[];
    active?: boolean;
  }>(),
  {
    active: false,
  },
);

const emit = defineEmits<{
  update: [mappings: string[]];
  refresh: [];
  linkProvider: [providerId: string];
  linkProviders: [providerIds: string[]];
}>();

interface MatchedKeyForMapping {
  keyId: string;
  keyName: string;
  maskedKey: string;
  providerName: string;
  providerId: string;
  matchedModels: string[];
}

interface ProviderGroup {
  providerId: string;
  providerName: string;
  keys: MatchedKeyForMapping[];
  isLinked: boolean;
}

const PAGE_SIZE = 25;
const { success: toastSuccess, error: toastError } = useToast();
const localMappings = ref<string[]>([...props.mappings]);
const originalMappings = ref<string[]>([...props.mappings]);
const isDirty = ref(false);
const saving = ref(false);
const expandedIndex = ref<number | null>(null);
const expandedPage = ref(1);
const previewData = ref<ModelMappingPreviewResponse | null>(null);
const loadingPreview = ref(false);
const previewError = ref<string | null>(null);
let previewSequence = 0;
let previewTimer: ReturnType<typeof setTimeout> | null = null;

const normalizedMappings = computed(() =>
  localMappings.value.map((mapping) => mapping.trim()),
);
const previewRules = computed(() => previewData.value?.rules || []);
const mappingValidations = computed(() =>
  normalizedMappings.value.map((pattern, index) => ({
    valid:
      !pattern ||
      previewRules.value[index]?.pattern !== pattern ||
      previewRules.value[index]?.valid !== false,
  })),
);
const hasValidationErrors = computed(() =>
  mappingValidations.value.some(
    (result, index) => normalizedMappings.value[index] !== "" && !result.valid,
  ),
);
const mappingMatchCounts = computed(() =>
  normalizedMappings.value.map((pattern, index) =>
    previewRules.value[index]?.pattern === pattern &&
    previewRules.value[index]?.valid
      ? previewRules.value[index].matched_mapping_count ??
        previewRules.value[index].matched_model_count
      : 0,
  ),
);

function getMappingMatchTitle(index: number): string {
  const rule = previewRules.value[index];
  if (!rule) return "";
  return `${rule.matched_model_count} 个上游模型名 · ${rule.matched_key_count} Key · ${rule.matched_provider_count} 个提供商`;
}
const expandedTotalPages = computed(() =>
  Math.max(
    1,
    Math.ceil((previewData.value?.expanded?.total_keys || 0) / PAGE_SIZE),
  ),
);
const expandedGroups = computed<ProviderGroup[]>(() => {
  const expanded = previewData.value?.expanded;
  if (!expanded || expanded.rule_index !== expandedIndex.value) return [];
  const groups = new Map<string, ProviderGroup>();
  for (const key of expanded.keys) {
    const item: MatchedKeyForMapping = {
      keyId: key.key_id,
      keyName: key.key_name,
      maskedKey: key.masked_key,
      providerName: key.provider_name,
      providerId: key.provider_id,
      matchedModels: key.matched_models,
    };
    const existing = groups.get(key.provider_id);
    if (existing) {
      existing.keys.push(item);
    } else {
      groups.set(key.provider_id, {
        providerId: key.provider_id,
        providerName: key.provider_name,
        keys: [item],
        isLinked: key.is_linked,
      });
    }
  }
  return Array.from(groups.values());
});

function scheduleMatchPreview() {
  if (!props.active) return;
  if (previewTimer) clearTimeout(previewTimer);
  previewTimer = setTimeout(() => void loadMatchPreview(), 250);
}

async function loadMatchPreview() {
  if (!props.active) return false;
  if (previewTimer) {
    clearTimeout(previewTimer);
    previewTimer = null;
  }
  const sequence = ++previewSequence;
  const modelId = props.globalModelId;
  loadingPreview.value = true;
  previewError.value = null;
  try {
    const preview = await getGlobalModelMappingPreview(modelId, {
      mappings: normalizedMappings.value,
      expanded_rule_index: expandedIndex.value,
      page: expandedPage.value,
      page_size: PAGE_SIZE,
    });
    if (sequence !== previewSequence || modelId !== props.globalModelId)
      return false;
    previewData.value = preview;
    return true;
  } catch (error: unknown) {
    if (sequence !== previewSequence) return false;
    log.error("加载匹配预览失败:", error);
    previewError.value = parseApiError(error, "加载映射预览失败");
    return false;
  } finally {
    if (sequence === previewSequence) loadingPreview.value = false;
  }
}

function markDirty() {
  isDirty.value = true;
  expandedPage.value = 1;
  scheduleMatchPreview();
}

function addMapping() {
  if (localMappings.value.length >= MAX_MAPPINGS_PER_MODEL) {
    toastError(`最多支持 ${MAX_MAPPINGS_PER_MODEL} 条映射规则`);
    return;
  }
  localMappings.value.push("");
  isDirty.value = true;
  expandedIndex.value = localMappings.value.length - 1;
  expandedPage.value = 1;
  scheduleMatchPreview();
}

function toggleExpand(index: number) {
  expandedIndex.value = expandedIndex.value === index ? null : index;
  expandedPage.value = 1;
  void loadMatchPreview();
}

function changeExpandedPage(page: number) {
  expandedPage.value = page;
  void loadMatchPreview();
}

async function removeMapping(index: number) {
  localMappings.value.splice(index, 1);
  if (expandedIndex.value === index) expandedIndex.value = null;
  else if (expandedIndex.value !== null && expandedIndex.value > index)
    expandedIndex.value--;
  isDirty.value = true;
  await saveMappings();
}

async function saveMappings() {
  if (!(await loadMatchPreview()) || hasValidationErrors.value) {
    toastError(previewError.value || "存在无效映射规则，无法保存");
    return;
  }
  const cleanedMappings = normalizedMappings.value.filter(Boolean);
  saving.value = true;
  try {
    const currentModel = await getGlobalModel(props.globalModelId);
    const updatedConfig = {
      ...(currentModel.config || {}),
      model_mappings: cleanedMappings,
    };
    if (cleanedMappings.length === 0) delete updatedConfig.model_mappings;
    const unlinkedProviderIds = Array.from(
      new Set(
        previewRules.value.flatMap(
          (rule: ModelMappingPreviewRule) => rule.unlinked_provider_ids,
        ),
      ),
    );

    await updateGlobalModel(props.globalModelId, { config: updatedConfig });
    localMappings.value = cleanedMappings;
    originalMappings.value = [...cleanedMappings];
    isDirty.value = false;
    if (unlinkedProviderIds.length > 0) {
      toastSuccess(
        `映射规则已保存，正在关联 ${unlinkedProviderIds.length} 个提供商...`,
      );
      emit("linkProviders", unlinkedProviderIds);
    } else {
      toastSuccess("映射规则已保存");
    }
    emit("update", cleanedMappings);
    emit("refresh");
    await loadMatchPreview();
  } catch (error) {
    log.error("保存映射规则失败:", error);
    toastError("保存失败，请重试");
    localMappings.value = [...originalMappings.value];
    isDirty.value = false;
  } finally {
    saving.value = false;
  }
}

watch(
  () => props.mappings,
  (mappings) => {
    localMappings.value = [...mappings];
    originalMappings.value = [...mappings];
    isDirty.value = false;
    scheduleMatchPreview();
  },
  { deep: true },
);

watch(
  () => props.globalModelId,
  () => {
    previewSequence++;
    previewData.value = null;
    previewError.value = null;
    expandedIndex.value = null;
    expandedPage.value = 1;
    scheduleMatchPreview();
  },
);

watch(
  () => props.active,
  (active) => {
    if (active && !previewData.value) void loadMatchPreview();
  },
);

onUnmounted(() => {
  previewSequence++;
  if (previewTimer) clearTimeout(previewTimer);
});

defineExpose({ refresh: loadMatchPreview });
</script>
