use futures::stream::{select_all, FuturesUnordered};
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
use tokio_stream::StreamExt;

pub struct WorkerHandle<Metric>
where
    Metric: PartialEq + Hash,
{
    metrics: Mutex<HashMap<Metric, Arc<AtomicUsize>>>,
    notify: Notify,
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub enum SyncMetric {
    Init,
    SyncGroupWelcomesProcessed,
    RequestsReceived,
    PayloadsSent,
    PayloadsProcessed,
}

impl<Metric> WorkerHandle<Metric>
where
    Metric: PartialEq + Eq + Hash + Clone + Copy,
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
    pub async fn wait(&self, metric: Metric, count: usize) {
        let metric = self.metrics.lock().entry(metric).or_default().clone();
        while metric.load(Ordering::SeqCst) < count {
            self.notify.notified().await;
        }
    }

    /// Blocks until a metrics specified count is met in at least one handle.
    /// Useful when testing several clients, and you need at least one of them to do a job.
    pub async fn wait_or(&self, mut others: Vec<&Self>, metric: Metric, count: usize) {
        others.push(self);
        let metrics: Vec<Arc<AtomicUsize>> = others
            .iter()
            .map(|h| h.metrics.lock().entry(metric).or_default().clone())
            .collect();

        while !metrics.iter().any(|m| m.load(Ordering::SeqCst) >= count) {
            let mut notify: FuturesUnordered<_> =
                others.iter().map(|h| h.notify.notified()).collect();
            notify.next().await;
        }
    }
}

impl WorkerHandle<SyncMetric> {
    pub async fn wait_for_init(&self) {
        self.wait(SyncMetric::Init, 1).await
    }
}
