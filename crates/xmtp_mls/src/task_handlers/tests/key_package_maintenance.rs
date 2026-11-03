//! The key-package tasks through the client's task handlers: seeding at
//! build, rotation and deletion turns, the welcome nudge and the worker's
//! reconnect classification of key-package errors.

use crate::prelude::*;
use crate::task_handlers::{MlsTaskHandlers, TaskWorkerError};
use crate::tester;
use crate::worker::key_package_maintenance::{
    KeyPackageMaintenanceError, rotate_if_needed, seed_and_reconcile_kp_tasks,
};
use crate::worker::tasks::{TaskWorker, test_support::run_and_reschedule_task};
use crate::worker::{WorkerConfig, WorkerKind};
use prost::Message;
use xmtp_db::StorageError;
use xmtp_db::prelude::*;
use xmtp_db::tasks::{NEVER_EXPIRES, NewTask, TaskDataHash, data_hash_for};
use xmtp_proto::xmtp::mls::database::{
    KpDeletion, KpRotation, Task as TaskProto, Task as TaskProtoDecode, task::Task as TaskKind,
};

fn rotation_proto() -> TaskProto {
    TaskProto {
        task: Some(TaskKind::KpRotation(KpRotation {})),
    }
}

fn deletion_proto() -> TaskProto {
    TaskProto {
        task: Some(TaskKind::KpDeletion(KpDeletion {})),
    }
}

fn rotation_hash() -> TaskDataHash {
    data_hash_for(&rotation_proto())
}

fn deletion_hash() -> TaskDataHash {
    data_hash_for(&deletion_proto())
}

/// The recurring seed the client stores for a key-package task: it never
/// expires and is never out of attempts.
fn seed(proto: TaskProto, now: i64) -> Result<NewTask, StorageError> {
    NewTask::builder()
        .originating_message_sequence_id(0)
        .expires_at_ns(NEVER_EXPIRES)
        .max_attempts(i32::MAX)
        .next_attempt_at_ns(now)
        .build(proto)
}

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

#[xmtp_common::test]
fn kp_errors_forward_db_reconnect() {
    use crate::worker::NeedsDbReconnect;
    let e = TaskWorkerError::from(KeyPackageMaintenanceError::Storage(disconnect_storage()));
    assert!(
        e.needs_db_reconnect(),
        "DB outage during KP work must trigger supervisor reconnect, not plain backoff"
    );
    let e = TaskWorkerError::from(crate::identity::IdentityError::from(disconnect_storage()));
    assert!(e.needs_db_reconnect());
    // Keystore pool loss (rotate/delete paths) must also restart the worker.
    let e = TaskWorkerError::from(crate::identity::IdentityError::OpenMlsStorageError(
        xmtp_db::sql_key_store::SqlKeyStoreError::Connection(xmtp_db::ConnectionError::Platform(
            xmtp_db::PlatformStorageError::PoolNeedsConnection,
        )),
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
    assert!(row_by_hash(&db, deletion_hash()).is_none());

    alix.rotate_and_upload_key_package().await?;

    assert!(
        row_by_hash(&db, deletion_hash()).is_some(),
        "manual rotation must self-heal the deletion singleton"
    );
    let has_pull_in = db.get_tasks()?.into_iter().any(|t| {
        matches!(
            TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
            Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == deletion_hash().as_ref()
        )
    });
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
    db.create_or_ignore_task(seed(rotation_proto(), now)?)?;
    make_rotation_due(&db).await;

    let row = row_by_hash(&db, rotation_hash()).unwrap();
    run_and_reschedule_task(&TaskWorker::new(alix.context.clone(), MlsTaskHandlers), row).await?;

    assert!(
        !db.is_identity_needs_rotation()?,
        "rotation must have happened"
    );
    let after = row_by_hash(&db, rotation_hash()).expect("recurring row survives");
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
    db.create_or_ignore_task(seed(rotation_proto(), now)?)?;
    // Deliberately NO KpDeletion seed: the handler must self-heal it.
    make_rotation_due(&db).await;

    let row = row_by_hash(&db, rotation_hash()).unwrap();
    run_and_reschedule_task(&TaskWorker::new(alix.context.clone(), MlsTaskHandlers), row).await?;

    assert!(
        row_by_hash(&db, deletion_hash()).is_some(),
        "rotation must recreate a missing KpDeletion singleton"
    );
    let has_pull_in = db.get_tasks()?.iter().any(|t| {
        matches!(
            TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
            Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == deletion_hash().as_ref()
        )
    });
    assert!(has_pull_in, "rotation must enqueue a deletion pull-in");
}

#[xmtp_common::test(unwrap_try = true)]
async fn deletion_task_sweeps_and_reschedules() {
    tester!(alix, worker_config: no_runner_cfg());
    let db = alix.context.db();
    let now = xmtp_common::time::now_ns();
    db.create_or_ignore_task(seed(deletion_proto(), now)?)?;

    // A rotation marks the superseded KP delete_at = now + 3s (test cfg).
    make_rotation_due(&db).await;
    rotate_if_needed(&alix.context).await?;
    assert!(db.min_key_package_delete_at_ns()?.is_some());
    xmtp_common::time::sleep(std::time::Duration::from_secs(4)).await; // pass the grace

    let row = row_by_hash(&db, deletion_hash()).unwrap();
    run_and_reschedule_task(&TaskWorker::new(alix.context.clone(), MlsTaskHandlers), row).await?;

    assert!(
        db.get_expired_key_packages()?.is_empty(),
        "sweep must delete expired KPs"
    );
    let after = row_by_hash(&db, deletion_hash()).expect("recurring row survives");
    assert!(
        after.next_attempt_at_ns > xmtp_common::time::now_ns(),
        "deletion reschedules to next pending deadline or far-future"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn kp_tasks_seeded_when_workers_run_absent_when_passive() {
    tester!(alix); // default: TaskRunner on -> seeds present
    let db = alix.context.db();
    assert!(row_by_hash(&db, rotation_hash()).is_some());
    assert!(row_by_hash(&db, deletion_hash()).is_some());

    tester!(bo, worker_config: no_runner_cfg()); // no TaskRunner -> no seeds
    let db = bo.context.db();
    assert!(row_by_hash(&db, rotation_hash()).is_none());
    assert!(row_by_hash(&db, deletion_hash()).is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn startup_reconcile_pulls_in_far_scheduled_row() {
    tester!(alix, worker_config: no_runner_cfg());
    let db = alix.context.db();
    let now = xmtp_common::time::now_ns();
    // Stale persisted row 30d out while the column says due-in-5s
    // (crash-between-writes scenario).
    db.create_or_ignore_task(seed(rotation_proto(), now)?)?;
    let row = row_by_hash(&db, rotation_hash()).unwrap();
    db.update_task(row.id, 0, now, now + 30 * xmtp_common::NS_IN_DAY)?;
    db.queue_key_package_rotation()?; // column := now + 5s

    seed_and_reconcile_kp_tasks(&alix.context)?;

    let pull_in = db
        .get_tasks()?
        .into_iter()
        .find(|t| {
            matches!(
                TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == rotation_hash().as_ref()
            )
        })
        .expect("reconcile must enqueue a rotation pull-in");
    run_and_reschedule_task(
        &TaskWorker::new(alix.context.clone(), MlsTaskHandlers),
        pull_in,
    )
    .await?;

    let after = row_by_hash(&db, rotation_hash()).unwrap();
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
    db.create_or_ignore_task(seed(rotation_proto(), now)?)?;
    // Post-registration column is ~now+30d: not due.
    let row = row_by_hash(&db, rotation_hash()).unwrap();
    run_and_reschedule_task(&TaskWorker::new(alix.context.clone(), MlsTaskHandlers), row).await?;

    assert!(
        row_by_hash(&db, deletion_hash()).is_none(),
        "must not seed deletion"
    );
    assert!(db.min_key_package_delete_at_ns()?.is_none());
    let after = row_by_hash(&db, rotation_hash()).unwrap();
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
    db.create_or_ignore_task(seed(rotation_proto(), now)?)?;
    // Simulate the seed having already dispatched not-due: park it on the column (~+30d).
    let parked = row_by_hash(&db, rotation_hash()).unwrap();
    run_and_reschedule_task(
        &TaskWorker::new(alix.context.clone(), MlsTaskHandlers),
        parked,
    )
    .await?;
    let parked_at = row_by_hash(&db, rotation_hash())
        .unwrap()
        .next_attempt_at_ns;
    assert!(
        parked_at > now + xmtp_common::NS_IN_DAY,
        "precondition: parked far out"
    );

    alix.queue_key_rotation()?; // welcome: column + pull-in, atomically

    let pull_in = db
        .get_tasks()?
        .into_iter()
        .find(|t| {
            matches!(
                TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == rotation_hash().as_ref()
            )
        })
        .expect("nudge must enqueue a durable pull-in");
    run_and_reschedule_task(
        &TaskWorker::new(alix.context.clone(), MlsTaskHandlers),
        pull_in,
    )
    .await?;

    let after = row_by_hash(&db, rotation_hash()).unwrap();
    let col = db.next_key_package_rotation_ns()?.unwrap();
    assert_eq!(
        after.next_attempt_at_ns, col,
        "rotation row must be pulled in to the lowered column"
    );
    // 5s queue debounce + 2s slack for local ops between `now` and the queue call.
    assert!(after.next_attempt_at_ns <= now + 7 * xmtp_common::NS_IN_SEC);
}
