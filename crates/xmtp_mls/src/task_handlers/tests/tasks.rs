//! The client's task handlers through one task-runner turn: rate-limit
//! delays through every wrapper, deferred and missing-target kinds, and the
//! pull-in arm.

use crate::context::CoreContext;
use crate::task_handlers::{MlsTaskHandlers, TaskWorkerError, UNKNOWN_TASK_DEFER_NS, test_hooks};
use crate::tester;
use crate::worker::device_sync::DeviceSyncError;
use crate::worker::tasks::TaskWorker;
use crate::worker::tasks::test_support::{
    enqueue_pull_in, run_and_reschedule_task, seed, unique_proto,
};
use crate::worker::{WorkerConfig, WorkerKind};
use prost::Message;
use std::sync::Arc;
use xmtp_db::tasks::{NewTask, QueryTasks, data_hash_for};
use xmtp_proto::xmtp::mls::database::{Task as TaskProto, task::Task as TaskKind};

/// The rejection a rate-limited publish gets from the backend.
fn rate_limited_rejection(delay: xmtp_common::time::Duration) -> tonic::Status {
    xmtp_api_grpc::error::rate_limited_status("publish", delay)
}

fn rate_limited(delay: xmtp_common::time::Duration) -> xmtp_api::ApiError {
    xmtp_api::dyn_err(xmtp_proto::api::ApiClientError::client(
        xmtp_api_grpc::error::GrpcError::Status(xmtp_api_grpc::error::rate_limited_status(
            "publish", delay,
        )),
    ))
}

/// Every task error that wraps a rate-limited request carries its delay
/// to the runner, which schedules no earlier than it.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
fn rate_limit_delays_reach_the_task_runner_through_each_wrapper() {
    use crate::{
        groups::GroupError, identity::IdentityError, identity_updates::IdentityUpdateError,
        worker::key_package_maintenance::KeyPackageMaintenanceError,
    };
    use xmtp_common::RetryableError;
    use xmtp_id::{associations::SignatureError, scw_verifier::VerifierError};
    let delay = xmtp_common::time::Duration::from_secs(300);
    let scw = || {
        IdentityUpdateError::from(SignatureError::from(VerifierError::Other(Box::new(
            rate_limited(delay),
        ))))
    };
    let mut summary = crate::groups::summary::SyncSummary::default();
    summary.add_publish_err(GroupError::WrappedApi(rate_limited(delay)));
    let errors = [
        TaskWorkerError::KeyPackageMaintenance(KeyPackageMaintenanceError::Rotation(
            IdentityError::ApiClient(rate_limited(delay)),
        )),
        TaskWorkerError::Identity(IdentityError::ApiClient(rate_limited(delay))),
        TaskWorkerError::Group(GroupError::IdentityUpdate(scw())),
        TaskWorkerError::Group(GroupError::SyncFailedToWait(Box::new(summary))),
        TaskWorkerError::DeviceSync(DeviceSyncError::IdentityUpdate(IdentityUpdateError::Api(
            rate_limited(delay),
        ))),
        TaskWorkerError::LoadGroup(crate::mls_store::MlsStoreError::Api(rate_limited(delay))),
    ];
    for error in errors {
        assert_eq!(error.rate_limit_delay(), Some(delay), "{error}");
    }
    let mut summary = crate::groups::summary::SyncSummary::default();
    summary.add_publish_err(GroupError::WrappedApi(rate_limited(delay)));
    let nested = [
        DeviceSyncError::Sync(Box::new(summary)).rate_limit_delay(),
        DeviceSyncError::MlsStore(crate::mls_store::MlsStoreError::Api(rate_limited(delay)))
            .rate_limit_delay(),
        crate::groups::GroupMessageProcessingError::IdentityUpdate(IdentityUpdateError::Api(
            rate_limited(delay),
        ))
        .rate_limit_delay(),
        crate::subscriptions::SubscribeError::LocalDelivery(
            crate::subscriptions::local_delivery::LocalDeliveryError::NetworkFailure(Arc::new(
                crate::subscriptions::incoming::IncomingError::Store(
                    crate::mls_store::MlsStoreError::Api(rate_limited(delay)),
                ),
            )),
        )
        .rate_limit_delay(),
    ];
    assert!(
        nested.iter().all(|found| *found == Some(delay)),
        "{nested:?}"
    );
    // The worker restart loop reads the same delay.
    use crate::worker::NeedsDbReconnect;
    assert_eq!(
        NeedsDbReconnect::restart_delay(&TaskWorkerError::Identity(IdentityError::ApiClient(
            rate_limited(delay)
        ))),
        Some(delay)
    );
    assert_eq!(
        NeedsDbReconnect::restart_delay(&DeviceSyncError::IdentityUpdate(
            IdentityUpdateError::Api(rate_limited(delay))
        )),
        Some(delay)
    );
    assert_eq!(
        xmtp_common::RetryableError::rate_limit_delay(&scw()),
        Some(delay)
    );
}

fn no_runner_cfg() -> WorkerConfig {
    let mut cfg = WorkerConfig::default();
    cfg.enabled.insert(WorkerKind::TaskRunner, false);
    cfg
}

fn seed_raw(db: &impl QueryTasks, data: Vec<u8>, expires: i64) -> xmtp_db::tasks::Task {
    let mut task = NewTask::builder()
        .originating_message_sequence_id(0)
        .next_attempt_at_ns(xmtp_common::time::now_ns() - 1)
        .expires_at_ns(expires)
        .build(unique_proto())
        .unwrap();
    task.data_hash = xmtp_common::sha256_bytes(&data);
    task.data = data;
    let hash = task.data_hash.clone();
    db.create_or_ignore_task(task).unwrap();
    db.get_tasks()
        .unwrap()
        .into_iter()
        .find(|t| t.data_hash == hash)
        .unwrap()
}

#[xmtp_common::test(unwrap_try = true)]
async fn undecodable_and_unhandled_kinds_are_deferred() {
    tester!(alix, worker_config: no_runner_cfg());
    let db = alix.context.db();
    let now = xmtp_common::time::now_ns();
    let liveness = TaskProto {
        task: Some(TaskKind::KpLiveness(Default::default())),
    };
    for (data, expires) in [
        (vec![0x52, 0x00], i64::MAX), // Unknown kind: field 10, empty message.
        (liveness.encode_to_vec(), i64::MAX),
        (vec![0x52, 0x00], now - 1),
    ] {
        let row = seed_raw(&db, data, expires);
        let id = row.id;
        let before = xmtp_common::time::now_ns();
        run_and_reschedule_task(&TaskWorker::new(alix.context.clone(), MlsTaskHandlers), row)
            .await?;
        let after = db.get_tasks()?.into_iter().find(|t| t.id == id);
        if expires < now {
            assert!(after.is_none(), "expired unknown kind must be reaped");
        } else {
            let after = after.expect("unhandled kind must survive");
            assert_eq!(after.attempts, 0, "deferral must not count as a failure");
            assert!(
                after.next_attempt_at_ns >= before + UNKNOWN_TASK_DEFER_NS,
                "deferral must push the deadline out"
            );
            db.delete_task(id)?;
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn add_missing_installations_missing_group_deletes_task() {
    tester!(alix, worker_config: no_runner_cfg());
    let db = alix.context.db();
    let now = xmtp_common::time::now_ns();
    let proto = TaskProto {
        task: Some(TaskKind::AddMissingInstallations(
            xmtp_proto::xmtp::mls::database::AddMissingInstallations {
                group_id: xmtp_common::rand_vec::<16>(),
            },
        )),
    };
    let row = seed(&db, proto, now - 1, i64::MAX, 0, 3);
    run_and_reschedule_task(&TaskWorker::new(alix.context.clone(), MlsTaskHandlers), row).await?;
    assert!(
        db.get_tasks()?.is_empty(),
        "task for a nonexistent group must be deleted, not retried"
    );
}

/// The delay counts from when the rejection arrived, not from when the
/// turn started, and a pull-in cannot run the task inside it.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_task_waits_the_delay_from_the_rejection() {
    use xmtp_common::time::Duration;
    tester!(alix, worker_config: no_runner_cfg());
    let db = alix.context.db();
    let now = xmtp_common::time::now_ns();
    let proto = unique_proto();
    let hash = data_hash_for(&proto);
    let row = seed(&db, proto, now - 1, i64::MAX, 0, i32::MAX);
    // Longer than the default 3 s first backoff, so only the rejection
    // instant, not the turn's start, gives the bound below.
    let delay = Duration::from_secs(5);
    test_hooks::set_rate_limit_override(Some((
        hash.to_vec(),
        Duration::from_millis(300),
        rate_limited_rejection(delay),
    )));
    run_and_reschedule_task(&TaskWorker::new(alix.context.clone(), MlsTaskHandlers), row).await?;
    test_hooks::set_rate_limit_override(None);
    let refused = xmtp_common::time::now_ns();
    let task = || {
        db.get_tasks()
            .unwrap()
            .into_iter()
            .find(|t| t.data_hash == hash.as_ref())
    };
    let held = task().expect("the task is kept");
    let hold = refused - 10_000_000 + delay.as_nanos() as i64;
    assert_eq!(held.attempts, 1);
    assert!(held.next_attempt_at_ns >= hold);

    // A pull-in, such as a startup or Welcome nudge, leaves a task in
    // failure backoff on its stored schedule.
    enqueue_pull_in(&alix.context, hash, xmtp_common::time::now_ns(), i64::MAX)?;
    let pull_in = db
        .get_tasks()?
        .into_iter()
        .find(|t| t.data_hash != hash.as_ref())
        .expect("the pull-in task");
    run_and_reschedule_task(
        &TaskWorker::new(alix.context.clone(), MlsTaskHandlers),
        pull_in,
    )
    .await?;
    let after = task().expect("a held task does not run");
    assert_eq!(after.attempts, 1);
    assert_eq!(after.next_attempt_at_ns, held.next_attempt_at_ns);
}

/// A rate-limit delay longer than a task's remaining lifetime does not
/// expire it: the wait the delay adds past the backoff is added to the
/// task's expiry, so the task runs once the delay has passed.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limit_delay_does_not_expire_the_task() {
    use xmtp_common::time::Duration;
    tester!(alix, worker_config: no_runner_cfg());
    let db = alix.context.db();
    let now = xmtp_common::time::now_ns();
    let proto = unique_proto();
    let hash = data_hash_for(&proto);
    let expires = now + xmtp_common::NS_IN_HOUR;
    let row = seed(&db, proto, now - 1, expires, 0, i32::MAX);
    let delay = Duration::from_secs(2 * 60 * 60);
    test_hooks::set_rate_limit_override(Some((
        hash.to_vec(),
        Duration::ZERO,
        rate_limited_rejection(delay),
    )));
    run_and_reschedule_task(&TaskWorker::new(alix.context.clone(), MlsTaskHandlers), row).await?;
    test_hooks::set_rate_limit_override(None);
    let held = db
        .get_tasks()?
        .into_iter()
        .find(|t| t.data_hash == hash.as_ref())
        .expect("the task is kept");
    assert!(held.next_attempt_at_ns >= now + delay.as_nanos() as i64);
    assert!(
        held.expires_at_ns > held.next_attempt_at_ns,
        "the task does not expire before it is due"
    );
}

/// A rate-limited turn does not use up the task's attempts, so a task
/// refused at its last attempt stays and runs after the delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_turn_keeps_the_tasks_attempts() {
    use xmtp_common::time::Duration;
    tester!(alix, worker_config: no_runner_cfg());
    let db = alix.context.db();
    let now = xmtp_common::time::now_ns();
    let proto = unique_proto();
    let hash = data_hash_for(&proto);
    let row = seed(&db, proto, now - 1, i64::MAX, 19, 20);
    test_hooks::set_rate_limit_override(Some((
        hash.to_vec(),
        Duration::ZERO,
        rate_limited_rejection(Duration::from_millis(1)),
    )));
    run_and_reschedule_task(&TaskWorker::new(alix.context.clone(), MlsTaskHandlers), row).await?;
    let task = || {
        db.get_tasks()
            .unwrap()
            .into_iter()
            .find(|t| t.data_hash == hash.as_ref())
    };
    let mut held = task().expect("the refused task is kept");
    assert_eq!(held.attempts, 19);
    held.next_attempt_at_ns = xmtp_common::time::now_ns() - 1;
    run_and_reschedule_task(
        &TaskWorker::new(alix.context.clone(), MlsTaskHandlers),
        held,
    )
    .await?;
    test_hooks::set_rate_limit_override(None);
    let held = task().expect("a second refusal keeps it too");
    assert_eq!(held.attempts, 19);
}

/// When the retry time of a rate-limited task cannot be stored, the turn
/// fails with the rejection, so the worker restarts after the delay instead
/// of resending the still-due task after its one-second restart delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_task_whose_hold_is_not_stored_holds_the_restart() {
    use crate::worker::NeedsDbReconnect;
    use xmtp_common::time::Duration;
    tester!(alix, worker_config: no_runner_cfg());
    let db = alix.context.db();
    let now = xmtp_common::time::now_ns();
    let proto = unique_proto();
    let hash = data_hash_for(&proto);
    let row = seed(&db, proto, now - 1, i64::MAX, 0, 20);
    let delay = Duration::from_secs(5 * 60);
    test_hooks::set_rate_limit_override(Some((
        hash.to_vec(),
        Duration::ZERO,
        rate_limited_rejection(delay),
    )));
    crate::worker::tasks::test_hooks::fail_next_reschedule();
    let stored_row = || {
        db.get_tasks()
            .unwrap()
            .into_iter()
            .find(|t| t.data_hash == hash.as_ref())
            .expect("the task is stored")
    };
    let before = stored_row();
    let result =
        run_and_reschedule_task(&TaskWorker::new(alix.context.clone(), MlsTaskHandlers), row).await;
    test_hooks::set_rate_limit_override(None);
    let error = result.expect_err("the write failed");
    assert_eq!(error.restart_delay(), Some(delay));
    let after = stored_row();
    assert_eq!(after.attempts, before.attempts);
    assert_eq!(after.next_attempt_at_ns, before.next_attempt_at_ns);
}

#[xmtp_common::test(unwrap_try = true)]
async fn pull_in_arm_lowers_existing_target() {
    tester!(alix, worker_config: no_runner_cfg());
    let db = alix.context.db();
    let now = xmtp_common::time::now_ns();
    // Far-future recurring target.
    let target_proto = unique_proto();
    let target_hash = data_hash_for(&target_proto);
    let far = now + 30 * xmtp_common::NS_IN_DAY;
    seed(&db, target_proto, far, i64::MAX, 0, i32::MAX);
    // Due pull-in aimed at it.
    enqueue_pull_in(&alix.context, target_hash, now + 1_000, i64::MAX)?;
    let pull_in_row = db
        .get_tasks()?
        .into_iter()
        .find(|t| t.data_hash != target_hash.as_ref())
        .expect("pull-in row exists");

    run_and_reschedule_task(
        &TaskWorker::new(alix.context.clone(), MlsTaskHandlers),
        pull_in_row,
    )
    .await?;

    let rows = db.get_tasks()?;
    let target = rows
        .iter()
        .find(|t| t.data_hash == target_hash.as_ref())
        .expect("target survives");
    assert_eq!(
        target.next_attempt_at_ns,
        now + 1_000,
        "arm must lower the target to the ceiling"
    );
    assert_eq!(rows.len(), 1, "applied pull-in must self-delete");
}

#[xmtp_common::test(unwrap_try = true)]
async fn pull_in_task_runs_and_pulls_in() {
    tester!(alix); // live TaskRunner
    let db = alix.context.db();
    let now = xmtp_common::time::now_ns();
    let proto = unique_proto();
    // Ceiling 1 day out: the exact-equality assert below proves lowering
    // regardless of magnitude, and a generous ceiling means the target can't
    // become due (and get dispatched/deleted) even on a pathologically slow
    // CI runner — only constraints are "> test wall time" and "!= far".
    let ceiling = now + xmtp_common::NS_IN_DAY;
    let far = now + 30 * xmtp_common::NS_IN_DAY;
    seed(&db, proto.clone(), far, i64::MAX, 0, i32::MAX);
    let hash = data_hash_for(&proto);

    enqueue_pull_in(&alix.context, hash, ceiling, i64::MAX)?;

    // Poll up to ~10s (wasm-safe): the worker dispatches the due pull-in,
    // which lowers the target and self-deletes.
    let mut pulled = false;
    for _ in 0..50u32 {
        xmtp_common::time::sleep(std::time::Duration::from_millis(200)).await;
        let rows = db.get_tasks()?;
        let target_ok = rows
            .iter()
            .any(|t| t.data_hash == hash.as_ref() && t.next_attempt_at_ns == ceiling);
        let pull_in_gone = !rows.iter().any(|t| {
            t.data_hash != hash.as_ref() && t.next_attempt_at_ns <= xmtp_common::time::now_ns()
        });
        if target_ok && pull_in_gone {
            pulled = true;
            break;
        }
    }
    assert!(
        pulled,
        "worker must apply the pull-in and lower the target deadline"
    );
}
