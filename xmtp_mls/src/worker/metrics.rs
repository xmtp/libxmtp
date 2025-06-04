use futures::stream::FuturesUnordered;
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    fmt::Debug,
    future::Future,
    hash::Hash,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::Notify;
use tokio_stream::StreamExt;

#[derive(Debug)]
pub struct WorkerMetrics<Metric> {
    metrics: Mutex<HashMap<Metric, Arc<AtomicUsize>>>,
    notify: Notify,
}

impl<Metric> Default for WorkerMetrics<Metric>
where
    Metric: PartialEq + Eq + Hash + Clone + Copy + Debug,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Metric> WorkerMetrics<Metric>
where
    Metric: PartialEq + Eq + Hash + Clone + Copy + Debug,
{
    pub fn new() -> Self {
        Self {
            metrics: Mutex::default(),
            notify: Notify::new(),
        }
    }

    pub fn get(&self, metric: Metric) -> usize {
        let mut lock = self.metrics.lock();
        let atomic = lock.entry(metric).or_default();
        atomic.load(Ordering::SeqCst)
    }

    pub(crate) fn increment_metric(&self, metric: Metric) {
        let mut lock = self.metrics.lock();
        let atomic = lock.entry(metric).or_default();
        atomic.fetch_add(1, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn reset_metrics(&self) {
        *self.metrics.lock() = HashMap::new();
    }

    /// Blocks until metric's specified count is met
    pub async fn wait(
        &self,
        metric_key: Metric,
        count: usize,
    ) -> Result<(), xmtp_common::time::Expired> {
        let metric = self.metrics.lock().entry(metric_key).or_default().clone();

        let secs = if cfg!(target_arch = "wasm32") { 15 } else { 5 };
        let result = xmtp_common::time::timeout(Duration::from_secs(secs), async {
            loop {
                if metric.load(Ordering::SeqCst) >= count {
                    return;
                }
                self.notify.notified().await;
            }
        })
        .await;

        let val = metric.load(Ordering::SeqCst);
        if val >= count {
            return Ok(());
        }
        tracing::error!("Timed out waiting for {metric_key:?} to be >= {count}. Value: {val}");

        result
    }

    pub async fn do_until<F, Fut>(
        &self,
        metric: Metric,
        count: usize,
        f: F,
    ) -> Result<(), xmtp_common::time::Expired>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = ()>,
    {
        let metric = self.metrics.lock().entry(metric).or_default().clone();
        xmtp_common::time::timeout(Duration::from_secs(20), async {
            while metric.load(Ordering::SeqCst) < count {
                f().await;
                xmtp_common::yield_().await;
            }
        })
        .await
    }

    pub fn clear_metric(&self, metric: Metric) {
        self.metrics
            .lock()
            .entry(metric)
            .or_default()
            .store(0, Ordering::SeqCst);
    }
}

#[async_trait::async_trait(?Send)]
pub trait WorkHandleCollection<Metric> {
    /// Blocks until a metrics specified count is met in at least one handle.
    /// Useful when testing several clients, and you need at least one of them to do a job.
    #[allow(unused)]
    async fn wait_one(&self, metric: Metric, count: usize);
}

#[async_trait::async_trait(?Send)]
impl<Metric> WorkHandleCollection<Metric> for Vec<&WorkerMetrics<Metric>>
where
    Metric: PartialEq + Eq + Hash + Clone + Copy,
{
    async fn wait_one(&self, metric: Metric, count: usize) {
        let metrics: Vec<Arc<AtomicUsize>> = self
            .iter()
            .map(|h| h.metrics.lock().entry(metric).or_default().clone())
            .collect();

        while !metrics.iter().any(|m| m.load(Ordering::SeqCst) >= count) {
            let mut notify: FuturesUnordered<_> =
                self.iter().map(|h| h.notify.notified()).collect();
            notify.next().await;
        }
    }
}
