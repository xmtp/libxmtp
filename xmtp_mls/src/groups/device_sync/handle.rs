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

    pub fn reset(&self) {
        *self.metrics.lock() = HashMap::new();
    }

    /// Blocks until metric's specified count is met
    pub async fn wait(&self, metric: Metric, count: usize) {
        let metric = self.metrics.lock().entry(metric).or_default().clone();
        while metric.load(Ordering::SeqCst) < count {
            self.notify.notified().await;
        }
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
    async fn wait_one(&self, metric: Metric, count: usize);
}

#[async_trait::async_trait(?Send)]
impl<Metric> WorkHandleCollection<Metric> for Vec<&WorkerHandle<Metric>>
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

impl WorkerHandle<SyncMetric> {
    pub async fn wait_for_init(&self) {
        self.wait(SyncMetric::Init, 1).await
    }
}
