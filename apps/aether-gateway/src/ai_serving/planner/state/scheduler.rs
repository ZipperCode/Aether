use aether_scheduler_core::{ClientSessionAffinity, SchedulerMinimalCandidateSelectionCandidate};
use std::time::Duration;
use tokio::time::Instant;

use super::{GatewayAuthApiKeySnapshot, PlannerAppState};
use crate::clock::{current_unix_secs, request_distribution_seed};
use crate::constants::{
    API_KEY_CONCURRENCY_WAIT_POLL_INTERVAL_MS, API_KEY_CONCURRENCY_WAIT_TIMEOUT_MS,
};
use crate::scheduler::candidate::{CandidateSchedulingContext, SchedulerSkippedCandidate};
use crate::scheduler::config::SchedulerOrderingConfig;
use crate::GatewayError;

impl<'a> PlannerAppState<'a> {
    /// 为模型目录兼容入口补充分布种子后列出候选；传入值仅作为真实 Unix 秒使用。
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn list_selectable_candidates(
        self,
        api_format: &str,
        global_model_name: &str,
        require_streaming: bool,
        required_capabilities: Option<&serde_json::Value>,
        auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
        client_session_affinity: Option<&ClientSessionAffinity>,
        now_unix_secs: u64,
        enable_model_directives: bool,
        ordering_config: Option<SchedulerOrderingConfig>,
    ) -> Result<Vec<SchedulerMinimalCandidateSelectionCandidate>, GatewayError> {
        let scheduling_context = CandidateSchedulingContext {
            now_unix_secs,
            load_balance_seed: request_distribution_seed(),
        };
        crate::scheduler::candidate::list_selectable_candidates(
            self.app().data.as_ref(),
            self.app(),
            api_format,
            global_model_name,
            require_streaming,
            required_capabilities,
            auth_snapshot,
            client_session_affinity,
            scheduling_context,
            enable_model_directives,
            ordering_config,
        )
        .await
    }

    /// 以具名时间/种子上下文列出候选及跳过原因，并传递请求排序配置。
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn list_selectable_candidates_with_skip_reasons(
        self,
        api_format: &str,
        global_model_name: &str,
        require_streaming: bool,
        required_capabilities: Option<&serde_json::Value>,
        auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
        client_session_affinity: Option<&ClientSessionAffinity>,
        scheduling_context: CandidateSchedulingContext,
        enable_model_directives: bool,
        ordering_config: Option<SchedulerOrderingConfig>,
    ) -> Result<
        (
            Vec<SchedulerMinimalCandidateSelectionCandidate>,
            Vec<SchedulerSkippedCandidate>,
        ),
        GatewayError,
    > {
        self.list_selectable_candidates_with_skip_reasons_for_request_operation(
            api_format,
            global_model_name,
            require_streaming,
            required_capabilities,
            auth_snapshot,
            client_session_affinity,
            scheduling_context,
            enable_model_directives,
            None,
            ordering_config,
        )
        .await
    }

    /// 按请求操作列出候选；并发等待只刷新真实时间，整个排序批次沿用原分布种子。
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn list_selectable_candidates_with_skip_reasons_for_request_operation(
        self,
        api_format: &str,
        global_model_name: &str,
        require_streaming: bool,
        required_capabilities: Option<&serde_json::Value>,
        auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
        client_session_affinity: Option<&ClientSessionAffinity>,
        scheduling_context: CandidateSchedulingContext,
        enable_model_directives: bool,
        request_operation: Option<&str>,
        ordering_config: Option<SchedulerOrderingConfig>,
    ) -> Result<
        (
            Vec<SchedulerMinimalCandidateSelectionCandidate>,
            Vec<SchedulerSkippedCandidate>,
        ),
        GatewayError,
    > {
        let wait_timeout = Duration::from_millis(API_KEY_CONCURRENCY_WAIT_TIMEOUT_MS);
        let wait_interval = Duration::from_millis(API_KEY_CONCURRENCY_WAIT_POLL_INTERVAL_MS.max(1));
        let wait_deadline = Instant::now() + wait_timeout;
        let mut attempt_context = scheduling_context;
        loop {
            let result = crate::scheduler::candidate::list_selectable_candidates_with_skip_reasons_for_request_operation(
                self.app().data.as_ref(),
                self.app(),
                api_format,
                global_model_name,
                require_streaming,
                required_capabilities,
                auth_snapshot,
                client_session_affinity,
                attempt_context,
                enable_model_directives,
                request_operation,
                ordering_config,
            )
            .await?;

            if !crate::scheduler::candidate::is_exact_all_skipped_by_auth_limit(
                &result.0, &result.1,
            ) {
                return Ok(result);
            }

            let now = Instant::now();
            if now >= wait_deadline {
                return Ok(result);
            }

            let remaining = wait_deadline.duration_since(now);
            tokio::time::sleep(wait_interval.min(remaining)).await;
            attempt_context.now_unix_secs = current_unix_secs();
        }
    }

    /// 对调用方枚举出的候选应用权限、亲和及具名时间/种子上下文。
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn list_selectable_enumerated_candidates_with_skip_reasons(
        self,
        api_format: &str,
        global_model_name: &str,
        candidates: Vec<SchedulerMinimalCandidateSelectionCandidate>,
        required_capabilities: Option<&serde_json::Value>,
        auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
        client_session_affinity: Option<&ClientSessionAffinity>,
        scheduling_context: CandidateSchedulingContext,
        ordering_config: Option<SchedulerOrderingConfig>,
    ) -> Result<
        (
            Vec<SchedulerMinimalCandidateSelectionCandidate>,
            Vec<SchedulerSkippedCandidate>,
        ),
        GatewayError,
    > {
        crate::scheduler::candidate::list_selectable_enumerated_candidates_with_skip_reasons(
            self.app(),
            api_format,
            global_model_name,
            candidates,
            required_capabilities,
            auth_snapshot,
            client_session_affinity,
            scheduling_context,
            ordering_config,
        )
        .await
    }

    /// 在没有请求模型名时按独占能力选择候选；等待仅刷新时间并保持排序种子稳定。
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn list_selectable_candidates_for_required_capability_without_requested_model(
        self,
        candidate_api_format: &str,
        required_capability: &str,
        require_streaming: bool,
        auth_snapshot: Option<&GatewayAuthApiKeySnapshot>,
        client_session_affinity: Option<&ClientSessionAffinity>,
        scheduling_context: CandidateSchedulingContext,
        ordering_config: Option<SchedulerOrderingConfig>,
    ) -> Result<Vec<SchedulerMinimalCandidateSelectionCandidate>, GatewayError> {
        let wait_timeout = Duration::from_millis(API_KEY_CONCURRENCY_WAIT_TIMEOUT_MS);
        let wait_interval = Duration::from_millis(API_KEY_CONCURRENCY_WAIT_POLL_INTERVAL_MS.max(1));
        let wait_deadline = Instant::now() + wait_timeout;
        let mut attempt_context = scheduling_context;

        loop {
            let (result, auth_limit_blocked) = crate::scheduler::candidate::list_selectable_candidates_for_required_capability_without_requested_model_with_auth_limit_signal(
                self.app().data.as_ref(),
                self.app(),
                candidate_api_format,
                required_capability,
                require_streaming,
                auth_snapshot,
                client_session_affinity,
                attempt_context,
                ordering_config,
            )
            .await?;

            if !auth_limit_blocked {
                return Ok(result);
            }

            let now = Instant::now();
            if now >= wait_deadline {
                return Ok(result);
            }

            let remaining = wait_deadline.duration_since(now);
            tokio::time::sleep(wait_interval.min(remaining)).await;
            attempt_context.now_unix_secs = current_unix_secs();
        }
    }
}
