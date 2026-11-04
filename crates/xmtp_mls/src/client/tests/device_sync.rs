//! Device sync driven by the client's task runner: the reconcile and retry
//! tasks that the client's task handlers run.

use crate::prelude::*;
use crate::tester;
use crate::utils::test::device_sync::{add_task_group_ids, decide_at, held, publish, receive};
#[cfg(not(target_arch = "wasm32"))]
use crate::worker::device_sync::*;
use prost::Message as _;
use xmtp_common::time::now_ns;
#[cfg(not(target_arch = "wasm32"))]
use xmtp_db::{
    ConnectionExt,
    consent_record::{ConsentValue, ConsentWrite, Owed, write_consent},
};
use xmtp_db::{
    consent_record::{ConsentKey, ConsentState},
    group::ConversationType,
    prelude::*,
};
#[cfg(not(target_arch = "wasm32"))]
use xmtp_id::utils::test::docker_smart_wallet;
use xmtp_mls_engine::groups::{GroupError, MlsGroup, group_permissions::PreconfiguredPolicies};
use xmtp_mls_group_state::group::GroupMetadataOptions;
use xmtp_proto::xmtp::mls::database::task::Task as TaskKindProto;

// verifies: SYNC-001
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sync_group_creation_add_survives_exit_and_spent_attempts() {
    use crate::task_handlers::MlsTaskHandlers;
    use xmtp_mls_core::worker::tasks::TaskWorker;
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    // Stop after the insert, before get_sync_group's inline add.
    let before = now_ns();
    let group = MlsGroup::create_and_insert(
        alix1.context.clone(),
        ConversationType::Sync,
        PreconfiguredPolicies::default().to_policy_set(),
        GroupMetadataOptions::default(),
        None,
    )?;
    assert_eq!(add_task_group_ids(&alix1), vec![group.group_id.to_vec()]);
    let db = alix1.context.db();
    let [task] = db.get_tasks()?.try_into().unwrap();
    assert_eq!(
        (task.expires_at_ns, task.max_attempts),
        (i64::MAX, i32::MAX)
    );
    assert!(task.next_attempt_at_ns >= before + 60 * xmtp_common::NS_IN_SEC);
    let task = db.update_task(task.id, 25, now_ns(), now_ns())?;
    xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
        &TaskWorker::new(alix1.context.clone(), MlsTaskHandlers),
        task,
    )
    .await?;
    assert!(group.with_group_snapshot(|mls| {
        Ok(mls
            .members()
            .any(|member| member.signature_key == alix2.context.installation_id().as_slice()))
    })?);
}

// verifies: SYNC-001
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sync_group_inline_add_keeps_retry_until_membership_complete() {
    use crate::task_handlers::MlsTaskHandlers;
    use crate::utils::test_mocks_helpers as kp;
    use xmtp_mls_core::worker::tasks::TaskWorker;

    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    let restore = kp::RestoreKeyPackages;
    kp::set_test_mode_upload_malformed_keypackage(true, Some(vec![alix2.installation_id.to_vec()]));
    let group = alix1.device_sync_client().get_sync_group().await?;
    let leaves = group.with_group_snapshot(|mls| Ok(mls.members().count()))?;
    assert_eq!(leaves, 1);
    assert_eq!(add_task_group_ids(&alix1), vec![group.group_id.to_vec()]);
    drop(restore);
    let db = alix1.context.db();
    let [task] = db.get_tasks()?.try_into().unwrap();
    let task = db.update_task(task.id, 0, now_ns(), now_ns())?;
    xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
        &TaskWorker::new(alix1.context.clone(), MlsTaskHandlers),
        task,
    )
    .await?;
    alix2.sync_welcomes().await?;
    assert!(alix2.group(&group.group_id).is_ok());
    assert!(add_task_group_ids(&alix1).is_empty());
}

/// While an own installation has no usable key package, a publish still
/// reaches the leaves the selected group holds and arms a durable retry.
/// After a restart, once the package is usable, the retry adds the
/// installation without an identity change, and the add's replay reaches it.
// verifies: SYNC-034, SYNC-035, GMOD-055
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn publish_reaches_present_leaves_and_retries_a_missing_one() {
    use crate::task_handlers::MlsTaskHandlers;
    use crate::utils::test_mocks_helpers::{
        RestoreKeyPackages, set_test_mode_upload_malformed_keypackage,
    };
    use xmtp_db::tasks::QueryTasks;
    use xmtp_mls_core::worker::tasks::TaskWorker;

    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    let group = alix1.device_sync_client().get_sync_group().await?;
    alix2.sync_welcomes().await?;
    tester!(alix3, from: alix1, disable_workers);
    alix1.inbox_state(true).await?;
    let alix3_key = alix3.context.installation_id().to_vec();
    let has_alix3 = || {
        group.with_group_snapshot(|mls_group| {
            Ok::<_, GroupError>(mls_group.members().any(|m| m.signature_key == alix3_key))
        })
    };
    let x = ConsentKey::inbox("x");
    decide_at(&alix1, &x, ConsentState::Denied, now_ns())?;
    let restore = RestoreKeyPackages;
    set_test_mode_upload_malformed_keypackage(true, Some(vec![alix3_key.clone()]));
    let published_at = now_ns();
    publish(&alix1).await?;
    receive(&alix2).await?;
    assert_eq!(held(&alix2, &x), held(&alix1, &x));
    assert!(!has_alix3()?);

    // The retry is a task row, kept while alix3 has no leaf.
    let db = alix1.context.db();
    let add_task = || {
        db.get_tasks().unwrap().into_iter().find(|task| {
            matches!(
                xmtp_proto::xmtp::mls::database::Task::decode(task.data.as_slice())
                    .ok()
                    .and_then(|task| task.task),
                Some(TaskKindProto::AddMissingInstallations(_))
            )
        })
    };
    let task = add_task().expect("a retry task");
    let interval = xmtp_configuration::SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS;
    assert!(task.next_attempt_at_ns >= published_at + interval);
    assert_eq!(
        (task.expires_at_ns, task.max_attempts),
        (i64::MAX, i32::MAX)
    );
    let task = db.update_task(task.id, 0, now_ns(), published_at + interval)?;
    xmtp_common::time::sleep(std::time::Duration::from_nanos(interval as u64)).await;
    xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
        &TaskWorker::new(alix1.context.clone(), MlsTaskHandlers),
        task.clone(),
    )
    .await?;
    let kept = add_task().expect("the retry task is kept");
    assert!(kept.next_attempt_at_ns > task.next_attempt_at_ns);
    assert!(!has_alix3()?);
    // A failed attempt waits the interval too, not the runner's backoff.
    xmtp_common::time::sleep(std::time::Duration::from_nanos(interval as u64)).await;
    crate::task_handlers::test_hooks::fail_next_sync_retry();
    let failed_at = now_ns();
    xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
        &TaskWorker::new(alix1.context.clone(), MlsTaskHandlers),
        kept,
    )
    .await?;
    assert!(
        !crate::task_handlers::test_hooks::take_sync_retry_failure(),
        "the retry did not take the armed failure"
    );
    let after = add_task().expect("the retry task is kept");
    assert_eq!(
        (
            after.attempts,
            after.next_attempt_at_ns >= failed_at + interval
        ),
        (0, true)
    );

    drop(restore);
    let sequence = db.get_latest_sequence_id_for_inbox(alix1.inbox_id())?;
    let _restarted = crate::builder::ClientBuilder::from_client(alix1.client.clone())
        .with_disable_workers(false)
        .worker_config(xmtp_mls_core::worker::WorkerConfig::default())
        .device_sync_worker_mode(crate::builder::DeviceSyncMode::Enabled)
        .build()
        .await?;
    xmtp_common::wait_for_eq(
        || async {
            let _ = receive(&alix3).await;
            held(&alix3, &x)
        },
        held(&alix1, &x),
    )
    .await?;
    assert!(has_alix3()?);
    assert_eq!(
        db.get_latest_sequence_id_for_inbox(alix1.inbox_id())?,
        sequence
    );
}

/// A newcomer that never creates a sync group of its own is added to the
/// selected one by a publish. The commit that adds it schedules its adds to
/// the user's conversations, so it joins a quiet one where no one sends.
// verifies: SYNC-014, SYNC-034
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn newcomer_added_by_a_publish_joins_quiet_conversations() {
    use crate::task_handlers::MlsTaskHandlers;
    use xmtp_db::tasks::QueryTasks;
    use xmtp_mls_core::worker::tasks::TaskWorker;

    tester!(alix1, disable_workers);
    tester!(bo, disable_workers);
    let quiet = alix1
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    alix1.device_sync_client().get_sync_group().await?;
    tester!(alix2, from: alix1, disable_workers);
    alix1.inbox_state(true).await?;
    decide_at(
        &alix1,
        &ConsentKey::inbox("x"),
        ConsentState::Denied,
        now_ns(),
    )?;
    publish(&alix1).await?;

    for task in alix1.context.db().get_tasks()? {
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(alix1.context.clone(), MlsTaskHandlers),
            task,
        )
        .await?;
    }
    alix2.sync_welcomes().await?;
    assert_eq!(alix2.context.db().all_sync_groups()?.len(), 1);
    assert!(alix2.group(&quiet.group_id).is_ok());
}

/// A sync group whose creation add failed is completed by its task even
/// after another sync group became selected, so the peer still receives
/// the newcomer's Welcome.
// verifies: SYNC-014, SYNC-034
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn creation_task_adds_peers_after_its_group_loses_selection() {
    use crate::task_handlers::MlsTaskHandlers;
    use xmtp_db::tasks::QueryTasks;
    use xmtp_mls_core::worker::tasks::TaskWorker;

    tester!(alix1, disable_workers);
    let older = alix1.device_sync_client().get_sync_group().await?;
    tester!(alix2, from: alix1, disable_workers);
    // As when the creation add failed: the group holds only alix2.
    let newer = MlsGroup::create_and_insert(
        alix2.context.clone(),
        ConversationType::Sync,
        PreconfiguredPolicies::default().to_policy_set(),
        GroupMetadataOptions::default(),
        None,
    )?;
    let sync = alix2.device_sync_client();
    sync.schedule_add_missing_installations_task(newer.group_id, None)?;
    // alix1's reconcile adds alix2 to the older group; alix2 installs that
    // Welcome later, so the older group is selected.
    alix1.inbox_state(true).await?;
    older.add_missing_installations().await?;
    alix2.sync_welcomes().await?;
    assert_eq!(sync.primary_sync_group()?.unwrap().group_id, older.group_id);

    for task in alix2.context.db().get_tasks()? {
        // Drive the delayed creation retry without waiting for its deadline.
        let task = alix2
            .context
            .db()
            .update_task(task.id, 0, now_ns(), now_ns())?;
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(alix2.context.clone(), MlsTaskHandlers),
            task,
        )
        .await?;
    }
    alix1.sync_welcomes().await?;
    assert!(alix1.group(&newer.group_id).is_ok());
}

/// A pool dropped while an own-state read verifies a smart contract wallet
/// signature fails an outbox publish as `Client(Storage)`, which stops the
/// worker for a reconnect. The sync group's creation scheduled its retry task
/// a minute out, so the test replaces it with one due now. That task reaches
/// the same read,
/// then records its failure on the dropped pool, so the runner gets that
/// store error instead of `TaskWorkerError::DeviceSync`.
#[cfg(not(target_arch = "wasm32"))]
#[rstest::rstest]
#[xmtp_common::timeout(std::time::Duration::from_secs(120))]
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn an_own_state_disconnect_keeps_the_reconnect_signal(
    #[future] docker_smart_wallet: xmtp_id::utils::test::SmartWalletContext,
) {
    use crate::{
        task_handlers::MlsTaskHandlers,
        test::builder_native_only::{HookedVerifier, register_scw_client},
    };
    use prost::Message;
    use std::sync::{Arc, OnceLock, atomic::AtomicUsize, atomic::Ordering};
    use xmtp_db::{diesel::RunQueryDsl, tasks::QueryTasks};
    use xmtp_mls_core::worker::{NeedsDbReconnect, tasks::TaskWorker};
    use xmtp_mls_identity::identity_updates::IdentityUpdateError;
    let before = Arc::new(OnceLock::<Box<dyn Fn() + Send + Sync>>::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let wallet = docker_smart_wallet.await;
    let verifier = HookedVerifier::new(&wallet, before.clone(), calls.clone());
    let (alix, _) = register_scw_client(wallet, Some(verifier)).await;
    let sync = alix.device_sync_client();
    let group = sync.get_sync_group().await.unwrap();
    let write = ConsentWrite::Merge {
        value: ConsentValue {
            state: ConsentState::Denied,
            consented_at_ns: now_ns(),
        },
        bound_ns: i64::MAX,
    };
    write_consent(
        &alix.context.db(),
        &ConsentKey::inbox("owed"),
        write,
        Owed::Mark,
    )
    .unwrap();
    let forget_own_state = || {
        alix.context
            .db()
            .raw_query(|c| {
                xmtp_db::diesel::delete(xmtp_db::schema::association_state::table).execute(c)
            })
            .unwrap();
    };
    forget_own_state();
    let db = alix.context.db();
    assert!(
        before
            .set(Box::new(move || {
                let _ = db.disconnect();
            }))
            .is_ok()
    );

    let verified = calls.load(Ordering::SeqCst);
    let error = sync.publish_outbox().await.unwrap_err();
    assert!(
        calls.load(Ordering::SeqCst) > verified,
        "publish did not verify"
    );
    assert!(
        matches!(
            &error,
            DeviceSyncError::IdentityUpdate(IdentityUpdateError::Storage(_))
        ),
        "{error:?}"
    );
    assert!(error.needs_db_reconnect());

    alix.context.db().reconnect().unwrap();
    forget_own_state();
    alix.context
        .db()
        .raw_query(|c| xmtp_db::diesel::delete(xmtp_db::schema::tasks::table).execute(c))
        .unwrap();
    sync.schedule_add_missing_installations_task(
        group.group_id,
        Some(xmtp_common::time::Duration::ZERO),
    )
    .unwrap();
    let task = alix
        .context
        .db()
        .get_tasks()
        .unwrap()
        .into_iter()
        .find(|task| {
            matches!(
                xmtp_proto::xmtp::mls::database::Task::decode(task.data.as_slice())
                    .ok()
                    .and_then(|task| task.task),
                Some(TaskKindProto::AddMissingInstallations(_))
            )
        })
        .expect("the retry task");
    assert!(
        task.next_attempt_at_ns <= now_ns(),
        "the retry task is not due"
    );
    let verified = calls.load(Ordering::SeqCst);
    let error = xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
        &TaskWorker::new(alix.context.clone(), MlsTaskHandlers),
        task,
    )
    .await
    .unwrap_err();
    assert!(
        calls.load(Ordering::SeqCst) > verified,
        "the task did not verify"
    );
    let chain = format!("{error:?}");
    let chain: Vec<&str> = chain.split('(').take(3).collect();
    assert_eq!(
        (chain.join("("), error.needs_db_reconnect()),
        ("Storage(Connection(Platform".to_string(), true),
    );
}
