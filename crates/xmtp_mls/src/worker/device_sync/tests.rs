use super::*;
use crate::{groups::send_message_opts::SendMessageOpts, tester};
use xmtp_db::{
    ConnectionExt,
    consent_record::ConsentState,
    group::{ConversationType, StoredGroup},
};

#[xmtp_common::test(unwrap_try = true)]
fn unknown_device_sync_content_is_ignored() {
    // Field 1 was DeviceSyncContent.request. It is reserved after history
    // transfer removal, so a message from an older installation is ignored.
    assert!(decode_supported_content(&[0x0a, 0x00]).is_none());
}

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_hmac_and_consent_preference_sync() {
    tester!(alix1, sync_worker);
    tester!(bo);

    let (dm, _) = alix1.test_talk_in_dm_with(&bo).await?;

    tester!(alix2, from: alix1);
    alix1.test_has_same_sync_group_as(&alix2).await?;

    // The TaskRunner adds alix2 to alix1's existing DM. Poll while alix2
    // syncs welcomes because both the membership commit and welcome delivery
    // are asynchronous.
    xmtp_common::wait_for_some(|| async {
        let _ = alix2.sync_welcomes().await;
        alix2.group(&dm.group_id).ok().map(|_| ())
    })
    .await
    .expect("alix2 must receive the DM via the TaskRunner membership add");

    alix1
        .worker()
        .register_interest(SyncMetric::HmacSent, 1)
        .wait()
        .await?;
    alix1
        .worker()
        .register_interest(SyncMetric::HmacReceived, 1)
        .wait()
        .await?;
    let alix1_keys = dm.hmac_keys(-1..=1)?;

    alix2
        .worker()
        .register_interest(SyncMetric::HmacReceived, 1)
        .wait()
        .await?;

    let alix2_dm = alix2.group(&dm.group_id)?;
    let alix2_keys = alix2_dm.hmac_keys(-1..=1)?;

    assert_eq!(alix1_keys[0].key, alix2_keys[0].key);
    assert_eq!(dm.consent_state()?, alix2_dm.consent_state()?);

    // Stream consent
    alix1.worker().clear_metric(SyncMetric::ConsentSent);
    dm.update_consent_state(ConsentState::Denied)?;
    alix1
        .worker()
        .register_interest(SyncMetric::ConsentSent, 1)
        .wait()
        .await?;

    alix2
        .worker()
        .register_interest(SyncMetric::ConsentReceived, 1)
        .wait()
        .await?;

    let alix2_dm = alix2.group(&dm.group_id)?;
    assert_eq!(alix2_dm.consent_state()?, ConsentState::Denied);

    // Now alix1 receives a group from bo, alix1 consents. Alix2 should see the group as consented as well.
    let bo_group = bo
        .create_group_with_members(&[alix1.inbox_id()], None, None)
        .await?;
    alix1.sync_welcomes().await?;
    let alix1_group = alix1.group(&bo_group.group_id)?;
    assert_eq!(alix1_group.consent_state()?, ConsentState::Unknown);

    // Wait for publication before syncing the new group. The device-sync
    // worker keeps receiving consent without an app message stream.
    alix1.worker().clear_metric(SyncMetric::ConsentSent);
    alix1_group.update_consent_state(ConsentState::Allowed)?;
    alix1
        .worker()
        .register_interest(SyncMetric::ConsentSent, 1)
        .wait()
        .await?;

    alix2.sync_all_welcomes_and_groups(None).await?;

    alix2
        .worker()
        .register_interest(SyncMetric::ConsentReceived, 2)
        .wait()
        .await?;
    let alix2_group = alix2.group(&bo_group.group_id)?;
    assert_eq!(alix2_group.consent_state()?, ConsentState::Allowed);
}

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_only_added_to_correct_groups() {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;

    tester!(alix1, stream, sync_worker);
    tester!(bo);

    let old_group = alix1
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    old_group
        .send_message(b"hi there", SendMessageOpts::default())
        .await?;
    alix1.context.db().raw_query(|conn| {
        diesel::update(dsl::groups.find(&old_group.group_id))
            .set((dsl::last_message_ns.eq(0), dsl::created_at_ns.eq(0)))
            .execute(conn)
    })?;

    let bo_group_denied = bo
        .create_group_with_members(&[alix1.inbox_id()], None, None)
        .await?;
    let bo_group_unknown = bo
        .create_group_with_members(&[alix1.inbox_id()], None, None)
        .await?;
    let bo_dm = bo.find_or_create_dm(alix1.inbox_id(), None).await?;

    alix1.sync_welcomes().await?;
    let alix_bo_group_denied = alix1.group(&bo_group_denied.group_id)?;
    let alix_bo_group_unknown = alix1.group(&bo_group_unknown.group_id)?;
    let alix_bo_dm = alix1.group(&bo_dm.group_id)?;

    alix_bo_dm.update_consent_state(ConsentState::Allowed)?;
    alix_bo_group_denied.update_consent_state(ConsentState::Denied)?;

    let new_group = alix1
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    new_group
        .send_message(b"hi there", SendMessageOpts::default())
        .await?;

    tester!(alix2, from: alix1);

    alix1
        .worker()
        .register_interest(SyncMetric::SyncGroupWelcomesProcessed, 1)
        .wait()
        .await?;

    // The adds are durable TaskRunner work now — the metric above fires when
    // the tasks are scheduled, not when the commits land. Poll until alix2
    // has been welcomed into every eligible group: the new fresh group, the
    // unknown-consent group, and the consented DM.
    xmtp_common::wait_for_some(|| async {
        let _ = alix2.sync_welcomes().await;
        (alix2.group(&new_group.group_id).is_ok()
            && alix2.group(&alix_bo_group_unknown.group_id).is_ok()
            && alix2.group(&alix_bo_dm.group_id).is_ok())
        .then_some(())
    })
    .await
    .expect("alix2 must be added to all eligible groups");

    // The negatives are meaningful once the positive set has converged: the
    // filtered-out groups never get an AddMissingInstallations task at all.
    // Not added to old stale group
    let alix2_old_group = alix2.group(&old_group.group_id);
    assert!(alix2_old_group.is_err());

    // Not added to denied group from Bo
    let alix2_bo_group_denied = alix2.group(&alix_bo_group_denied.group_id);
    assert!(alix2_bo_group_denied.is_err());
}

#[xmtp_common::timeout(std::time::Duration::from_secs(15))]
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_new_devices_not_added_to_old_sync_groups() {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;

    tester!(alix1, sync_worker);
    tester!(alix2, from: alix1);

    alix1.test_has_same_sync_group_as(&alix2).await?;
    let groups = alix1.find_groups(GroupQueryArgs {
        include_sync_groups: true,
        ..Default::default()
    })?;
    for group in groups {
        group.maybe_update_installations(None).await?;
    }

    // alix1 should have it's own created sync group and alix2's sync group
    let alix1_sync_groups: Vec<StoredGroup> = alix1.context.db().raw_query(|conn| {
        dsl::groups
            .filter(dsl::conversation_type.eq(ConversationType::Sync))
            .load(conn)
    })?;
    assert_eq!(alix1_sync_groups.len(), 2);

    // alix2 should not be added to alix1's old sync group

    alix2.sync_welcomes().await?;
    let alix2_sync_groups: Vec<StoredGroup> = alix2.context.db().raw_query(|conn| {
        dsl::groups
            .filter(dsl::conversation_type.eq(ConversationType::Sync))
            .load(conn)
    })?;
    assert_eq!(alix2_sync_groups.len(), 1);
}

#[xmtp_common::timeout(std::time::Duration::from_secs(60))]
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_incremental_consent() {
    tester!(alix1, sync_worker);
    tester!(alix2, from: alix1);

    tester!(bo);
    let (dm, _) = bo.test_talk_in_dm_with(&alix1).await?;
    alix1
        .worker()
        .register_interest(SyncMetric::ConsentSent, 1)
        .wait()
        .await?;

    alix2.sync_all_welcomes_and_groups(None).await?;

    alix2
        .worker()
        .register_interest(SyncMetric::ConsentReceived, 1)
        .wait()
        .await?;

    let dm2 = alix2.group(&dm.group_id)?;
    assert_eq!(dm2.consent_state()?, ConsentState::Allowed);
}

#[xmtp_common::timeout(std::time::Duration::from_secs(60))]
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_task_runner_adds_new_installation_to_groups() {
    // Live sync worker + live TaskRunner (tester! defaults enable the runner).
    // `stream` keeps alix1 receiving welcomes so the sync-group welcome from
    // alix2's registration reaches alix1's device-sync worker.
    tester!(alix1, stream, sync_worker);
    tester!(bo);

    let group = alix1
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    tester!(alix2, from: alix1);

    // The welcome handler enqueues the task; the TaskRunner publishes the
    // membership commit; alix2 then receives the group via welcome. Poll —
    // the whole chain is async.
    xmtp_common::wait_for_some(|| async {
        let _ = alix2.sync_welcomes().await;
        alix2.group(&group.group_id).ok().map(|_| ())
    })
    .await
    .expect("alix2 must receive the group via the TaskRunner membership add");
}

#[xmtp_common::timeout(std::time::Duration::from_secs(30))]
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_sync_group_creation_leaves_no_reconcile_task() {
    use crate::worker::{WorkerConfig, WorkerKind};
    use prost::Message;
    use xmtp_db::tasks::QueryTasks;
    use xmtp_proto::xmtp::mls::database::{Task as TaskProto, task::Task as TaskKind};

    // TaskRunner disabled so any enqueued row would be observable (not consumed).
    let mut cfg = WorkerConfig::default();
    cfg.enabled.insert(WorkerKind::TaskRunner, false);
    tester!(alix, worker_config: cfg);

    alix.device_sync_client().get_sync_group().await?;

    // The durable reconcile task is armed only from the inline add's error
    // path. A successful creation must NOT leave one behind — an enqueue-first
    // version duplicated the reconcile (and its identity fetch) on every
    // sync-group creation, breaking the pinned network-call-count tests on
    // mobile bindings.
    let has_task = alix.context.db().get_tasks()?.iter().any(|t| {
        matches!(
            TaskProto::decode(t.data.as_slice())
                .ok()
                .and_then(|p| p.task),
            Some(TaskKind::AddMissingInstallations(_))
        )
    });
    assert!(
        !has_task,
        "successful sync-group creation must not enqueue a reconcile task"
    );
}

#[xmtp_common::timeout(std::time::Duration::from_secs(30))]
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_welcome_schedules_add_installation_tasks() {
    use crate::worker::{WorkerConfig, WorkerKind};
    use prost::Message;
    use xmtp_db::tasks::QueryTasks;
    use xmtp_proto::xmtp::mls::database::{Task as TaskProto, task::Task as TaskKind};

    // TaskRunner disabled so enqueued rows are observable (not consumed).
    let mut cfg = WorkerConfig::default();
    cfg.enabled.insert(WorkerKind::TaskRunner, false);
    tester!(alix1, worker_config: cfg);
    tester!(bo);

    let group = alix1
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    let count_add_tasks = || -> Vec<Vec<u8>> {
        alix1
            .context
            .db()
            .get_tasks()
            .unwrap()
            .iter()
            .filter_map(|t| match TaskProto::decode(t.data.as_slice()).ok()?.task {
                Some(TaskKind::AddMissingInstallations(a)) => Some(a.group_id),
                _ => None,
            })
            .collect()
    };

    // Call the schedule path directly (unit level — no live sync worker needed).
    let scheduled = alix1
        .device_sync_client()
        .schedule_add_installations_to_groups()?;
    assert!(scheduled >= 1);

    let add_tasks = count_add_tasks();
    assert!(
        add_tasks.iter().any(|gid| gid == &group.group_id.to_vec()),
        "expected an AddMissingInstallations task for the conversation group"
    );

    // Re-scheduling dedups on payload hash: row count stays put.
    alix1
        .device_sync_client()
        .schedule_add_installations_to_groups()?;
    let after = count_add_tasks();
    assert_eq!(
        add_tasks.len(),
        after.len(),
        "create_or_ignore must dedup identical payloads"
    );
}
