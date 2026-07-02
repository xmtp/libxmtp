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
    let cleaner = KeyPackagesCleanerWorker::new(context.clone());
    cleaner.delete_expired_key_packages()
}

/// Post-welcome nudge: after queue_key_package_rotation lowers the column,
/// durably pull the KpRotation task in to match. Never expires — losing it
/// re-parks rotation ~30d out (the 5s debounce is a security property).
pub(crate) fn nudge_rotation<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<(), StorageError> {
    let at = context
        .db()
        .next_key_package_rotation_ns()?
        .unwrap_or_else(xmtp_common::time::now_ns);
    enqueue_pull_in(context, kp_rotation_hash(), at, NEVER_EXPIRES)
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
    // None = pre-registration (no identity row): the seed row already fires at
    // startup; a pull-in to `now` would be redundant noise.
    if let Some(rot) = db.next_key_package_rotation_ns()? {
        enqueue_pull_in(context, kp_rotation_hash(), rot, NEVER_EXPIRES)?;
    }
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

    #[xmtp_common::test(unwrap_try = true)]
    async fn kp_tasks_seeded_when_workers_run_absent_when_passive() {
        tester!(alix); // default: TaskRunner on -> seeds present
        let db = alix.context.db();
        assert!(row_by_hash(&db, &kp_rotation_hash()).is_some());
        assert!(row_by_hash(&db, &kp_deletion_hash()).is_some());

        tester!(bo, worker_config: no_runner_cfg()); // no TaskRunner -> no seeds
        let db = bo.context.db();
        assert!(row_by_hash(&db, &kp_rotation_hash()).is_none());
        assert!(row_by_hash(&db, &kp_deletion_hash()).is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn startup_reconcile_pulls_in_far_scheduled_row() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        // Stale persisted row 30d out while the column says due-in-5s
        // (crash-between-writes scenario).
        db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)?;
        let row = row_by_hash(&db, &kp_rotation_hash()).unwrap();
        db.update_task(row.id, 0, now, now + 30 * xmtp_common::NS_IN_DAY)?;
        db.queue_key_package_rotation()?; // column := now + 5s

        seed_and_reconcile_kp_tasks(&alix.context)?;

        let pull_in = db
            .get_tasks()?
            .into_iter()
            .find(|t| {
                matches!(
                    TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                    Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == kp_rotation_hash()
                )
            })
            .expect("reconcile must enqueue a rotation pull-in");
        TaskWorker::run_and_reschedule_task(pull_in, &alix.context).await?;

        let after = row_by_hash(&db, &kp_rotation_hash()).unwrap();
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
        let row = row_by_hash(&db, &kp_rotation_hash()).unwrap();
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        assert!(
            row_by_hash(&db, &kp_deletion_hash()).is_none(),
            "must not seed deletion"
        );
        assert!(db.min_key_package_delete_at_ns()?.is_none());
        let after = row_by_hash(&db, &kp_rotation_hash()).unwrap();
        assert_eq!(
            after.next_attempt_at_ns,
            db.next_key_package_rotation_ns()?.unwrap()
        );
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
        let parked = row_by_hash(&db, &kp_rotation_hash()).unwrap();
        TaskWorker::run_and_reschedule_task(parked, &alix.context).await?;
        let parked_at = row_by_hash(&db, &kp_rotation_hash())
            .unwrap()
            .next_attempt_at_ns;
        assert!(
            parked_at > now + xmtp_common::NS_IN_DAY,
            "precondition: parked far out"
        );

        db.queue_key_package_rotation()?; // welcome lowers column to now+5s
        nudge_rotation(&alix.context)?;

        let pull_in = db
            .get_tasks()?
            .into_iter()
            .find(|t| {
                matches!(
                    TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                    Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == kp_rotation_hash()
                )
            })
            .expect("nudge must enqueue a durable pull-in");
        TaskWorker::run_and_reschedule_task(pull_in, &alix.context).await?;

        let after = row_by_hash(&db, &kp_rotation_hash()).unwrap();
        let col = db.next_key_package_rotation_ns()?.unwrap();
        assert_eq!(
            after.next_attempt_at_ns, col,
            "rotation row must be pulled in to the lowered column"
        );
        // 5s queue debounce + 2s slack for local ops between `now` and the queue call.
        assert!(after.next_attempt_at_ns <= now + 7 * xmtp_common::NS_IN_SEC);
    }
}
