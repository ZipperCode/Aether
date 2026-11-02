use super::*;
use crate::SqlxRequestCandidateReadRepository;
use aether_data_contracts::repository::candidates::{
    RequestCandidateStatus, UpsertRequestCandidateRecord,
};

async fn repository() -> SqlxUsageReadRepository {
    let factory = PostgresPoolFactory::new(PostgresPoolConfig {
        database_url: std::env::var("AETHER_TEST_DATABASE_URL")
            .expect("AETHER_TEST_DATABASE_URL must name a disposable, prepared database"),
        min_connections: 1,
        max_connections: 3,
        acquire_timeout_ms: 10_000,
        idle_timeout_ms: 30_000,
        max_lifetime_ms: 60_000,
        statement_cache_capacity: 64,
        require_ssl: false,
    })
    .unwrap();
    let repository = SqlxUsageReadRepository::new(factory.connect_lazy().unwrap());
    crate::run_migrations(repository.pool()).await.unwrap();
    repository
}

fn candidate(request_id: &str, status: RequestCandidateStatus) -> UpsertRequestCandidateRecord {
    UpsertRequestCandidateRecord {
        id: uuid::Uuid::new_v4().to_string(),
        request_id: request_id.into(),
        user_id: None,
        api_key_id: None,
        username: None,
        api_key_name: None,
        candidate_index: 0,
        retry_index: 0,
        provider_id: None,
        endpoint_id: None,
        key_id: None,
        status,
        skip_reason: None,
        is_cached: Some(false),
        status_code: Some(200),
        error_type: None,
        error_message: None,
        latency_ms: Some(12),
        concurrent_requests: None,
        extra_data: None,
        required_capabilities: None,
        created_at_unix_ms: Some(1_700_000_000_000),
        started_at_unix_ms: Some(1_700_000_000_000),
        finished_at_unix_ms: None,
    }
}

async fn status(repository: &SqlxUsageReadRepository, request_id: &str) -> (String, String) {
    sqlx::query_as("SELECT u.status, c.status FROM usage u JOIN request_candidates c USING (request_id) WHERE request_id = $1")
        .bind(request_id).fetch_one(repository.pool()).await.unwrap()
}

async fn cleanup(repository: &SqlxUsageReadRepository, request_ids: &[String]) {
    sqlx::query("DELETE FROM request_candidates WHERE request_id = ANY($1)")
        .bind(request_ids)
        .execute(repository.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM usage WHERE request_id = ANY($1)")
        .bind(request_ids)
        .execute(repository.pool())
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires AETHER_TEST_DATABASE_URL and production-prepared PostgreSQL"]
async fn live_stream_cleanup_preserves_heartbeat_and_reclaims_abandoned_requests() {
    let repository = repository().await;
    let candidates = SqlxRequestCandidateReadRepository::new(repository.pool().clone());
    let now = Utc::now().timestamp() as u64;
    let old = now - 3600;
    let ids = (0..5)
        .map(|_| uuid::Uuid::new_v4().to_string())
        .collect::<Vec<_>>();
    for (index, id) in ids.iter().enumerate() {
        let mut usage = first_byte_usage_record(
            id,
            "stream-cleanup",
            old,
            Some(json!({"requested_reasoning_effort": "high"})),
        );
        if index == 1 {
            usage.status = "pending".into();
            usage.is_stream = Some(false);
        }
        repository.upsert(usage).await.unwrap();
        let mut record = candidate(
            id,
            if index == 1 {
                RequestCandidateStatus::Pending
            } else {
                RequestCandidateStatus::Streaming
            },
        );
        if index == 3 {
            record.status = RequestCandidateStatus::Success;
            record.extra_data = Some(json!({"stream_completed": true}));
        }
        candidates.upsert(record).await.unwrap();
    }
    for id in &ids[..2] {
        repository.heartbeat_active_request(id, now).await.unwrap();
    }
    // Untrusted/malformed metadata must not poison the entire cleanup batch.
    sqlx::query(r#"UPDATE usage SET request_metadata = '{"runtime_heartbeat_unix_secs":"invalid"}'::json WHERE request_id = $1"#)
        .bind(&ids[4]).execute(repository.pool()).await.unwrap();
    let clock: (i64, Option<i32>, Option<i32>, serde_json::Value) = sqlx::query_as(
        "SELECT updated_at_unix_secs, response_time_ms, first_byte_time_ms, request_metadata FROM usage WHERE request_id = $1")
        .bind(&ids[0]).fetch_one(repository.pool()).await.unwrap();
    assert_eq!(clock.0, old as i64);
    assert_eq!((clock.1, clock.2), (Some(14), Some(12)));
    assert_eq!(clock.3["runtime_heartbeat_unix_secs"], now);
    assert_eq!(clock.3["requested_reasoning_effort"], "high");

    // A full nonterminal snapshot must not erase the independent execution heartbeat.
    let update = first_byte_usage_record(
        &ids[0],
        "stream-cleanup",
        old,
        Some(json!({"requested_reasoning_effort": "medium"})),
    );
    repository.upsert(update.clone()).await.unwrap();
    repository.upsert_first_byte(update).await.unwrap();
    let summary = repository
        .cleanup_stale_pending_requests(now - 600, now, 10, 1)
        .await
        .unwrap();
    assert_eq!((summary.failed, summary.recovered), (2, 1));
    assert_eq!(
        status(&repository, &ids[0]).await,
        ("streaming".into(), "streaming".into())
    );
    assert_eq!(
        status(&repository, &ids[1]).await,
        ("pending".into(), "pending".into())
    );
    assert_eq!(
        status(&repository, &ids[2]).await,
        ("failed".into(), "failed".into())
    );
    assert_eq!(
        status(&repository, &ids[3]).await,
        ("completed".into(), "success".into())
    );
    assert_eq!(
        status(&repository, &ids[4]).await,
        ("failed".into(), "failed".into())
    );

    // Continue well beyond the original creation cutoff; no total lifetime is imposed.
    repository
        .heartbeat_active_request(&ids[0], now + 3600)
        .await
        .unwrap();
    let summary = repository
        .cleanup_stale_pending_requests(now + 3000, now + 3600, 10, 1)
        .await
        .unwrap();
    assert_eq!(summary.failed, 1);
    assert_eq!(status(&repository, &ids[0]).await.0, "streaming");
    assert_eq!(status(&repository, &ids[1]).await.0, "failed");
    let summary = repository
        .cleanup_stale_pending_requests(now + 3601, now + 4201, 10, 1)
        .await
        .unwrap();
    assert_eq!(summary.failed, 1);
    assert_eq!(
        status(&repository, &ids[0]).await,
        ("failed".into(), "failed".into())
    );
    repository
        .heartbeat_active_request(&ids[0], now + 5000)
        .await
        .unwrap();
    assert_eq!(status(&repository, &ids[0]).await.0, "failed");
    cleanup(&repository, &ids).await;
}

#[tokio::test]
#[ignore = "requires AETHER_TEST_DATABASE_URL and production-prepared PostgreSQL"]
async fn live_stream_terminal_repairs_only_legacy_false_success_candidate() {
    let repository = repository().await;
    let candidates = SqlxRequestCandidateReadRepository::new(repository.pool().clone());
    let now = Utc::now().timestamp() as u64;
    let mut ids = Vec::new();
    for (is_stream, completed, terminal_status) in [
        (true, false, "failed"),
        (true, false, "cancelled"),
        (true, true, "failed"),
        (false, false, "failed"),
    ] {
        let id = uuid::Uuid::new_v4().to_string();
        ids.push(id.clone());
        let mut usage = first_byte_usage_record(&id, "stream-terminal", now - 1000, None);
        usage.is_stream = Some(is_stream);
        let mut served = candidate(&id, RequestCandidateStatus::Success);
        served.finished_at_unix_ms = Some((now - 100) * 1000);
        served.extra_data = completed.then(|| json!({"stream_completed": true}));
        usage.candidate_id = Some(served.id.clone());
        candidates.upsert(served.clone()).await.unwrap();
        let mut peer = candidate(&id, RequestCandidateStatus::Success);
        peer.candidate_index = 1;
        candidates.upsert(peer.clone()).await.unwrap();
        repository.upsert(usage.clone()).await.unwrap();
        sqlx::query("UPDATE usage SET status = 'completed' WHERE request_id = $1")
            .bind(&id)
            .execute(repository.pool())
            .await
            .unwrap();
        usage.status = terminal_status.into();
        usage.status_code = Some(if terminal_status == "cancelled" {
            499
        } else {
            502
        });
        usage.billing_status = "void".into();
        usage.response_time_ms = Some(900_000);
        usage.finalized_at_unix_secs = Some(now);
        usage.updated_at_unix_secs = now;
        repository.upsert(usage.clone()).await.unwrap();
        let rows = candidates.list_by_request_id(&id).await.unwrap();
        let expected = if is_stream && !completed {
            if terminal_status == "cancelled" {
                RequestCandidateStatus::Cancelled
            } else {
                RequestCandidateStatus::Failed
            }
        } else {
            RequestCandidateStatus::Success
        };
        assert_eq!(rows[0].status, expected);
        assert_eq!(rows[1].status, RequestCandidateStatus::Success);
        if is_stream && !completed {
            assert_eq!(rows[0].status_code, usage.status_code);
            assert_eq!(rows[0].latency_ms, Some(900_000));
            assert_eq!(rows[0].finished_at_unix_ms, Some(now * 1000));
            served.status = RequestCandidateStatus::Streaming;
            candidates.upsert(served).await.unwrap();
            assert_eq!(
                candidates.list_by_request_id(&id).await.unwrap()[0].status,
                expected
            );
        }
    }
    cleanup(&repository, &ids).await;
}
