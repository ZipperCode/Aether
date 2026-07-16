use std::{
    collections::HashMap,
    future::Future,
    hash::Hash,
    sync::{Arc, Mutex},
};
use tokio::sync::Notify;

struct Entry<V> {
    state: Mutex<EntryState<V>>,
    notify: Notify,
}

struct EntryState<V> {
    result: Option<V>,
    cancelled: bool,
}

impl<V> Default for Entry<V> {
    fn default() -> Self {
        Self {
            state: Mutex::new(EntryState {
                result: None,
                cancelled: false,
            }),
            notify: Notify::new(),
        }
    }
}

pub struct AsyncSingleflight<K, V> {
    flights: Arc<Mutex<HashMap<K, Arc<Entry<V>>>>>,
}

impl<K, V> Default for AsyncSingleflight<K, V> {
    fn default() -> Self {
        Self {
            flights: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

struct Leader<K: Eq + Hash, V> {
    key: K,
    entry: Arc<Entry<V>>,
    flights: Arc<Mutex<HashMap<K, Arc<Entry<V>>>>>,
    armed: bool,
}

impl<K: Eq + Hash, V> Leader<K, V> {
    fn remove(&self) {
        let mut flights = self.flights.lock().unwrap();
        if flights
            .get(&self.key)
            .is_some_and(|current| Arc::ptr_eq(current, &self.entry))
        {
            flights.remove(&self.key);
        }
    }

    fn complete(mut self, result: V) -> V
    where
        V: Clone,
    {
        self.entry.state.lock().unwrap().result = Some(result.clone());
        self.remove();
        self.armed = false;
        self.entry.notify.notify_waiters();
        result
    }
}

impl<K: Eq + Hash, V> Drop for Leader<K, V> {
    fn drop(&mut self) {
        if self.armed {
            self.entry.state.lock().unwrap().cancelled = true;
            self.remove();
            self.entry.notify.notify_waiters();
        }
    }
}

impl<K, V> AsyncSingleflight<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    pub async fn run<F, Fut>(&self, key: K, operation: F) -> V
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = V>,
    {
        let (entry, leader) = {
            let mut flights = self.flights.lock().unwrap();
            if let Some(entry) = flights.get(&key) {
                (Arc::clone(entry), false)
            } else {
                let entry = Arc::new(Entry::default());
                flights.insert(key.clone(), Arc::clone(&entry));
                (entry, true)
            }
        };
        if leader {
            let guard = Leader {
                key,
                entry,
                flights: Arc::clone(&self.flights),
                armed: true,
            };
            return guard.complete(operation().await);
        }
        loop {
            let notified = entry.notify.notified();
            let (result, cancelled) = {
                let state = entry.state.lock().unwrap();
                (state.result.clone(), state.cancelled)
            };
            if let Some(result) = result {
                return result;
            }
            if cancelled {
                return Box::pin(self.run(key, operation)).await;
            }
            notified.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Barrier;

    #[tokio::test]
    async fn shares_result_and_executes_once() {
        let flight = Arc::new(AsyncSingleflight::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(2));
        let first = {
            let flight = Arc::clone(&flight);
            let calls = Arc::clone(&calls);
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                flight
                    .run("key", || async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        barrier.wait().await;
                        17
                    })
                    .await
            })
        };
        tokio::task::yield_now().await;
        let second = {
            let flight = Arc::clone(&flight);
            tokio::spawn(async move {
                flight
                    .run("key", || async { panic!("waiter executed") })
                    .await
            })
        };
        barrier.wait().await;
        assert_eq!(first.await.unwrap(), second.await.unwrap());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancellation_cleans_up_for_retry() {
        let flight = Arc::new(AsyncSingleflight::default());
        let started = Arc::new(Notify::new());
        let leader = {
            let flight = Arc::clone(&flight);
            let started = Arc::clone(&started);
            tokio::spawn(async move {
                flight
                    .run("key", || async move {
                        started.notify_one();
                        std::future::pending::<usize>().await
                    })
                    .await
            })
        };
        started.notified().await;
        leader.abort();
        let _ = leader.await;
        assert_eq!(flight.run("key", || async { 23 }).await, 23);
    }

    #[tokio::test]
    async fn shares_complete_operation_including_persisted_outcome() {
        for iteration in 0..64 {
            let flight = Arc::new(AsyncSingleflight::default());
            let upstream_calls = Arc::new(AtomicUsize::new(0));
            let persist_calls = Arc::new(AtomicUsize::new(0));
            let failure_count = Arc::new(AtomicUsize::new(0));
            let started = Arc::new(Notify::new());
            let release = Arc::new(Notify::new());

            let leader = {
                let flight = Arc::clone(&flight);
                let upstream_calls = Arc::clone(&upstream_calls);
                let persist_calls = Arc::clone(&persist_calls);
                let failure_count = Arc::clone(&failure_count);
                let started = Arc::clone(&started);
                let release = Arc::clone(&release);
                tokio::spawn(async move {
                    flight
                        .run("provider:key", || async move {
                            upstream_calls.fetch_add(1, Ordering::SeqCst);
                            started.notify_one();
                            release.notified().await;
                            persist_calls.fetch_add(1, Ordering::SeqCst);
                            let failures = failure_count.fetch_add(1, Ordering::SeqCst) + 1;
                            format!("error:failure_count={failures}")
                        })
                        .await
                })
            };
            started.notified().await;
            let waiter = {
                let flight = Arc::clone(&flight);
                tokio::spawn(async move {
                    flight
                        .run("provider:key", || async {
                            panic!("waiter executed operation")
                        })
                        .await
                })
            };
            tokio::task::yield_now().await;
            release.notify_one();

            let leader_outcome = leader.await.unwrap();
            let waiter_outcome = waiter.await.unwrap();
            assert_eq!(leader_outcome, waiter_outcome, "iteration {iteration}");
            assert_eq!(
                upstream_calls.load(Ordering::SeqCst),
                1,
                "iteration {iteration}"
            );
            assert_eq!(
                persist_calls.load(Ordering::SeqCst),
                1,
                "iteration {iteration}"
            );
            assert_eq!(
                failure_count.load(Ordering::SeqCst),
                1,
                "iteration {iteration}"
            );
        }
    }
}
