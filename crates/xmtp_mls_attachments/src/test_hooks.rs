use super::*;

/// A pause: the paused operation notifies the first and waits for the
/// second.
pub type Pause = (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>);

/// Where an operation pauses.
#[derive(Clone, Copy, Debug)]
pub enum PausePoint {
    /// A sweep, after it reads the expired rows.
    Sweep,
    /// A deletion, while it holds the deletion mark.
    Delete,
    /// An upload, before it writes its outcome under the event lock.
    OutcomeLock,
    /// An upload attempt, before its task panics.
    AttemptPanic,
    /// A download, before its task panics.
    DownloadPanic,
    /// A download, before it moves its file into place.
    DownloadMove,
    /// A download, before it publishes its metadata.
    DownloadPublish,
    /// A download, before it reports success.
    DownloadCompletion,
    /// A download, before it reports failure.
    DownloadFailure,
    /// A download, when it finds an existing file.
    DownloadExisting,
    /// A reconciliation, after it takes its snapshot.
    ReconcileSnapshot,
}

/// A local storage step that fails once.
#[derive(Clone, Copy, Debug)]
pub enum Fault {
    /// The removal of an uploaded attachment's staged file.
    StagedRemoval,
    /// The cleanup after a download's metadata write fails.
    DownloadCleanup,
}

/// A pause of a creation at its first file move. The creation records the
/// paths it moves.
pub struct CreateMovePause {
    /// The relative path of the plaintext file the creation moves.
    pub path: Mutex<Option<String>>,
    /// The relative path of the staged ciphertext the creation moves.
    pub staged_path: Mutex<Option<String>>,
    /// Notified when the creation reaches the pause.
    pub entered: tokio::sync::Notify,
    /// Notify it to let the paused creation continue.
    pub resume: tokio::sync::Notify,
}

/// A pause of a creation at one of its publication points, before or
/// after the staged file. The creation records the paths it publishes.
pub struct CreatePublishPause {
    /// Pause after the staged file is published. Otherwise the pause
    /// comes after the plaintext file and before the staged file.
    pub after_staged: bool,
    /// The relative path of the plaintext file the creation publishes.
    pub path: Mutex<Option<String>>,
    /// The relative path of the staged ciphertext the creation publishes.
    pub staged_path: Mutex<Option<String>>,
    /// Notified when the creation reaches the pause.
    pub entered: tokio::sync::Notify,
    /// Notify it to let the paused creation continue.
    pub resume: tokio::sync::Notify,
}

/// Fail the next `count` reconciliations of every runtime.
pub fn fail_next_reconciles(count: usize) {
    FAIL_NEXT_RECONCILES.store(count, AtomicOrdering::SeqCst);
}

impl AttachmentRuntime {
    /// Pause the next operations at `point` with `pause`, or stop pausing
    /// there when it is `None`.
    pub fn set_pause(&self, point: PausePoint, pause: Option<Pause>) {
        let slot = match point {
            PausePoint::Sweep => &self.sweep_pause,
            PausePoint::Delete => &self.delete_pause,
            PausePoint::OutcomeLock => &self.outcome_lock_pause,
            PausePoint::AttemptPanic => &self.attempt_panic_pause,
            PausePoint::DownloadPanic => &self.download_panic_pause,
            PausePoint::DownloadMove => &self.download_move_pause,
            PausePoint::DownloadPublish => &self.download_publish_pause,
            PausePoint::DownloadCompletion => &self.download_completion_pause,
            PausePoint::DownloadFailure => &self.download_failure_pause,
            PausePoint::DownloadExisting => &self.download_existing_pause,
            PausePoint::ReconcileSnapshot => &self.reconcile_snapshot_pause,
        };
        *slot.lock() = pause;
    }

    /// Fail the next `fault` step.
    pub fn fail_next(&self, fault: Fault) {
        match fault {
            Fault::StagedRemoval => &self.fail_next_staged_removal,
            Fault::DownloadCleanup => &self.fail_next_download_cleanup,
        }
        .store(true, AtomicOrdering::SeqCst);
    }

    /// Pause creations at their first file move.
    pub fn set_create_move_pause(&self, pause: Option<Arc<CreateMovePause>>) {
        *self.create_move_pause.lock() = pause;
    }

    /// Pause creations at a publication point.
    pub fn set_create_publish_pause(&self, pause: Option<Arc<CreatePublishPause>>) {
        *self.create_publish_pause.lock() = pause;
    }

    /// Run upload leases with these timings.
    pub fn set_lease_timing(&self, duration: Duration, renew: Duration, poll: Duration) {
        *self.lease_timing.lock() = LeaseTiming::for_test(duration, renew, poll);
    }

    /// How many upload outcome writes have failed.
    pub fn outcome_write_errors(&self) -> usize {
        self.outcome_write_errors.load(AtomicOrdering::SeqCst)
    }

    /// How many upload lease extensions have failed.
    pub fn lease_extension_errors(&self) -> usize {
        self.lease_extension_errors.load(AtomicOrdering::SeqCst)
    }

    /// The lock that orders the outcome and deletion events of `key`.
    pub fn event_lock_for_test(&self, key: &str) -> Arc<AsyncMutex<()>> {
        self.event_lock(key)
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl test_support::PendingProbe {
    /// Replace this client's upload lease.
    pub fn set_lease(&self, lease: Option<([u8; 16], i64)>) {
        *self.0.lease.lock() = lease;
    }
}
