//! Key-package maintenance as TaskRunner consumers: payload/seed helpers and the
//! rotate/sweep work the `KpRotation`/`KpDeletion` dispatch arms call into.
//! Recurrence + nudging come from the generic layer (TaskOutcome, PullInDeadline).

use crate::context::XmtpSharedContext;
use crate::worker::key_package_cleaner::{KeyPackagesCleanerError, KeyPackagesCleanerWorker};
use crate::worker::tasks::enqueue_pull_in;
use xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION;
use xmtp_db::StorageError;
use xmtp_db::prelude::*;
use xmtp_db::tasks::{NEVER_EXPIRES, NewTask, data_hash_for};
use xmtp_proto::xmtp::mls::database::{
    KpDeletion, KpRotation, Task as TaskProto, task::Task as TaskKind,
};

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

pub(crate) fn kp_rotation_hash() -> Vec<u8> {
    data_hash_for(&kp_rotation_proto())
}

pub(crate) fn kp_deletion_hash() -> Vec<u8> {
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
) -> Result<bool, KeyPackagesCleanerError> {
    if !context
        .db()
        .is_identity_needs_rotation()
        .map_err(KeyPackagesCleanerError::Metadata)?
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
        .map_err(KeyPackagesCleanerError::Rotation)?;
    Ok(true)
}

/// Delete expired local key-package material (delete_at_ns <= now). Late execution
/// is harmless — deletion is local-only; the network copy expires independently.
pub(crate) fn sweep_expired<Context: XmtpSharedContext + 'static>(
    context: &Context,
) -> Result<(), KeyPackagesCleanerError> {
    let mut cleaner = KeyPackagesCleanerWorker::new(context.clone());
    cleaner.delete_expired_key_packages()
}

/// Idempotent startup seeding + reconcile: pull-ins only LOWER task deadlines to
/// the live DB columns, repairing rows stranded by a crash mid-nudge.
#[expect(dead_code, reason = "wired by builder seeding (Task 5)")]
pub(crate) fn seed_and_reconcile_kp_tasks<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<(), StorageError> {
    let db = context.db();
    let now = xmtp_common::time::now_ns();
    db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)?;
    db.create_or_ignore_task(kp_seed(kp_deletion_proto(), now)?)?;
    let rot = db.next_key_package_rotation_ns()?.unwrap_or(now); // NULL = due now
    enqueue_pull_in(context, kp_rotation_hash(), rot, NEVER_EXPIRES)?;
    if let Some(del) = db.min_key_package_delete_at_ns()? {
        enqueue_pull_in(context, kp_deletion_hash(), del, NEVER_EXPIRES)?;
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
        // Disable the KP cleaner so it does not race with the manually-driven
        // task handler and rotate the key package before our arm runs.
        cfg.enabled.insert(WorkerKind::KeyPackageCleaner, false);
        cfg
    }

    fn row_by_hash(db: &impl QueryTasks, hash: &[u8]) -> Option<xmtp_db::tasks::Task> {
        db.get_tasks()
            .expect("get_tasks should not fail")
            .into_iter()
            .find(|t| t.data_hash == hash)
    }

    async fn make_rotation_due(db: &impl QueryIdentity) {
        db.queue_key_package_rotation()
            .expect("queue_key_package_rotation should not fail"); // column := now + 5s
        xmtp_common::time::sleep(std::time::Duration::from_secs(6)).await;
    }

    #[xmtp_common::test]
    fn kp_errors_forward_db_reconnect() {
        use crate::worker::NeedsDbReconnect;
        use crate::worker::key_package_cleaner::KeyPackagesCleanerError;
        use crate::worker::tasks::TaskWorkerError;
        let e = TaskWorkerError::from(KeyPackagesCleanerError::Storage(disconnect_storage()));
        assert!(
            e.needs_db_reconnect(),
            "DB outage during KP work must trigger supervisor reconnect, not plain backoff"
        );
        let e = TaskWorkerError::from(crate::identity::IdentityError::from(disconnect_storage()));
        assert!(e.needs_db_reconnect());
        // A non-disconnect storage failure must NOT stop the worker.
        let e = TaskWorkerError::from(KeyPackagesCleanerError::Storage(benign_storage()));
        assert!(
            !e.needs_db_reconnect(),
            "benign storage errors must back off, not restart the supervisor"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn rotation_task_rotates_and_reschedules() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)?;
        make_rotation_due(&db).await;

        let row = row_by_hash(&db, &kp_rotation_hash()).unwrap();
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        assert!(
            !db.is_identity_needs_rotation()?,
            "rotation must have happened"
        );
        let after = row_by_hash(&db, &kp_rotation_hash()).expect("recurring row survives");
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

        let row = row_by_hash(&db, &kp_rotation_hash()).unwrap();
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        assert!(
            row_by_hash(&db, &kp_deletion_hash()).is_some(),
            "rotation must recreate a missing KpDeletion singleton"
        );
        let has_pull_in = db.get_tasks()?.iter().any(|t| {
            matches!(
                TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == kp_deletion_hash()
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

        let row = row_by_hash(&db, &kp_deletion_hash()).unwrap();
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        assert!(
            db.get_expired_key_packages()?.is_empty(),
            "sweep must delete expired KPs"
        );
        let after = row_by_hash(&db, &kp_deletion_hash()).expect("recurring row survives");
        assert!(
            after.next_attempt_at_ns > xmtp_common::time::now_ns(),
            "deletion reschedules to next pending deadline or far-future"
        );
    }
}
