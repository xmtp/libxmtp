use super::*;
use crate::{groups::send_message_opts::SendMessageOpts, tester};
use xmtp_db::{
    ConnectionExt,
    consent_record::{ConsentState, ConsentType, StoredConsentRecord},
    group::{ConversationType, GroupMembershipState, StoredGroup},
};

// verifies: EVENT-024, SYNC-020
#[rstest::rstest]
#[timeout(std::time::Duration::from_secs(180))]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn thousand_local_consent_changes_publish_once_without_echo() {
    use std::sync::Arc;
    use tokio::sync::Notify;

    tester!(alix1, sync_worker);
    tester!(alix2, from: alix1);
    alix1.test_has_same_sync_group_as(&alix2).await?;
    alix1.worker().clear_metric(SyncMetric::ConsentSent);
    alix2.worker().clear_metric(SyncMetric::ConsentSent);
    alix2.worker().clear_metric(SyncMetric::ConsentReceived);

    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    *worker::test_hooks::BLOCK_NEXT_PREFERENCE_PUBLISH.lock() = Some((
        alix1.installation_id.to_vec(),
        entered.clone(),
        release.clone(),
    ));
    let record = |index| {
        StoredConsentRecord::new(
            ConsentType::InboxId,
            ConsentState::Allowed,
            format!("consent-burst-{index}"),
        )
    };
    alix1.set_consent_states(&[record(0)]).await?;
    xmtp_common::time::timeout(std::time::Duration::from_secs(10), entered.notified()).await?;
    for index in 1..1_000 {
        alix1.set_consent_states(&[record(index)]).await?;
    }
    release.notify_one();
    xmtp_common::time::timeout(std::time::Duration::from_secs(160), async {
        while alix1.worker().get(SyncMetric::ConsentSent) < 1_000
            || alix2.worker().get(SyncMetric::ConsentReceived) < 1_000
        {
            xmtp_common::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await?;
    assert_eq!(alix1.worker().get(SyncMetric::ConsentSent), 1_000);
    assert_eq!(alix2.worker().get(SyncMetric::ConsentReceived), 1_000);
    assert_eq!(alix2.worker().get(SyncMetric::ConsentSent), 0);
}

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn failed_preference_publish_is_retried_after_worker_restart() {
    use std::sync::Arc;
    use tokio::sync::Notify;

    tester!(alix, sync_worker);
    alix.client.wait_for_sync_worker_init().await;
    alix.worker().clear_metric(SyncMetric::ConsentSent);
    let failed = Arc::new(Notify::new());
    *worker::test_hooks::FAIL_NEXT_PREFERENCE_PUBLISH.lock() =
        Some((alix.installation_id.to_vec(), failed.clone()));

    alix.set_consent_states(&[StoredConsentRecord::new(
        ConsentType::InboxId,
        ConsentState::Allowed,
        "retry-after-failed-publish".into(),
    )])
    .await?;
    xmtp_common::time::timeout(std::time::Duration::from_secs(10), failed.notified()).await?;
    alix.worker()
        .register_interest(SyncMetric::ConsentSent, 1)
        .wait()
        .await?;
    assert_eq!(alix.worker().get(SyncMetric::ConsentSent), 1);
}

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn reconnect_retries_preference_publish_aborted_in_flight() {
    use std::sync::Arc;
    use tokio::sync::Notify;

    tester!(alix, persistent_db, sync_worker);
    alix.client.wait_for_sync_worker_init().await;
    alix.worker().clear_metric(SyncMetric::ConsentSent);
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    *worker::test_hooks::BLOCK_NEXT_PREFERENCE_PUBLISH.lock() = Some((
        alix.installation_id.to_vec(),
        entered.clone(),
        release.clone(),
    ));
    alix.set_consent_states(&[StoredConsentRecord::new(
        ConsentType::InboxId,
        ConsentState::Allowed,
        "reconnect-in-flight-publish".into(),
    )])
    .await?;
    xmtp_common::time::timeout(std::time::Duration::from_secs(10), entered.notified()).await?;
    assert_eq!(alix.worker().get(SyncMetric::ConsentSent), 0);
    alix.client.reconnect_db()?;
    alix.worker()
        .register_interest(SyncMetric::ConsentSent, 1)
        .wait()
        .await?;
    assert_eq!(alix.worker().get(SyncMetric::ConsentSent), 1);
}

// verifies: SYNC-023
#[xmtp_common::test(unwrap_try = true)]
fn unknown_device_sync_content_is_ignored() {
    // Field 1 was DeviceSyncContent.request. It is reserved after history
    // transfer removal, so a message from an older installation is ignored.
    assert!(decode_supported_content(&[0x0a, 0x00]).is_none());
}

// verifies: SYNC-021
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
    let published = alix1.context.events().subscribe(
        xmtp_events::EventFilter::default()
            .with_internal(|event| matches!(event, InternalEvent::SyncMessagePublished)),
        None,
    );
    dm.update_consent_state(ConsentState::Denied)?;
    alix1
        .worker()
        .register_interest(SyncMetric::ConsentSent, 1)
        .wait()
        .await?;
    assert!(matches!(
        published.next().await.unwrap().internal,
        Some(InternalEvent::SyncMessagePublished)
    ));

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

// verifies: SYNC-014
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

// verifies: SYNC-014
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

// verifies: SYNC-010
#[rstest::rstest]
#[case::another_inbox_adds(false)]
#[case::another_inbox_is_a_leaf(true)]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sync_welcome_naming_another_inbox_is_rejected(#[case] own_adder: bool) {
    tester!(alix1);
    tester!(eve);
    let sync_group = |context| {
        MlsGroup::create_and_insert(
            context,
            ConversationType::Sync,
            PreconfiguredPolicies::default().to_policy_set(),
            GroupMetadataOptions::default(),
            None,
        )
    };
    // The adder is always a leaf, so no Welcome names only a foreign adder.
    // alix1 creates its group before alix2 registers, so one commit adds
    // alix2 and eve together and alix2's Welcome names eve as a leaf.
    let alix1_group = own_adder
        .then(|| sync_group(alix1.context.clone()))
        .transpose()
        .unwrap();
    tester!(alix2, from: alix1);
    let foreign_id = match alix1_group {
        Some(group) => {
            group.add_members(&[eve.inbox_id()]).await.unwrap();
            group.group_id
        }
        None => {
            let group = sync_group(eve.context.clone()).unwrap();
            group.add_members(&[alix1.inbox_id()]).await.unwrap();
            group.group_id
        }
    };

    alix2.sync_welcomes().await.unwrap();

    let db = alix2.context.db();
    assert!(db.find_group(&foreign_id).unwrap().is_none());
    let topic = xmtp_db::incoming_envelope::StreamTopic {
        entity_id: alix2.context.installation_id().to_vec(),
        kind: xmtp_db::incoming_envelope::NetworkEntityKind::Welcome,
    };
    let rejected = db.read_last_rejection(&topic).unwrap().unwrap();
    assert_eq!(rejected.code, "invalid_welcome");
    assert!(
        db.pending_envelope(&topic, rejected.sequence_id)
            .unwrap()
            .is_none()
    );
    let target = alix2.device_sync_client().get_sync_group().await.unwrap();
    assert_ne!(target.group_id, foreign_id);
}

// verifies: SYNC-010
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sync_welcome_from_own_installation_is_joined() {
    tester!(alix1);
    tester!(alix2, from: alix1);
    let alix2_group = alix2.device_sync_client().get_sync_group().await?;

    alix1.sync_welcomes().await?;

    let stored = alix1.context.db().find_group(&alix2_group.group_id)??;
    assert_eq!(stored.membership_state, GroupMembershipState::Allowed);
    assert_eq!(stored.added_by_inbox_id, alix1.inbox_id());
    let target = alix1.device_sync_client().get_sync_group().await?;
    assert_eq!(target.group_id, alix2_group.group_id);
}

/// The part of a stored sync group that names another inbox.
#[derive(Clone, Copy)]
enum Foreign {
    Adder,
    Leaf,
    AdderAndLeaf,
}

// verifies: SYNC-010
#[rstest::rstest]
#[case::adder(Foreign::Adder)]
#[case::leaf(Foreign::Leaf)]
#[case::adder_and_leaf(Foreign::AdderAndLeaf)]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn stored_sync_group_naming_another_inbox_is_never_the_target(#[case] foreign: Foreign) {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;

    tester!(alix);
    tester!(eve);
    let own_id = alix
        .device_sync_client()
        .get_sync_group()
        .await
        .unwrap()
        .group_id;
    // Stands in for a foreign sync group that an older build stored.
    let foreign_id = match foreign {
        Foreign::Adder => alix.create_group(None, None).unwrap().group_id,
        Foreign::Leaf => {
            alix.create_group_with_members(&[eve.inbox_id()], None, None)
                .await
                .unwrap()
                .group_id
        }
        Foreign::AdderAndLeaf => {
            let group = eve
                .create_group_with_members(&[alix.inbox_id()], None, None)
                .await
                .unwrap();
            alix.sync_welcomes().await.unwrap();
            group.group_id
        }
    };
    let added_by = match foreign {
        Foreign::Leaf => alix.inbox_id(),
        Foreign::Adder | Foreign::AdderAndLeaf => eve.inbox_id(),
    };
    alix.context
        .db()
        .raw_query(|conn| {
            diesel::update(dsl::groups.find(&foreign_id))
                .set((
                    dsl::conversation_type.eq(ConversationType::Sync),
                    dsl::created_at_ns.eq(i64::MAX),
                    dsl::added_by_inbox_id.eq(added_by),
                ))
                .execute(conn)
        })
        .unwrap();

    let target = alix.device_sync_client().get_sync_group().await.unwrap();
    assert_eq!(target.group_id, own_id);
}

// verifies: SYNC-011
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sync_message_from_another_inbox_is_not_applied() {
    use diesel::prelude::*;
    use preference_sync::PreferenceUpdate;
    use xmtp_db::schema::groups::dsl;
    use xmtp_db::user_preferences::StoredUserPreferences;
    use xmtp_proto::xmtp::device_sync::content::PreferenceUpdates;

    tester!(alix1);
    tester!(alix2, from: alix1);
    tester!(eve);
    let eve_group = eve
        .create_group_with_members(&[alix1.inbox_id()], None, None)
        .await?;
    alix1.sync_welcomes().await?;
    alix2.sync_welcomes().await?;
    // Stands in for a foreign sync group that an older build accepted.
    alix1.context.db().raw_query(|conn| {
        diesel::update(dsl::groups.find(&eve_group.group_id))
            .set(dsl::conversation_type.eq(ConversationType::Sync))
            .execute(conn)
    })?;

    // Eve's key has the later cycle time, so it wins if it is applied.
    let update = |entity: &str, key: u8, cycled_at_ns: i64| {
        let updates = vec![
            PreferenceUpdate::Consent(StoredConsentRecord::new(
                ConsentType::InboxId,
                ConsentState::Denied,
                entity.to_string(),
            )),
            PreferenceUpdate::Hmac {
                key: vec![key; 42],
                cycled_at_ns,
            },
        ];
        sync_message_bytes(ContentProto::PreferenceUpdates(PreferenceUpdates {
            updates: updates.into_iter().map(Into::into).collect(),
        }))
    };
    let now = now_ns();
    eve_group
        .send_message(
            &update("from-eve", 1, now + NS_IN_DAY),
            SendMessageOpts::default(),
        )
        .await?;
    alix2
        .group(&eve_group.group_id)?
        .send_message(&update("from-alix2", 2, now), SendMessageOpts::default())
        .await?;
    alix1.group(&eve_group.group_id)?.sync().await?;

    let db = alix1.context.db();
    let pending = db.unprocessed_sync_group_messages()?;
    assert!(
        pending
            .iter()
            .any(|message| message.sender_inbox_id == eve.inbox_id())
    );
    let client = alix1.device_sync_client();
    client
        .process_sync_group_messages(&client.metrics, pending)
        .await?;

    let consent = |entity: &str| db.get_consent_record(entity.to_string(), ConsentType::InboxId);
    assert!(consent("from-eve")?.is_none());
    assert_eq!(consent("from-alix2")??.state, ConsentState::Denied);
    assert_eq!(
        StoredUserPreferences::load(&db)?.hmac_key,
        Some(vec![2; 42])
    );
    assert!(db.unprocessed_sync_group_messages()?.is_empty());
}

// The same event reaches the JSON destination and the public app log callback.
#[xmtp_common::test(unwrap_try = true)]
#[cfg(not(target_arch = "wasm32"))]
fn incoming_preference_logs_omit_secret_material() {
    use std::sync::Arc;
    use xmtp_logging::{Level, LogRecord, LogSinkTarget, SinkError, test_logging::LogCapture};

    struct Capture(parking_lot::Mutex<Vec<LogRecord>>);
    impl LogSinkTarget for Capture {
        fn on_record(&self, record: LogRecord) -> Result<(), SinkError> {
            self.0.lock().push(record);
            Ok(())
        }
    }

    let key = b"synthetic-private-sync-key-e7a619c3".to_vec();
    let entity = "synthetic-private-consent-identity";
    let updates = vec![
        preference_sync::PreferenceUpdate::Hmac {
            key: key.clone(),
            cycled_at_ns: 9_007_199_254_740_993,
        }
        .into(),
        preference_sync::PreferenceUpdate::Consent(StoredConsentRecord::new(
            ConsentType::InboxId,
            ConsentState::Denied,
            entity.to_string(),
        ))
        .into(),
    ];
    let sink = Arc::new(Capture(parking_lot::Mutex::new(Vec::new())));
    let capture = LogCapture::with_sink(Level::Info, Some(sink.clone()));
    tracing::dispatcher::with_default(&capture.dispatch(), || {
        worker::log_incoming_preference_updates(&updates);
    });
    let json = capture.output();
    assert!(json.contains("Incoming preference updates"));
    let records = sink.0.lock();
    assert_eq!(records.len(), 1);
    assert!(records[0].message.contains("Incoming preference updates"));
    for sensitive in [
        format!("{key:?}"),
        entity.to_string(),
        "9007199254740993".to_string(),
    ] {
        assert!(
            !json.contains(&sensitive),
            "sensitive sync material reached JSON logging; synthetic JSON: {json}; synthetic app records: {records:?}"
        );
        assert!(
            !records[0].message.contains(&sensitive),
            "sensitive material reached the app log message"
        );
        assert!(
            records[0]
                .fields
                .values()
                .all(|value| !value.contains(&sensitive)),
            "sensitive material reached app log fields"
        );
    }
    assert!(json.contains("\"update_count\":2"));
    assert_eq!(records[0].message, "Incoming preference updates");
    assert_eq!(
        records[0].fields.get("update_count"),
        Some(&"2".to_string())
    );
}

// verifies: PROC-036
#[xmtp_common::test(unwrap_try = true)]
#[cfg(not(target_arch = "wasm32"))]
async fn stored_preference_logs_omit_installation_id_and_secret_material() {
    use std::sync::Arc;
    use tracing::instrument::WithSubscriber;
    use xmtp_db::user_preferences::StoredUserPreferences;
    use xmtp_logging::{Level, LogRecord, LogSinkTarget, SinkError, test_logging::LogCapture};
    use xmtp_proto::xmtp::device_sync::content::PreferenceUpdates;

    struct Capture(parking_lot::Mutex<Vec<LogRecord>>);
    impl LogSinkTarget for Capture {
        fn on_record(&self, record: LogRecord) -> Result<(), SinkError> {
            self.0.lock().push(record);
            Ok(())
        }
    }

    tester!(alix, disable_workers);
    let client = alix.device_sync_client();
    let group = client.get_sync_group().await?;
    let mut key = b"synthetic-private-sync-key-4bd3e901".to_vec();
    key.resize(42, 0xa7);
    let entity = "synthetic-private-consent-identifier";
    let updates = vec![
        preference_sync::PreferenceUpdate::Hmac {
            key: key.clone(),
            cycled_at_ns: i64::MAX - 1,
        }
        .into(),
        preference_sync::PreferenceUpdate::Consent(StoredConsentRecord::new(
            ConsentType::InboxId,
            ConsentState::Denied,
            entity.into(),
        ))
        .into(),
    ];
    group
        .send_message(
            &sync_message_bytes(ContentProto::PreferenceUpdates(PreferenceUpdates {
                updates,
            })),
            SendMessageOpts::default(),
        )
        .await?;
    let db = alix.context.db();
    let messages = db.unprocessed_sync_group_messages()?;
    assert_eq!(messages.len(), 1);
    let sink = Arc::new(Capture(parking_lot::Mutex::new(Vec::new())));
    let capture = LogCapture::with_sink(Level::Info, Some(sink.clone()));
    client
        .process_sync_group_messages(&client.metrics, messages)
        .with_subscriber(capture.dispatch())
        .await?;

    assert!(db.unprocessed_sync_group_messages()?.is_empty());
    assert_eq!(
        StoredUserPreferences::load(&db)?.hmac_key,
        Some(key.clone())
    );
    assert_eq!(
        db.get_consent_record(entity.into(), ConsentType::InboxId)??
            .state,
        ConsentState::Denied
    );
    let json = capture.output();
    let records = sink.0.lock();
    assert!(json.contains("storing preference updates"));
    assert!(
        records
            .iter()
            .any(|record| record.message.contains("storing preference updates"))
    );
    for sensitive in [
        alix.context.installation_id().to_string(),
        hex::encode(alix.context.installation_id()),
        format!("{key:?}"),
        hex::encode(&key),
        entity.into(),
    ] {
        assert!(
            !json.contains(&sensitive),
            "sensitive preference data reached JSON"
        );
        assert!(
            records.iter().all(|record| {
                !record.message.contains(&sensitive)
                    && record
                        .fields
                        .values()
                        .all(|value| !value.contains(&sensitive))
            }),
            "sensitive preference data reached the log sink"
        );
    }
}
