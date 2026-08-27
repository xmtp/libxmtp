//! Key-package maintenance as TaskRunner consumers: payload/seed helpers and the
//! rotate/sweep work the `KpRotation`/`KpDeletion` dispatch arms call into.
//! Recurrence + nudging come from the generic layer (TaskOutcome, PullInDeadline).

use crate::context::XmtpSharedContext;
use crate::identity::IdentityError;
use crate::worker::NeedsDbReconnect;
use crate::worker::tasks::enqueue_pull_in;
use openmls_traits::storage::StorageProvider;
use thiserror::Error;
use xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION;
use xmtp_db::MlsProviderExt;
use xmtp_db::StorageError;
use xmtp_db::prelude::*;
use xmtp_db::tasks::{NEVER_EXPIRES, NewTask, TaskDataHash, data_hash_for};
use xmtp_proto::xmtp::mls::database::{
    KpDeletion, KpRotation, Task as TaskProto, task::Task as TaskKind,
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

pub(crate) fn kp_rotation_hash() -> TaskDataHash {
    data_hash_for(&kp_rotation_proto())
}

pub(crate) fn kp_deletion_hash() -> TaskDataHash {
    data_hash_for(&kp_deletion_proto())
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
pub(crate) async fn rotate_if_needed<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<bool, KeyPackageMaintenanceError> {
    if !context
        .db()
        .is_identity_needs_rotation()
        .await
        .map_err(KeyPackageMaintenanceError::Metadata)?
    {
        return Ok(false);
    }
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

/// Delete one key package's local material (keystore entry + PQ references).
pub(crate) async fn delete_key_package<Context: XmtpSharedContext>(
    context: &Context,
    hash_ref: Vec<u8>,
    pq_pub_key: Option<Vec<u8>>,
) -> Result<(), IdentityError> {
    let openmls_hash_ref = crate::identity::deserialize_key_package_hash_ref(&hash_ref)?;
    let mls_provider = context.mls_provider();
    let key_store = mls_provider.key_store();

    (key_store.delete_key_package(&openmls_hash_ref)).await?;

    if let Some(pq_pub_key) = pq_pub_key {
        key_store
            .delete_key_package_reference(
                crate::identity::pq_key_package_references_key(&pq_pub_key)?.as_slice(),
            )
            .await?;
        key_store.delete_key_package_wrapper_key(&hash_ref).await?;
    }

    Ok(())
}

/// Delete expired local key-package material (delete_at_ns <= now). Late execution
/// is harmless — deletion is local-only; the network copy expires independently.
pub(crate) async fn sweep_expired<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<(), KeyPackageMaintenanceError> {
    let conn = context.db();

    // Propagate (don't swallow) so the supervisor's reconnect path can fire.
    let expired_kps = conn
        .get_expired_key_packages()
        .await
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
        .await
        .map_err(KeyPackageMaintenanceError::DeleteKeyPackage)?;
    }

    if let Some(max_id) = expired_kps.iter().map(|kp| kp.id).max() {
        conn.delete_key_package_history_up_to_id(max_id)
            .await
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
pub(crate) async fn queue_key_rotation<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<(), StorageError> {
    let now = xmtp_common::time::now_ns();
    context
        .db()
        .queue_key_rotation_with_nudge(&kp_rotation_hash(), kp_seed(kp_rotation_proto(), now)?)
        .await?;
    // In-memory only; must stay outside the transaction.
    context.task_channels().wake();
    Ok(())
}

/// After anything marks superseded KPs for deletion: ensure the KpDeletion
/// singleton exists (pull-in against a missing target is a no-op), then pull
/// it in to the earliest pending delete_at. No-op when nothing is marked, so
/// it is safe (and idempotent) to call on every dispatch.
pub(crate) async fn nudge_deletion<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<(), StorageError> {
    let db = context.db();
    let now = xmtp_common::time::now_ns();
    if let Some(at) = db.min_key_package_delete_at_ns().await? {
        db.create_or_ignore_task(kp_seed(kp_deletion_proto(), now)?)
            .await?;
        enqueue_pull_in(context, kp_deletion_hash(), at, NEVER_EXPIRES).await?;
    }
    Ok(())
}

/// Idempotent startup seeding + reconcile: pull-ins only LOWER task deadlines to
/// the live DB columns, repairing rows stranded by a crash mid-nudge.
pub(crate) async fn seed_and_reconcile_kp_tasks<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<(), StorageError> {
    let db = context.db();
    let now = xmtp_common::time::now_ns();
    db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)
        .await?;
    db.create_or_ignore_task(kp_seed(kp_deletion_proto(), now)?)
        .await?;
    // None = pre-registration (no identity row): the seed row already fires at
    // startup; a pull-in to `now` would be redundant noise.
    if let Some(rot) = db.next_key_package_rotation_ns().await? {
        enqueue_pull_in(context, kp_rotation_hash(), rot, NEVER_EXPIRES).await?;
    }
    if let Some(del) = db.min_key_package_delete_at_ns().await? {
        enqueue_pull_in(context, kp_deletion_hash(), del, NEVER_EXPIRES).await?;
    }
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

    async fn row_by_hash(
        db: &impl QueryTasks,
        hash: impl AsRef<[u8]>,
    ) -> Option<xmtp_db::tasks::Task> {
        db.get_tasks()
            .await
            .expect("get_tasks should not fail")
            .into_iter()
            .find(|t| t.data_hash == hash.as_ref())
    }

    async fn make_rotation_due(db: &impl QueryIdentity) {
        db.queue_key_package_rotation()
            .await
            .expect("queue_key_package_rotation should not fail"); // column := now + 5s
        xmtp_common::time::sleep(std::time::Duration::from_secs(6)).await;
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
        assert!(row_by_hash(&db, kp_deletion_hash()).await.is_none());

        alix.rotate_and_upload_key_package().await?;

        assert!(
            row_by_hash(&db, kp_deletion_hash()).await.is_some(),
            "manual rotation must self-heal the deletion singleton"
        );
        let has_pull_in = db.get_tasks().await?.into_iter().any(|t| matches!(
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
        db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)
            .await?;
        make_rotation_due(&db).await;

        let row = row_by_hash(&db, kp_rotation_hash()).await.unwrap();
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        assert!(
            !db.is_identity_needs_rotation().await?,
            "rotation must have happened"
        );
        let after = row_by_hash(&db, kp_rotation_hash())
            .await
            .expect("recurring row survives");
        let col = db.next_key_package_rotation_ns().await?.unwrap();
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
        db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)
            .await?;
        // Deliberately NO KpDeletion seed: the handler must self-heal it.
        make_rotation_due(&db).await;

        let row = row_by_hash(&db, kp_rotation_hash()).await.unwrap();
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        assert!(
            row_by_hash(&db, kp_deletion_hash()).await.is_some(),
            "rotation must recreate a missing KpDeletion singleton"
        );
        let has_pull_in = db.get_tasks().await?.iter().any(|t| {
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
        db.create_or_ignore_task(kp_seed(kp_deletion_proto(), now)?)
            .await?;

        // A rotation marks the superseded KP delete_at = now + 3s (test cfg).
        make_rotation_due(&db).await;
        rotate_if_needed(&alix.context).await?;
        assert!(db.min_key_package_delete_at_ns().await?.is_some());
        xmtp_common::time::sleep(std::time::Duration::from_secs(4)).await; // pass the grace

        let row = row_by_hash(&db, kp_deletion_hash()).await.unwrap();
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        assert!(
            db.get_expired_key_packages().await?.is_empty(),
            "sweep must delete expired KPs"
        );
        let after = row_by_hash(&db, kp_deletion_hash())
            .await
            .expect("recurring row survives");
        assert!(
            after.next_attempt_at_ns > xmtp_common::time::now_ns(),
            "deletion reschedules to next pending deadline or far-future"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn kp_tasks_seeded_when_workers_run_absent_when_passive() {
        tester!(alix); // default: TaskRunner on -> seeds present
        let db = alix.context.db();
        assert!(row_by_hash(&db, kp_rotation_hash()).await.is_some());
        assert!(row_by_hash(&db, kp_deletion_hash()).await.is_some());

        tester!(bo, worker_config: no_runner_cfg()); // no TaskRunner -> no seeds
        let db = bo.context.db();
        assert!(row_by_hash(&db, kp_rotation_hash()).await.is_none());
        assert!(row_by_hash(&db, kp_deletion_hash()).await.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn startup_reconcile_pulls_in_far_scheduled_row() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        // Stale persisted row 30d out while the column says due-in-5s
        // (crash-between-writes scenario).
        db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)
            .await?;
        let row = row_by_hash(&db, kp_rotation_hash()).await.unwrap();
        db.update_task(row.id, 0, now, now + 30 * xmtp_common::NS_IN_DAY)
            .await?;
        db.queue_key_package_rotation().await?; // column := now + 5s

        seed_and_reconcile_kp_tasks(&alix.context).await?;

        let pull_in = db
            .get_tasks().await?
            .into_iter()
            .find(|t| {
                matches!(
                    TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                    Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == kp_rotation_hash().as_ref()
                )
            })
            .expect("reconcile must enqueue a rotation pull-in");
        TaskWorker::run_and_reschedule_task(pull_in, &alix.context).await?;

        let after = row_by_hash(&db, kp_rotation_hash()).await.unwrap();
        let col = db.next_key_package_rotation_ns().await?.unwrap();
        assert_eq!(after.next_attempt_at_ns, col);
    }

    /// KpRotation firing while NOT due must not rotate or seed deletion — it just
    /// re-syncs its deadline to the column (spurious-wake safety).
    #[xmtp_common::test(unwrap_try = true)]
    async fn rotation_task_not_due_reschedules_without_rotating() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)
            .await?;
        // Post-registration column is ~now+30d: not due.
        let row = row_by_hash(&db, kp_rotation_hash()).await.unwrap();
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        assert!(
            row_by_hash(&db, kp_deletion_hash()).await.is_none(),
            "must not seed deletion"
        );
        assert!(db.min_key_package_delete_at_ns().await?.is_none());
        let after = row_by_hash(&db, kp_rotation_hash()).await.unwrap();
        assert_eq!(
            after.next_attempt_at_ns,
            db.next_key_package_rotation_ns().await?.unwrap()
        );
    }

    /// The welcome nudge must self-heal a missing rotation seed (e.g. startup
    /// seeding never ran) instead of enqueuing a dropped-on-miss pull-in.
    #[xmtp_common::test(unwrap_try = true)]
    async fn welcome_nudge_selfheals_missing_rotation_seed() {
        tester!(alix, worker_config: no_runner_cfg()); // no TaskRunner -> no seeds
        let db = alix.context.db();
        assert!(row_by_hash(&db, kp_rotation_hash()).await.is_none());

        queue_key_rotation(&alix.context).await?;

        assert!(
            row_by_hash(&db, kp_rotation_hash()).await.is_some(),
            "nudge must recreate the missing KpRotation singleton"
        );
        let has_pull_in = db.get_tasks().await?.iter().any(|t| {
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
        db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)
            .await?;
        // Simulate the seed having already dispatched not-due: park it on the column (~+30d).
        let parked = row_by_hash(&db, kp_rotation_hash()).await.unwrap();
        TaskWorker::run_and_reschedule_task(parked, &alix.context).await?;
        let parked_at = row_by_hash(&db, kp_rotation_hash())
            .await
            .unwrap()
            .next_attempt_at_ns;
        assert!(
            parked_at > now + xmtp_common::NS_IN_DAY,
            "precondition: parked far out"
        );

        queue_key_rotation(&alix.context).await?; // welcome: column + pull-in, atomically

        let pull_in = db
            .get_tasks().await?
            .into_iter()
            .find(|t| {
                matches!(
                    TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                    Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == kp_rotation_hash().as_ref()
                )
            })
            .expect("nudge must enqueue a durable pull-in");
        TaskWorker::run_and_reschedule_task(pull_in, &alix.context).await?;

        let after = row_by_hash(&db, kp_rotation_hash()).await.unwrap();
        let col = db.next_key_package_rotation_ns().await?.unwrap();
        assert_eq!(
            after.next_attempt_at_ns, col,
            "rotation row must be pulled in to the lowered column"
        );
        // 5s queue debounce + 2s slack for local ops between `now` and the queue call.
        assert!(after.next_attempt_at_ns <= now + 7 * xmtp_common::NS_IN_SEC);
    }
}
