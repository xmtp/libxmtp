//! Key-package maintenance as TaskRunner consumers: payload/seed helpers and the
//! rotate/sweep work the `KpRotation`/`KpDeletion` dispatch arms call into.
//! Recurrence + nudging come from the generic layer (TaskOutcome, PullInDeadline).

use crate::context::XmtpSharedContext;
use crate::identity::IdentityError;
use crate::worker::NeedsDbReconnect;
use crate::worker::tasks::enqueue_pull_in;
use openmls_traits::storage::StorageProvider;
use thiserror::Error;
use xmtp_common::NS_IN_SEC;
use xmtp_configuration::{
    CREATE_PQ_KEY_PACKAGE_EXTENSION, KEY_PACKAGE_LIVENESS_INTERVAL_NS,
    KEY_PACKAGE_LIVENESS_MIN_REMAINING_LIFETIME_NS, KEY_PACKAGE_LIVENESS_RETRY_INTERVAL_NS,
};
use xmtp_db::MlsProviderExt;
use xmtp_db::StorageError;
use xmtp_db::XmtpOpenMlsProvider;
use xmtp_db::prelude::*;
use xmtp_db::sql_key_store::{KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY};
use xmtp_db::tasks::{NEVER_EXPIRES, NewTask, TaskDataHash, data_hash_for};
use xmtp_id::key_package::VerifiedKeyPackageV2;
use xmtp_proto::xmtp::mls::database::{
    KpDeletion, KpLiveness, KpRotation, Task as TaskProto, task::Task as TaskKind,
};

#[derive(Debug, Error)]
pub enum KeyPackageMaintenanceError {
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

impl NeedsDbReconnect for KeyPackageMaintenanceError {
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

pub(crate) fn kp_rotation_proto() -> TaskProto {
    TaskProto {
        task: Some(TaskKind::KpRotation(KpRotation {})),
    }
}

pub(crate) fn kp_deletion_proto() -> TaskProto {
    TaskProto {
        task: Some(TaskKind::KpDeletion(KpDeletion {})),
    }
}

pub(crate) fn kp_liveness_proto() -> TaskProto {
    TaskProto {
        task: Some(TaskKind::KpLiveness(KpLiveness {})),
    }
}

pub(crate) fn kp_rotation_hash() -> TaskDataHash {
    data_hash_for(&kp_rotation_proto())
}

pub(crate) fn kp_deletion_hash() -> TaskDataHash {
    data_hash_for(&kp_deletion_proto())
}

pub(crate) fn kp_liveness_hash() -> TaskDataHash {
    data_hash_for(&kp_liveness_proto())
}

/// Never-expire recurring seed: the reaper's
/// `expires_at_ns < now || attempts >= max_attempts` check can never fire.
pub(crate) fn kp_seed(proto: TaskProto, now: i64) -> Result<NewTask, StorageError> {
    NewTask::builder()
        .originating_message_sequence_id(0)
        .originating_message_originator_id(0)
        .expires_at_ns(NEVER_EXPIRES)
        .max_attempts(i32::MAX)
        .next_attempt_at_ns(now)
        .build(proto)
}

/// Rotate + upload a fresh key package if the identity's rotation deadline is due.
/// Returns whether a rotation happened. `rotate_and_upload_key_package` internally
/// rolls the rotation column +30d and marks superseded KPs `delete_at = now+grace`.
///
/// Both branches log at INFO. A silent "not due" branch hid the incident: it looks
/// the same as a client with nothing to do. The task parks ~30 days out, so the
/// log volume is small.
pub(crate) async fn rotate_if_needed<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<bool, KeyPackageMaintenanceError> {
    let db = context.db();
    if !db
        .is_identity_needs_rotation()
        .map_err(KeyPackageMaintenanceError::Metadata)?
    {
        let deadline = db
            .next_key_package_rotation_ns()
            .map_err(KeyPackageMaintenanceError::Metadata)?;
        tracing::info!(
            next_rotation_at_ns = ?deadline,
            due_in_ns = ?deadline.map(|d| d.saturating_sub(xmtp_common::time::now_ns())),
            "Key package rotation not due; skipping"
        );
        return Ok(false);
    }
    tracing::info!("Key package rotation is due; rotating");
    context
        .identity()
        .rotate_and_upload_key_package(
            context.api(),
            context.mls_storage(),
            CREATE_PQ_KEY_PACKAGE_EXTENSION,
        )
        .await
        .map_err(KeyPackageMaintenanceError::Rotation)?;
    Ok(true)
}

#[cfg(test)]
pub(crate) mod test_hooks {
    use super::LivenessOutcome;
    use std::sync::Mutex;
    /// Force a probe outcome instead of a network call. The test backend cannot
    /// forget a published key package, so this is the only way to run the
    /// unhealthy branches end to end. Assumes process-per-test isolation.
    pub(crate) static PROBE_OVERRIDE: Mutex<Option<LivenessOutcome>> = Mutex::new(None);
}

/// Result of probing the network for THIS installation's own key package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LivenessOutcome {
    /// A key package is published, verifies, and has comfortable lifetime left.
    Healthy { remaining_ns: i64 },
    /// Nothing published for this installation.
    Absent,
    /// Published but unusable. Verification includes the lifetime, so an expired
    /// key package lands here.
    Unverifiable,
    /// Valid, but inside `KEY_PACKAGE_LIVENESS_MIN_REMAINING_LIFETIME_NS` of
    /// expiry. The client missed several rotations to get here.
    ExpiringSoon { remaining_ns: i64 },
    /// No verdict: offline, 5xx, timeout, or a backend defect. Says nothing
    /// about our key package, so it must not change state.
    Inconclusive,
}

impl LivenessOutcome {
    /// Whether this outcome means our published key package cannot be relied on.
    fn needs_rotation(&self) -> bool {
        matches!(
            self,
            Self::Absent | Self::Unverifiable | Self::ExpiringSoon { .. }
        )
    }
}

/// Classify a failed single-installation fetch.
///
/// The batch API is positional. It reports a wrong-length response as
/// `MismatchedKeyPackages`. Only "asked for 1, got 0" proves absence. Any other
/// count is a backend defect. If we called that absence, one backend incident
/// would rotate every client's key package every interval and repair nothing.
fn classify_fetch_error(err: &xmtp_api::ApiError) -> LivenessOutcome {
    match err {
        xmtp_api::ApiError::MismatchedKeyPackages {
            key_packages: 0,
            installation_keys: 1,
        } => LivenessOutcome::Absent,
        error => {
            tracing::warn!(%error, "Key package liveness probe failed; treating as inconclusive");
            LivenessOutcome::Inconclusive
        }
    }
}

/// Classify the bytes the network returned for `installation_id`.
///
/// Pure: no I/O, and `now_ns` is a parameter. Every wire shape is testable
/// without a network or an override. `bytes` is `None` if the response carried
/// no entry for this installation.
fn classify_key_package(
    installation_id: &[u8],
    bytes: Option<&[u8]>,
    now_ns: i64,
) -> LivenessOutcome {
    // Two wire shapes mean "nothing published": no entry, and an empty payload.
    // The network returns the empty payload for a dead installation.
    let Some(bytes) = bytes.filter(|b| !b.is_empty()) else {
        return LivenessOutcome::Absent;
    };

    let key_package =
        match VerifiedKeyPackageV2::from_bytes(&XmtpOpenMlsProvider::<()>::new_crypto(), bytes) {
            Ok(key_package) => key_package,
            Err(error) => {
                // `from_bytes` verifies the lifetime, so an expired key package
                // lands here, not in the margin check below.
                tracing::warn!(%error, "Published key package failed verification");
                return LivenessOutcome::Unverifiable;
            }
        };

    // The API matches responses to requests by position, so a backend defect can
    // return a valid key package that belongs to someone else. It verifies and
    // looks healthy, but we stay unaddable: the silent loop we must break.
    // Inconclusive, not Unverifiable — a new upload cannot repair a mapping
    // defect, and rotating would churn the whole fleet during one incident.
    if key_package.installation_id() != installation_id {
        tracing::error!(
            expected = hex::encode(installation_id),
            got = hex::encode(key_package.installation_id()),
            "Key package response is for a different installation; backend mapping defect"
        );
        return LivenessOutcome::Inconclusive;
    }

    let Some(lifetime) = key_package.life_time() else {
        return LivenessOutcome::Unverifiable;
    };
    // MLS lifetimes are UNIX seconds; everything else here is nanoseconds.
    let not_after_ns = (lifetime.not_after as i64).saturating_mul(NS_IN_SEC);
    let remaining_ns = not_after_ns.saturating_sub(now_ns);
    if remaining_ns <= KEY_PACKAGE_LIVENESS_MIN_REMAINING_LIFETIME_NS {
        LivenessOutcome::ExpiringSoon { remaining_ns }
    } else {
        LivenessOutcome::Healthy { remaining_ns }
    }
}

/// Ask the network if THIS installation has a usable key package.
///
/// Read-only. A network failure returns `Inconclusive`, never `Err`, so the
/// outcome alone tells the caller what to do next.
pub(crate) async fn probe_key_package_liveness<Context: XmtpSharedContext>(
    context: &Context,
) -> LivenessOutcome {
    #[cfg(test)]
    if let Some(outcome) = test_hooks::PROBE_OVERRIDE.lock().unwrap().clone() {
        return outcome;
    }
    probe_installation_key_package(context, context.installation_id().to_vec()).await
}

/// Fetch + classify. Takes the installation id so tests can probe an
/// unregistered id against the real backend.
///
/// Calls the API directly, not `MlsStore`. `MlsStore` deserializes eagerly, so
/// it reports an empty payload as a verification error. The network returns an
/// empty payload for a dead installation, and operators need those told apart.
async fn probe_installation_key_package<Context: XmtpSharedContext>(
    context: &Context,
    installation_id: Vec<u8>,
) -> LivenessOutcome {
    let key_packages = match context
        .api()
        .fetch_key_packages(vec![installation_id.clone()])
        .await
    {
        Ok(key_packages) => key_packages,
        Err(e) => return classify_fetch_error(&e),
    };
    classify_key_package(
        &installation_id,
        key_packages
            .get(installation_id.as_slice())
            .map(Vec::as_slice),
        xmtp_common::time::now_ns(),
    )
}

/// When the liveness check may next run, from the recorded stamp.
///
/// Trust a stamp only if it is in the past and inside one interval. A future
/// stamp (clock skew or corruption) or an old one gives `now`, which means due.
/// A wrong timestamp must never disable this check: that is the failure the
/// check exists to catch. Startup reconciliation calls this for the same reason.
fn next_liveness_deadline(checked_at: Option<i64>, now: i64) -> i64 {
    let Some(checked_at) = checked_at else {
        return now;
    };
    let elapsed = now.saturating_sub(checked_at);
    if elapsed < 0 {
        tracing::warn!(
            checked_at_ns = checked_at,
            now_ns = now,
            "Key package liveness stamp is in the future (clock skew or corruption); treating as due"
        );
        return now;
    }
    if elapsed >= KEY_PACKAGE_LIVENESS_INTERVAL_NS {
        return now;
    }
    checked_at.saturating_add(KEY_PACKAGE_LIVENESS_INTERVAL_NS)
}

/// Run the throttled liveness check. Returns the deadline to reschedule the
/// `KpLiveness` task to.
///
/// The throttle is a DB column, not the task row's deadline: a `PullInDeadline`
/// nudge overwrites that deadline, so the row cannot record both the next run
/// and the last one. This handler is therefore the authority, and nudges are free.
pub(crate) async fn run_liveness_check<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<i64, KeyPackageMaintenanceError> {
    let db = context.db();
    let now = xmtp_common::time::now_ns();

    // If the client is not registered, do not probe. It has uploaded nothing
    // yet, so the probe would warn about a missing key package and make that
    // warning worthless. Registration stamps the throttle, so nothing is lost.
    if !context.identity().is_ready() {
        tracing::debug!("Identity not yet registered; skipping key package liveness check");
        return Ok(now.saturating_add(KEY_PACKAGE_LIVENESS_INTERVAL_NS));
    }

    let checked_at = db
        .key_package_liveness_checked_at_ns()
        .map_err(KeyPackageMaintenanceError::Metadata)?;
    let next_due = next_liveness_deadline(checked_at, now);
    if next_due > now {
        tracing::debug!(
            next_check_at_ns = next_due,
            "Key package liveness check throttled"
        );
        return Ok(next_due);
    }

    let outcome = probe_key_package_liveness(context).await;
    if outcome == LivenessOutcome::Inconclusive {
        // Do not stamp the throttle. An offline client must not spend its
        // window on a probe that learned nothing.
        return Ok(now.saturating_add(KEY_PACKAGE_LIVENESS_RETRY_INTERVAL_NS));
    }

    if outcome.needs_rotation() {
        tracing::warn!(
            ?outcome,
            "This installation has no usable key package on the network; queueing a rotation"
        );
        // The one repair path, shared with the welcome nudge.
        queue_key_rotation(context)?;
    } else {
        tracing::info!(?outcome, "Key package liveness verified");
    }

    db.record_key_package_liveness_check()
        .map_err(KeyPackageMaintenanceError::Metadata)?;
    Ok(now.saturating_add(KEY_PACKAGE_LIVENESS_INTERVAL_NS))
}

/// Ask the `KpLiveness` task to run at the next opportunity. Creates the seed
/// first, because a pull-in with no target is dropped.
///
/// `not_later_than_ns` is a fixed `0`, not `now`, so every call builds the same
/// payload and `create_or_ignore_task` coalesces on `data_hash`. Only one
/// pull-in can be pending. The handler's throttle decides if a probe runs.
pub(crate) fn nudge_liveness<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<(), StorageError> {
    let now = xmtp_common::time::now_ns();
    context
        .db()
        .create_or_ignore_task(kp_seed(kp_liveness_proto(), now)?)?;
    enqueue_pull_in(context, kp_liveness_hash(), 0, NEVER_EXPIRES)
}

/// Delete one key package's local material (keystore entry + PQ references).
pub(crate) fn delete_key_package<Context: XmtpSharedContext>(
    context: &Context,
    hash_ref: Vec<u8>,
    pq_pub_key: Option<Vec<u8>>,
) -> Result<(), IdentityError> {
    let openmls_hash_ref = crate::identity::deserialize_key_package_hash_ref(&hash_ref)?;
    let mls_provider = context.mls_provider();
    let key_store = mls_provider.key_store();

    key_store.delete_key_package(&openmls_hash_ref)?;

    if let Some(pq_pub_key) = pq_pub_key {
        key_store.delete(
            KEY_PACKAGE_REFERENCES,
            crate::identity::pq_key_package_references_key(&pq_pub_key)?.as_slice(),
        )?;
        key_store.delete(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, &hash_ref)?;
    }

    Ok(())
}

/// Delete expired local key-package material (delete_at_ns <= now). Late execution
/// is harmless — deletion is local-only; the network copy expires independently.
pub(crate) fn sweep_expired<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<(), KeyPackageMaintenanceError> {
    let conn = context.db();

    // Propagate (don't swallow) so the supervisor's reconnect path can fire.
    let expired_kps = conn
        .get_expired_key_packages()
        .map_err(KeyPackageMaintenanceError::Fetch)?;
    if expired_kps.is_empty() {
        return Ok(());
    }

    tracing::info!("Deleting {} expired key packages", expired_kps.len());
    for kp in &expired_kps {
        delete_key_package(
            context,
            kp.key_package_hash_ref.clone(),
            kp.post_quantum_public_key.clone(),
        )
        .map_err(KeyPackageMaintenanceError::DeleteKeyPackage)?;
    }

    if let Some(max_id) = expired_kps.iter().map(|kp| kp.id).max() {
        conn.delete_key_package_history_up_to_id(max_id)
            .map_err(KeyPackageMaintenanceError::Deletion)?;
        tracing::info!(
            "Deleted {} expired key packages (up to ID {}) from local DB and state",
            expired_kps.len(),
            max_id
        );
    }

    Ok(())
}

/// Post-welcome rotation queue: atomically lower/init the rotation column (5s
/// debounce — a security property) AND enqueue its pull-in in one transaction,
/// then wake the worker. Neither write can land without the other. The rotation
/// seed rides along in the same transaction (insert-or-ignore) so the pull-in
/// always has a live target, even if startup seeding never ran.
pub(crate) fn queue_key_rotation<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<(), StorageError> {
    let now = xmtp_common::time::now_ns();
    context
        .db()
        .queue_key_rotation_with_nudge(&kp_rotation_hash(), kp_seed(kp_rotation_proto(), now)?)?;
    // In-memory only; must stay outside the transaction.
    context.task_channels().wake();
    Ok(())
}

/// After anything marks superseded KPs for deletion: ensure the KpDeletion
/// singleton exists (pull-in against a missing target is a no-op), then pull
/// it in to the earliest pending delete_at. No-op when nothing is marked, so
/// it is safe (and idempotent) to call on every dispatch.
pub(crate) fn nudge_deletion<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<(), StorageError> {
    let db = context.db();
    let now = xmtp_common::time::now_ns();
    if let Some(at) = db.min_key_package_delete_at_ns()? {
        db.create_or_ignore_task(kp_seed(kp_deletion_proto(), now)?)?;
        enqueue_pull_in(context, kp_deletion_hash(), at, NEVER_EXPIRES)?;
    }
    Ok(())
}

/// Idempotent startup seeding + reconcile: pull-ins only LOWER task deadlines to
/// the live DB columns, repairing rows stranded by a crash mid-nudge.
pub(crate) fn seed_and_reconcile_kp_tasks<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<(), StorageError> {
    let db = context.db();
    let now = xmtp_common::time::now_ns();
    db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)?;
    db.create_or_ignore_task(kp_seed(kp_deletion_proto(), now)?)?;
    // Insert-or-ignore keeps an existing row's deadline. A fresh seed is due at
    // `now`, so the first build after upgrade checks at once. That rescues
    // installations that are already unreachable.
    db.create_or_ignore_task(kp_seed(kp_liveness_proto(), now)?)?;
    // None = pre-registration (no identity row): the seed row already fires at
    // startup; a pull-in to `now` would be redundant noise.
    if let Some(rot) = db.next_key_package_rotation_ns()? {
        enqueue_pull_in(context, kp_rotation_hash(), rot, NEVER_EXPIRES)?;
    }
    if let Some(del) = db.min_key_package_delete_at_ns()? {
        enqueue_pull_in(context, kp_deletion_hash(), del, NEVER_EXPIRES)?;
    }
    // Unconditional, unlike the two above. A forward clock jump can strand this
    // row at a future deadline, and the dispatcher skips future rows, so the
    // handler's clamp cannot run. Only a pull-in computed here rescues it.
    // Pull-ins lower deadlines only, so a healthy row is unchanged.
    enqueue_pull_in(
        context,
        kp_liveness_hash(),
        next_liveness_deadline(db.key_package_liveness_checked_at_ns()?, now),
        NEVER_EXPIRES,
    )?;
    Ok(())
}

// Native-only: `PoolNeedsConnection` (and `db_needs_connection`) only exist with
// teeth on native targets; wasm has no connection pool. Mirrors the gate on
// worker.rs's disconnect_propagation_tests.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::tester;
    use crate::worker::tasks::TaskWorker;
    use crate::worker::{WorkerConfig, WorkerKind};
    use prost::Message;
    use xmtp_proto::xmtp::mls::database::Task as TaskProtoDecode;

    /// A `StorageError` that signals the connection pool was dropped.
    fn disconnect_storage() -> xmtp_db::StorageError {
        xmtp_db::StorageError::Platform(xmtp_db::PlatformStorageError::PoolNeedsConnection)
    }

    /// A storage error that is NOT a disconnect — must never trip the contract.
    fn benign_storage() -> xmtp_db::StorageError {
        xmtp_db::StorageError::InvalidHmacLength
    }

    fn no_runner_cfg() -> WorkerConfig {
        let mut cfg = WorkerConfig::default();
        cfg.enabled.insert(WorkerKind::TaskRunner, false);
        cfg
    }

    fn row_by_hash(db: &impl QueryTasks, hash: impl AsRef<[u8]>) -> Option<xmtp_db::tasks::Task> {
        db.get_tasks()
            .expect("get_tasks should not fail")
            .into_iter()
            .find(|t| t.data_hash == hash.as_ref())
    }

    async fn make_rotation_due(db: &impl QueryIdentity) {
        db.queue_key_package_rotation()
            .expect("queue_key_package_rotation should not fail"); // column := now + 5s
        xmtp_common::time::sleep(std::time::Duration::from_secs(6)).await;
    }

    /// Registration stamps the liveness throttle, so a test that needs the check
    /// to run must first move the stamp back past one interval. Writing the
    /// column beats sleeping: the interval is a full day, and it stays a full
    /// day so that no unrelated test starts probing the network on client build.
    fn make_liveness_due(db: &impl QueryIdentity) {
        db.set_key_package_liveness_checked_at_ns(
            xmtp_common::time::now_ns() - 2 * KEY_PACKAGE_LIVENESS_INTERVAL_NS,
        )
        .expect("set_key_package_liveness_checked_at_ns should not fail");
    }

    /// Does a pull-in aimed at `target` exist in the tasks table?
    fn has_pull_in_for(db: &impl QueryTasks, target: TaskDataHash) -> bool {
        db.get_tasks()
            .expect("get_tasks should not fail")
            .iter()
            .any(|t| {
                matches!(
                    TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                    Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == target.as_ref()
                )
            })
    }

    /// Dispatch the `KpLiveness` singleton, seeding it first if absent.
    async fn run_liveness_task<C: XmtpSharedContext + 'static>(context: &C) {
        let db = context.db();
        let now = xmtp_common::time::now_ns();
        let seed = kp_seed(kp_liveness_proto(), now).expect("seed builds");
        db.create_or_ignore_task(seed).expect("seed inserts");
        let row = row_by_hash(&db, kp_liveness_hash()).expect("liveness singleton exists");
        TaskWorker::run_and_reschedule_task(row, context)
            .await
            .expect("liveness dispatch should not fail");
    }

    /// Force the next probe to report `outcome`, for the duration of the guard.
    struct ProbeOverride;
    impl ProbeOverride {
        fn set(outcome: LivenessOutcome) -> Self {
            *test_hooks::PROBE_OVERRIDE
                .lock()
                .expect("probe override lock is never poisoned") = Some(outcome);
            Self
        }
    }
    impl Drop for ProbeOverride {
        fn drop(&mut self) {
            if let Ok(mut guard) = test_hooks::PROBE_OVERRIDE.lock() {
                *guard = None;
            }
        }
    }

    #[xmtp_common::test]
    fn kp_errors_forward_db_reconnect() {
        use crate::worker::NeedsDbReconnect;
        use crate::worker::tasks::TaskWorkerError;
        let e = TaskWorkerError::from(KeyPackageMaintenanceError::Storage(disconnect_storage()));
        assert!(
            e.needs_db_reconnect(),
            "DB outage during KP work must trigger supervisor reconnect, not plain backoff"
        );
        let e = TaskWorkerError::from(crate::identity::IdentityError::from(disconnect_storage()));
        assert!(e.needs_db_reconnect());
        // Keystore pool loss (rotate/delete paths) must also restart the worker.
        let e = TaskWorkerError::from(crate::identity::IdentityError::OpenMlsStorageError(
            xmtp_db::sql_key_store::SqlKeyStoreError::Connection(
                xmtp_db::ConnectionError::Platform(
                    xmtp_db::PlatformStorageError::PoolNeedsConnection,
                ),
            ),
        ));
        assert!(e.needs_db_reconnect());
        // A non-disconnect storage failure must NOT stop the worker.
        let e = TaskWorkerError::from(KeyPackageMaintenanceError::Storage(benign_storage()));
        assert!(
            !e.needs_db_reconnect(),
            "benign storage errors must back off, not restart the supervisor"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn manual_rotation_nudges_deletion() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        assert!(row_by_hash(&db, kp_deletion_hash()).is_none());

        alix.rotate_and_upload_key_package().await?;

        assert!(
            row_by_hash(&db, kp_deletion_hash()).is_some(),
            "manual rotation must self-heal the deletion singleton"
        );
        let has_pull_in = db.get_tasks()?.into_iter().any(|t| matches!(
            TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
            Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == kp_deletion_hash().as_ref()
        ));
        assert!(
            has_pull_in,
            "manual rotation must enqueue a deletion pull-in"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn rotation_task_rotates_and_reschedules() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)?;
        make_rotation_due(&db).await;

        let row = row_by_hash(&db, kp_rotation_hash()).unwrap();
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        assert!(
            !db.is_identity_needs_rotation()?,
            "rotation must have happened"
        );
        let after = row_by_hash(&db, kp_rotation_hash()).expect("recurring row survives");
        let col = db.next_key_package_rotation_ns()?.unwrap();
        assert_eq!(
            after.next_attempt_at_ns, col,
            "reschedule must read the live column"
        );
        assert_eq!(after.attempts, 0);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn rotation_ensures_and_pulls_in_deletion_when_singleton_missing() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)?;
        // Deliberately NO KpDeletion seed: the handler must self-heal it.
        make_rotation_due(&db).await;

        let row = row_by_hash(&db, kp_rotation_hash()).unwrap();
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        assert!(
            row_by_hash(&db, kp_deletion_hash()).is_some(),
            "rotation must recreate a missing KpDeletion singleton"
        );
        let has_pull_in = db.get_tasks()?.iter().any(|t| {
            matches!(
                TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == kp_deletion_hash().as_ref()
            )
        });
        assert!(has_pull_in, "rotation must enqueue a deletion pull-in");
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn deletion_task_sweeps_and_reschedules() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        db.create_or_ignore_task(kp_seed(kp_deletion_proto(), now)?)?;

        // A rotation marks the superseded KP delete_at = now + 3s (test cfg).
        make_rotation_due(&db).await;
        rotate_if_needed(&alix.context).await?;
        assert!(db.min_key_package_delete_at_ns()?.is_some());
        xmtp_common::time::sleep(std::time::Duration::from_secs(4)).await; // pass the grace

        let row = row_by_hash(&db, kp_deletion_hash()).unwrap();
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        assert!(
            db.get_expired_key_packages()?.is_empty(),
            "sweep must delete expired KPs"
        );
        let after = row_by_hash(&db, kp_deletion_hash()).expect("recurring row survives");
        assert!(
            after.next_attempt_at_ns > xmtp_common::time::now_ns(),
            "deletion reschedules to next pending deadline or far-future"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn kp_tasks_seeded_when_workers_run_absent_when_passive() {
        tester!(alix); // default: TaskRunner on -> seeds present
        let db = alix.context.db();
        assert!(row_by_hash(&db, kp_rotation_hash()).is_some());
        assert!(row_by_hash(&db, kp_deletion_hash()).is_some());
        assert!(row_by_hash(&db, kp_liveness_hash()).is_some());

        tester!(bo, worker_config: no_runner_cfg()); // no TaskRunner -> no seeds
        let db = bo.context.db();
        assert!(row_by_hash(&db, kp_rotation_hash()).is_none());
        assert!(row_by_hash(&db, kp_deletion_hash()).is_none());
        assert!(row_by_hash(&db, kp_liveness_hash()).is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn startup_reconcile_pulls_in_far_scheduled_row() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        // Stale persisted row 30d out while the column says due-in-5s
        // (crash-between-writes scenario).
        db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)?;
        let row = row_by_hash(&db, kp_rotation_hash()).unwrap();
        db.update_task(row.id, 0, now, now + 30 * xmtp_common::NS_IN_DAY)?;
        db.queue_key_package_rotation()?; // column := now + 5s

        seed_and_reconcile_kp_tasks(&alix.context)?;

        let pull_in = db
            .get_tasks()?
            .into_iter()
            .find(|t| {
                matches!(
                    TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                    Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == kp_rotation_hash().as_ref()
                )
            })
            .expect("reconcile must enqueue a rotation pull-in");
        TaskWorker::run_and_reschedule_task(pull_in, &alix.context).await?;

        let after = row_by_hash(&db, kp_rotation_hash()).unwrap();
        let col = db.next_key_package_rotation_ns()?.unwrap();
        assert_eq!(after.next_attempt_at_ns, col);
    }

    /// KpRotation firing while NOT due must not rotate or seed deletion — it just
    /// re-syncs its deadline to the column (spurious-wake safety).
    #[xmtp_common::test(unwrap_try = true)]
    async fn rotation_task_not_due_reschedules_without_rotating() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)?;
        // Post-registration column is ~now+30d: not due.
        let row = row_by_hash(&db, kp_rotation_hash()).unwrap();
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        assert!(
            row_by_hash(&db, kp_deletion_hash()).is_none(),
            "must not seed deletion"
        );
        assert!(db.min_key_package_delete_at_ns()?.is_none());
        let after = row_by_hash(&db, kp_rotation_hash()).unwrap();
        assert_eq!(
            after.next_attempt_at_ns,
            db.next_key_package_rotation_ns()?.unwrap()
        );
        // "Not rotating" must hand off to the watchdog rather than return
        // silently — this branch is the one that hid the original incident.
        assert!(
            row_by_hash(&db, kp_liveness_hash()).is_some(),
            "not-due rotation must self-heal the liveness singleton"
        );
        assert!(
            has_pull_in_for(&db, kp_liveness_hash()),
            "not-due rotation must nudge the liveness check"
        );
    }

    /// The happy path. A new client has a live key package, so the check stamps
    /// the throttle and does not touch rotation.
    #[xmtp_common::test(unwrap_try = true)]
    async fn liveness_healthy_kp_records_and_does_not_queue_rotation() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let before = db.next_key_package_rotation_ns()?;
        make_liveness_due(&db);

        run_liveness_task(&alix.context).await;

        assert!(
            db.key_package_liveness_checked_at_ns()?.is_some(),
            "a conclusive check must stamp the throttle"
        );
        assert!(
            !db.is_identity_needs_rotation()?,
            "a live key package must not queue a rotation"
        );
        assert_eq!(
            db.next_key_package_rotation_ns()?,
            before,
            "the rotation deadline must be untouched"
        );
        assert!(
            !has_pull_in_for(&db, kp_rotation_hash()),
            "a live key package must not nudge rotation"
        );
        let after = row_by_hash(&db, kp_liveness_hash()).expect("recurring row survives");
        assert!(
            after.next_attempt_at_ns > xmtp_common::time::now_ns(),
            "the liveness row must park one interval out"
        );
    }

    /// The repair path. An absent key package must lower the rotation deadline
    /// AND nudge the rotation task. Without the nudge the task stays parked ~30
    /// days out and the installation stays unreachable.
    #[xmtp_common::test(unwrap_try = true)]
    async fn liveness_absent_kp_queues_and_nudges_rotation() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        make_liveness_due(&db);
        let _probe = ProbeOverride::set(LivenessOutcome::Absent);

        run_liveness_task(&alix.context).await;

        let deadline = db
            .next_key_package_rotation_ns()?
            .expect("rotation deadline is set post-registration");
        assert!(
            deadline
                <= xmtp_common::time::now_ns() + xmtp_configuration::KEY_PACKAGE_QUEUE_INTERVAL_NS,
            "an absent key package must lower the rotation deadline into the debounce window"
        );
        assert!(
            has_pull_in_for(&db, kp_rotation_hash()),
            "an absent key package must nudge the parked rotation task"
        );
        assert!(
            db.key_package_liveness_checked_at_ns()?.is_some(),
            "an unhealthy-but-conclusive check still stamps the throttle"
        );
    }

    /// The throttle must stop a repeat probe, however often the check is
    /// nudged. Without it, a nudge could force unlimited network probes.
    #[xmtp_common::test(unwrap_try = true)]
    async fn liveness_throttle_skips_repeat_check() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        // Registration already stamped the throttle; do NOT wait it out.
        let checked_at = db
            .key_package_liveness_checked_at_ns()?
            .expect("registration stamps the throttle");
        // If the throttle failed to hold, this override would queue a rotation.
        let _probe = ProbeOverride::set(LivenessOutcome::Absent);

        run_liveness_task(&alix.context).await;

        assert!(
            !db.is_identity_needs_rotation()?,
            "a throttled check must not probe, and so must not queue a rotation"
        );
        assert!(!has_pull_in_for(&db, kp_rotation_hash()));
        assert_eq!(
            db.key_package_liveness_checked_at_ns()?,
            Some(checked_at),
            "a throttled check must not re-stamp the throttle"
        );
        let after = row_by_hash(&db, kp_liveness_hash()).unwrap();
        assert_eq!(
            after.next_attempt_at_ns,
            checked_at + KEY_PACKAGE_LIVENESS_INTERVAL_NS,
            "a throttled check reschedules to when the window actually opens"
        );
    }

    /// Nudges must coalesce, so the payload carries no deadline.
    #[xmtp_common::test(unwrap_try = true)]
    async fn liveness_nudge_is_idempotent() {
        tester!(alix, worker_config: no_runner_cfg()); // no TaskRunner -> no seeds
        let db = alix.context.db();
        assert!(row_by_hash(&db, kp_liveness_hash()).is_none());

        for _ in 0..3 {
            nudge_liveness(&alix.context)?;
        }

        assert!(
            row_by_hash(&db, kp_liveness_hash()).is_some(),
            "nudge must self-heal the missing liveness singleton"
        );
        let pull_ins = db
            .get_tasks()?
            .into_iter()
            .filter(|t| {
                matches!(
                    TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                    Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == kp_liveness_hash().as_ref()
                )
            })
            .count();
        assert_eq!(pull_ins, 1, "repeated nudges must collapse to one pull-in");
    }

    /// A future stamp must not suppress the watchdog. A wrong local timestamp
    /// that disables key package maintenance is the failure this check exists
    /// to catch, so the fix must not repeat it.
    #[xmtp_common::test(unwrap_try = true)]
    async fn liveness_future_stamp_does_not_disable_check() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        // Simulate a stamp written while the clock was a year fast.
        db.set_key_package_liveness_checked_at_ns(
            xmtp_common::time::now_ns() + 365 * xmtp_common::NS_IN_DAY,
        )?;
        let _probe = ProbeOverride::set(LivenessOutcome::Absent);

        run_liveness_task(&alix.context).await;

        let deadline = db
            .next_key_package_rotation_ns()?
            .expect("rotation deadline is set post-registration");
        assert!(
            deadline
                <= xmtp_common::time::now_ns() + xmtp_configuration::KEY_PACKAGE_QUEUE_INTERVAL_NS,
            "a future stamp must not suppress the probe — the watchdog must still repair"
        );
        assert!(has_pull_in_for(&db, kp_rotation_hash()));
        let stamp = db.key_package_liveness_checked_at_ns()?.unwrap();
        assert!(
            stamp <= xmtp_common::time::now_ns(),
            "the conclusive check must overwrite the bogus future stamp"
        );
    }

    /// The dispatcher skips a task with a future deadline. A forward clock jump
    /// writes such a deadline, so the handler's clamp never runs and the
    /// watchdog stops. Only startup reconciliation rescues it. This test uses
    /// the real due-check, not a direct handler call.
    #[xmtp_common::test(unwrap_try = true)]
    async fn startup_reconcile_rescues_liveness_row_stranded_by_clock_skew() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        let year = 365 * xmtp_common::NS_IN_DAY;
        // A check that ran under a year-fast clock: both the stamp and the task
        // row's deadline land a year out.
        db.create_or_ignore_task(kp_seed(kp_liveness_proto(), now)?)?;
        let row = row_by_hash(&db, kp_liveness_hash()).unwrap();
        db.update_task(row.id, 0, now, now + year)?;
        db.set_key_package_liveness_checked_at_ns(now + year)?;

        // Precondition: the dispatcher will not run it — the clamp is unreachable.
        let stranded = row_by_hash(&db, kp_liveness_hash()).unwrap();
        TaskWorker::run_and_reschedule_task(stranded, &alix.context).await?;
        assert_eq!(
            row_by_hash(&db, kp_liveness_hash())
                .unwrap()
                .next_attempt_at_ns,
            now + year,
            "precondition: a future-dated row is skipped, not run"
        );

        seed_and_reconcile_kp_tasks(&alix.context)?;

        let pull_in = db
            .get_tasks()?
            .into_iter()
            .find(|t| {
                matches!(
                    TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                    Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == kp_liveness_hash().as_ref()
                )
            })
            .expect("reconcile must enqueue a liveness pull-in even with a stamp present");
        TaskWorker::run_and_reschedule_task(pull_in, &alix.context).await?;

        assert!(
            row_by_hash(&db, kp_liveness_hash())
                .unwrap()
                .next_attempt_at_ns
                <= xmtp_common::time::now_ns(),
            "reconcile must pull a clock-skew-stranded liveness row back to due"
        );
    }

    /// `next_liveness_deadline` decides if a stamp is trusted. Pin every
    /// boundary.
    #[xmtp_common::test]
    fn liveness_deadline_never_trusts_an_out_of_range_stamp() {
        let now = 1_000 * xmtp_common::NS_IN_DAY;
        let interval = KEY_PACKAGE_LIVENESS_INTERVAL_NS;
        assert_eq!(
            next_liveness_deadline(None, now),
            now,
            "never checked = due"
        );
        assert_eq!(
            next_liveness_deadline(Some(now), now),
            now + interval,
            "just checked throttles a full interval"
        );
        assert_eq!(
            next_liveness_deadline(Some(now - interval), now),
            now,
            "exactly one interval old is due"
        );
        assert_eq!(
            next_liveness_deadline(Some(now + interval), now),
            now,
            "a future stamp is due, not trusted"
        );
        assert_eq!(
            next_liveness_deadline(Some(i64::MAX), now),
            now,
            "a saturating-far-future stamp is due, not trusted"
        );
        assert_eq!(
            next_liveness_deadline(Some(0), now),
            now,
            "an ancient stamp is due"
        );
    }

    /// Runs against the real backend, not the probe override, because the
    /// override hides changes in this logic. This test found that the network
    /// returns an empty payload instead of a count mismatch.
    #[xmtp_common::test(unwrap_try = true)]
    async fn probe_reports_absent_for_unregistered_installation() {
        tester!(alix, worker_config: no_runner_cfg());

        let outcome =
            probe_installation_key_package(&alix.context, xmtp_common::rand_vec::<32>()).await;

        assert_eq!(outcome, LivenessOutcome::Absent);
    }

    /// Every wire shape, classified directly. No network and no override, so
    /// these stay honest if the probe body changes.
    #[xmtp_common::test(unwrap_try = true)]
    async fn classify_covers_every_response_shape() {
        tester!(alix, worker_config: no_runner_cfg());
        let id = alix.context.installation_id().to_vec();
        let now = xmtp_common::time::now_ns();

        // Our own real, freshly published key package.
        let mut fetched = alix
            .context
            .api()
            .fetch_key_packages(vec![id.clone()])
            .await?;
        let bytes = fetched.remove(id.as_slice()).expect("own KP is published");

        assert_eq!(
            classify_key_package(&id, None, now),
            LivenessOutcome::Absent,
            "no entry for us means nothing is published"
        );
        assert_eq!(
            classify_key_package(&id, Some(&[]), now),
            LivenessOutcome::Absent,
            "an EMPTY payload is the shape the network actually returns for a dead installation"
        );
        assert_eq!(
            classify_key_package(&id, Some(b"not a key package"), now),
            LivenessOutcome::Unverifiable,
            "corrupt bytes are unusable, but are NOT 'absent'"
        );
        // Same valid bytes, different requested id: the positional API means a
        // server/cache defect can hand back someone else's perfectly valid KP.
        assert_eq!(
            classify_key_package(&xmtp_common::rand_vec::<32>(), Some(&bytes), now),
            LivenessOutcome::Inconclusive,
            "a valid key package for a DIFFERENT installation must never read as healthy — \
             but it is a backend mapping defect, so it must not trigger rotation either"
        );
        assert!(
            matches!(
                classify_key_package(&id, Some(&bytes), now),
                LivenessOutcome::Healthy { .. }
            ),
            "our own live key package is healthy"
        );
        // Same key package, evaluated from inside the expiry margin.
        let LivenessOutcome::Healthy { remaining_ns } =
            classify_key_package(&id, Some(&bytes), now)
        else {
            panic!("precondition: own KP is healthy");
        };
        let near_expiry = now + remaining_ns - KEY_PACKAGE_LIVENESS_MIN_REMAINING_LIFETIME_NS + 1;
        assert!(
            matches!(
                classify_key_package(&id, Some(&bytes), near_expiry),
                LivenessOutcome::ExpiringSoon { .. }
            ),
            "inside the margin, a still-valid key package must be repaired early"
        );
    }

    /// Only "asked for 1, got 0" proves absence. Any other count is a backend
    /// defect. Calling it absence would rotate every client, and repair nothing.
    #[xmtp_common::test]
    fn classify_fetch_error_only_treats_empty_response_as_absent() {
        use xmtp_api::ApiError;
        assert_eq!(
            classify_fetch_error(&ApiError::MismatchedKeyPackages {
                key_packages: 0,
                installation_keys: 1
            }),
            LivenessOutcome::Absent
        );
        assert_eq!(
            classify_fetch_error(&ApiError::MismatchedKeyPackages {
                key_packages: 2,
                installation_keys: 1
            }),
            LivenessOutcome::Inconclusive,
            "an overfull response is a server defect, not evidence about us"
        );
    }

    /// The welcome nudge must self-heal a missing rotation seed (e.g. startup
    /// seeding never ran) instead of enqueuing a dropped-on-miss pull-in.
    #[xmtp_common::test(unwrap_try = true)]
    async fn welcome_nudge_selfheals_missing_rotation_seed() {
        tester!(alix, worker_config: no_runner_cfg()); // no TaskRunner -> no seeds
        let db = alix.context.db();
        assert!(row_by_hash(&db, kp_rotation_hash()).is_none());

        queue_key_rotation(&alix.context)?;

        assert!(
            row_by_hash(&db, kp_rotation_hash()).is_some(),
            "nudge must recreate the missing KpRotation singleton"
        );
        let has_pull_in = db.get_tasks()?.iter().any(|t| {
            matches!(
                TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == kp_rotation_hash().as_ref()
            )
        });
        assert!(has_pull_in, "nudge must enqueue a rotation pull-in");
    }

    /// Regression: welcome nudge must pull the parked rotation task in even when
    /// the seed dispatched BEFORE the column was lowered (the startup race).
    #[xmtp_common::test(unwrap_try = true)]
    async fn welcome_nudge_pulls_in_parked_rotation() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)?;
        // Simulate the seed having already dispatched not-due: park it on the column (~+30d).
        let parked = row_by_hash(&db, kp_rotation_hash()).unwrap();
        TaskWorker::run_and_reschedule_task(parked, &alix.context).await?;
        let parked_at = row_by_hash(&db, kp_rotation_hash())
            .unwrap()
            .next_attempt_at_ns;
        assert!(
            parked_at > now + xmtp_common::NS_IN_DAY,
            "precondition: parked far out"
        );

        queue_key_rotation(&alix.context)?; // welcome: column + pull-in, atomically

        let pull_in = db
            .get_tasks()?
            .into_iter()
            .find(|t| {
                matches!(
                    TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                    Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == kp_rotation_hash().as_ref()
                )
            })
            .expect("nudge must enqueue a durable pull-in");
        TaskWorker::run_and_reschedule_task(pull_in, &alix.context).await?;

        let after = row_by_hash(&db, kp_rotation_hash()).unwrap();
        let col = db.next_key_package_rotation_ns()?.unwrap();
        assert_eq!(
            after.next_attempt_at_ns, col,
            "rotation row must be pulled in to the lowered column"
        );
        // 5s queue debounce + 2s slack for local ops between `now` and the queue call.
        assert!(after.next_attempt_at_ns <= now + 7 * xmtp_common::NS_IN_SEC);
    }
}
