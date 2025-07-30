use futures::FutureExt;
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    fmt::Debug,
    future::Future,
    hash::Hash,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::Notify;
use xmtp_common::types::InstallationId;

pub struct MetricInterest<Metric> {
    fut: Pin<Box<dyn Future<Output = ()> + Send>>,
    count: usize,
    info: Info,
    metric: Metric,
}

impl<Metric> MetricInterest<Metric>
where
    Metric: Debug,
{
    /// Wait for a metric to be resolved
    pub async fn wait(self) -> Result<(), xmtp_common::time::Expired> {
        let Self {
            fut,
            count,
            info,
            metric,
        } = self;

        let secs = if cfg!(target_arch = "wasm32") { 15 } else { 5 };
        let result = xmtp_common::time::timeout(Duration::from_secs(secs), fut).await;
        tracing::info!(
            "[{}] GOT {metric:?} REGISTER INTEREST {:?}",
            hex::encode(info.installation_id),
            result
        );
        if info.count() >= count {
            return Ok(());
        }
        tracing::error!(
            "Timed out waiting for {:?} to be >= {}. Value: {}",
            metric,
            count,
            info.count()
        );
        result
    }
}

/// Information and interest in a specific metric
#[derive(Debug, Clone)]
struct Info {
    count: Arc<AtomicUsize>,
    // each Notify is a task waiting on this metric to be incremented
    notify: Arc<Notify>,
    installation_id: InstallationId,
}

impl Info {
    fn new(installation_id: InstallationId) -> Self {
        Self {
            count: Arc::new(AtomicUsize::default()),
            notify: Arc::new(Notify::new()),
            installation_id,
        }
    }

    fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    fn increment(&self) {
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    fn clear(&self) {
        self.count.store(0, Ordering::SeqCst)
    }

    // Registers interest in the next time this event is fired.
    // Returns a future that resolves when this event resolves
    fn register_interest(&self) -> impl Future<Output = ()> {
        Notify::notified_owned(self.notify.clone())
    }

    fn fire(&self) {
        self.notify.notify_waiters();
    }
}

#[derive(Debug)]
pub struct WorkerMetrics<Metric> {
    metrics: Mutex<HashMap<Metric, Info>>,
    installation_id: InstallationId,
}

impl<Metric> WorkerMetrics<Metric>
where
    Metric: PartialEq + Eq + Hash + Clone + Copy + Debug,
{
    pub fn new(installation_id: InstallationId) -> Self {
        Self {
            metrics: Mutex::default(),
            installation_id,
        }
    }

    fn info(&self, metric: Metric) -> Info {
        self.metrics
            .lock()
            .entry(metric)
            .or_insert(Info::new(self.installation_id))
            .clone()
    }

    pub fn get(&self, metric: Metric) -> usize {
        self.info(metric).count()
    }

    pub(crate) fn increment_metric(&self, metric: Metric) {
        self.info(metric).increment();
        tracing::info!("[{}] firing {metric:?}", hex::encode(self.installation_id));
        self.info(metric).fire();
    }

    pub fn reset_metrics(&self) {
        *self.metrics.lock() = HashMap::new();
    }

    /// Register interest in a metric at a certain 'count'.
    /// Returns a MetricInterest type that can be used to wait for the metric to reac hthese
    /// parameters
    pub fn register_interest(&self, metric_key: Metric, count: usize) -> MetricInterest<Metric> {
        tracing::info!("registering interest in {metric_key:?}");
        MetricInterest {
            fut: self.info(metric_key).register_interest().boxed(),
            count,
            info: self.info(metric_key),
            metric: metric_key,
        }
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
        let mut m = self.metrics.lock();
        let info = m.entry(metric).or_insert(Info::new(self.installation_id));
        xmtp_common::time::timeout(Duration::from_secs(20), async {
            while info.count() < count {
                f().await;
                xmtp_common::task::yield_now().await;
            }
        })
        .await
    }

    pub fn clear_metric(&self, metric: Metric) {
        self.metrics
            .lock()
            .entry(metric)
            .or_insert(Info::new(self.installation_id))
            .clear();
    }
}
