use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::Notify;

#[derive(Default)]
pub struct SyncWorkerHandle {
    pub(super) new_installations_added_to_groups: AtomicUsize,
    notify: Notify,
}

impl SyncWorkerHandle {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub fn new_installations_added_to_groups(&self) -> usize {
        self.new_installations_added_to_groups
            .load(Ordering::Relaxed)
    }
}
