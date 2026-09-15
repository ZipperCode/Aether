use std::time::{Duration, Instant};

use aether_contracts::{ExecutionPlan, ExecutionResponseObservation, RequestBody};
use aether_provider_transport::antigravity::{
    is_antigravity_corrupted_thought_signature, repair_antigravity_thought_signatures,
};
use serde_json::{json, Value};

use super::transport::{
    resolve_non_stream_total_timeout_for_request, resolve_stream_first_byte_timeout,
};
use crate::GatewayError;

/// 内部候选诊断字段；只保存固定错误类型与发送序号，不保存签名或凭据。
pub(crate) const RECOVERY_FIELD: &str = "antigravity_signature_recovery";

/// 显式适配身份与 Gemini 格式共同限定恢复范围，不能凭 URL 或模型名猜测提供商。
pub(crate) fn is_antigravity_signature_plan(plan: &ExecutionPlan, context: Option<&Value>) -> bool {
    plan.provider_api_format
        .eq_ignore_ascii_case("gemini:generate_content")
        && context
            .and_then(|value| value.get("envelope_name"))
            .and_then(Value::as_str)
            == Some("antigravity:v1internal")
}

/// 一次逻辑候选的签名恢复预算；OAuth 重试不得创建新的预算。
pub(crate) struct AntigravitySignatureRecovery {
    /// 仅对显式 Antigravity Gemini 计划开启。
    enabled: bool,
    /// 与 OAuth 重试独立，最多发送一次兼容正文。
    attempted: bool,
    /// 首次发送前的单调时间，兼容请求不能重置截止时间。
    started_at: Instant,
    /// 流式首字节或非流式总耗时上限，含默认值。
    timeout: Option<Duration>,
    /// 原始拒绝及恢复结果，写入现有候选诊断。
    diagnostic: Option<Value>,
}

impl AntigravitySignatureRecovery {
    /// 正文已进入一次性兼容恢复，后续发送必须沿用本候选剩余时间。
    pub(crate) fn has_prepared_retry(&self) -> bool {
        self.attempted
    }

    /// 在首次发送前建立候选预算；首次正文保持原样。
    pub(crate) fn new(plan: &ExecutionPlan, context: Option<&Value>) -> Self {
        let enabled = is_antigravity_signature_plan(plan, context);
        Self {
            enabled,
            attempted: false,
            started_at: Instant::now(),
            timeout: enabled.then(|| {
                resolve_stream_first_byte_timeout(plan)
                    .or_else(|| {
                        resolve_non_stream_total_timeout_for_request(
                            plan.stream,
                            &plan.provider_api_format,
                            plan.timeouts.as_ref(),
                        )
                    })
                    .expect(
                        "Antigravity stream or sync execution has a configured default deadline",
                    )
            }),
            diagnostic: None,
        }
    }

    /// 只用真实响应状态与已解码 JSON 判定，不能把 HTTP 200 内嵌错误当作触发条件。
    pub(crate) fn recognizes(&self, status: u16, body: Option<&Value>) -> bool {
        self.enabled
            && body.is_some_and(|body| is_antigravity_corrupted_thought_signature(status, body))
    }

    /// 消费一次修复预算并更新实际发送正文及其 capture；无可改字段则保持原计划。
    pub(crate) fn prepare_retry(
        &mut self,
        plan: &mut ExecutionPlan,
        context: &mut Option<Value>,
        status: u16,
        body: Option<&Value>,
        observation: Option<&ExecutionResponseObservation>,
    ) -> Result<bool, GatewayError> {
        if !self.recognizes(status, body) || self.attempted {
            return Ok(false);
        }
        // 私有适配计划使用 JSON；不得改写与实际发送字节不一致的旁路表示。
        let repaired = if plan.body.body_bytes_b64.is_none() && plan.body.body_ref.is_none() {
            plan.body
                .json_body
                .as_ref()
                .and_then(repair_antigravity_thought_signatures)
        } else {
            None
        };
        self.diagnostic = Some(json!({
            "original_status": 400,
            "original_error_status": "INVALID_ARGUMENT",
            "original_error_message": "Corrupted thought signature",
            "original_request_order_id": observation.map(|value| &value.request_order_id),
            "outcome": if repaired.is_some() { "retrying" } else { "unchanged" },
        }));
        let Some(repaired) = repaired else {
            return Ok(false);
        };
        self.reduce_deadline(plan)?;
        self.attempted = true;
        plan.body = RequestBody::from_json(repaired.clone());
        let context = context.get_or_insert_with(|| json!({}));
        context["provider_request_body"] = repaired;
        Ok(true)
    }

    /// 每次后续发送都只使用候选剩余时间；耗尽时发送零个额外请求。
    pub(crate) fn reduce_deadline(&mut self, plan: &mut ExecutionPlan) -> Result<(), GatewayError> {
        let Some(timeout) = self.timeout else {
            return Ok(());
        };
        let remaining_ms = timeout
            .saturating_sub(self.started_at.elapsed())
            .as_millis() as u64;
        if remaining_ms == 0 {
            if let Some(diagnostic) = self.diagnostic.as_mut() {
                diagnostic["outcome"] = json!("deadline_exhausted");
            }
            return Err(GatewayError::Internal(
                "Antigravity signature recovery deadline exhausted".to_string(),
            ));
        }
        let timeouts = plan.timeouts.get_or_insert_with(Default::default);
        if plan.stream {
            timeouts.first_byte_ms = Some(remaining_ms);
        } else {
            timeouts.total_ms = Some(remaining_ms);
        }
        Ok(())
    }

    /// 记录最终 HTTP 结果及独立发送序号，成功结果不抹去第一次拒绝证据。
    pub(crate) fn observe(
        &mut self,
        status: u16,
        observation: Option<&ExecutionResponseObservation>,
    ) {
        if let Some(diagnostic) = self.diagnostic.as_mut() {
            diagnostic["final_status"] = json!(status);
            diagnostic["final_request_order_id"] =
                json!(observation.map(|value| &value.request_order_id));
            if self.attempted {
                diagnostic["outcome"] = json!(if status < 400 {
                    "recovered"
                } else {
                    "rejected"
                });
            }
        }
    }

    /// 恢复发送发生传输错误时保留首次拒绝，不把未收到响应记录为恢复成功。
    pub(crate) fn observe_transport_error(&mut self) {
        if let Some(diagnostic) = self.diagnostic.as_mut() {
            diagnostic["outcome"] = json!("transport_error");
        }
    }

    /// 返回固定字段的恢复诊断，供管理员原有候选追踪写入，不包含正文或签名。
    pub(crate) fn diagnostic(&self) -> Option<Value> {
        self.diagnostic.clone()
    }

    /// 当前候选仍处于执行中；仅合并诊断，最终状态、费用和租约仍由原有终态路径处理。
    pub(crate) async fn record_diagnostic(
        &self,
        state: &crate::AppState,
        plan: &ExecutionPlan,
        context: Option<&Value>,
    ) {
        if let Some(diagnostic) = &self.diagnostic {
            crate::request_candidate_runtime::record_local_request_candidate_extra_data(
                state,
                plan,
                context,
                aether_data_contracts::repository::candidates::RequestCandidateStatus::Streaming,
                None,
                None,
                json!({RECOVERY_FIELD: diagnostic}),
            )
            .await;
        }
    }

    /// 把恢复诊断并入现有报告上下文，由候选唯一终态写入负责持久化。
    pub(crate) fn apply_to_context(&self, context: &mut Option<Value>) {
        if let Some(diagnostic) = &self.diagnostic {
            context.get_or_insert_with(|| json!({}))[RECOVERY_FIELD] = diagnostic.clone();
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
        routing::post,
        Json, Router,
    };
    use std::sync::{Arc, Mutex};

    /// 真实 HTTP 测试服务器，记录每次正文与认证头；析构仅终止该测试创建的监听任务。
    pub(crate) struct SignatureServer {
        /// 当前测试的回环 URL。
        pub(crate) url: String,
        /// 按 HTTP 到达顺序保存合成请求。
        pub(crate) requests: Arc<Mutex<Vec<(HeaderMap, Value)>>>,
        /// 测试持有的监听任务，只由本夹具负责中止。
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for SignatureServer {
        /// 测试失败或成功都释放自有服务器，不留下后台监听。
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    /// 按给定状态序列回应，超过序列时继续最后一个状态，以便检测意外多次重试。
    pub(crate) async fn signature_server(stream: bool, statuses: Vec<u16>) -> SignatureServer {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&requests);
        let listener = crate::test_support::bind_loopback_listener().await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let app = Router::new().route("/", post(move |headers: HeaderMap, Json(body): Json<Value>| {
                let observed = Arc::clone(&observed);
                let statuses = statuses.clone();
                async move {
                    let index = {
                        let mut requests = observed.lock().unwrap();
                        requests.push((headers, body));
                        requests.len() - 1
                    };
                    let status = statuses[index.min(statuses.len() - 1)];
                    let body = if status == 200 {
                        json!({"response": {"candidates": [{"content": {"role": "model", "parts": [{"text": "OK"}]}, "finishReason": "STOP"}], "usageMetadata": {"promptTokenCount": 1, "candidatesTokenCount": 1, "totalTokenCount": 2}}})
                    } else { rejection() };
                    if stream && status == 200 {
                        (StatusCode::OK, [("content-type", "text/event-stream")], Body::from(format!("data: {body}\n\n"))).into_response()
                    } else { (StatusCode::from_u16(status).unwrap(), Json(body)).into_response() }
                }
            }));
            axum::serve(listener, app).await.unwrap();
        });
        SignatureServer {
            url,
            requests,
            task,
        }
    }

    /// 构造只含合成身份和私有 Gemini 历史的计划，不读取生产凭据。
    pub(crate) fn signature_plan(stream: bool, url: &str) -> ExecutionPlan {
        serde_json::from_value(json!({
            "request_id": uuid::Uuid::new_v4().to_string(), "candidate_id": uuid::Uuid::new_v4().to_string(),
            "provider_id": "signature-provider", "endpoint_id": "signature-endpoint", "key_id": "signature-key",
            "method": "POST", "url": url, "headers": {"authorization": "Bearer signature-test", "content-type": "application/json"},
            "content_type": "application/json", "stream": stream,
            "client_api_format": "gemini:generate_content", "provider_api_format": "gemini:generate_content",
            "model_name": "gemini-test", "body": {"json_body": {"request": {"contents": [{"role": "model", "parts": [{"functionCall": {"name": "lookup", "args": {"thoughtSignature": "business-data"}}, "thoughtSignature": "foreign-signature"}]}]}}}
        })).unwrap()
    }

    /// 生产报错的最小合成响应。
    pub(crate) fn rejection() -> Value {
        json!({"error": {"status": "INVALID_ARGUMENT", "message": "Corrupted thought signature."}})
    }

    /// 与实际私有适配一致的身份和候选元数据。
    pub(crate) fn context(plan: &ExecutionPlan) -> Option<Value> {
        Some(
            json!({"envelope_name": "antigravity:v1internal", "has_envelope": true,
            "candidate_index": 0, "retry_index": 0, "request_id": plan.request_id,
            "candidate_id": plan.candidate_id, "client_api_format": plan.client_api_format,
            "provider_api_format": plan.provider_api_format, "mapped_model": "gemini-test",
            "original_request_body": plan.body.json_body}),
        )
    }

    /// 恢复预算不因重复拒绝重置；身份不符时不触发，截止时间耗尽时正文保持不变。
    #[test]
    fn antigravity_signature_recovery_gate_budget_and_deadline() {
        let mut plan = signature_plan(false, "http://localhost/");
        let mut context = context(&plan);
        let original = plan.body.clone();
        let mut recovery = AntigravitySignatureRecovery::new(&plan, context.as_ref());
        assert!(!recovery
            .prepare_retry(&mut plan, &mut context, 200, Some(&rejection()), None)
            .unwrap());
        assert_eq!(plan.body, original);
        assert!(recovery
            .prepare_retry(&mut plan, &mut context, 400, Some(&rejection()), None)
            .unwrap());
        assert!(!recovery
            .prepare_retry(&mut plan, &mut context, 400, Some(&rejection()), None)
            .unwrap());
        let remaining = plan.timeouts.as_ref().unwrap().total_ms.unwrap();
        recovery.started_at -= Duration::from_secs(1);
        recovery.reduce_deadline(&mut plan).unwrap();
        assert!(plan.timeouts.as_ref().unwrap().total_ms.unwrap() < remaining);
        recovery.observe(200, None);
        assert_eq!(recovery.diagnostic().unwrap()["outcome"], "recovered");

        let mut disabled = AntigravitySignatureRecovery::new(&plan, None);
        assert!(!disabled.recognizes(400, Some(&rejection())));
        disabled.reduce_deadline(&mut plan).unwrap();

        let mut expired = signature_plan(true, "http://localhost/");
        let original = expired.body.clone();
        let mut context = super::tests::context(&expired);
        let mut recovery = AntigravitySignatureRecovery::new(&expired, context.as_ref());
        recovery.timeout = Some(Duration::from_millis(1));
        recovery.started_at -= Duration::from_millis(5);
        assert!(recovery
            .prepare_retry(&mut expired, &mut context, 400, Some(&rejection()), None)
            .is_err());
        assert_eq!(expired.body, original);
        assert_eq!(
            recovery.diagnostic().unwrap()["outcome"],
            "deadline_exhausted"
        );
    }
}
