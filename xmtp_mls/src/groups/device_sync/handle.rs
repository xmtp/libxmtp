use parking_lot::Mutex;
use std::{
    collections::HashMap,
    hash::Hash,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::sync::Notify;

pub struct WorkerHandle<Metric>
where
    Metric: PartialEq + Hash,
{
    metrics: Mutex<HashMap<Metric, Arc<AtomicUsize>>>,
    notify: Notify,
}

#[derive(PartialEq, Eq, Hash)]
pub enum SyncMetric {
    Init,
    SyncGroupWelcomesProcessed,
    RequestsReceived,
    PayloadsSent,
    PayloadsProcessed,
}

impl<Metric> WorkerHandle<Metric>
where
    Metric: PartialEq + Eq + Hash,
{
    pub(super) fn new() -> Self {
        Self {
            metrics: Mutex::default(),
            notify: Notify::new(),
        }
    }

    pub fn get_metric_count(&self, metric: Metric) -> usize {
        let mut lock = self.metrics.lock();
        let atomic = lock.entry(metric).or_default();
        atomic.load(Ordering::SeqCst)
    }

    pub(super) fn increment_metric(&self, metric: Metric) {
        let mut lock = self.metrics.lock();
        let atomic = lock.entry(metric).or_default();
        atomic.fetch_add(1, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// Blocks until metric's specified count is met
    pub async fn block(&self, metric: Metric, count: usize) {
        let metric = self.metrics.lock().entry(metric).or_default().clone();
        while metric.load(Ordering::SeqCst) < count {
            self.notify.notified().await;
        }
    }
}
