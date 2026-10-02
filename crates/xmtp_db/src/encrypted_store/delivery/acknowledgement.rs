//! Cancellation at the final delivery acknowledgement commit boundary.

#[cfg(any(test, feature = "test-utils"))]
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

const ACTIVE: u8 = 0;
const CANCELLED: u8 = 1;
const COMMIT_ADMITTED: u8 = 2;

/// Rust-only state shared by a pending iterator request and its storage write.
/// Cancellation never waits for the database writer.
#[derive(Default)]
pub struct DeliveryAckRequest {
    state: AtomicU8,
    #[cfg(any(test, feature = "test-utils"))]
    observer: parking_lot::Mutex<Option<Arc<dyn Fn(DeliveryAckPhase) + Send + Sync>>>,
}

impl DeliveryAckRequest {
    /// Cancel only a request whose final commit has not been admitted.
    pub fn cancel(&self) {
        let _ = self
            .state
            .compare_exchange(ACTIVE, CANCELLED, Ordering::AcqRel, Ordering::Acquire);
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::Acquire) == CANCELLED
    }

    pub(crate) fn admit_commit(&self) -> bool {
        self.state
            .compare_exchange(ACTIVE, COMMIT_ADMITTED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn set_observer(&self, observer: Arc<dyn Fn(DeliveryAckPhase) + Send + Sync>) {
        *self.observer.lock() = Some(observer);
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn observe(&self, phase: DeliveryAckPhase) {
        let observer = self.observer.lock().clone();
        if let Some(observer) = observer {
            observer(phase);
        }
    }
}

/// Private test phases in the real acknowledgement transaction.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryAckPhase {
    BeforeWriter,
    WriterAcquired,
    TentativeUpdate,
    CommitAdmitted,
}
