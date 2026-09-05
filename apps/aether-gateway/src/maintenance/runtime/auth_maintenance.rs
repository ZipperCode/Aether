use std::sync::{Arc, LazyLock};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::GatewayError;

/// 认证维护共享并发环境变量；同时约束 OAuth 刷新与账号自检的完整凭据在途量。
const AUTH_MAINTENANCE_CONCURRENCY_ENV: &str = "AETHER_AUTH_MAINTENANCE_CONCURRENCY";
/// 未配置认证维护并发时的默认值，兼顾刷新吞吐与小内存实例的稳定性。
const AUTH_MAINTENANCE_DEFAULT_CONCURRENCY: usize = 4;
/// 认证维护至少保留一个执行槽，避免错误配置永久停用后台刷新。
const AUTH_MAINTENANCE_MIN_CONCURRENCY: usize = 1;
/// 认证维护并发硬上限，防止错误配置重新引入按 Key 放大的内存峰值。
const AUTH_MAINTENANCE_MAX_CONCURRENCY: usize = 64;

/// OAuth 刷新与账号自检共用的进程级许可闸门。
#[derive(Debug, Clone)]
pub(super) struct AuthMaintenanceGate {
    /// Tokio 信号量保存实际许可；克隆闸门仍指向同一组进程内许可。
    semaphore: Arc<Semaphore>,
    /// 归一化后的最大在途数，用于 worker 自身的惰性任务缓冲上限。
    concurrency: usize,
}

impl AuthMaintenanceGate {
    /// 从进程环境读取并归一化认证维护并发配置。
    fn from_env() -> Self {
        Self::new(normalize_auth_maintenance_concurrency(
            std::env::var(AUTH_MAINTENANCE_CONCURRENCY_ENV)
                .ok()
                .as_deref(),
        ))
    }

    /// 以归一化后的并发数构造共享闸门；测试也通过该入口创建隔离实例。
    fn new(concurrency: usize) -> Self {
        let concurrency = concurrency.clamp(
            AUTH_MAINTENANCE_MIN_CONCURRENCY,
            AUTH_MAINTENANCE_MAX_CONCURRENCY,
        );
        Self {
            semaphore: Arc::new(Semaphore::new(concurrency)),
            concurrency,
        }
    }

    /// 返回 worker 可同时轮询的最大任务数，避免一次创建并轮询全部候选 future。
    pub(super) const fn concurrency(&self) -> usize {
        self.concurrency
    }

    /// 异步取得一个 owned permit；成功值离开作用域或任务取消时会自动归还许可。
    pub(super) async fn acquire(&self) -> Result<OwnedSemaphorePermit, GatewayError> {
        Arc::clone(&self.semaphore)
            .acquire_owned()
            .await
            .map_err(|_| {
                GatewayError::Internal("authentication maintenance gate closed".to_string())
            })
    }
}

/// 解析认证维护并发配置；缺失、空白或非法值回退默认值，合法值限制到 `1..=64`。
fn normalize_auth_maintenance_concurrency(raw_value: Option<&str>) -> usize {
    raw_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(AUTH_MAINTENANCE_DEFAULT_CONCURRENCY)
        .clamp(
            AUTH_MAINTENANCE_MIN_CONCURRENCY,
            AUTH_MAINTENANCE_MAX_CONCURRENCY,
        )
}

/// 返回全进程唯一的认证维护闸门；两个后台 worker 的克隆共享同一信号量。
pub(super) fn shared_auth_maintenance_gate() -> AuthMaintenanceGate {
    static GATE: LazyLock<AuthMaintenanceGate> = LazyLock::new(AuthMaintenanceGate::from_env);
    GATE.clone()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::watch;

    use super::{normalize_auth_maintenance_concurrency, AuthMaintenanceGate};

    /// 原子更新并记录执行期间观察到的最大在途任务数。
    fn record_maximum(maximum: &AtomicUsize, current: usize) {
        let mut observed = maximum.load(Ordering::SeqCst);
        while current > observed {
            match maximum.compare_exchange(observed, current, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => break,
                Err(actual) => observed = actual,
            }
        }
    }

    /// 等待指定数量的任务同时持有许可，确保测试真正触达配置上限。
    async fn wait_for_in_flight(in_flight: &AtomicUsize, expected: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while in_flight.load(Ordering::SeqCst) < expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("authentication maintenance tasks should reach configured concurrency");
    }

    /// 用同一个闸门执行一批模拟维护任务，并返回所有任务句柄。
    fn spawn_bounded_tasks(
        gate: AuthMaintenanceGate,
        count: usize,
        in_flight: Arc<AtomicUsize>,
        maximum: Arc<AtomicUsize>,
        release: watch::Receiver<bool>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        (0..count)
            .map(|_| {
                let gate = gate.clone();
                let in_flight = Arc::clone(&in_flight);
                let maximum = Arc::clone(&maximum);
                let mut release = release.clone();
                tokio::spawn(async move {
                    let _permit = gate.acquire().await.expect("gate should remain open");
                    let current = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    record_maximum(&maximum, current);
                    while !*release.borrow() {
                        release
                            .changed()
                            .await
                            .expect("release sender should remain available");
                    }
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect()
    }

    /// 验证配置缺失或非法时使用默认值，合法数值始终限制在安全范围内。
    #[test]
    fn auth_maintenance_concurrency_uses_default_and_clamps_bounds() {
        assert_eq!(normalize_auth_maintenance_concurrency(None), 4);
        assert_eq!(normalize_auth_maintenance_concurrency(Some("")), 4);
        assert_eq!(normalize_auth_maintenance_concurrency(Some("invalid")), 4);
        assert_eq!(normalize_auth_maintenance_concurrency(Some("0")), 1);
        assert_eq!(normalize_auth_maintenance_concurrency(Some("12")), 12);
        assert_eq!(normalize_auth_maintenance_concurrency(Some("1000")), 64);
    }

    /// 验证 6,000 个维护候选共享四个许可，完整工作最大在途数不会随候选数增长。
    #[tokio::test]
    async fn six_thousand_candidates_never_exceed_the_shared_limit() {
        let gate = AuthMaintenanceGate::new(4);
        let in_flight = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let (release_tx, release_rx) = watch::channel(false);
        let tasks = spawn_bounded_tasks(
            gate,
            6_000,
            Arc::clone(&in_flight),
            Arc::clone(&maximum),
            release_rx,
        );

        wait_for_in_flight(&in_flight, 4).await;
        assert_eq!(maximum.load(Ordering::SeqCst), 4);
        release_tx.send(true).expect("release should be observed");
        for task in tasks {
            task.await.expect("bounded task should complete");
        }
        assert_eq!(in_flight.load(Ordering::SeqCst), 0);
    }

    /// 验证模拟 OAuth 与账号自检的两组任务共享同一进程级并发上限。
    #[tokio::test]
    async fn oauth_and_self_check_share_one_combined_limit() {
        let gate = AuthMaintenanceGate::new(3);
        let in_flight = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let (release_tx, release_rx) = watch::channel(false);
        let mut tasks = spawn_bounded_tasks(
            gate.clone(),
            32,
            Arc::clone(&in_flight),
            Arc::clone(&maximum),
            release_rx.clone(),
        );
        tasks.extend(spawn_bounded_tasks(
            gate,
            32,
            Arc::clone(&in_flight),
            Arc::clone(&maximum),
            release_rx,
        ));

        wait_for_in_flight(&in_flight, 3).await;
        assert_eq!(maximum.load(Ordering::SeqCst), 3);
        release_tx.send(true).expect("release should be observed");
        for task in tasks {
            task.await.expect("bounded task should complete");
        }
        assert_eq!(in_flight.load(Ordering::SeqCst), 0);
    }

    /// 验证等待许可的任务被取消后不会占用或吞掉信号量许可。
    #[tokio::test]
    async fn cancelling_a_waiter_does_not_leak_a_permit() {
        let gate = AuthMaintenanceGate::new(1);
        let held = gate
            .acquire()
            .await
            .expect("first permit should be available");
        let waiting_gate = gate.clone();
        let waiter = tokio::spawn(async move {
            waiting_gate
                .acquire()
                .await
                .expect("waiter should acquire unless cancelled")
        });
        tokio::task::yield_now().await;
        waiter.abort();
        assert!(waiter
            .await
            .expect_err("waiter should be cancelled")
            .is_cancelled());

        drop(held);
        let recovered = tokio::time::timeout(Duration::from_secs(1), gate.acquire())
            .await
            .expect("permit should become available after cancellation")
            .expect("gate should remain open");
        drop(recovered);
    }

    /// 防止 OAuth 维护路径回退为一次读取全部完整 Key；轻量投影后才允许单 Key 强读。
    #[test]
    fn oauth_refresh_uses_lightweight_projection_before_strong_read() {
        let source = include_str!("oauth_token_refresh.rs");
        assert!(
            source.contains("list_provider_catalog_auth_maintenance_candidates_by_provider_ids")
        );
        assert!(source.contains("list_provider_catalog_keys_by_ids_strong"));
        assert!(
            !source.contains("list_provider_catalog_keys_by_provider_ids"),
            "OAuth refresh must not load the complete provider key catalog"
        );
    }
}
