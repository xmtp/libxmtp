use parking_lot::Mutex;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::sync::Notify;

#[derive(Default)]
pub struct WorkerHandle<Metric>
where
    Metric: PartialEq + Hash,
{
    metrics: Mutex<HashMap<Metric, Arc<AtomicUsize>>>,
    notify: Notify,
}

#[derive(PartialEq, Eq, Hash)]
pub enum SyncWorkerMetric {
    SyncGroupWelcomesProcessed,
}

impl WorkerHandle<Metric>
where
    Metric: PartialEq + Hash,
{
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub fn get_metric_count(&self, metric: Metric) -> usize {
        let atomic = self.metrics.lock().entry(metric).or_default();
        atomic.load(Ordering::SeqCst)
    }
    pub(super) fn increment_metric(&self, metric: T) {
        let atomic = self.metrics.lock().entry(metric).or_default();
        atomic.fetch_add(1, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// Blocks until metric's specified count is met
    pub async fn wait_for_count(&self, metric: Metric, count: usize) {
        let metric = self.metrics.lock().entry(metric).or_default().clone();
        while metric.load(Ordering::SeqCst) < count {
            self.notify.notified().await;
        }
    }
}
