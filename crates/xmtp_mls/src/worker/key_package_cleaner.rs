use crate::context::XmtpSharedContext;
use crate::identity::IdentityError;
use crate::identity::pq_key_package_references_key;
use crate::worker::BoxedWorker;
use crate::worker::NeedsDbReconnect;
use crate::worker::WorkerResult;
use crate::worker::{Worker, WorkerFactory, WorkerKind};
use futures::TryFutureExt;
use openmls_traits::storage::StorageProvider;
use thiserror::Error;
use tracing::Instrument;
use xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION;
use xmtp_db::prelude::*;
use xmtp_db::{
    MlsProviderExt, StorageError,
    encrypted_store::key_package_history::StoredKeyPackageHistoryEntry,
    sql_key_store::{KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY},
};

#[derive(Debug, PartialEq, Eq)]
enum WakePlan {
    RunNow,
    SleepUntil(i64), // absolute deadline ns
}

/// Decide the next wake from the two maintenance deadlines:
/// - `next_rotation`: `None` (NULL) = rotation due now; else the absolute rotation time.
/// - `next_deletion`: soonest pending `delete_at_ns`, or `None` if nothing to delete.
///
/// Run now if either is already due; otherwise sleep until the SOONEST future deadline.
fn plan(next_rotation: Option<i64>, next_deletion: Option<i64>, now: i64) -> WakePlan {
    // NULL rotation or a past rotation/deletion deadline => act now. This also
    // means `next_rotation` is `Some` (and future) past this point.
    let Some(rotation) = next_rotation.filter(|&at| at > now) else {
        return WakePlan::RunNow;
    };
    if next_deletion.is_some_and(|at| at <= now) {
        return WakePlan::RunNow;
    }
    // Both deadlines (whichever are present) are in the future; sleep to the soonest.
    WakePlan::SleepUntil(next_deletion.map_or(rotation, |del| rotation.min(del)))
}

#[derive(Clone)]
pub struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::KeyPackageCleaner
    }

    fn create(
        &self,
        metrics: Option<crate::worker::DynMetrics>,
    ) -> (BoxedWorker, Option<crate::worker::DynMetrics>) {
        (
            Box::new(KeyPackagesCleanerWorker::new(self.context.clone())) as Box<_>,
            metrics,
        )
    }
}

#[derive(Debug, Error)]
pub enum KeyPackagesCleanerError {
    #[error("generic storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("generic identity error: {0}")]
    Identity(#[from] IdentityError),
    #[error("metadata error: {0}")]
    Metadata(StorageError),
    #[error("failed to fetch expired key packages: {0}")]
    Fetch(StorageError),
    #[error("failed to delete key package: {0}")]
    DeleteKeyPackage(IdentityError),
    #[error("deletion error: {0}")]
    Deletion(StorageError),
    #[error("rotation error: {0}")]
    Rotation(IdentityError),
}

impl NeedsDbReconnect for KeyPackagesCleanerError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Identity(s) => s.needs_db_reconnect(),
            Self::Metadata(s) => s.db_needs_connection(),
            Self::Fetch(s) => s.db_needs_connection(),
            Self::DeleteKeyPackage(s) => s.needs_db_reconnect(),
            Self::Deletion(s) => s.db_needs_connection(),
            Self::Rotation(s) => s.needs_db_reconnect(),
        }
    }
}

#[xmtp_common::async_trait]
impl<Context> Worker for KeyPackagesCleanerWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::KeyPackageCleaner
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        self.run().map_err(|e| Box::new(e) as Box<_>).await
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        Self: Sized,
        C: XmtpSharedContext + 'static,
    {
        Factory { context }
    }
}

pub struct KeyPackagesCleanerWorker<Context> {
    context: Context,
}

impl<Context> KeyPackagesCleanerWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

impl<Context> KeyPackagesCleanerWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(&mut self) -> Result<(), KeyPackagesCleanerError> {
        let receiver = self.context.key_package_channels().receiver.clone();
        let mut receiver = receiver.lock().await;
        loop {
            // Drain any pending re-arm signals before computing the plan so we
            // don't lose a wakeup that arrived while we were working.
            while receiver.try_recv().is_ok() {}

            let db = self.context.db();
            let next_rotation = db
                .next_key_package_rotation_ns()
                .map_err(KeyPackagesCleanerError::Metadata)?;
            match plan(
                next_rotation,
                db.min_key_package_delete_at_ns()
                    .map_err(KeyPackagesCleanerError::Metadata)?,
                xmtp_common::time::now_ns(),
            ) {
                WakePlan::RunNow => {
                    self.maintain(next_rotation).await?;
                }
                WakePlan::SleepUntil(deadline) => {
                    let dur = std::time::Duration::from_nanos(
                        (deadline - xmtp_common::time::now_ns()).max(0) as u64,
                    );
                    tokio::select! {
                        // Re-arm signal: recompute the deadline, do NOT run work.
                        // `None` means every sender was dropped (context torn down) —
                        // stop rather than busy-spin on a closed channel.
                        msg = receiver.recv() => { if msg.is_none() { return Ok(()); } }
                        // Deadline elapsed: time to do maintenance.
                        () = xmtp_common::time::sleep(dur) => {
                            self.maintain(next_rotation).await?;
                        }
                    }
                }
            }
        }
    }

    /// One maintenance pass. `next_rotation` is the rotation deadline already
    /// read by the caller (`None`/past = rotation due) — reused here so we don't
    /// re-query the identity row. Guards read OUTSIDE the span; if nothing is
    /// due, returns immediately with no span. Otherwise a single `worker_turn`
    /// span wraps the work so tracing records the full operation (incl. errors).
    async fn maintain(
        &mut self,
        next_rotation: Option<i64>,
    ) -> Result<(), KeyPackagesCleanerError> {
        let expired = self
            .context
            .db()
            .get_expired_key_packages()
            .map_err(KeyPackagesCleanerError::Fetch)?;
        // Same predicate as `is_identity_needs_rotation()`, from the value the
        // caller already fetched: NULL or a past deadline means rotate now.
        let rotate_due = next_rotation.is_none_or(|at| at <= xmtp_common::time::now_ns());

        if expired.is_empty() && !rotate_due {
            return Ok(());
        }

        let span = tracing::info_span!(
            "worker_turn",
            worker = ?self.kind(),
            operation = "worker_turn"
        );
        async {
            if !expired.is_empty() {
                self.delete_key_packages(expired)?;
            }
            if rotate_due {
                self.rotate_last_key_package_if_needed().await?;
            }
            Ok::<(), KeyPackagesCleanerError>(())
        }
        .instrument(span)
        .await
    }

    /// Delete a key package from the local database.
    pub(crate) fn delete_key_package(
        &self,
        hash_ref: Vec<u8>,
        pq_pub_key: Option<Vec<u8>>,
    ) -> Result<(), IdentityError> {
        let openmls_hash_ref = crate::identity::deserialize_key_package_hash_ref(&hash_ref)?;
        let mls_provider = self.context.mls_provider();
        let key_store = mls_provider.key_store();

        key_store.delete_key_package(&openmls_hash_ref)?;

        if let Some(pq_pub_key) = pq_pub_key {
            key_store.delete(
                KEY_PACKAGE_REFERENCES,
                pq_key_package_references_key(&pq_pub_key)?.as_slice(),
            )?;
            key_store.delete(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, &hash_ref)?;
        }

        Ok(())
    }

    /// Delete an already-fetched list of expired key packages from local state
    /// and the database history table.
    fn delete_key_packages(
        &mut self,
        expired: Vec<StoredKeyPackageHistoryEntry>,
    ) -> Result<(), KeyPackagesCleanerError> {
        let conn = self.context.db();
        for kp in &expired {
            self.delete_key_package(
                kp.key_package_hash_ref.clone(),
                kp.post_quantum_public_key.clone(),
            )
            .map_err(KeyPackagesCleanerError::DeleteKeyPackage)?;
        }
        if let Some(max_id) = expired.iter().map(|kp| kp.id).max() {
            conn.delete_key_package_history_up_to_id(max_id)
                .map_err(KeyPackagesCleanerError::Deletion)?;
            tracing::info!(
                "Deleted {} expired key packages (up to ID {})",
                expired.len(),
                max_id
            );
        }
        Ok(())
    }

    /// Upload a fresh key package if the current one has passed its rotation deadline.
    async fn rotate_last_key_package_if_needed(&mut self) -> Result<(), KeyPackagesCleanerError> {
        tracing::info!("Rotating key package");
        self.context
            .identity()
            .rotate_and_upload_key_package(
                self.context.api(),
                self.context.mls_storage(),
                CREATE_PQ_KEY_PACKAGE_EXTENSION,
            )
            .await
            .map_err(KeyPackagesCleanerError::Rotation)?;
        tracing::info!("Key package rotation successful");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn plan_cases() {
        use super::{WakePlan, plan};
        // NULL rotation = due now (regardless of deletion).
        assert_eq!(plan(None, None, 100), WakePlan::RunNow);
        assert_eq!(plan(None, Some(200), 100), WakePlan::RunNow);
        // Past rotation/deletion = due now.
        assert_eq!(plan(Some(99), None, 100), WakePlan::RunNow);
        assert_eq!(plan(Some(100), None, 100), WakePlan::RunNow);
        assert_eq!(plan(Some(200), Some(99), 100), WakePlan::RunNow); // deletion past
        // Both future: sleep to the soonest.
        assert_eq!(plan(Some(101), None, 100), WakePlan::SleepUntil(101));
        assert_eq!(plan(Some(300), Some(150), 100), WakePlan::SleepUntil(150)); // deletion sooner
        assert_eq!(plan(Some(150), Some(300), 100), WakePlan::SleepUntil(150)); // rotation sooner
        assert_eq!(plan(Some(200), None, 100), WakePlan::SleepUntil(200));
    }

    /// Requires a live XMTP node. Verifies that `queue_key_rotation` marks the
    /// identity for rotation and wakes the KeyPackageCleaner worker, which then
    /// uploads a new key package within ~12 s.
    #[xmtp_common::test(unwrap_try = true)]
    async fn queue_key_rotation_wakes_worker_and_rotates() {
        use crate::tester;

        tester!(client);

        let installation_id = client.installation_public_key().to_vec();

        // Snapshot the current on-network init key.
        // `VerifiedKeyPackageV2::hpke_init_key()` returns an owned Vec<u8>.
        let before = {
            let mut map = client
                .get_key_packages_for_installation_ids(vec![installation_id.clone()])
                .await?;
            let entry = map
                .remove(&installation_id)
                .expect("installation not found in response")?;
            entry.hpke_init_key()
        };

        // Queue a rotation and wake the worker.
        client.queue_key_rotation().await?;

        // Poll up to 12 s (60 × 200 ms) for the on-network key to change.
        // Uses `xmtp_common::time::sleep` so the test compiles for wasm too
        // (std::time::Instant panics on wasm).
        let mut after = None;
        for _ in 0..60u32 {
            xmtp_common::time::sleep(std::time::Duration::from_millis(200)).await;
            let mut map = client
                .get_key_packages_for_installation_ids(vec![installation_id.clone()])
                .await?;
            let entry = map
                .remove(&installation_id)
                .expect("installation not found in response")?;
            let key = entry.hpke_init_key();
            if key != before {
                after = Some(key);
                break;
            }
        }

        assert!(
            after.is_some(),
            "key package did not rotate after queue_key_rotation + worker wake"
        );
    }
}
