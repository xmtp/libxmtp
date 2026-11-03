use std::sync::Arc;

#[derive(Clone)]
pub struct TaskWorkerChannels {
    /// Serializes notification requests across inline calls and task turns.
    pub(crate) notification_request: Arc<tokio::sync::Mutex<()>>,
    notification_revision: Arc<std::sync::atomic::AtomicUsize>,
}

impl Default for TaskWorkerChannels {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskWorkerChannels {
    pub fn new() -> Self {
        Self {
            notification_request: Default::default(),
            notification_revision: Default::default(),
        }
    }

    /// Invalidate a prepared notification batch after a local change.
    pub fn mark_notification_changed(&self) {
        use std::sync::atomic::Ordering;
        self.notification_revision.fetch_add(1, Ordering::AcqRel);
    }

    pub(crate) fn notification_revision(&self) -> usize {
        self.notification_revision
            .load(std::sync::atomic::Ordering::Acquire)
    }
}
