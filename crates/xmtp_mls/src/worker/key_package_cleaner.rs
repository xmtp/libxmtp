use crate::context::XmtpSharedContext;
use crate::identity::IdentityError;
use crate::identity::pq_key_package_references_key;
use crate::worker::BoxedWorker;
use crate::worker::NeedsDbReconnect;
use crate::worker::WorkerResult;
use crate::worker::{Worker, WorkerFactory, WorkerKind};
use futures::StreamExt;
use futures::TryFutureExt;
use openmls_traits::storage::StorageProvider;
use std::time::Duration;
use thiserror::Error;
use tracing::Instrument;
use xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION;
use xmtp_db::encrypted_store::key_package_history::StoredKeyPackageHistoryEntry;
use xmtp_db::prelude::*;
use xmtp_db::{
    MlsProviderExt, StorageError,
    sql_key_store::{KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY},
};

/// Coarse fallback cadence: backstops the 30-day rotation and 1-day deletion.
/// Welcome-queued rotation does NOT wait for this — it rides the wake channel.
/// Configurable per-kind via WorkerConfig; a small global default also applies
/// to this worker unless a KeyPackageCleaner override is set.
pub const INTERVAL_DURATION: Duration = Duration::from_secs(60 * 60); // 1 hour

/// Debounce window matching KEY_PACKAGE_QUEUE_INTERVAL_NS (5 s).
/// After a channel nudge we sleep this long so the queued rotation is due,
/// then drain any further nudges that arrived during the sleep.
const KEY_PACKAGE_QUEUE_INTERVAL: Duration =
    Duration::from_nanos(xmtp_configuration::KEY_PACKAGE_QUEUE_INTERVAL_NS as u64);

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
        let (base, jitter) = self
            .context
            .worker_interval(WorkerKind::KeyPackageCleaner, INTERVAL_DURATION);
        let mut intervals = xmtp_common::time::jittered_interval_stream(base, jitter);
        let receiver = self.context.key_package_channels().receiver.clone();
        let mut receiver = receiver.lock().await;

        // No explicit startup pass: on native, jittered_interval_stream's first
        // tick is immediate, so the first loop iteration covers an overdue/NULL
        // client. (On wasm the first interval tick waits one period; a fresh
        // client there relies on the queue→wake path, and overdue rotation is
        // bounded by the coarse interval — fine at 30d/1d work cadence.)
        loop {
            tokio::select! {
                _ = intervals.next() => {}                 // coarse fallback wake
                _ = receiver.recv()  => {                  // a rotation was queued (now+5s)
                    // Wait out the debounce window so the queued rotation is due,
                    // THEN drain: a nudge that arrived during the sleep is consumed
                    // here (not re-slept), avoiding a redundant second 5s sleep.
                    xmtp_common::time::sleep(KEY_PACKAGE_QUEUE_INTERVAL).await;
                    while receiver.try_recv().is_ok() {}
                }
            }
            self.maintain().await?;
        }
    }

    /// Run a maintenance pass. Reads two cheap guards OUTSIDE any span; returns
    /// early (NO span) when idle. Otherwise opens exactly ONE `worker_turn` span
    /// wrapping the real work, so a failing delete/rotate is recorded.
    async fn maintain(&mut self) -> Result<(), KeyPackagesCleanerError> {
        let expired_kps = self
            .context
            .db()
            .get_expired_key_packages()
            .map_err(KeyPackagesCleanerError::Fetch)?;
        let rotate_due = self
            .context
            .db()
            .is_identity_needs_rotation()
            .map_err(KeyPackagesCleanerError::Metadata)?;

        if expired_kps.is_empty() && !rotate_due {
            return Ok(()); // idle: no span, no work
        }

        let span = tracing::info_span!(
            "worker_turn",
            worker = ?self.kind(),
            operation = "worker_turn"
        );
        async {
            if !expired_kps.is_empty() {
                self.delete_key_packages(expired_kps)?;
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

    /// Delete the given already-fetched expired key packages. The caller fetched
    /// the list so the span-gate can decide whether to open a span without a
    /// second scan of key_package_history.
    fn delete_key_packages(
        &mut self,
        expired_kps: Vec<StoredKeyPackageHistoryEntry>,
    ) -> Result<(), KeyPackagesCleanerError> {
        let conn = self.context.db();
        tracing::info!("Deleting {} expired key packages", expired_kps.len());
        for kp in &expired_kps {
            self.delete_key_package(
                kp.key_package_hash_ref.clone(),
                kp.post_quantum_public_key.clone(),
            )
            .map_err(KeyPackagesCleanerError::DeleteKeyPackage)?;
        }
        if let Some(max_id) = expired_kps.iter().map(|kp| kp.id).max() {
            conn.delete_key_package_history_up_to_id(max_id)
                .map_err(KeyPackagesCleanerError::Deletion)?;
            tracing::info!(
                "Deleted {} expired key packages (up to ID {}) from local DB and state",
                expired_kps.len(),
                max_id
            );
        }
        Ok(())
    }

    /// Check if we need to rotate the keys and upload new keypackage if the las one rotate in has passed
    async fn rotate_last_key_package_if_needed(&mut self) -> Result<(), KeyPackagesCleanerError> {
        let conn = self.context.db();

        if conn
            .is_identity_needs_rotation()
            .map_err(KeyPackagesCleanerError::Metadata)?
        {
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
            return Ok(());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{identity::serialize_key_package_hash_ref, tester};
    use std::time::Duration;
    use xmtp_db::prelude::QueryIdentity;

    /// Helper: fetch the network key package's HPKE init key for `client`.
    async fn current_init_key<C: crate::context::XmtpSharedContext>(
        client: &crate::Client<C>,
    ) -> Vec<u8> {
        let installation_id = client.installation_public_key().to_vec();
        let mut map = client
            .get_key_packages_for_installation_ids(vec![installation_id.clone()])
            .await
            .expect("fetch key packages");
        let kp = map
            .remove(&installation_id)
            .expect("installation id present")
            .expect("key package resolved");
        serialize_key_package_hash_ref(&kp.inner, &client.context.mls_provider())
            .expect("serialize hash ref")
    }

    /// Asserts that calling `queue_key_rotation` wakes the `KeyPackageCleaner`
    /// worker and causes a real key-package rotation within a bounded wait.
    ///
    /// The worker interval is overridden to 100 ms by `WorkerConfig::for_testing()`
    /// (injected via `TesterBuilder::default`), so the coarse-cadence tick fires
    /// quickly.  The wake channel fires even sooner: after the 5-second debounce
    /// the worker calls `maintain()` and uploads the new key package.
    ///
    /// NOTE: this test requires a live XMTP node.  In environments without one
    /// it will fail with a connection-refused error, which is an infrastructure
    /// limitation rather than a logic failure.
    #[xmtp_common::test]
    async fn queue_key_rotation_wakes_worker_and_rotates() {
        tester!(client);

        // Snapshot the current on-network key-package hash.
        let init_key_before = current_init_key(&client).await;

        // Queue a rotation: lowers next_key_package_rotation_ns to now+5s and
        // fires the wake channel. (It does NOT make rotation due *now* — the
        // deadline is 5s out, which is the debounce the worker waits through.)
        client.queue_key_rotation().await.unwrap();

        // Bounded poll: the debounce is 5 s, so wait up to 12 s total
        // (60 × 200 ms). Iteration-counted rather than wall-clock so it works on
        // wasm, where std::time::Instant::now() panics (no std clock).
        let mut init_key_after = None;
        for _ in 0..60 {
            xmtp_common::time::sleep(Duration::from_millis(200)).await;
            let key = current_init_key(&client).await;
            if key != init_key_before {
                init_key_after = Some(key);
                break;
            }
        }
        let init_key_after = init_key_after.expect(
            "key package did not rotate within 12 s after queue_key_rotation; \
             worker wake path may be broken",
        );

        assert_ne!(
            init_key_before, init_key_after,
            "key package should have rotated after queue_key_rotation + worker wake"
        );

        // Rotation flag should be cleared by the worker.
        let still_needs_rotation = client.context.db().is_identity_needs_rotation().unwrap();
        assert!(
            !still_needs_rotation,
            "rotation flag should be cleared after worker ran"
        );
    }
}
