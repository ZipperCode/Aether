use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{AcquireError, OwnedSemaphorePermit, Semaphore};

#[derive(Debug)]
pub struct OfficialProviderBackgroundLimiter {
    permits_per_provider: usize,
    semaphores: Mutex<BTreeMap<String, Arc<Semaphore>>>,
}

impl OfficialProviderBackgroundLimiter {
    pub fn new(permits_per_provider: usize) -> Self {
        Self {
            permits_per_provider: permits_per_provider.max(1),
            semaphores: Mutex::new(BTreeMap::new()),
        }
    }

    pub async fn acquire(&self, provider_id: &str) -> Result<OwnedSemaphorePermit, AcquireError> {
        let semaphore = {
            let mut semaphores = self
                .semaphores
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Arc::clone(
                semaphores
                    .entry(provider_id.to_string())
                    .or_insert_with(|| Arc::new(Semaphore::new(self.permits_per_provider))),
            )
        };
        semaphore.acquire_owned().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn same_provider_is_bounded_while_other_provider_progresses() {
        let limiter = Arc::new(OfficialProviderBackgroundLimiter::new(3));
        let first = limiter.acquire("provider-a").await.expect("permit");
        let second = limiter.acquire("provider-a").await.expect("permit");
        let third = limiter.acquire("provider-a").await.expect("permit");
        let waiting_limiter = Arc::clone(&limiter);
        let waiting = tokio::spawn(async move { waiting_limiter.acquire("provider-a").await });
        tokio::task::yield_now().await;

        assert!(!waiting.is_finished());
        let other = limiter
            .acquire("provider-b")
            .await
            .expect("independent permit");
        drop(first);
        assert!(waiting.await.expect("join").is_ok());
        drop((second, third, other));
    }

    #[tokio::test]
    async fn cancelled_waiters_do_not_leak_permits() {
        let limiter = Arc::new(OfficialProviderBackgroundLimiter::new(1));
        for _ in 0..3 {
            let held = limiter.acquire("provider-a").await.expect("held permit");
            let waiting_limiter = Arc::clone(&limiter);
            let waiting = tokio::spawn(async move { waiting_limiter.acquire("provider-a").await });
            tokio::task::yield_now().await;
            waiting.abort();
            let _ = waiting.await;
            drop(held);
        }
        assert!(limiter.acquire("provider-a").await.is_ok());
    }
}
