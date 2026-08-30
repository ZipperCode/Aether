use super::*;
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{Duration, Instant};

const CAPABILITY_SUITE_VERSION: &str = "capability-v1";
const CAPABILITY_MAX_CONCURRENCY: usize = 4;
const CAPABILITY_MAX_OUTPUT_TOKENS: u64 = 1024;
const CAPABILITY_INVALID_REQUEST: &str = "Invalid model capability test request";
const CAPABILITY_INVALID_REQUEST_IDS: &str = "Model capability test IDs must be non-empty";
const CAPABILITY_MODEL_NOT_FOUND: &str = "Provider model not found";
const CAPABILITY_MODEL_INACTIVE: &str = "Provider model is inactive";
const CAPABILITY_MODEL_NOT_TEXT: &str = "Provider model does not support text capability testing";
const CAPABILITY_REFERENCE_REQUIRED: &str = "Saved capability test reference is required";
const CAPABILITY_REFERENCE_INVALID: &str = "Saved capability test reference is invalid";
const CAPABILITY_REFERENCE_EQUALS_TARGET: &str = "Reference must differ from target";
const CAPABILITY_UNSUPPORTED_FORMAT: &str = "Endpoint format does not support capability testing";
const CAPABILITY_PINNED_CANDIDATE_INVALID: &str =
    "Pinned endpoint and API key are not available for this model";

/// 能力检测规模；quick 用于 40 题快筛，verify 用于 100 题复核。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum CapabilityMode {
    /// 每个维度 8 题，共 40 题。
    Quick,
    /// 每个维度 20 题，共 100 题。
    Verify,
}

impl CapabilityMode {
    /// 返回每个维度的题数，五个维度始终等额。
    fn questions_per_dimension(self) -> usize {
        match self {
            Self::Quick => 8,
            Self::Verify => 20,
        }
    }

    /// 返回该模式判断偏离所需的最低已解析覆盖率。
    fn minimum_coverage(self) -> f64 {
        match self {
            Self::Quick => 0.90,
            Self::Verify => 0.95,
        }
    }

    /// 返回该模式允许整个同步检测占用的硬时限。
    fn timeout(self) -> Duration {
        match self {
            Self::Quick => Duration::from_secs(10 * 60),
            Self::Verify => Duration::from_secs(20 * 60),
        }
    }

    /// 返回该模式报告偏离前要求的最低参考分差。
    fn minimum_score_gap(self) -> f64 {
        match self {
            Self::Quick => 0.15,
            Self::Verify => 0.10,
        }
    }

    /// 返回该模式单侧精确 McNemar 检验的显著性阈值。
    fn significance(self) -> f64 {
        match self {
            Self::Quick => 0.05,
            Self::Verify => 0.01,
        }
    }
}

/// 题面语言配置；bilingual 在每个维度内平均分配中英文题。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum CapabilityLanguage {
    /// 全部使用中文题面。
    Zh,
    /// 全部使用英文题面。
    En,
    /// 每个维度中英文各半。
    Bilingual,
}

/// 单题实际使用的题面语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum QuestionLanguage {
    /// 中文题面。
    Zh,
    /// 英文题面。
    En,
}

/// 随机客观题的五个等权能力维度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum CapabilityDimension {
    /// 数量与算术关系。
    Quantitative,
    /// 明确前提下的逻辑关系。
    Logical,
    /// 小型算法或状态迭代。
    Algorithmic,
    /// 人造词汇的语言组合关系。
    Language,
    /// 多约束指令遵循。
    Instruction,
}

const CAPABILITY_DIMENSIONS: [CapabilityDimension; 5] = [
    CapabilityDimension::Quantitative,
    CapabilityDimension::Logical,
    CapabilityDimension::Algorithmic,
    CapabilityDimension::Language,
    CapabilityDimension::Instruction,
];

/// 四选一答案；序列化时保持产品合同要求的 A-D 大写字母。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
enum CapabilityChoice {
    /// 第一项。
    #[serde(rename = "A")]
    A,
    /// 第二项。
    #[serde(rename = "B")]
    B,
    /// 第三项。
    #[serde(rename = "C")]
    C,
    /// 第四项。
    #[serde(rename = "D")]
    D,
}

impl CapabilityChoice {
    /// 把零基选项位置转换为稳定的 A-D 答案。
    fn from_index(index: usize) -> Self {
        match index {
            0 => Self::A,
            1 => Self::B,
            2 => Self::C,
            _ => Self::D,
        }
    }

    /// 把单个 ASCII 字母解析为能力检测选项。
    fn from_char(value: char) -> Option<Self> {
        match value.to_ascii_uppercase() {
            'A' => Some(Self::A),
            'B' => Some(Self::B),
            'C' => Some(Self::C),
            'D' => Some(Self::D),
            _ => None,
        }
    }
}

/// 浏览器可提交的最小能力检测请求；题目、答案和 seed 均由服务端持有。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityRequest {
    /// 目标提供商内部 ID。
    provider_id: String,
    /// 目标 ProviderModel 内部 ID，不接受浏览器直接传模型名。
    model_id: String,
    /// 本次目标固定使用的 endpoint ID。
    endpoint_id: String,
    /// 本次目标固定使用的 Key ID。
    api_key_id: String,
    /// 40 题快筛或 100 题复核。
    mode: CapabilityMode,
    /// 中文、英文或双语题面。
    language: CapabilityLanguage,
    /// 为 true 时从目标模型 config 读取并重新校验可信参考。
    #[serde(default)]
    use_saved_reference: bool,
    /// 客户端诊断用请求 ID；不参与题目生成，也不原样转发给上游。
    #[serde(default)]
    request_id: Option<String>,
}

/// 保存在目标模型 config 中的可信参考四元组，不包含任何密钥明文。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct CapabilityReferenceConfig {
    /// 参考提供商内部 ID。
    provider_id: String,
    /// 参考 ProviderModel 内部 ID。
    model_id: String,
    /// 参考 endpoint 内部 ID。
    endpoint_id: String,
    /// 参考 Key 内部 ID。
    api_key_id: String,
}

/// 已完成目录、映射、端点与 Key 校验的单一执行对象。
#[derive(Clone)]
struct PinnedCapabilitySubject {
    /// 提供商目录记录，仅在服务端执行期间使用。
    provider: StoredProviderCatalogProvider,
    /// ProviderModel 记录，用于返回稳定的内部 ID。
    model: StoredAdminProviderModel,
    /// 现有模型测试链解析出的唯一候选。
    candidate: ProviderQueryTestCandidate,
    /// 映射前的请求模型名。
    requested_model: String,
}

/// 返回给前端的固定执行对象描述，不暴露 endpoint URL、Key 名称或凭据。
#[derive(Debug, Serialize)]
struct CapabilitySubjectDescriptor {
    /// 提供商内部 ID。
    provider_id: String,
    /// ProviderModel 内部 ID。
    model_id: String,
    /// endpoint 内部 ID。
    endpoint_id: String,
    /// Key 内部 ID。
    api_key_id: String,
    /// 映射前模型名。
    requested_model: String,
    /// 实际发往固定候选的模型名。
    effective_model: String,
    /// 实际上游协议格式。
    api_format: String,
}

/// 本次检测固定采用的安全请求轮廓。
#[derive(Debug, Serialize)]
struct CapabilityRequestProfile {
    /// 能力检测始终使用非流式请求。
    stream: bool,
    /// 支持的文本协议使用零温度。
    temperature: f64,
    /// 单题最大输出 token 数。
    max_output_tokens: u64,
    /// 能力检测不提供工具。
    tools_enabled: bool,
    /// 能力检测不启用搜索。
    search_enabled: bool,
}

/// 服务端生成且仅在运行期间存在的随机客观题。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CapabilityQuestion {
    /// 题目在当前 seed 下的稳定 UUID v5。
    id: String,
    /// 结果排序所用的固定序号。
    ordinal: usize,
    /// 五维之一。
    dimension: CapabilityDimension,
    /// 实际题面语言。
    language: QuestionLanguage,
    /// 发送给目标与参考的完全相同题面。
    prompt: String,
    /// 唯一正确的 A-D 选项。
    answer: CapabilityChoice,
}

/// 并发队列中区分目标与参考的角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CapabilitySubjectRole {
    /// 被检测目标。
    Target,
    /// 用户指定的可信参考。
    Reference,
}

/// 单次上游调用的有界工作项。
struct CapabilityExecutionTask {
    /// 目标或参考角色。
    role: CapabilitySubjectRole,
    /// 固定候选，Arc 避免为每题复制凭据记录。
    subject: Arc<PinnedCapabilitySubject>,
    /// 本题客观题。
    question: CapabilityQuestion,
    /// 每题唯一且不可关联到凭据的内部 trace ID。
    trace_id: String,
}

/// 单题结果状态；只有 scored 会进入正确率分母。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CapabilityItemStatus {
    /// 已解析出单一选项，可判定对错。
    Scored,
    /// 连接或执行链未得到上游 HTTP 结果。
    NetworkFailure,
    /// 上游明确返回限流。
    RateLimited,
    /// 单题或上游执行超时。
    Timeout,
    /// 上游安全策略或内容过滤阻止回答。
    Filtered,
    /// 模型以自然语言明确拒绝作答。
    Refused,
    /// 输出达到长度限制，不能作为稳定判分样本。
    Truncated,
    /// 有文本但无法解析出唯一最终选项。
    Unparseable,
    /// 上游返回其他非成功状态。
    UpstreamError,
    /// 请求取消后未完成的工作项。
    Cancelled,
}

/// 可从上游响应安全抽取的 token 与费用汇总。
#[derive(Debug, Clone, Default, Serialize)]
struct CapabilityUsage {
    /// 输入 token；上游未提供时为 null。
    input_tokens: Option<u64>,
    /// 输出 token；上游未提供时为 null。
    output_tokens: Option<u64>,
    /// 总 token；上游未提供时由输入与输出相加。
    total_tokens: Option<u64>,
    /// 上游明确提供的美元成本；Aether 不在此处重新计价。
    cost_usd: Option<f64>,
}

impl CapabilityUsage {
    /// 把单题可用 usage 累加到本次对象汇总，缺失字段保持缺失语义。
    fn add(&mut self, other: &Self) {
        self.input_tokens = add_optional_u64(self.input_tokens, other.input_tokens);
        self.output_tokens = add_optional_u64(self.output_tokens, other.output_tokens);
        self.total_tokens = add_optional_u64(self.total_tokens, other.total_tokens);
        self.cost_usd = add_optional_f64(self.cost_usd, other.cost_usd);
    }

    /// 判断上游是否至少提供了一个 usage 或费用字段。
    fn is_empty(&self) -> bool {
        self.input_tokens.is_none()
            && self.output_tokens.is_none()
            && self.total_tokens.is_none()
            && self.cost_usd.is_none()
    }
}

/// 某个目标在单题上的脱敏观察结果。
#[derive(Debug, Clone, Serialize)]
struct CapabilityObservation {
    /// 解析后的 A-D 选项；失败时为 null。
    parsed_option: Option<CapabilityChoice>,
    /// 结果分类。
    status: CapabilityItemStatus,
    /// 已评分时的正确性，其他状态为 null。
    correct: Option<bool>,
    /// 执行 runtime 报告的上游耗时。
    latency_ms: Option<u64>,
    /// 单题可取得的 token 与费用。
    usage: Option<CapabilityUsage>,
}

/// 前端逐题结果；不返回题面、完整请求、响应或推理内容。
#[derive(Debug, Serialize)]
struct CapabilityItemResult {
    /// 服务端题目 ID。
    question_id: String,
    /// 能力维度。
    dimension: CapabilityDimension,
    /// 题面语言。
    language: QuestionLanguage,
    /// 唯一正确选项。
    expected_option: CapabilityChoice,
    /// 目标模型观察结果。
    target: CapabilityObservation,
    /// 参考模型观察结果；未启用参考时为 null。
    reference: Option<CapabilityObservation>,
}

/// 不进入正确率分母的失败分类计数。
#[derive(Debug, Default, Serialize)]
struct CapabilityFailureCounts {
    /// 网络或执行链失败数。
    network_failure: usize,
    /// 限流数。
    rate_limited: usize,
    /// 超时数。
    timeout: usize,
    /// 过滤数。
    filtered: usize,
    /// 明确拒答数。
    refused: usize,
    /// 截断数。
    truncated: usize,
    /// 无法解析数。
    unparseable: usize,
    /// 其他上游错误数。
    upstream_error: usize,
    /// 取消数。
    cancelled: usize,
}

impl CapabilityFailureCounts {
    /// 按单题状态更新对应失败桶，已评分项不计入失败。
    fn record(&mut self, status: CapabilityItemStatus) {
        match status {
            CapabilityItemStatus::Scored => {}
            CapabilityItemStatus::NetworkFailure => self.network_failure += 1,
            CapabilityItemStatus::RateLimited => self.rate_limited += 1,
            CapabilityItemStatus::Timeout => self.timeout += 1,
            CapabilityItemStatus::Filtered => self.filtered += 1,
            CapabilityItemStatus::Refused => self.refused += 1,
            CapabilityItemStatus::Truncated => self.truncated += 1,
            CapabilityItemStatus::Unparseable => self.unparseable += 1,
            CapabilityItemStatus::UpstreamError => self.upstream_error += 1,
            CapabilityItemStatus::Cancelled => self.cancelled += 1,
        }
    }
}

/// 单个能力维度的判分统计。
#[derive(Debug, Serialize)]
struct CapabilityDimensionMetrics {
    /// 能力维度。
    dimension: CapabilityDimension,
    /// 计划题数。
    planned: usize,
    /// 成功解析并评分的题数。
    scored: usize,
    /// 正确题数。
    correct: usize,
    /// 已评分覆盖率，范围 0-1。
    coverage: f64,
    /// 该维度正确率；没有可评分题时为 null。
    score: Option<f64>,
}

/// 目标或参考的完整脱敏统计。
#[derive(Debug, Serialize)]
struct CapabilitySubjectMetrics {
    /// 计划题数。
    planned: usize,
    /// 已评分题数。
    scored: usize,
    /// 正确题数。
    correct: usize,
    /// 已评分覆盖率，范围 0-1。
    coverage: f64,
    /// 五维正确率等权平均；任一维无样本时为 null。
    score: Option<f64>,
    /// 总体二项比例 95% Wilson 下界。
    wilson_low: Option<f64>,
    /// 总体二项比例 95% Wilson 上界。
    wilson_high: Option<f64>,
    /// 五个等权维度的明细。
    dimensions: Vec<CapabilityDimensionMetrics>,
    /// 失败分类。
    failures: CapabilityFailureCounts,
    /// 所有单题上游耗时之和。
    elapsed_ms: u64,
    /// 可取得的 token 与费用汇总。
    usage: Option<CapabilityUsage>,
}

/// 同题配对比较与单侧精确 McNemar 统计。
#[derive(Debug, Serialize)]
struct CapabilityComparison {
    /// 双方均成功解析的配对题数。
    paired: usize,
    /// 配对题覆盖率，范围 0-1。
    paired_coverage: f64,
    /// 参考正确而目标错误的题数。
    reference_only_correct: usize,
    /// 目标正确而参考错误的题数。
    target_only_correct: usize,
    /// 参考总体分减目标总体分。
    score_gap: Option<f64>,
    /// 单侧精确 McNemar 二项尾概率。
    p_value: f64,
}

/// 产品允许展示的六种固定结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CapabilityVerdict {
    /// 未配置参考，仅展示能力画像。
    ProfileOnly,
    /// 快筛未发现大幅偏离。
    NoLargeDeviation,
    /// 快筛达到阈值，建议使用新 seed 复核。
    NeedsVerification,
    /// 复核未发现显著偏离。
    NoSignificantDeviation,
    /// 复核确认与参考存在统计显著的能力偏离。
    SignificantDeviation,
    /// 覆盖、配对或时限不足，不能判断。
    Inconclusive,
}

/// 无法判断时的机器可读原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CapabilityInconclusiveReason {
    /// 整体同步运行达到硬时限。
    TotalTimeout,
    /// 目标已解析覆盖不足。
    TargetCoverage,
    /// 参考已解析覆盖不足。
    ReferenceCoverage,
    /// 双方同题可配对覆盖不足。
    PairedCoverage,
}

/// 能力检测 API 的完整 typed 响应。
#[derive(Debug, Serialize)]
struct CapabilityResponse {
    /// 单次运行 ID。
    run_id: String,
    /// 题目生成合同版本。
    suite_version: &'static str,
    /// 本次服务端 UUID v4 seed。
    seed: String,
    /// quick 或 verify。
    mode: CapabilityMode,
    /// 请求的语言配置。
    language: CapabilityLanguage,
    /// 固定结论。
    verdict: CapabilityVerdict,
    /// 无法判断原因；其他结论为 null。
    inconclusive_reason: Option<CapabilityInconclusiveReason>,
    /// 目标固定候选描述。
    target: CapabilitySubjectDescriptor,
    /// 参考固定候选描述。
    reference: Option<CapabilitySubjectDescriptor>,
    /// 目标评分。
    target_metrics: CapabilitySubjectMetrics,
    /// 参考评分。
    reference_metrics: Option<CapabilitySubjectMetrics>,
    /// 配对统计。
    comparison: Option<CapabilityComparison>,
    /// 不含题面和原始输出的逐题结果。
    items: Vec<CapabilityItemResult>,
    /// 整个同步请求的墙钟耗时。
    elapsed_ms: u64,
    /// 实际请求轮廓。
    request_profile: CapabilityRequestProfile,
    /// 防止把行为结果误读为身份认证的固定免责声明。
    disclaimer: &'static str,
}

/// 候选解析可能返回安全 HTTP 错误或内部 GatewayError。
enum CapabilityResolveFailure {
    /// 已构造的客户端错误响应。
    Response(Response<Body>),
    /// 数据或运行时内部错误。
    Gateway(GatewayError),
}

impl From<GatewayError> for CapabilityResolveFailure {
    /// 保留现有 GatewayError 的集中脱敏与状态映射。
    fn from(value: GatewayError) -> Self {
        Self::Gateway(value)
    }
}

/// 执行模型能力检测：服务端生成题集、固定候选并在同一同步请求内完成评分。
pub(crate) async fn build_admin_provider_query_test_model_capability_response(
    state: &AdminAppState<'_>,
    payload: &Value,
) -> Result<Response<Body>, GatewayError> {
    let request = match serde_json::from_value::<CapabilityRequest>(payload.clone()) {
        Ok(request) => request,
        Err(_) => {
            return Ok(capability_error_response(
                http::StatusCode::BAD_REQUEST,
                CAPABILITY_INVALID_REQUEST,
            ));
        }
    };
    if !capability_request_ids_are_valid(&request) {
        return Ok(capability_error_response(
            http::StatusCode::BAD_REQUEST,
            CAPABILITY_INVALID_REQUEST_IDS,
        ));
    }
    let started_at = Instant::now();
    let target = match resolve_pinned_capability_subject(
        state,
        &request.provider_id,
        &request.model_id,
        &request.endpoint_id,
        &request.api_key_id,
    )
    .await
    {
        Ok(subject) => Arc::new(subject),
        Err(CapabilityResolveFailure::Response(response)) => return Ok(response),
        Err(CapabilityResolveFailure::Gateway(error)) => return Err(error),
    };

    let reference_config = if request.use_saved_reference {
        match read_saved_reference(&target.model) {
            Ok(Some(reference)) => Some(reference),
            Ok(None) => {
                return Ok(capability_error_response(
                    http::StatusCode::BAD_REQUEST,
                    CAPABILITY_REFERENCE_REQUIRED,
                ));
            }
            Err(()) => {
                return Ok(capability_error_response(
                    http::StatusCode::BAD_REQUEST,
                    CAPABILITY_REFERENCE_INVALID,
                ));
            }
        }
    } else {
        None
    };
    if reference_config
        .as_ref()
        .is_some_and(|reference| capability_reference_equals_target(reference, &request))
    {
        return Ok(capability_error_response(
            http::StatusCode::BAD_REQUEST,
            CAPABILITY_REFERENCE_EQUALS_TARGET,
        ));
    }
    let reference = if let Some(reference) = reference_config.as_ref() {
        match resolve_pinned_capability_subject(
            state,
            &reference.provider_id,
            &reference.model_id,
            &reference.endpoint_id,
            &reference.api_key_id,
        )
        .await
        {
            Ok(subject) => Some(Arc::new(subject)),
            Err(CapabilityResolveFailure::Response(_)) => {
                return Ok(capability_error_response(
                    http::StatusCode::BAD_REQUEST,
                    CAPABILITY_REFERENCE_INVALID,
                ));
            }
            Err(CapabilityResolveFailure::Gateway(error)) => return Err(error),
        }
    } else {
        None
    };

    let run_uuid = Uuid::new_v4();
    let seed = Uuid::new_v4();
    let questions = generate_capability_suite(seed, request.mode, request.language);
    let (observations, total_timed_out) = execute_capability_suite(
        state,
        run_uuid,
        request.mode,
        &questions,
        Arc::clone(&target),
        reference.as_ref().map(Arc::clone),
    )
    .await;
    let mut items = build_capability_items(
        &questions,
        observations,
        reference.is_some(),
        total_timed_out,
    );
    let target_metrics = build_subject_metrics(&items, CapabilitySubjectRole::Target);
    let reference_metrics = reference
        .as_ref()
        .map(|_| build_subject_metrics(&items, CapabilitySubjectRole::Reference));
    let comparison = reference_metrics
        .as_ref()
        .map(|metrics| build_capability_comparison(&items, &target_metrics, metrics));
    let (verdict, inconclusive_reason) = decide_capability_verdict(
        request.mode,
        &target_metrics,
        reference_metrics.as_ref(),
        comparison.as_ref(),
        total_timed_out,
    );
    items.shrink_to_fit();

    Ok(Json(CapabilityResponse {
        run_id: run_uuid.to_string(),
        suite_version: CAPABILITY_SUITE_VERSION,
        seed: seed.to_string(),
        mode: request.mode,
        language: request.language,
        verdict,
        inconclusive_reason,
        target: capability_subject_descriptor(&target),
        reference: reference.as_deref().map(capability_subject_descriptor),
        target_metrics,
        reference_metrics,
        comparison,
        items,
        elapsed_ms: u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
        request_profile: CapabilityRequestProfile {
            stream: false,
            temperature: 0.0,
            max_output_tokens: CAPABILITY_MAX_OUTPUT_TOKENS,
            tools_enabled: false,
            search_enabled: false,
        },
        disclaimer: "This result measures capability behavior only and does not authenticate model identity.",
    })
    .into_response())
}

/// 校验四个目标 ID 均为非空；其他字段由 serde 枚举约束。
fn capability_request_ids_are_valid(request: &CapabilityRequest) -> bool {
    [
        request.provider_id.as_str(),
        request.model_id.as_str(),
        request.endpoint_id.as_str(),
        request.api_key_id.as_str(),
    ]
    .iter()
    .all(|value| !value.trim().is_empty())
        && request
            .request_id
            .as_deref()
            .is_none_or(|value| value.trim().len() <= 128)
}

/// 构造不包含内部错误、候选或凭据细节的 provider-query 客户端错误。
fn capability_error_response(status: http::StatusCode, detail: &'static str) -> Response<Body> {
    (status, Json(json!({ "detail": detail }))).into_response()
}

/// 从目标 ProviderModel 的原始 config 读取可信参考；结构损坏时拒绝自动替换。
fn read_saved_reference(
    model: &StoredAdminProviderModel,
) -> Result<Option<CapabilityReferenceConfig>, ()> {
    let Some(value) = model
        .config
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|config| config.get("capability_test_reference"))
    else {
        return Ok(None);
    };
    let reference =
        serde_json::from_value::<CapabilityReferenceConfig>(value.clone()).map_err(|_| ())?;
    let valid = [
        reference.provider_id.as_str(),
        reference.model_id.as_str(),
        reference.endpoint_id.as_str(),
        reference.api_key_id.as_str(),
    ]
    .iter()
    .all(|value| !value.trim().is_empty());
    valid.then_some(reference).ok_or(()).map(Some)
}

/// 判断保存的参考四元组是否与本次目标完全相同。
fn capability_reference_equals_target(
    reference: &CapabilityReferenceConfig,
    request: &CapabilityRequest,
) -> bool {
    reference.provider_id == request.provider_id
        && reference.model_id == request.model_id
        && reference.endpoint_id == request.endpoint_id
        && reference.api_key_id == request.api_key_id
}

/// 复用现有模型测试映射、端点/Key 过滤与 transport 能力，解析恰好一个固定文本候选。
async fn resolve_pinned_capability_subject(
    state: &AdminAppState<'_>,
    provider_id: &str,
    model_id: &str,
    endpoint_id: &str,
    api_key_id: &str,
) -> Result<PinnedCapabilitySubject, CapabilityResolveFailure> {
    let Some(model) = state
        .get_admin_provider_model(provider_id, model_id)
        .await?
    else {
        return Err(CapabilityResolveFailure::Response(
            capability_error_response(http::StatusCode::NOT_FOUND, CAPABILITY_MODEL_NOT_FOUND),
        ));
    };
    if !model.is_active {
        return Err(CapabilityResolveFailure::Response(
            capability_error_response(http::StatusCode::BAD_REQUEST, CAPABILITY_MODEL_INACTIVE),
        ));
    }
    if !capability_model_supports_text_generation(&model) {
        return Err(CapabilityResolveFailure::Response(
            capability_error_response(http::StatusCode::BAD_REQUEST, CAPABILITY_MODEL_NOT_TEXT),
        ));
    }
    let Some(provider) = state
        .app()
        .read_provider_catalog_providers_by_ids(&[provider_id.to_string()])
        .await?
        .into_iter()
        .find(|provider| provider.id == provider_id && provider.is_active)
    else {
        return Err(CapabilityResolveFailure::Response(
            capability_error_response(
                http::StatusCode::NOT_FOUND,
                ADMIN_PROVIDER_QUERY_PROVIDER_NOT_FOUND_DETAIL,
            ),
        ));
    };
    let requested_model = model
        .global_model_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(model.provider_model_name.as_str())
        .to_string();
    let candidate_payload = json!({
        "provider_id": provider_id,
        "model": requested_model,
        "endpoint_id": endpoint_id,
        "api_key_id": api_key_id,
        "mode": "global",
        "apply_model_mapping": true,
    });
    let mut candidates = provider_query_build_kiro_test_candidates(
        state,
        &provider,
        &candidate_payload,
        Some(requested_model.as_str()),
        Some(&model),
    )
    .await
    .map_err(CapabilityResolveFailure::Response)?;
    if candidates.len() != 1
        || !candidates[0].endpoint.is_active
        || candidates[0].scheduler_skip_reason.is_some()
    {
        return Err(CapabilityResolveFailure::Response(
            capability_error_response(
                http::StatusCode::BAD_REQUEST,
                CAPABILITY_PINNED_CANDIDATE_INVALID,
            ),
        ));
    }
    let candidate = candidates.remove(0);
    let format = provider_query_normalize_api_format_alias(&candidate.endpoint.api_format);
    let adapter = provider_query_test_adapter_for_provider_api_format(
        &provider.provider_type,
        &candidate.endpoint.api_format,
    );
    if !matches!(
        format.as_str(),
        "openai:chat" | "openai:responses" | "claude:messages" | "gemini:generate_content"
    ) || !matches!(
        adapter,
        Some(
            ProviderQueryTestAdapter::Standard
                | ProviderQueryTestAdapter::Grok
                | ProviderQueryTestAdapter::Antigravity
        )
    ) {
        return Err(CapabilityResolveFailure::Response(
            capability_error_response(http::StatusCode::BAD_REQUEST, CAPABILITY_UNSUPPORTED_FORMAT),
        ));
    }

    Ok(PinnedCapabilitySubject {
        provider,
        model,
        candidate,
        requested_model,
    })
}

/// 合并 Global/ProviderModel/映射声明判断文本生成族；历史无声明模型保持生成族兼容。
fn capability_model_supports_text_generation(model: &StoredAdminProviderModel) -> bool {
    let mut declared = crate::model_metadata::global_model_declared_families(
        model.global_model_config.as_ref(),
        model.global_model_supported_capabilities.as_ref(),
    );
    if let Some(config) = model.config.as_ref() {
        declared.merge(crate::model_metadata::declared_model_families(
            config, false,
        ));
    }
    if let Some(mappings) = model
        .provider_model_mappings
        .as_ref()
        .and_then(Value::as_array)
    {
        for mapping in mappings {
            declared.merge(crate::model_metadata::declared_model_families(
                mapping, true,
            ));
        }
    }
    if declared.supports_api_format("openai:chat") {
        return true;
    }
    let explicitly_non_text = [model.config.as_ref(), model.global_model_config.as_ref()]
        .into_iter()
        .flatten()
        .filter_map(|config| config.get("model_type").and_then(Value::as_str))
        .any(|model_type| {
            matches!(
                model_type.trim().to_ascii_lowercase().as_str(),
                "embedding" | "rerank" | "image" | "video" | "audio" | "realtime" | "files"
            )
        });
    !explicitly_non_text && declared.supports_api_format_or_legacy_generation("openai:chat")
}

/// 生成前端可安全展示的固定候选描述。
fn capability_subject_descriptor(subject: &PinnedCapabilitySubject) -> CapabilitySubjectDescriptor {
    CapabilitySubjectDescriptor {
        provider_id: subject.provider.id.clone(),
        model_id: subject.model.id.clone(),
        endpoint_id: subject.candidate.endpoint.id.clone(),
        api_key_id: subject.candidate.key.id.clone(),
        requested_model: subject.requested_model.clone(),
        effective_model: subject.candidate.effective_model.clone(),
        api_format: provider_query_normalize_api_format_alias(
            &subject.candidate.endpoint.api_format,
        ),
    }
}

/// 按 UUID v5 从 seed 与标签派生稳定伪随机整数，不新增随机数依赖。
fn capability_derived_u64(seed: Uuid, label: &str) -> u64 {
    let derived = Uuid::new_v5(&seed, label.as_bytes());
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&derived.as_bytes()[..8]);
    u64::from_le_bytes(bytes)
}

/// 对四个互异选项做确定性 Fisher-Yates 排列，并同步返回正确选项字母。
fn shuffle_capability_options(
    seed: Uuid,
    label: &str,
    mut options: Vec<String>,
    correct_index: usize,
) -> (Vec<String>, CapabilityChoice) {
    let correct = options[correct_index].clone();
    let mut state = capability_derived_u64(seed, label);
    for index in (1..options.len()).rev() {
        let selected = usize::try_from(state % u64::try_from(index + 1).unwrap_or(1)).unwrap_or(0);
        options.swap(index, selected);
        state = state.rotate_left(17) ^ 0x9e37_79b9_7f4a_7c15;
    }
    let answer_index = options
        .iter()
        .position(|option| option == &correct)
        .unwrap_or(0);
    (options, CapabilityChoice::from_index(answer_index))
}

/// 把题干和四个选项组装为只允许返回 A-D 的中英文题面。
fn format_capability_prompt(stem: &str, options: &[String], language: QuestionLanguage) -> String {
    let instruction = match language {
        QuestionLanguage::Zh => "只输出一个选项字母（A、B、C 或 D），不要解释。",
        QuestionLanguage::En => {
            "Output only one option letter (A, B, C, or D), with no explanation."
        }
    };
    format!(
        "{stem}\nA. {}\nB. {}\nC. {}\nD. {}\n{instruction}",
        options[0], options[1], options[2], options[3]
    )
}

/// 按版本、seed、模式和语言生成等额五维题集；同 seed 的结果逐字可复现。
fn generate_capability_suite(
    seed: Uuid,
    mode: CapabilityMode,
    language: CapabilityLanguage,
) -> Vec<CapabilityQuestion> {
    let per_dimension = mode.questions_per_dimension();
    let mut questions = Vec::with_capacity(per_dimension * CAPABILITY_DIMENSIONS.len());
    for dimension in CAPABILITY_DIMENSIONS {
        for index in 0..per_dimension {
            let question_language = match language {
                CapabilityLanguage::Zh => QuestionLanguage::Zh,
                CapabilityLanguage::En => QuestionLanguage::En,
                CapabilityLanguage::Bilingual if index < per_dimension / 2 => QuestionLanguage::Zh,
                CapabilityLanguage::Bilingual => QuestionLanguage::En,
            };
            questions.push(build_capability_question(
                seed,
                dimension,
                index,
                questions.len(),
                question_language,
            ));
        }
    }
    questions
}

/// 用运行时派生参数构造一题唯一答案客观题，不依赖公开题库或 LLM 裁判。
fn build_capability_question(
    seed: Uuid,
    dimension: CapabilityDimension,
    index: usize,
    ordinal: usize,
    language: QuestionLanguage,
) -> CapabilityQuestion {
    let label = format!("{CAPABILITY_SUITE_VERSION}:{dimension:?}:{index}");
    let state = capability_derived_u64(seed, &label);
    let (stem, options, correct_index) = match dimension {
        CapabilityDimension::Quantitative => {
            let left = 10 + i64::try_from(state % 41).unwrap_or(0);
            let right = 3 + i64::try_from((state >> 8) % 18).unwrap_or(0);
            let factor = 2 + i64::try_from((state >> 16) % 8).unwrap_or(0);
            let answer = (left + right) * factor;
            let stem = match language {
                QuestionLanguage::Zh => format!("计算：({left} + {right}) × {factor} = ?"),
                QuestionLanguage::En => format!("Calculate: ({left} + {right}) × {factor} = ?"),
            };
            (
                stem,
                vec![answer, answer + 1, answer - 1, answer + factor]
                    .into_iter()
                    .map(|value| value.to_string())
                    .collect(),
                0,
            )
        }
        CapabilityDimension::Logical => {
            let names = [
                format!("K{}", 10 + state % 80),
                format!("M{}", 10 + (state >> 8) % 80),
                format!("R{}", 10 + (state >> 16) % 80),
                format!("T{}", 10 + (state >> 24) % 80),
            ];
            let ask_heaviest = state & 1 == 0;
            let stem = match (language, ask_heaviest) {
                (QuestionLanguage::Zh, true) => format!(
                    "已知 {} 比 {} 重，{} 比 {} 重，{} 比 {} 重。哪个最重？",
                    names[0], names[1], names[1], names[2], names[2], names[3]
                ),
                (QuestionLanguage::Zh, false) => format!(
                    "已知 {} 比 {} 重，{} 比 {} 重，{} 比 {} 重。哪个最轻？",
                    names[0], names[1], names[1], names[2], names[2], names[3]
                ),
                (QuestionLanguage::En, true) => format!(
                    "{} is heavier than {}, {} is heavier than {}, and {} is heavier than {}. Which is heaviest?",
                    names[0], names[1], names[1], names[2], names[2], names[3]
                ),
                (QuestionLanguage::En, false) => format!(
                    "{} is heavier than {}, {} is heavier than {}, and {} is heavier than {}. Which is lightest?",
                    names[0], names[1], names[1], names[2], names[2], names[3]
                ),
            };
            (stem, names.to_vec(), if ask_heaviest { 0 } else { 3 })
        }
        CapabilityDimension::Algorithmic => {
            let start = 2 + i64::try_from(state % 12).unwrap_or(0);
            let delta = 1 + i64::try_from((state >> 8) % 7).unwrap_or(0);
            let after_one = start * 2 + delta;
            let after_two = after_one * 2 + delta;
            let answer = after_two * 2 + delta;
            let stem = match language {
                QuestionLanguage::Zh => {
                    format!("令 x = {start}，连续执行 3 次 x = 2x + {delta}。最终 x 是多少？")
                }
                QuestionLanguage::En => format!(
                    "Set x = {start}. Apply x = 2x + {delta} exactly 3 times. What is the final x?"
                ),
            };
            (
                stem,
                vec![answer, after_two, answer - 1, answer + 2]
                    .into_iter()
                    .map(|value| value.to_string())
                    .collect(),
                0,
            )
        }
        CapabilityDimension::Language => {
            let adjective_one = format!("mira{}", state % 10);
            let adjective_two = format!("selo{}", (state >> 8) % 10);
            let noun_one = format!("nava{}", (state >> 16) % 10);
            let noun_two = format!("tori{}", (state >> 24) % 10);
            let stem = match language {
                QuestionLanguage::Zh => format!(
                    "在人造语言中，{adjective_one}=红色，{adjective_two}=蓝色，{noun_one}=圆形，{noun_two}=方形；修饰词在名词前。哪个短语表示“红色方形”？"
                ),
                QuestionLanguage::En => format!(
                    "In a made-up language, {adjective_one}=red, {adjective_two}=blue, {noun_one}=circle, and {noun_two}=square; modifiers precede nouns. Which phrase means 'red square'?"
                ),
            };
            (
                stem,
                vec![
                    format!("{adjective_one} {noun_two}"),
                    format!("{adjective_two} {noun_two}"),
                    format!("{adjective_one} {noun_one}"),
                    format!("{noun_two} {adjective_one}"),
                ],
                0,
            )
        }
        CapabilityDimension::Instruction => {
            let prefix = char::from(b'P' + u8::try_from(state % 6).unwrap_or(0));
            let other_prefix = char::from(b'V' + u8::try_from((state >> 8) % 5).unwrap_or(0));
            let suffix = char::from(b'K' + u8::try_from((state >> 16) % 6).unwrap_or(0));
            let other_suffix = char::from(b'R' + u8::try_from((state >> 24) % 6).unwrap_or(0));
            let digits = 10 + (state >> 32) % 90;
            let one_digit = 1 + (state >> 40) % 9;
            let stem = match language {
                QuestionLanguage::Zh => format!(
                    "选择同时满足以下条件的代码：以 {prefix} 开头，恰好包含两个数字，并以 {suffix} 结尾。"
                ),
                QuestionLanguage::En => format!(
                    "Choose the code that starts with {prefix}, contains exactly two digits, and ends with {suffix}."
                ),
            };
            (
                stem,
                vec![
                    format!("{prefix}A{digits}{suffix}"),
                    format!("{other_prefix}A{digits}{suffix}"),
                    format!("{prefix}A{one_digit}{suffix}"),
                    format!("{prefix}A{digits}{other_suffix}"),
                ],
                0,
            )
        }
    };
    let (options, answer) =
        shuffle_capability_options(seed, &format!("{label}:options"), options, correct_index);
    CapabilityQuestion {
        id: Uuid::new_v5(&seed, format!("{label}:id").as_bytes()).to_string(),
        ordinal,
        dimension,
        language,
        prompt: format_capability_prompt(&stem, &options, language),
        answer,
    }
}

/// 以全局并发上限 4 执行目标与参考的同一题集；达到 deadline 时丢弃未完成 future。
async fn execute_capability_suite(
    state: &AdminAppState<'_>,
    run_id: Uuid,
    mode: CapabilityMode,
    questions: &[CapabilityQuestion],
    target: Arc<PinnedCapabilitySubject>,
    reference: Option<Arc<PinnedCapabilitySubject>>,
) -> (
    BTreeMap<(CapabilitySubjectRole, usize), CapabilityObservation>,
    bool,
) {
    let mut tasks = Vec::with_capacity(questions.len() * if reference.is_some() { 2 } else { 1 });
    for question in questions {
        for (role, subject) in [(CapabilitySubjectRole::Target, Arc::clone(&target))]
            .into_iter()
            .chain(
                reference
                    .as_ref()
                    .map(|subject| (CapabilitySubjectRole::Reference, Arc::clone(subject))),
            )
        {
            tasks.push(CapabilityExecutionTask {
                role,
                subject,
                question: question.clone(),
                trace_id: Uuid::new_v5(&run_id, format!("{role:?}:{}", question.id).as_bytes())
                    .to_string(),
            });
        }
    }

    let deadline = Instant::now() + mode.timeout();
    let mut pending = Box::pin(
        stream::iter(tasks)
            .map(|task| async move {
                let key = (task.role, task.question.ordinal);
                let observation = execute_capability_task(state, &task).await;
                (key, observation)
            })
            .buffer_unordered(CAPABILITY_MAX_CONCURRENCY),
    );
    let mut observations = BTreeMap::new();
    let mut timed_out = false;
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {
                timed_out = true;
                break;
            }
            next = pending.next() => match next {
                Some((key, observation)) => {
                    observations.insert(key, observation);
                }
                None => break,
            }
        }
    }
    drop(pending);
    (observations, timed_out)
}

/// 按目标协议生成确定性请求体，确保 Responses 使用 input 而不是联通测试默认消息。
fn build_capability_request_body(api_format: &str, prompt: &str) -> Value {
    if provider_query_normalize_api_format_alias(api_format) == "openai:responses" {
        json!({
            "input": prompt,
            "temperature": 0,
            "max_output_tokens": CAPABILITY_MAX_OUTPUT_TOKENS,
            "stream": false,
        })
    } else {
        json!({
            "messages": [{ "role": "user", "content": prompt }],
            "temperature": 0,
            "max_tokens": CAPABILITY_MAX_OUTPUT_TOKENS,
            "stream": false,
        })
    }
}

/// 使用现有 adapter 与 execution runtime 执行一题，并只保留脱敏后的判分信息。
async fn execute_capability_task(
    state: &AdminAppState<'_>,
    task: &CapabilityExecutionTask,
) -> CapabilityObservation {
    let request_body = build_capability_request_body(
        &task.subject.candidate.endpoint.api_format,
        &task.question.prompt,
    );
    let payload = json!({
        "provider_id": task.subject.provider.id,
        "model": task.subject.requested_model,
        "endpoint_id": task.subject.candidate.endpoint.id,
        "api_key_id": task.subject.candidate.key.id,
        "mode": "global",
        "apply_model_mapping": true,
        "request_body": request_body,
    });
    let adapter = provider_query_test_adapter_for_provider_api_format(
        &task.subject.provider.provider_type,
        &task.subject.candidate.endpoint.api_format,
    );
    let execution = match adapter {
        Some(ProviderQueryTestAdapter::Standard) => {
            provider_query_execute_standard_test_candidate(
                state,
                &task.subject.provider,
                &task.subject.candidate,
                &payload,
                "/api/admin/provider-query/test-model-capability",
                &task.trace_id,
            )
            .await
        }
        Some(ProviderQueryTestAdapter::Grok) => {
            provider_query_execute_grok_test_candidate(
                state,
                &task.subject.provider,
                &task.subject.candidate,
                &payload,
                "/api/admin/provider-query/test-model-capability",
                &task.trace_id,
            )
            .await
        }
        Some(ProviderQueryTestAdapter::Antigravity) => {
            provider_query_execute_antigravity_test_candidate(
                state,
                &task.subject.provider,
                &task.subject.candidate,
                &payload,
                "/api/admin/provider-query/test-model-capability",
                &task.trace_id,
                &task.subject.requested_model,
            )
            .await
        }
        _ => Ok(provider_query_skipped_execution_outcome(
            Value::Null,
            CAPABILITY_UNSUPPORTED_FORMAT,
        )),
    };
    match execution {
        Ok(execution) => capability_observation_from_execution(&execution, task.question.answer),
        Err(error) => CapabilityObservation {
            parsed_option: None,
            status: classify_capability_gateway_error(&error),
            correct: None,
            latency_ms: None,
            usage: None,
        },
    }
}

/// 将现有模型测试执行结果归一化为评分状态，不复制或返回上游原文。
fn capability_observation_from_execution(
    execution: &ProviderQueryExecutionOutcome,
    expected: CapabilityChoice,
) -> CapabilityObservation {
    let latency_ms = execution.latency_ms;
    let Some(body) = execution.response_body.as_ref() else {
        return CapabilityObservation {
            parsed_option: None,
            status: classify_capability_execution_failure(execution),
            correct: None,
            latency_ms,
            usage: None,
        };
    };
    let usage = extract_capability_usage(body).filter(|usage| !usage.is_empty());
    if execution.status != "success" {
        return CapabilityObservation {
            parsed_option: None,
            status: classify_capability_execution_failure(execution),
            correct: None,
            latency_ms,
            usage,
        };
    }
    if capability_response_has_marker(
        body,
        &[
            "finish_reason",
            "stop_reason",
            "finishReason",
            "blockReason",
            "reason",
        ],
        &[
            "content_filter",
            "safety",
            "recitation",
            "blocked",
            "prohibited_content",
            "blocklist",
        ],
    ) {
        return CapabilityObservation {
            parsed_option: None,
            status: CapabilityItemStatus::Filtered,
            correct: None,
            latency_ms,
            usage,
        };
    }
    if capability_response_has_marker(
        body,
        &["finish_reason", "stop_reason", "finishReason", "reason"],
        &["length", "max_tokens", "max_token", "max_output_tokens"],
    ) {
        return CapabilityObservation {
            parsed_option: None,
            status: CapabilityItemStatus::Truncated,
            correct: None,
            latency_ms,
            usage,
        };
    }
    if capability_response_has_structured_refusal(body) {
        return CapabilityObservation {
            parsed_option: None,
            status: CapabilityItemStatus::Refused,
            correct: None,
            latency_ms,
            usage,
        };
    }
    let response_text = extract_capability_response_text(body);
    if response_text
        .as_deref()
        .is_some_and(capability_text_is_refusal)
    {
        return CapabilityObservation {
            parsed_option: None,
            status: CapabilityItemStatus::Refused,
            correct: None,
            latency_ms,
            usage,
        };
    }
    let parsed_option = response_text.as_deref().and_then(parse_capability_choice);
    CapabilityObservation {
        parsed_option,
        status: if parsed_option.is_some() {
            CapabilityItemStatus::Scored
        } else {
            CapabilityItemStatus::Unparseable
        },
        correct: parsed_option.map(|value| value == expected),
        latency_ms,
        usage,
    }
}

/// 将执行 runtime 的 typed 错误归入超时或网络失败，避免把硬时限误报为网络问题。
fn classify_capability_gateway_error(error: &GatewayError) -> CapabilityItemStatus {
    let message = match error {
        GatewayError::LocalExecutionPlanningTimeout { .. }
        | GatewayError::AdmissionTimeout { .. } => return CapabilityItemStatus::Timeout,
        GatewayError::Client { status, .. } if *status == http::StatusCode::TOO_MANY_REQUESTS => {
            return CapabilityItemStatus::RateLimited;
        }
        GatewayError::UpstreamUnavailable { message, .. }
        | GatewayError::ControlUnavailable { message, .. }
        | GatewayError::Client { message, .. }
        | GatewayError::Internal(message) => message,
    };
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("timeout") || normalized.contains("timed out") {
        CapabilityItemStatus::Timeout
    } else {
        CapabilityItemStatus::NetworkFailure
    }
}

/// 根据 HTTP 状态和脱敏错误摘要区分网络、限流、超时与其他上游错误。
fn classify_capability_execution_failure(
    execution: &ProviderQueryExecutionOutcome,
) -> CapabilityItemStatus {
    if execution.status == "skipped" {
        return CapabilityItemStatus::NetworkFailure;
    }
    if execution.status_code == Some(429) {
        return CapabilityItemStatus::RateLimited;
    }
    let timeout_message = execution.error_message.as_deref().is_some_and(|value| {
        let normalized = value.to_ascii_lowercase();
        normalized.contains("timeout") || normalized.contains("timed out")
    });
    if matches!(execution.status_code, Some(408 | 504)) || timeout_message {
        return CapabilityItemStatus::Timeout;
    }
    let filtered_body = execution.response_body.as_ref().is_some_and(|body| {
        capability_response_has_marker(
            body,
            &[
                "finish_reason",
                "stop_reason",
                "finishReason",
                "blockReason",
                "reason",
                "code",
                "type",
            ],
            &[
                "content_filter",
                "safety",
                "recitation",
                "policy_violation",
                "prohibited_content",
                "blocklist",
            ],
        )
    });
    let filtered_message = execution.error_message.as_deref().is_some_and(|message| {
        let normalized = message.to_ascii_lowercase();
        normalized.contains("content filter")
            || normalized.contains("content_filter")
            || normalized.contains("safety policy")
            || normalized.contains("policy violation")
            || normalized.contains("policy_violation")
            || normalized.contains("content was flagged")
    });
    if filtered_body || filtered_message {
        return CapabilityItemStatus::Filtered;
    }
    if execution.status_code.is_none() {
        CapabilityItemStatus::NetworkFailure
    } else {
        CapabilityItemStatus::UpstreamError
    }
}

/// 从四种受支持的聚合响应形状抽取可见文本；不会回传或记录该文本。
fn extract_capability_response_text(body: &Value) -> Option<String> {
    if let Some(text) = body
        .get("output_text")
        .or_else(|| body.get("output").filter(|value| value.is_string()))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(text.to_string());
    }
    if let Some(content) = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(capability_text_from_value)
    {
        return Some(content);
    }
    for value in [body.get("output"), body.get("content")]
        .into_iter()
        .flatten()
    {
        if let Some(text) = capability_text_from_value(value) {
            return Some(text);
        }
    }
    if let Some(text) = body
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|candidates| candidates.first())
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(capability_text_from_value)
    {
        return Some(text);
    }
    body.get("response")
        .and_then(extract_capability_response_text)
}

/// 从字符串、文本 part 数组或嵌套 content 中拼接可见文本。
fn capability_text_from_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => (!text.trim().is_empty()).then(|| text.trim().to_string()),
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(capability_text_from_value)
                .collect::<Vec<_>>()
                .join("\n");
            (!text.is_empty()).then_some(text)
        }
        Value::Object(object) => object
            .get("text")
            .or_else(|| object.get("content"))
            .and_then(capability_text_from_value),
        _ => None,
    }
}

/// 识别 OpenAI Chat/Responses 的结构化 refusal 字段，避免将明确拒答误记为无法解析。
fn capability_response_has_structured_refusal(body: &Value) -> bool {
    match body {
        Value::Array(values) => values
            .iter()
            .any(capability_response_has_structured_refusal),
        Value::Object(values) => {
            values
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("refusal"))
                || values
                    .get("refusal")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty())
                || values
                    .values()
                    .any(capability_response_has_structured_refusal)
        }
        _ => false,
    }
}

/// 在嵌套响应中查找过滤或截断终止标记，键和值均不区分大小写。
fn capability_response_has_marker(body: &Value, keys: &[&str], markers: &[&str]) -> bool {
    match body {
        Value::Array(values) => values
            .iter()
            .any(|value| capability_response_has_marker(value, keys, markers)),
        Value::Object(values) => values.iter().any(|(key, value)| {
            let key_matches = keys
                .iter()
                .any(|expected| key.eq_ignore_ascii_case(expected));
            let value_matches = value.as_str().is_some_and(|raw| {
                let normalized = raw.to_ascii_lowercase();
                markers.iter().any(|marker| normalized.contains(marker))
            });
            (key_matches && value_matches) || capability_response_has_marker(value, keys, markers)
        }),
        _ => false,
    }
}

/// 识别常见中英文明确拒答短语；普通解释仍交给严格答案解析器判为 unparseable。
fn capability_text_is_refusal(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    [
        "i cannot answer",
        "i can't answer",
        "i can’t answer",
        "unable to answer",
        "cannot comply",
        "can't comply",
        "can’t comply",
        "无法回答",
        "不能回答",
        "拒绝回答",
        "无法协助",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

/// 解析明确单一选项，或最后一行 `FINAL: X` / `答案: X`；多选项文本拒绝判分。
fn parse_capability_choice(text: &str) -> Option<CapabilityChoice> {
    let last_line = text
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())?
        .trim();
    let last_upper = last_line.to_ascii_uppercase();
    if last_upper.starts_with("FINAL") || last_line.starts_with("答案") {
        let rest = if last_upper.starts_with("FINAL") {
            &last_line["FINAL".len()..]
        } else {
            &last_line["答案".len()..]
        };
        return unique_capability_choice(rest.trim_start_matches([':', '：', ' ', '\t']));
    }
    unique_capability_choice(text)
}

/// 扫描独立 A-D token，只有全文出现唯一一种候选字母时才返回答案。
fn unique_capability_choice(text: &str) -> Option<CapabilityChoice> {
    let chars = text.char_indices().collect::<Vec<_>>();
    let mut choices = BTreeSet::new();
    for (position, (byte_index, value)) in chars.iter().enumerate() {
        let Some(choice) = CapabilityChoice::from_char(*value) else {
            continue;
        };
        let previous_is_ascii_letter = position
            .checked_sub(1)
            .and_then(|index| chars.get(index))
            .is_some_and(|(_, value)| value.is_ascii_alphabetic());
        let next_is_ascii_letter = chars
            .get(position + 1)
            .is_some_and(|(_, value)| value.is_ascii_alphabetic());
        if !previous_is_ascii_letter && !next_is_ascii_letter && *byte_index < text.len() {
            choices.insert(choice);
        }
    }
    (choices.len() == 1).then(|| *choices.iter().next().expect("one choice exists"))
}

/// 从 OpenAI、Claude、Gemini 或包装后的 response usage 中提取公共 token/费用字段。
fn extract_capability_usage(body: &Value) -> Option<CapabilityUsage> {
    let usage = body
        .get("usage")
        .or_else(|| body.get("usageMetadata"))
        .or_else(|| body.get("response").and_then(|value| value.get("usage")))?;
    let input_tokens = capability_u64_field(
        usage,
        &["input_tokens", "prompt_tokens", "promptTokenCount"],
    );
    let output_tokens = capability_u64_field(
        usage,
        &["output_tokens", "completion_tokens", "candidatesTokenCount"],
    );
    let total_tokens =
        capability_u64_field(usage, &["total_tokens", "totalTokenCount"]).or_else(|| {
            match (input_tokens, output_tokens) {
                (Some(input), Some(output)) => Some(input.saturating_add(output)),
                _ => None,
            }
        });
    let cost_usd = capability_f64_field(usage, &["cost_usd", "total_cost_usd", "cost"])
        .or_else(|| capability_f64_field(body, &["cost_usd", "total_cost_usd"]));
    Some(CapabilityUsage {
        input_tokens,
        output_tokens,
        total_tokens,
        cost_usd,
    })
}

/// 按候选字段名读取非负整数 token 值。
fn capability_u64_field(value: &Value, fields: &[&str]) -> Option<u64> {
    fields
        .iter()
        .find_map(|field| value.get(*field).and_then(Value::as_u64))
}

/// 按候选字段名读取有限且非负的费用值。
fn capability_f64_field(value: &Value, fields: &[&str]) -> Option<f64> {
    fields
        .iter()
        .find_map(|field| value.get(*field).and_then(Value::as_f64))
        .filter(|value| value.is_finite() && *value >= 0.0)
}

/// 将已完成观察与题集按序合并；总超时未完成项明确标记为 timeout。
fn build_capability_items(
    questions: &[CapabilityQuestion],
    mut observations: BTreeMap<(CapabilitySubjectRole, usize), CapabilityObservation>,
    has_reference: bool,
    total_timed_out: bool,
) -> Vec<CapabilityItemResult> {
    questions
        .iter()
        .map(|question| CapabilityItemResult {
            question_id: question.id.clone(),
            dimension: question.dimension,
            language: question.language,
            expected_option: question.answer,
            target: observations
                .remove(&(CapabilitySubjectRole::Target, question.ordinal))
                .unwrap_or_else(|| missing_capability_observation(total_timed_out)),
            reference: has_reference.then(|| {
                observations
                    .remove(&(CapabilitySubjectRole::Reference, question.ordinal))
                    .unwrap_or_else(|| missing_capability_observation(total_timed_out))
            }),
        })
        .collect()
}

/// 为 deadline 或客户端取消后未完成的单题生成明确失败状态。
fn missing_capability_observation(total_timed_out: bool) -> CapabilityObservation {
    CapabilityObservation {
        parsed_option: None,
        status: if total_timed_out {
            CapabilityItemStatus::Timeout
        } else {
            CapabilityItemStatus::Cancelled
        },
        correct: None,
        latency_ms: None,
        usage: None,
    }
}

/// 按角色汇总覆盖率、五维等权分、Wilson 区间、失败、耗时与 usage。
fn build_subject_metrics(
    items: &[CapabilityItemResult],
    role: CapabilitySubjectRole,
) -> CapabilitySubjectMetrics {
    let mut dimensions = Vec::with_capacity(CAPABILITY_DIMENSIONS.len());
    let mut failures = CapabilityFailureCounts::default();
    let mut usage = CapabilityUsage::default();
    let mut elapsed_ms = 0_u64;
    for dimension in CAPABILITY_DIMENSIONS {
        let dimension_items = items
            .iter()
            .filter(|item| item.dimension == dimension)
            .filter_map(|item| match role {
                CapabilitySubjectRole::Target => Some(&item.target),
                CapabilitySubjectRole::Reference => item.reference.as_ref(),
            })
            .collect::<Vec<_>>();
        let planned = dimension_items.len();
        let scored = dimension_items
            .iter()
            .filter(|item| item.status == CapabilityItemStatus::Scored)
            .count();
        let correct = dimension_items
            .iter()
            .filter(|item| item.correct == Some(true))
            .count();
        dimensions.push(CapabilityDimensionMetrics {
            dimension,
            planned,
            scored,
            correct,
            coverage: ratio(scored, planned),
            score: ratio_option(correct, scored),
        });
        for observation in dimension_items {
            failures.record(observation.status);
            elapsed_ms = elapsed_ms.saturating_add(observation.latency_ms.unwrap_or(0));
            if let Some(item_usage) = observation.usage.as_ref() {
                usage.add(item_usage);
            }
        }
    }
    let planned = dimensions.iter().map(|item| item.planned).sum();
    let scored = dimensions.iter().map(|item| item.scored).sum();
    let correct = dimensions.iter().map(|item| item.correct).sum();
    let dimension_scores = dimensions
        .iter()
        .filter_map(|item| item.score)
        .collect::<Vec<_>>();
    let score = (dimension_scores.len() == CAPABILITY_DIMENSIONS.len())
        .then(|| dimension_scores.iter().sum::<f64>() / CAPABILITY_DIMENSIONS.len() as f64);
    let (wilson_low, wilson_high) = wilson_interval(correct, scored);
    CapabilitySubjectMetrics {
        planned,
        scored,
        correct,
        coverage: ratio(scored, planned),
        score,
        wilson_low,
        wilson_high,
        dimensions,
        failures,
        elapsed_ms,
        usage: (!usage.is_empty()).then_some(usage),
    }
}

/// 构造同题配对表并计算参考弱优假设下的单侧精确 McNemar p 值。
fn build_capability_comparison(
    items: &[CapabilityItemResult],
    target: &CapabilitySubjectMetrics,
    reference: &CapabilitySubjectMetrics,
) -> CapabilityComparison {
    let mut paired = 0_usize;
    let mut reference_only_correct = 0_usize;
    let mut target_only_correct = 0_usize;
    for item in items {
        let (Some(target_correct), Some(reference_correct)) = (
            item.target.correct,
            item.reference.as_ref().and_then(|value| value.correct),
        ) else {
            continue;
        };
        paired += 1;
        match (target_correct, reference_correct) {
            (false, true) => reference_only_correct += 1,
            (true, false) => target_only_correct += 1,
            _ => {}
        }
    }
    CapabilityComparison {
        paired,
        paired_coverage: ratio(paired, items.len()),
        reference_only_correct,
        target_only_correct,
        score_gap: reference
            .score
            .zip(target.score)
            .map(|(reference, target)| reference - target),
        p_value: exact_mcnemar_one_sided(reference_only_correct, target_only_correct),
    }
}

/// 按锁定阈值产生固定 verdict；覆盖、配对或总超时优先返回 inconclusive。
fn decide_capability_verdict(
    mode: CapabilityMode,
    target: &CapabilitySubjectMetrics,
    reference: Option<&CapabilitySubjectMetrics>,
    comparison: Option<&CapabilityComparison>,
    total_timed_out: bool,
) -> (CapabilityVerdict, Option<CapabilityInconclusiveReason>) {
    if total_timed_out {
        return (
            CapabilityVerdict::Inconclusive,
            Some(CapabilityInconclusiveReason::TotalTimeout),
        );
    }
    if target.coverage < mode.minimum_coverage() {
        return (
            CapabilityVerdict::Inconclusive,
            Some(CapabilityInconclusiveReason::TargetCoverage),
        );
    }
    let Some(reference) = reference else {
        return (CapabilityVerdict::ProfileOnly, None);
    };
    if reference.coverage < mode.minimum_coverage() {
        return (
            CapabilityVerdict::Inconclusive,
            Some(CapabilityInconclusiveReason::ReferenceCoverage),
        );
    }
    let Some(comparison) = comparison else {
        return (
            CapabilityVerdict::Inconclusive,
            Some(CapabilityInconclusiveReason::PairedCoverage),
        );
    };
    if comparison.paired_coverage < mode.minimum_coverage() {
        return (
            CapabilityVerdict::Inconclusive,
            Some(CapabilityInconclusiveReason::PairedCoverage),
        );
    }
    let deviates = comparison
        .score_gap
        // 浮点平均会让数学上恰好 10pp 的差值略小于 0.10；仅吸收计算舍入误差。
        .is_some_and(|gap| gap + 1e-12 >= mode.minimum_score_gap())
        && comparison.p_value < mode.significance();
    match (mode, deviates) {
        (CapabilityMode::Quick, true) => (CapabilityVerdict::NeedsVerification, None),
        (CapabilityMode::Quick, false) => (CapabilityVerdict::NoLargeDeviation, None),
        (CapabilityMode::Verify, true) => (CapabilityVerdict::SignificantDeviation, None),
        (CapabilityMode::Verify, false) => (CapabilityVerdict::NoSignificantDeviation, None),
    }
}

/// 计算二项比例 95% Wilson 区间；无已评分样本时返回 null/null。
fn wilson_interval(correct: usize, scored: usize) -> (Option<f64>, Option<f64>) {
    if scored == 0 {
        return (None, None);
    }
    let n = scored as f64;
    let proportion = correct as f64 / n;
    let z = 1.959_963_984_540_054_f64;
    let z_squared = z * z;
    let denominator = 1.0 + z_squared / n;
    let center = (proportion + z_squared / (2.0 * n)) / denominator;
    let margin = z * ((proportion * (1.0 - proportion) / n + z_squared / (4.0 * n * n)).sqrt())
        / denominator;
    (
        Some((center - margin).max(0.0)),
        Some((center + margin).min(1.0)),
    )
}

/// 计算 discordant pairs 在 p=0.5 下从 b 到 n 的单侧精确二项尾概率。
fn exact_mcnemar_one_sided(reference_only_correct: usize, target_only_correct: usize) -> f64 {
    let discordant = reference_only_correct + target_only_correct;
    if discordant == 0 || reference_only_correct <= target_only_correct {
        return 1.0;
    }
    let denominator = 2_f64.powi(i32::try_from(discordant).unwrap_or(i32::MAX));
    let mut coefficient = 1.0_f64;
    let mut tail = 0.0_f64;
    for successes in 0..=discordant {
        if successes >= reference_only_correct {
            tail += coefficient / denominator;
        }
        if successes < discordant {
            coefficient *= (discordant - successes) as f64 / (successes + 1) as f64;
        }
    }
    tail.min(1.0)
}

/// 计算整数比例；计划数为零时返回 0，避免 NaN 进入 JSON。
fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

/// 计算可选整数比例；分母为零时保持“无可评分样本”语义。
fn ratio_option(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator > 0).then(|| ratio(numerator, denominator))
}

/// 对两个可选 token 计数做饱和相加。
fn add_optional_u64(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

/// 对两个可选美元费用做有限值相加。
fn add_optional_f64(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left + right),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// typed 请求合同只接受内部 ID、模式、语言、参考开关与诊断 request_id。
    #[test]
    fn capability_request_accepts_locked_contract() {
        let request = serde_json::from_value::<CapabilityRequest>(json!({
            "provider_id": "provider-capability",
            "model_id": "model-capability",
            "endpoint_id": "endpoint-capability-chat",
            "api_key_id": "key-capability-primary",
            "mode": "quick",
            "language": "en",
            "use_saved_reference": false,
            "request_id": "provider-capability-test"
        }))
        .expect("locked request contract should deserialize");
        assert!(capability_request_ids_are_valid(&request));
    }

    /// 固定 seed 必须同时证明题数、五维等额、双语等额与逐字可复现。
    #[test]
    fn capability_suite_is_reproducible_and_balanced() {
        let seed =
            Uuid::parse_str("61e5c8f5-cd27-42e7-8b21-2fbf62d6c432").expect("seed should parse");
        for (mode, per_dimension) in [(CapabilityMode::Quick, 8), (CapabilityMode::Verify, 20)] {
            let first = generate_capability_suite(seed, mode, CapabilityLanguage::Bilingual);
            let second = generate_capability_suite(seed, mode, CapabilityLanguage::Bilingual);
            assert_eq!(first, second);
            assert_eq!(first.len(), per_dimension * 5);
            for dimension in CAPABILITY_DIMENSIONS {
                let dimension_questions = first
                    .iter()
                    .filter(|question| question.dimension == dimension)
                    .collect::<Vec<_>>();
                assert_eq!(dimension_questions.len(), per_dimension);
                assert_eq!(
                    dimension_questions
                        .iter()
                        .filter(|question| question.language == QuestionLanguage::Zh)
                        .count(),
                    per_dimension / 2
                );
                assert_eq!(
                    dimension_questions
                        .iter()
                        .filter(|question| question.language == QuestionLanguage::En)
                        .count(),
                    per_dimension / 2
                );
            }
        }
    }

    /// 单语配置必须保持五维配额且不混入另一种题面语言。
    #[test]
    fn capability_suite_honors_single_language_quota() {
        let seed = Uuid::nil();
        let zh = generate_capability_suite(seed, CapabilityMode::Quick, CapabilityLanguage::Zh);
        let en = generate_capability_suite(seed, CapabilityMode::Quick, CapabilityLanguage::En);
        assert!(zh
            .iter()
            .all(|question| question.language == QuestionLanguage::Zh));
        assert!(en
            .iter()
            .all(|question| question.language == QuestionLanguage::En));
    }

    /// 解析器接受明确单选和最后一行 FINAL/答案，同时拒绝歧义答案。
    #[test]
    fn capability_choice_parser_is_strict_about_ambiguity() {
        assert_eq!(parse_capability_choice("A"), Some(CapabilityChoice::A));
        assert_eq!(
            parse_capability_choice("Reasoning mentioned B.\nFINAL: C"),
            Some(CapabilityChoice::C)
        );
        assert_eq!(
            parse_capability_choice("先考虑 A。\n答案：D"),
            Some(CapabilityChoice::D)
        );
        assert_eq!(parse_capability_choice("A or B"), None);
        assert_eq!(parse_capability_choice("No final choice"), None);
        assert!(capability_text_is_refusal("I cannot answer this request."));
        assert!(capability_text_is_refusal("抱歉，我无法回答。"));
    }

    /// Responses 必须收到随机题 input，其他文本协议继续使用 messages，且输出上限字段各自正确。
    #[test]
    fn capability_request_body_preserves_question_for_each_request_family() {
        let responses = build_capability_request_body("openai:responses", "unique question");
        assert_eq!(responses["input"], json!("unique question"));
        assert_eq!(responses["max_output_tokens"], json!(1024));
        assert!(responses.get("messages").is_none());

        let chat = build_capability_request_body("openai:chat", "unique question");
        assert_eq!(chat["messages"][0]["content"], json!("unique question"));
        assert_eq!(chat["max_tokens"], json!(1024));
        assert!(chat.get("input").is_none());
    }

    /// 构造分类测试所需的最小脱敏执行结果。
    fn execution_outcome(
        status: &'static str,
        status_code: Option<u16>,
        error_message: Option<&str>,
        response_body: Option<Value>,
    ) -> ProviderQueryExecutionOutcome {
        ProviderQueryExecutionOutcome {
            status,
            skip_reason: None,
            error_message: error_message.map(ToOwned::to_owned),
            status_code,
            latency_ms: Some(1),
            request_url: String::new(),
            request_headers: BTreeMap::new(),
            request_body: Value::Null,
            response_headers: BTreeMap::new(),
            response_body,
        }
    }

    /// HTTP 过滤错误和 Responses 不完整原因必须分别归入 filtered 与 truncated。
    #[test]
    fn capability_classifies_filter_and_responses_truncation() {
        let filtered = execution_outcome(
            "failed",
            Some(400),
            Some("request rejected"),
            Some(json!({"error": {"code": "content_filter", "message": "rejected"}})),
        );
        assert_eq!(
            classify_capability_execution_failure(&filtered),
            CapabilityItemStatus::Filtered
        );

        let truncated = execution_outcome(
            "success",
            Some(200),
            None,
            Some(json!({
                "status": "incomplete",
                "incomplete_details": {"reason": "max_output_tokens"},
                "output_text": "A"
            })),
        );
        let observation = capability_observation_from_execution(&truncated, CapabilityChoice::A);
        assert_eq!(observation.status, CapabilityItemStatus::Truncated);
        assert_eq!(observation.parsed_option, None);
        assert_eq!(observation.correct, None);

        let refusal = execution_outcome(
            "success",
            Some(200),
            None,
            Some(json!({
                "output": [{
                    "type": "message",
                    "content": [{"type": "refusal", "refusal": "I must decline."}]
                }]
            })),
        );
        let observation = capability_observation_from_execution(&refusal, CapabilityChoice::A);
        assert_eq!(observation.status, CapabilityItemStatus::Refused);
        assert_eq!(observation.parsed_option, None);
        assert_eq!(observation.correct, None);
    }

    /// runtime 的显式与文本超时错误都不能落入 network_failure。
    #[test]
    fn capability_classifies_gateway_timeouts_separately() {
        assert_eq!(
            classify_capability_gateway_error(&GatewayError::AdmissionTimeout {
                trace_id: "trace".to_string(),
                gate: "upstream",
                queue_budget_ms: 10,
            }),
            CapabilityItemStatus::Timeout
        );
        assert_eq!(
            classify_capability_gateway_error(&GatewayError::Internal(
                "provider non-stream request total timeout".to_string()
            )),
            CapabilityItemStatus::Timeout
        );
        assert_eq!(
            classify_capability_gateway_error(&GatewayError::Internal(
                "connection reset".to_string()
            )),
            CapabilityItemStatus::NetworkFailure
        );
    }

    /// 四种受支持协议的同步聚合结果必须复用同一脱敏文本抽取边界。
    #[test]
    fn capability_extracts_text_from_supported_response_shapes() {
        let cases = [
            json!({"choices":[{"message":{"content":"A"}}]}),
            json!({"output":[{"content":[{"type":"output_text","text":"B"}]}]}),
            json!({"content":[{"type":"text","text":"C"}]}),
            json!({"candidates":[{"content":{"parts":[{"text":"D"}]}}]}),
        ];
        for (index, body) in cases.iter().enumerate() {
            assert_eq!(
                extract_capability_response_text(body),
                Some(char::from(b'A' + u8::try_from(index).expect("small index")).to_string())
            );
        }
    }

    /// 常见 OpenAI、Claude 与 Gemini usage 字段必须归一到同一 token 合同。
    #[test]
    fn capability_extracts_usage_without_guessing_cost() {
        let openai = extract_capability_usage(&json!({
            "usage": {"prompt_tokens": 3, "completion_tokens": 2, "total_tokens": 5}
        }))
        .expect("usage should exist");
        assert_eq!(openai.input_tokens, Some(3));
        assert_eq!(openai.output_tokens, Some(2));
        assert_eq!(openai.total_tokens, Some(5));
        assert_eq!(openai.cost_usd, None);

        let gemini = extract_capability_usage(&json!({
            "usageMetadata": {"promptTokenCount": 4, "candidatesTokenCount": 6}
        }))
        .expect("usage should exist");
        assert_eq!(gemini.total_tokens, Some(10));
    }

    /// Wilson 与精确 McNemar 使用锁定的二项统计定义。
    #[test]
    fn capability_statistics_match_known_values() {
        let (low, high) = wilson_interval(5, 10);
        assert!((low.expect("low") - 0.2366).abs() < 0.001);
        assert!((high.expect("high") - 0.7634).abs() < 0.001);
        assert!((exact_mcnemar_one_sided(5, 0) - 0.03125).abs() < f64::EPSILON);
        assert_eq!(exact_mcnemar_one_sided(2, 2), 1.0);
    }

    /// 构造阈值测试所需的最小评分对象。
    fn metrics(coverage: f64, score: f64) -> CapabilitySubjectMetrics {
        CapabilitySubjectMetrics {
            planned: 40,
            scored: (40.0 * coverage) as usize,
            correct: (40.0 * coverage * score) as usize,
            coverage,
            score: Some(score),
            wilson_low: Some(0.0),
            wilson_high: Some(1.0),
            dimensions: Vec::new(),
            failures: CapabilityFailureCounts::default(),
            elapsed_ms: 0,
            usage: None,
        }
    }

    /// 快筛和复核只有同时越过覆盖、分差与严格 p 值阈值才报告偏离。
    #[test]
    fn capability_verdict_enforces_locked_thresholds() {
        let target = metrics(0.95, 0.60);
        let reference = metrics(0.95, 0.75);
        assert_eq!(
            decide_capability_verdict(CapabilityMode::Quick, &target, None, None, false),
            (CapabilityVerdict::ProfileOnly, None)
        );
        let comparison = CapabilityComparison {
            paired: 38,
            paired_coverage: 0.95,
            reference_only_correct: 8,
            target_only_correct: 1,
            score_gap: Some(0.15),
            p_value: 0.049,
        };
        assert_eq!(
            decide_capability_verdict(
                CapabilityMode::Quick,
                &target,
                Some(&reference),
                Some(&comparison),
                false,
            ),
            (CapabilityVerdict::NeedsVerification, None)
        );
        let exact_threshold = CapabilityComparison {
            p_value: 0.05,
            ..comparison
        };
        assert_eq!(
            decide_capability_verdict(
                CapabilityMode::Quick,
                &target,
                Some(&reference),
                Some(&exact_threshold),
                false,
            )
            .0,
            CapabilityVerdict::NoLargeDeviation
        );
        let verify_target = metrics(0.95, 0.65);
        let verify_reference = metrics(0.95, 0.75);
        let verify_comparison = CapabilityComparison {
            paired: 38,
            paired_coverage: 0.95,
            reference_only_correct: 10,
            target_only_correct: 1,
            score_gap: Some(0.75_f64 - 0.65_f64),
            p_value: 0.009,
        };
        assert_eq!(
            decide_capability_verdict(
                CapabilityMode::Verify,
                &verify_target,
                Some(&verify_reference),
                Some(&verify_comparison),
                false,
            )
            .0,
            CapabilityVerdict::SignificantDeviation
        );
        let low_coverage = metrics(0.89, 0.60);
        assert_eq!(
            decide_capability_verdict(
                CapabilityMode::Quick,
                &low_coverage,
                Some(&reference),
                Some(&comparison),
                false,
            ),
            (
                CapabilityVerdict::Inconclusive,
                Some(CapabilityInconclusiveReason::TargetCoverage)
            )
        );
    }
}
