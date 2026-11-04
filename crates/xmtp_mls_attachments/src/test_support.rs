use super::*;

/// The size of each piece a download streams.
pub const CHUNK: usize = super::CHUNK;
/// How long an upload lease lasts.
pub const LEASE_DURATION: Duration = super::LEASE_DURATION;
/// How often a running upload renews its lease.
pub const LEASE_RENEW: Duration = super::LEASE_RENEW;

impl AttachmentRuntime {
    /// The local store attachment files are kept in.
    pub fn local_store(&self) -> Result<&Arc<dyn LocalStore>, AttachmentClientError> {
        self.store()
    }

    /// Reconcile stored attachments with their local files again.
    pub async fn reconcile_again<Context: AttachmentContext>(
        &self,
        context: &Context,
    ) -> Result<(), AttachmentClientError> {
        self.reconcile(context).await
    }

    /// Whether the runtime still holds shared state for the pending
    /// attachment with `digest`.
    pub fn holds_pending(&self, digest: &str) -> bool {
        self.pending.lock().contains_key(digest)
    }

    /// The running download of the plaintext at `relative`, if any.
    pub fn running_download(&self, relative: &str) -> Option<DownloadProbe> {
        self.downloads
            .lock()
            .get(relative)
            .cloned()
            .map(DownloadProbe)
    }
}

impl AttachmentClientError {
    /// The error of `cause` alone, as the runtime records it.
    pub fn of_cause(cause: Cause) -> Self {
        Self::new(cause)
    }
}

/// One running download, as the callers that wait for it share it.
pub struct DownloadProbe(Arc<DownloadShared>);

impl DownloadProbe {
    /// How many callers wait for this download's outcome.
    pub fn waiters(&self) -> usize {
        self.0.outcome.receiver_count()
    }

    /// Whether a deletion cancelled this download.
    pub fn is_cancelled(&self) -> bool {
        self.0.cancel.is_cancelled()
    }
}

/// The state that every handle of one pending attachment shares.
#[derive(Clone)]
pub struct PendingProbe(pub(super) Arc<PendingShared>);

impl PendingProbe {
    /// This client's upload lease: its token and expiry.
    pub fn lease(&self) -> Option<([u8; 16], i64)> {
        *self.0.lease.lock()
    }

    /// The upload attempt this client runs, if any.
    pub fn attempt(&self) -> Option<AttemptProbe> {
        self.0.attempt.lock().clone().map(AttemptProbe)
    }

    /// Whether an operation holds the attachment's state lock.
    pub fn state_is_held(&self) -> bool {
        self.0.state.try_lock().is_err()
    }

    /// The status last published to watchers.
    pub fn watched_status(&self) -> PendingAttachmentStatus {
        self.0.watch.borrow().clone()
    }

    /// Whether `other` is a handle of the same pending attachment.
    pub fn shares_with(&self, other: &PendingProbe) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// One upload attempt of a pending attachment.
pub struct AttemptProbe(Arc<PendingAttempt>);

impl AttemptProbe {
    /// Cancelled once the attempt has ended.
    pub fn done(&self) -> &CancellationToken {
        &self.0.done
    }
}

impl<Context: AttachmentContext> PendingAttachment<Context> {
    /// The state this handle shares with the other handles of the
    /// attachment.
    pub fn probe(&self) -> PendingProbe {
        PendingProbe(self.shared.clone())
    }

    /// The reference events carry for this attachment.
    pub fn reference_for_test(&self) -> AttachmentRef {
        self.reference()
    }

    /// Run one upload under the lease `token`, as an attempt does.
    pub async fn upload_once_for_test(
        &self,
        token: &[u8; 16],
    ) -> Result<(), AttachmentClientError> {
        self.upload_once(token).await
    }
}
