use super::*;
use crate::client::ConsentAct;
use crate::utils::test::crafted_welcome::assert_welcome_rejected;
use crate::{groups::send_message_opts::SendMessageOpts, tester};
use xmtp_common::NS_IN_HOUR;
use xmtp_db::{
    ConnectionExt,
    consent_record::{
        ConsentKey, ConsentState, ConsentType, ConsentValue, ConsentWrite, MAX_AHEAD_NS, Owed,
        StoredConsentRecord, write_consent,
    },
    group::{ConversationType, GroupMembershipState},
    sync_outbox::has_owed,
    user_preferences::StoredUserPreferences,
};
#[cfg(not(target_arch = "wasm32"))]
use xmtp_logging::{LogRecord, LogSinkTarget, SinkError};

#[cfg(not(target_arch = "wasm32"))]
struct Capture(parking_lot::Mutex<Vec<LogRecord>>);
#[cfg(not(target_arch = "wasm32"))]
impl LogSinkTarget for Capture {
    fn on_record(&self, record: LogRecord) -> Result<(), SinkError> {
        self.0.lock().push(record);
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn device_sync_creation_logs_omit_full_group_id() {
    use tracing::instrument::WithSubscriber;
    use xmtp_logging::{Level, test_logging::LogCapture};

    tester!(alix, disable_workers);
    let sync = alix.context.device_sync_client();
    assert!(sync.primary_sync_group()?.is_none());
    let capture = LogCapture::new(Level::Trace);
    let group = sync
        .get_sync_group()
        .with_subscriber(capture.dispatch())
        .await?;
    let output = capture.output();
    let event = output
        .lines()
        .find(|line| line.contains("Creating sync group:"))
        .expect("group creation emitted its INFO event");
    assert!(
        !event.contains(&hex::encode(group.group_id)),
        "group creation log contains the full sync group ID"
    );
}

/// Store `state` at `at` and mark it owed, as an installation whose clock read
/// `at` when the user decided.
fn decide_at(
    client: &crate::utils::Tester,
    key: &ConsentKey,
    state: ConsentState,
    at: i64,
) -> Result<(), StorageError> {
    let value = ConsentValue {
        state,
        consented_at_ns: at,
    };
    let write = ConsentWrite::Merge {
        value,
        bound_ns: i64::MAX,
    };
    write_consent(&client.context.db(), key, write, Owed::Mark)?;
    Ok(())
}

/// The value `client` holds for `key`.
fn held(client: &crate::utils::Tester, key: &ConsentKey) -> Option<ConsentValue> {
    client
        .context
        .db()
        .consent_row(key)
        .unwrap()
        .map(|row| row.value())
}

/// Read every sync group, then apply what arrived.
async fn receive(client: &crate::utils::Tester) -> Result<(), DeviceSyncError> {
    client.sync_all_welcomes_and_device_sync_groups().await?;
    client
        .device_sync_client()
        .process_sync_group_messages()
        .await
}

/// Publish everything `client` owes.
async fn publish(client: &crate::utils::Tester) -> Result<(), DeviceSyncError> {
    client.device_sync_client().publish_outbox().await
}

/// `newer` creates a sync group that holds every installation of the inbox.
/// Each of `others` installs its Welcome and sends what it owes there, and
/// every installation applies what arrived.
async fn join_sync_group(
    newer: &crate::utils::Tester,
    others: &[&crate::utils::Tester],
) -> Result<GroupId, DeviceSyncError> {
    let group = newer.device_sync_client().get_sync_group().await?;
    for other in others {
        other.sync_welcomes().await?;
        publish(other).await?;
    }
    publish(newer).await?;
    for client in others.iter().copied().chain([newer]) {
        receive(client).await?;
    }
    Ok(group.group_id)
}

fn consent_update(
    entity_type: ConsentType,
    state: ConsentState,
    entity: &str,
) -> PreferenceUpdateMessage {
    PreferenceUpdateMessage {
        update: Some(PreferenceUpdateProto::Consent(
            StoredConsentRecord::new(entity_type, state, entity.into()).into(),
        )),
    }
}

fn root_key_update(key: Vec<u8>, cycled_at_ns: i64) -> PreferenceUpdateMessage {
    PreferenceUpdateMessage {
        update: Some(PreferenceUpdateProto::Hmac(HmacKeyUpdate {
            key,
            cycled_at_ns,
        })),
    }
}

fn sync_bytes(updates: Vec<PreferenceUpdateMessage>) -> Zeroizing<Vec<u8>> {
    sync_message_bytes(ContentProto::PreferenceUpdates(PreferenceUpdates {
        updates,
    }))
}

// verifies: PROC-036
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn device_sync_logs_omit_message_payload() {
    use tracing::instrument::WithSubscriber;
    use xmtp_logging::{Level, test_logging::LogCapture};

    tester!(alix, disable_workers);
    xmtp_db::user_preferences::ensure_root_key(&alix.context.db())?;
    let capture = LogCapture::new(Level::Trace);
    alix.device_sync_client()
        .publish_outbox()
        .with_subscriber(capture.dispatch())
        .await?;
    let output = capture.output();
    assert!(
        output.contains("publishing owed preferences"),
        "publish emitted its INFO event"
    );
    assert!(
        !output.contains("\"content\":"),
        "publish span records the message payload"
    );
}

/// A thousand decisions are each published once, and the receiver, which got
/// them from the selected sync group, does not send them back.
// verifies: EVENT-024, SYNC-020, SYNC-027
#[rstest::rstest]
#[timeout(std::time::Duration::from_secs(240))]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn thousand_local_consent_changes_publish_once_without_echo() {
    use std::time::Duration;
    async fn until(done: impl AsyncFn() -> bool) {
        xmtp_common::time::timeout(Duration::from_secs(200), async {
            while !done().await {
                xmtp_common::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("condition holds in time");
    }
    tester!(alix1, sync_worker);
    tester!(alix2, from: alix1);
    alix1.test_has_same_sync_group_as(&alix2).await?;
    let drained =
        async || !has_owed(&alix1.context.db()).unwrap() && !has_owed(&alix2.context.db()).unwrap();
    until(drained).await;
    alix1.worker().clear_metric(SyncMetric::ConsentSent);
    alix2.worker().clear_metric(SyncMetric::ConsentSent);

    let keys: Vec<_> = (0..1_000)
        .map(|i| ConsentKey::inbox(&format!("consent-burst-{i}")))
        .collect();
    for key in &keys {
        let act = ConsentAct::new(
            ConsentType::InboxId,
            ConsentState::Allowed,
            key.entity.clone(),
        );
        alix1.set_consent_states(&[act]).await?;
    }
    until(async || keys.iter().all(|key| held(&alix2, key).is_some())).await;
    until(drained).await;
    assert_eq!(alix1.worker().get(SyncMetric::ConsentSent), 1_000);
    assert_eq!(
        alix2.worker().get(SyncMetric::ConsentSent),
        0,
        "the receiver echoed"
    );
    for key in &keys {
        assert_eq!(held(&alix2, key), held(&alix1, key));
    }
}

/// Changes are owed in the database: fifty acts on one key are one record,
/// failed passes never drop it, and a client opened later on the same
/// database publishes it.
// verifies: SYNC-027, SYNC-029, SYNC-033
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn outbox_is_durable_and_coalesced() {
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    let group_id = join_sync_group(&alix2, &[&alix1]).await?;
    let key = ConsentKey::inbox("durable");
    // The own identity state is unreadable, so every publish waits.
    let restore = hide_own_identity_updates(alix1.context.db(), alix1.inbox_id())?;
    for state in [ConsentState::Denied, ConsentState::Allowed]
        .into_iter()
        .cycle()
        .take(50)
    {
        let act = ConsentAct::new(ConsentType::InboxId, state, key.entity.clone());
        alix1.set_consent_states(&[act]).await?;
    }
    for _ in 0..25 {
        assert!(publish(&alix1).await.is_err());
    }
    assert!(alix1.context.db().consent_row(&key)?.unwrap().owed_seq > 0);
    restore()?;
    // A client opened later on the same database holds no state in memory.
    let reopened = crate::builder::ClientBuilder::from_client(alix1.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    reopened.device_sync_client().publish_outbox().await?;
    receive(&alix2).await?;
    assert_eq!(held(&alix2, &key), held(&alix1, &key));
    assert_eq!(held(&alix2, &key).unwrap().state, ConsentState::Allowed);
    let carried = alix2
        .context
        .db()
        .sync_group_messages_paged(0, 100)?
        .iter()
        .filter(|message| message.group_id == group_id)
        .filter_map(|message| decode_sync_message(&message.decrypted_message_bytes))
        .filter(|content| {
            matches!(&content.0, ContentProto::PreferenceUpdates(p) if p.updates.iter().any(|u|
                matches!(&u.update, Some(PreferenceUpdateProto::Consent(c)) if c.entity == "durable")))
        })
        .count();
    assert_eq!(carried, 1, "fifty acts on one key are one published record");
}

// verifies: SYNC-023
#[xmtp_common::test(unwrap_try = true)]
fn unknown_device_sync_content_is_ignored() {
    // Field 1 was DeviceSyncContent.request. It is reserved after history
    // transfer removal, so a message from an older installation is ignored.
    assert!(decode_supported_content(&[0x0a, 0x00]).is_none());
}

// verifies: SYNC-027, SYNC-029, SYNC-033
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

    // Wait for the state, not for a ConsentReceived count: the count is
    // cumulative, and alix2 may already have received alix1's earlier
    // Allowed update for the DM, which would end the wait before Denied
    // arrives.
    xmtp_common::wait_for_eq(
        || async { alix2.group(&dm.group_id).unwrap().consent_state().unwrap() },
        ConsentState::Denied,
    )
    .await?;

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

    let alix2_group = alix2.group(&bo_group.group_id)?;
    xmtp_common::wait_for_eq(
        || async { alix2_group.consent_state().unwrap() },
        ConsentState::Allowed,
    )
    .await?;
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

/// The periodic membership refresh skips sync groups, so it adds no newer
/// installation to an older one.
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn periodic_refresh_adds_no_installation_to_a_sync_group() {
    tester!(alix1, disable_workers);
    let old = alix1.device_sync_client().get_sync_group().await?;
    tester!(alix2, from: alix1, disable_workers);
    alix2.device_sync_client().get_sync_group().await?;
    alix1.sync_welcomes().await?;
    let alix1_sync_groups = alix1.context.db().all_sync_groups()?;
    assert_eq!(alix1_sync_groups.len(), 2);
    for group in &alix1_sync_groups {
        alix1
            .group(&group.id)?
            .maybe_update_installations(Some(0))
            .await?;
    }

    alix2.sync_welcomes().await?;
    let alix2_sync_groups = alix2.context.db().all_sync_groups()?;
    assert_eq!(alix2_sync_groups.len(), 1);
    assert!(alix2_sync_groups.iter().all(|g| g.id != old.group_id));
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
    use crate::utils::test::crafted_welcome::{CraftedWelcome, send_crafted_welcome_with};
    use xmtp_mls_common::group_keys::{GroupKeys, GroupRoot};
    use xmtp_proto::types::Cursor;

    tester!(alix1, disable_workers);
    tester!(eve, disable_workers);
    let alix2 = if own_adder {
        tester!(alix2, from: alix1, disable_workers);
        Some(alix2)
    } else {
        None
    };
    let root = GroupRoot::generate();
    let group_id = GroupKeys::derive(&root).group_id();
    // A current client rejects its own commit that adds eve to a sync group,
    // so the Welcome is crafted. The adder is always a leaf, so no Welcome
    // names only a foreign adder.
    let (adder, other_member) = match &alix2 {
        Some(alix2) => (alix2, Some(&*eve)),
        None => (&eve, None),
    };
    let options = CraftedWelcome {
        conversation_type: ConversationType::Sync,
        other_member,
        ..Default::default()
    };
    send_crafted_welcome_with(adder, &alix1, &group_id, Some(&root), options).await;

    alix1.wait_for_newest_welcome_rejected(Cursor(0)).await;

    let foreign_id = GroupId::from(group_id);
    assert_welcome_rejected(&alix1, &foreign_id);
    let target = alix1.device_sync_client().get_sync_group().await.unwrap();
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

// verifies: SYNC-010
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sync_group_commit_adding_another_inbox_is_rejected() {
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    tester!(eve);
    let group = alix1.device_sync_client().get_sync_group().await?;
    alix2.sync_welcomes().await?;
    let alix2_group = alix2.group(&group.group_id)?;
    let epoch = alix2_group.epoch().await?;

    // alix1's own check rejects the commit after it is published.
    assert!(group.add_members(&[eve.inbox_id()]).await.is_err());
    let topic = xmtp_db::incoming_envelope::StreamTopic::group(group.group_id);
    let db = alix2.context.db();
    let rejection = xmtp_common::wait_for_some(|| async {
        let _ = alix2_group.sync().await;
        db.read_last_rejection(&topic).unwrap()
    })
    .await
    .expect("alix2 did not reject the commit");
    assert_eq!(rejection.code, "commit_validation_failure");

    let members = alix2_group.members().await?;
    assert!(members.iter().all(|m| m.inbox_id != eve.inbox_id()));
    assert_eq!(alix2_group.epoch().await?, epoch);
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

// verifies: SYNC-030, SYNC-010
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sync_message_from_another_inbox_is_not_applied() {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;

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
        let mut bad = consent_update(ConsentType::InboxId, ConsentState::Denied, entity);
        if let Some(PreferenceUpdateProto::Consent(record)) = &mut bad.update {
            record.entity_type = 3; // Retired ADDRESS record.
        }
        sync_bytes(vec![
            bad,
            consent_update(ConsentType::InboxId, ConsentState::Denied, entity),
            root_key_update(vec![key; 42], cycled_at_ns),
        ])
    };
    let now = now_ns();
    eve_group
        .send_message(
            &update("from-eve", 1, now + NS_IN_HOUR),
            SendMessageOpts::default(),
        )
        .await?;
    alix2
        .group(&eve_group.group_id)?
        .send_message(&update("from-alix2", 2, now), SendMessageOpts::default())
        .await?;
    alix1.group(&eve_group.group_id)?.sync().await?;

    let db = alix1.context.db();
    let pending = db.unprocessed_sync_group_messages(None)?;
    assert!(
        pending
            .iter()
            .any(|message| message.sender_inbox_id == eve.inbox_id())
    );
    alix1
        .device_sync_client()
        .process_sync_group_messages()
        .await?;

    assert!(held(&alix1, &ConsentKey::inbox("from-eve")).is_none());
    assert_eq!(
        held(&alix1, &ConsentKey::inbox("from-alix2"))
            .unwrap()
            .state,
        ConsentState::Denied
    );
    assert_eq!(
        StoredUserPreferences::load(&db)?.hmac_key,
        Some(vec![2; 42].into())
    );
    assert!(
        db.unprocessed_sync_group_messages(None)?
            .iter()
            .all(|message| message.sender_inbox_id != eve.inbox_id())
    );
}

/// Fails the own-state read like an unreachable verifier until the returned fn runs.
pub(super) fn hide_own_identity_updates<'a>(
    db: impl ConnectionExt + 'a,
    inbox_id: &'a str,
) -> Result<impl FnOnce() -> Result<usize, xmtp_db::ConnectionError> + 'a, xmtp_db::ConnectionError>
{
    use diesel::RunQueryDsl;
    let set = move |from: &str, to: &str| {
        let sql =
            format!("UPDATE identity_updates SET inbox_id = '{to}' WHERE inbox_id = '{from}'");
        db.raw_query(|conn| diesel::sql_query(sql).execute(conn))
    };
    set(inbox_id, &format!("{inbox_id}-hidden"))?;
    Ok(move || set(&format!("{inbox_id}-hidden"), inbox_id))
}

// verifies: SYNC-016
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn failing_sync_message_stays_pending() {
    use diesel::RunQueryDsl;
    use xmtp_db::processed_device_sync_messages::SYNC_MESSAGE_PAGE;
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    join_sync_group(&alix2, &[&alix1]).await?;
    let db = alix1.context.db();
    db.raw_query(|conn| {
        diesel::sql_query(
            "CREATE TRIGGER fail_sync_consent BEFORE INSERT ON consent_records \
         WHEN NEW.entity = 'retry' BEGIN SELECT RAISE(ABORT, 'injected sync failure'); END",
        )
        .execute(conn)
    })?;
    let retry = ConsentKey::inbox("retry");
    decide_at(&alix2, &retry, ConsentState::Denied, now_ns())?;
    publish(&alix2).await?;
    // Later rows still apply in the same pass while `retry` stays Pending.
    let more: Vec<_> = (0..SYNC_MESSAGE_PAGE)
        .map(|i| ConsentKey::inbox(&format!("more-{i}")))
        .collect();
    for key in &more {
        decide_at(&alix2, key, ConsentState::Allowed, now_ns())?;
        publish(&alix2).await?;
    }
    receive(&alix1).await?;
    for key in &more {
        assert!(held(&alix1, key).is_some());
    }
    for _ in 0..4 {
        receive(&alix1).await?;
    }
    assert_eq!(db.unprocessed_sync_group_messages(None)?.len(), 1);
    assert!(held(&alix1, &retry).is_none());
    db.raw_query(|conn| diesel::sql_query("DROP TRIGGER fail_sync_consent").execute(conn))?;
    receive(&alix1).await?;
    assert_eq!(held(&alix1, &retry), held(&alix2, &retry));
    assert!(db.unprocessed_sync_group_messages(None)?.is_empty());
}

/// A message an own installation sent before its revocation is applied, also
/// when the receiver processes it after the revocation. One sent after it is
/// dropped.
// verifies: SYNC-030
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sender_judged_at_envelope_time() {
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    tester!(alix3, from: alix1, disable_workers);
    let group = alix1.device_sync_client().get_sync_group().await?;
    alix2.sync_welcomes().await?;
    alix3.sync_welcomes().await?;
    let before = ConsentKey::inbox("sent-before-revocation");
    decide_at(&alix2, &before, ConsentState::Denied, now_ns())?;
    publish(&alix2).await?;
    revoke(&alix3, &alix2).await?;
    let after = sync_bytes(vec![consent_update(
        ConsentType::InboxId,
        ConsentState::Denied,
        "sent-after-revocation",
    )]);
    alix2
        .group(&group.group_id)?
        .send_message(&after, SendMessageOpts::default())
        .await?;

    receive(&alix1).await?;
    assert_eq!(held(&alix1, &before), held(&alix2, &before));
    assert!(held(&alix1, &ConsentKey::inbox("sent-after-revocation")).is_none());
    assert!(
        alix1
            .context
            .db()
            .unprocessed_sync_group_messages(None)?
            .is_empty()
    );
}

/// Revoke `revoked`'s installation with `owner`'s wallet signature.
pub(super) async fn revoke(
    owner: &crate::utils::Tester,
    revoked: &crate::utils::Tester,
) -> Result<(), ClientError> {
    use xmtp_id::associations::test_utils::add_wallet_signature;

    let identity = owner.identity_updates();
    let mut request = identity
        .revoke_installations(vec![revoked.context.installation_id().to_vec()])
        .await?;
    add_wallet_signature(&mut request, &owner.builder.owner).await;
    identity.apply_signature_request(request).await
}

/// After a revocation the revoker cycles the root key. A removal-only refresh
/// drops the revoked leaf and adds no newcomer. The selected group lacks
/// alix4, so the next publish adds it, or a peer's commit already did; the
/// commit that adds it owes alix4 every record alix1 had already sent.
// verifies: SYNC-034, SYNC-035, SYNC-026
#[rstest::rstest]
#[case::local_adder(false)]
#[case::peer_adder(true)]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn publish_adds_the_installation_the_selected_group_lacks(
    #[case] peer_adds: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;
    use xmtp_db::sync_outbox::{clear_owed, owed_page};
    use xmtp_mls_validation::commit::extract_group_membership;

    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    tester!(alix3, from: alix1, disable_workers);
    let group = alix1.device_sync_client().get_sync_group().await?;
    alix2.sync_welcomes().await?;
    alix3.sync_welcomes().await?;
    tester!(alix4, from: alix1, disable_workers);
    alix4.device_sync_client().get_sync_group().await?;
    alix1.sync_welcomes().await?;
    // As when the older group's Welcome is installed after alix4's.
    alix1.context.db().raw_query(|conn| {
        diesel::update(dsl::groups.find(&group.group_id))
            .set(dsl::created_at_ns.eq(now_ns()))
            .execute(conn)
    })?;
    let selected = alix1.device_sync_client().primary_sync_group()?.unwrap();
    assert_eq!(selected.group_id, group.group_id);
    // Already sent, so not owed: records that alix3 lacks, and the root key.
    let records = [
        (ConsentKey::inbox("time-zero"), ConsentState::Allowed, 0),
        (ConsentKey::inbox("sent-before"), ConsentState::Denied, 5),
    ];
    for (key, state, at) in &records {
        decide_at(&alix1, key, *state, *at)?;
    }
    let db = alix1.context.db();
    clear_owed(&db, &owed_page(&db, PAGE_UPDATES, PAGE_BYTES)?)?;
    assert!(!has_owed(&db)?);
    revoke(&alix1, &alix2).await?;
    let cycled = StoredUserPreferences::load(&db)?;
    assert!(
        cycled.hmac_key_owed_seq > 0,
        "the revocation cycled an owed key"
    );

    let snapshot = || {
        group.with_group_snapshot(|mls_group| {
            let membership = extract_group_membership(mls_group.extensions())?;
            let leaves: Vec<_> = mls_group.members().map(|m| m.signature_key).collect();
            Ok::<_, GroupError>((leaves, membership.failed_installations))
        })
    };
    let alix2_key = alix2.context.installation_id().to_vec();
    let alix4_key = alix4.context.installation_id().to_vec();
    if peer_adds {
        // One commit swaps alix2's leaf for alix4's and adds no inbox.
        alix3
            .group(&group.group_id)?
            .add_missing_installations()
            .await?;
        group.sync().await?;
        let (leaves, _) = snapshot()?;
        assert_eq!(leaves.len(), 3);
        assert!(!leaves.contains(&alix2_key) && leaves.contains(&alix4_key));
    } else {
        group.remove_revoked_installations().await?;
        let (leaves, failed) = snapshot()?;
        assert!(!leaves.contains(&alix2_key));
        assert!(
            !leaves.contains(&alix4_key),
            "a removal-only refresh added a newer installation"
        );
        // A later joiner checks the skipped newcomer against this list.
        assert!(failed.contains(&alix4_key));
    }
    // A client opened later on the same database holds nothing in memory.
    let reopened = crate::builder::ClientBuilder::from_client(alix1.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    reopened.device_sync_client().publish_outbox().await?;
    assert!(snapshot()?.0.contains(&alix4_key));

    // alix3 accepts the commits and cycles on the revocation too; the two
    // converge on the greater key, which the revoked alix2 never reads.
    receive(&alix3).await?;
    publish(&alix3).await?;
    receive(&alix1).await?;
    let key = |alix: &crate::utils::Tester| {
        StoredUserPreferences::load(alix.context.db())
            .unwrap()
            .hmac_key
    };
    assert_eq!(key(&alix1), key(&alix3));
    let _ = receive(&alix2).await;
    assert_ne!(key(&alix2), key(&alix1));
    receive(&alix4).await?;
    for (record, ..) in &records {
        assert_eq!(
            held(&alix4, record),
            held(&alix1, record),
            "{}",
            record.entity
        );
    }
    Ok(())
}

/// While an own installation has no usable key package, a publish still
/// reaches the leaves the selected group holds and arms a durable retry.
/// After a restart, once the package is usable, the retry adds the
/// installation without an identity change, and the add's replay reaches it.
// verifies: SYNC-034, SYNC-035, GMOD-055
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn publish_reaches_present_leaves_and_retries_a_missing_one() {
    use crate::utils::test_mocks_helpers::{
        RestoreKeyPackages, set_test_mode_upload_malformed_keypackage,
    };
    use crate::worker::tasks::TaskWorker;
    use xmtp_db::tasks::QueryTasks;

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
    xmtp_common::time::sleep(std::time::Duration::from_nanos(interval as u64)).await;
    TaskWorker::run_and_reschedule_task(task.clone(), &alix1.context).await?;
    let kept = add_task().expect("the retry task is kept");
    assert!(kept.next_attempt_at_ns > task.next_attempt_at_ns);
    assert!(!has_alix3()?);

    drop(restore);
    let sequence = db.get_latest_sequence_id_for_inbox(alix1.inbox_id())?;
    let _restarted = crate::builder::ClientBuilder::from_client(alix1.client.clone())
        .with_disable_workers(false)
        .worker_config(crate::worker::WorkerConfig::default())
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
/// A retry row that an older build stored with the three-day lifetime is
/// replaced by one that never expires before any attempt that could fail.
// verifies: SYNC-034
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn an_expiring_retry_row_is_replaced_by_a_durable_one() {
    use crate::worker::tasks::TaskWorker;
    use xmtp_db::tasks::QueryTasks;

    tester!(alix1, disable_workers);
    let group = alix1.device_sync_client().get_sync_group().await?;
    let db = alix1.context.db();
    schedule_add_missing_installations(&db, alix1.context.events(), group.group_id, None, false)?;
    let [old] = db.get_tasks()?.try_into().unwrap();
    assert_ne!(old.expires_at_ns, i64::MAX);
    TaskWorker::run_and_reschedule_task(old, &alix1.context).await?;
    let [row] = db.get_tasks()?.try_into().unwrap();
    assert_eq!((row.expires_at_ns, row.max_attempts), (i64::MAX, i32::MAX));
}

/// A sync group whose creation add failed is completed by its task even
/// after another sync group became selected, so the peer still receives
/// the newcomer's Welcome.
// verifies: SYNC-014, SYNC-034
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn creation_task_adds_peers_after_its_group_loses_selection() {
    use crate::worker::tasks::TaskWorker;
    use xmtp_db::tasks::QueryTasks;

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
        TaskWorker::run_and_reschedule_task(task, &alix2.context).await?;
    }
    alix1.sync_welcomes().await?;
    assert!(alix1.group(&newer.group_id).is_ok());
}

// A root HMAC key that another installation syncs is a `hmac_keys.updated`
// trigger (EVENT section 1). One message with two keys is one change.
// verifies: EVENT-001
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn synced_hmac_key_emits_hmac_keys_updated_once() {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;
    use xmtp_events::{ClientEvent, EventEnvelope, EventFilter, EventKind};

    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1);
    tester!(bo);
    let group = bo
        .create_group_with_members(&[alix1.inbox_id()], None, None)
        .await?;
    alix1.sync_welcomes().await?;
    alix2.sync_welcomes().await?;
    // Stands in for the shared sync group, which needs the sync worker.
    alix1.context.db().raw_query(|conn| {
        diesel::update(dsl::groups.find(&group.group_id))
            .set(dsl::conversation_type.eq(ConversationType::Sync))
            .execute(conn)
    })?;

    let now = now_ns();
    let updates = sync_bytes(vec![
        root_key_update(vec![1; 42], now),
        root_key_update(vec![2; 42], now + 1),
    ]);
    alix2
        .group(&group.group_id)?
        .send_message(&updates, SendMessageOpts::default())
        .await?;
    alix1.group(&group.group_id)?.sync().await?;

    let events = alix1
        .context
        .events()
        .subscribe_app(EventFilter::new([EventKind::HmacKeysUpdated]))?;
    let db = alix1.context.db();
    alix1
        .device_sync_client()
        .process_sync_group_messages()
        .await?;

    assert_eq!(
        StoredUserPreferences::load(&db)?.hmac_key,
        Some(vec![2; 42].into())
    );
    assert!(matches!(
        events.drain().as_slice(),
        [EventEnvelope {
            client: Some(ClientEvent::HmacKeysUpdated(_)),
            ..
        }]
    ));
}

// The same event reaches the JSON destination and the public app log callback.
#[xmtp_common::test(unwrap_try = true)]
#[cfg(not(target_arch = "wasm32"))]
fn incoming_preference_logs_omit_secret_material() {
    use std::sync::Arc;
    use xmtp_logging::{Level, test_logging::LogCapture};

    let key = b"synthetic-private-sync-key-e7a619c3".to_vec();
    let entity = "synthetic-private-consent-identity";
    let updates = vec![
        root_key_update(key.clone(), 9_007_199_254_740_993),
        consent_update(ConsentType::InboxId, ConsentState::Denied, entity),
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
async fn stored_preference_logs_omit_secret_material() {
    use std::sync::Arc;
    use tracing::instrument::WithSubscriber;
    use xmtp_logging::{Level, test_logging::LogCapture};

    tester!(alix, disable_workers);
    let client = alix.device_sync_client();
    let group = client.get_sync_group().await?;
    let mut key = b"synthetic-private-sync-key-4bd3e901".to_vec();
    key.resize(42, 0xa7);
    let entity = "synthetic-private-consent-identifier";
    let updates = vec![
        root_key_update(key.clone(), now_ns() + xmtp_common::NS_IN_HOUR),
        consent_update(ConsentType::InboxId, ConsentState::Denied, entity),
    ];
    group
        .send_message(&sync_bytes(updates), SendMessageOpts::default())
        .await?;
    let db = alix.context.db();
    assert_eq!(db.unprocessed_sync_group_messages(None)?.len(), 1);
    let sink = Arc::new(Capture(parking_lot::Mutex::new(Vec::new())));
    let capture = LogCapture::with_sink(Level::Info, Some(sink.clone()));
    client
        .process_sync_group_messages()
        .with_subscriber(capture.dispatch())
        .await?;

    assert!(db.unprocessed_sync_group_messages(None)?.is_empty());
    assert_eq!(
        StoredUserPreferences::load(&db)?.hmac_key,
        Some(key.clone().into())
    );
    assert_eq!(
        held(&alix, &ConsentKey::inbox(entity)).unwrap().state,
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
    for sensitive in [format!("{key:?}"), hex::encode(&key), entity.into()] {
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

/// A peer marks everything owed in the transaction that installs a
/// newcomer's sync Welcome, and schedules the adds there, so a peer that
/// stops before its worker runs still sends it all when it runs again: a
/// time-0 record, an inbox record for a DM the newcomer holds no group for,
/// and the root key.
// verifies: SYNC-014, SYNC-035
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn newcomer_gets_everything_after_peer_restart() {
    use prost::Message;
    use xmtp_db::tasks::QueryTasks;
    use xmtp_proto::xmtp::mls::database::{Task as TaskProto, task::Task as TaskKind};

    tester!(alix1, disable_workers);
    tester!(bo, disable_workers);
    let group = alix1
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let records = [
        (ConsentKey::inbox("time-zero"), ConsentState::Allowed, 0),
        (ConsentKey::inbox(bo.inbox_id()), ConsentState::Denied, 5),
    ];
    for (key, state, at) in &records {
        decide_at(&alix1, key, *state, *at)?;
    }
    xmtp_db::user_preferences::ensure_root_key(&alix1.context.db())?;
    publish(&alix1).await?;
    assert!(!has_owed(&alix1.context.db())?);

    tester!(alix2, from: alix1, disable_workers);
    alix2.device_sync_client().get_sync_group().await?;
    alix1.sync_welcomes().await?;
    // The Welcome's transaction marked everything owed and scheduled the adds.
    assert!(has_owed(&alix1.context.db())?);
    let adds: Vec<_> = alix1
        .context
        .db()
        .get_tasks()?
        .iter()
        .filter_map(|t| match TaskProto::decode(t.data.as_slice()).ok()?.task {
            Some(TaskKind::AddMissingInstallations(add)) => Some(add.group_id),
            _ => None,
        })
        .collect();
    assert!(adds.contains(&group.group_id.to_vec()));

    // A client opened later on the same database holds nothing in memory.
    let reopened = crate::builder::ClientBuilder::from_client(alix1.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    reopened.device_sync_client().publish_outbox().await?;
    receive(&alix2).await?;
    for (key, ..) in &records {
        assert_eq!(held(&alix2, key), held(&alix1, key), "{}", key.entity);
    }
    let group_key = ConsentKey::group(group.group_id.as_ref());
    assert_eq!(held(&alix2, &group_key), held(&alix1, &group_key));
    // The newcomer may have made its own root key first; the greater one
    // becomes the account key once both have published.
    publish(&alix2).await?;
    receive(&alix1).await?;
    publish(&alix1).await?;
    receive(&alix2).await?;
    assert_eq!(
        StoredUserPreferences::load(alix2.context.db())?.hmac_key,
        StoredUserPreferences::load(alix1.context.db())?.hmac_key
    );
}

/// A peer that holds no root key makes one when it installs a newcomer's
/// sync Welcome, owes it, and reports it.
// verifies: SYNC-035, SYNC-026, EVENT-001
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sync_welcome_makes_a_root_key_when_none_is_held() {
    use xmtp_events::{ClientEvent, EventFilter, EventKind};
    tester!(alix1, disable_workers);
    assert!(
        StoredUserPreferences::load(alix1.context.db())?
            .hmac_key
            .is_none()
    );
    let events = alix1
        .context
        .events()
        .subscribe_app(EventFilter::new([EventKind::HmacKeysUpdated]))?;
    tester!(alix2, from: alix1, disable_workers);
    alix2.device_sync_client().get_sync_group().await?;
    alix1.sync_welcomes().await?;
    let preferences = StoredUserPreferences::load(alix1.context.db())?;
    assert!(preferences.hmac_key.is_some());
    assert!(preferences.hmac_key_owed_seq > 0);
    assert!(
        events
            .drain()
            .iter()
            .any(|event| matches!(event.client, Some(ClientEvent::HmacKeysUpdated(_))))
    );
}

/// A root key made in a write that rolls back is not reported: its events
/// go out only when the write that stores it commits.
// verifies: EVENT-001, SYNC-026
#[xmtp_common::test(unwrap_try = true)]
async fn root_key_made_in_a_rolled_back_write_is_not_reported() {
    use xmtp_db::TransactionOutcome;
    use xmtp_events::{EventFilter, EventKind};
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let db = alix.context.db();
    assert!(StoredUserPreferences::load(&db)?.hmac_key.is_none());
    let events = alix
        .context
        .events()
        .subscribe_app(EventFilter::new([EventKind::HmacKeysUpdated]))?;
    crate::state_tx::state_write_with_events(
        alix.context.mls_storage(),
        alix.context.events(),
        |tx, events| {
            group.hmac_keys_in(tx.storage().db(), events, 0..=0)?;
            Ok::<_, StorageError>(TransactionOutcome::<()>::Rollback)
        },
    )?;
    assert!(StoredUserPreferences::load(&db)?.hmac_key.is_none());
    assert!(events.drain().is_empty());
    // The same write, committed, reports the key once.
    group.hmac_keys(0..=0)?;
    assert!(StoredUserPreferences::load(&db)?.hmac_key.is_some());
    assert_eq!(events.drain().len(), 1);
}

/// A value learned from an older sync group is passed on to the selected
/// one, so it reaches a newcomer whose replay was already sent.
// verifies: SYNC-027
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn older_group_value_reaches_newcomer() {
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    join_sync_group(&alix2, &[&alix1]).await?;
    // alix2 blocks X in the older group and never runs again.
    let x = ConsentKey::inbox("x");
    decide_at(&alix2, &x, ConsentState::Denied, now_ns())?;
    publish(&alix2).await?;

    tester!(alix3, from: alix1, disable_workers);
    let newer = alix3.device_sync_client().get_sync_group().await?;
    alix1.sync_welcomes().await?;
    let selected = alix1.device_sync_client().primary_sync_group()?.unwrap();
    assert_eq!(selected.group_id, newer.group_id);
    publish(&alix1).await?;
    receive(&alix3).await?;
    assert!(held(&alix3, &x).is_none());

    // alix1 reads the older group and passes the value on.
    receive(&alix1).await?;
    assert!(alix1.context.db().consent_row(&x)?.unwrap().owed_seq > 0);
    publish(&alix1).await?;
    receive(&alix3).await?;
    assert_eq!(held(&alix3, &x), held(&alix2, &x));
}

/// After the clock is set back, a newcomer's sync group is still stored
/// after every held group and selected, so the replay reaches it.
// verifies: SYNC-031, SYNC-002
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn clock_set_back_still_selects_newcomer_group() {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;
    tester!(alix1, disable_workers);
    let older = alix1.device_sync_client().get_sync_group().await?;
    let ahead = now_ns() + 2 * NS_IN_HOUR;
    alix1.context.db().raw_query(|conn| {
        diesel::update(dsl::groups.find(&older.group_id))
            .set(dsl::created_at_ns.eq(ahead))
            .execute(conn)
    })?;
    tester!(alix2, from: alix1, disable_workers);
    let newer = alix2.device_sync_client().get_sync_group().await?;
    alix1.sync_welcomes().await?;
    let stored = alix1.context.db().find_group(&newer.group_id)?.unwrap();
    assert!(stored.created_at_ns > ahead);
    let selected = alix1.device_sync_client().primary_sync_group()?.unwrap();
    assert_eq!(selected.group_id, newer.group_id);
    let key = ConsentKey::inbox("set-back");
    decide_at(&alix1, &key, ConsentState::Allowed, now_ns())?;
    publish(&alix1).await?;
    receive(&alix2).await?;
    assert_eq!(held(&alix2, &key), held(&alix1, &key));
}

/// A page whose read-back is not published, because its confirmation failed
/// or the deployment refused it, clears no mark: the pass fails, and a later
/// pass sends the values again and only then clears them.
// verifies: SYNC-033
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn unpublished_page_stays_owed_until_sent_again() {
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    join_sync_group(&alix2, &[&alix1]).await?;
    let x = ConsentKey::inbox("unconfirmed");
    decide_at(&alix1, &x, ConsentState::Denied, now_ns())?;
    let owed = || {
        alix1
            .context
            .db()
            .consent_row(&x)
            .unwrap()
            .unwrap()
            .owed_seq
    };
    *UNCONFIRMED_NEXT_PAGE.lock() = Some(alix1.context.installation_id().to_vec());
    assert!(matches!(
        publish(&alix1).await,
        Err(DeviceSyncError::OutboxPageUnpublished)
    ));
    assert!(owed() > 0, "an unconfirmed page clears nothing");
    publish(&alix1).await?;
    assert_eq!(owed(), 0);
    receive(&alix2).await?;
    assert_eq!(held(&alix2, &x), held(&alix1, &x));
}

/// The revocation's root key is timed a day after the revocation's server
/// time, so a key the revoked installation sent before it, timed far ahead,
/// never replaces it, in either processing order.
// verifies: SYNC-026, SYNC-030, SYNC-032
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn revocation_key_beats_delayed_pre_revocation_key() {
    use crate::identity_updates::load_identity_updates_for_client;
    use xmtp_db::user_preferences::{RootKeyWrite, write_root_key};
    // alix1 cycles, then applies; alix2 is revoked; alix3 revokes; alix4
    // applies, then cycles.
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    tester!(alix3, from: alix1, disable_workers);
    tester!(alix4, from: alix1, disable_workers);
    alix1.device_sync_client().get_sync_group().await?;
    for client in [&alix2, &alix3, &alix4] {
        client.sync_welcomes().await?;
    }
    let k = [0x4c; 42];
    let ahead = RootKeyWrite::Received {
        key: &k,
        cycled_at_ns: now_ns() + 2 * NS_IN_DAY,
        bound_ns: i64::MAX,
    };
    write_root_key(&alix2.context.db(), ahead, Owed::Mark)?;
    publish(&alix2).await?;
    receive(&alix4).await?;
    let key_of =
        |client: &crate::utils::Tester| StoredUserPreferences::load(client.context.db()).unwrap();
    assert_eq!(key_of(&alix4).hmac_key, Some(k.to_vec().into()));

    revoke(&alix3, &alix2).await?;
    let revoked_at = alix3
        .context
        .db()
        .get_identity_updates(alix3.inbox_id(), None, None)?
        .last()
        .unwrap()
        .server_timestamp_ns;
    let db = alix1.context.db();
    load_identity_updates_for_client(&alix1.context, &db, &[alix1.inbox_id()]).await?;
    let cycled = key_of(&alix1);
    assert!(cycled.hmac_key_cycled_at_ns.unwrap() >= revoked_at + MAX_AHEAD_NS);
    receive(&alix1).await?;
    assert_eq!(key_of(&alix1).hmac_key, cycled.hmac_key);
    assert_ne!(cycled.hmac_key, Some(k.to_vec().into()));

    // alix4 applied C's key first, then loads the revocation and cycles past it.
    let db = alix4.context.db();
    load_identity_updates_for_client(&alix4.context, &db, &[alix4.inbox_id()]).await?;
    let after = key_of(&alix4);
    assert_ne!(after.hmac_key, Some(k.to_vec().into()));
    assert!(after.hmac_key_cycled_at_ns.unwrap() >= revoked_at + MAX_AHEAD_NS);
    // The revoked installation does not cycle on its own revocation.
    let db = alix2.context.db();
    load_identity_updates_for_client(&alix2.context, &db, &[alix2.inbox_id()]).await?;
    assert_eq!(key_of(&alix2).hmac_key, Some(k.to_vec().into()));
}

/// A reconcile task armed after a rate-limited add is due only once the
/// add's delay has passed, so the runner does not resend it at once.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_add_arms_its_reconcile_after_the_delay() {
    tester!(alix, disable_workers);
    let sync = alix.context.device_sync_client();
    let group = alix.create_group(None, None)?;
    // Longer than a task's default three-day expiry.
    let delay = xmtp_common::time::Duration::from_secs(4 * 24 * 60 * 60);
    let before = xmtp_common::time::now_ns();
    sync.schedule_add_missing_installations_task(group.group_id, Some(delay))?;
    let task = alix
        .context
        .db()
        .get_tasks()?
        .into_iter()
        .find(|task| {
            matches!(
                xmtp_proto::xmtp::mls::database::Task::decode(task.data.as_slice())
                    .ok()
                    .and_then(|task| task.task),
                Some(TaskKindProto::AddMissingInstallations(_))
            )
        })
        .expect("the reconcile task");
    assert!(task.next_attempt_at_ns >= before + delay.as_nanos() as i64);
    assert!(
        task.expires_at_ns > task.next_attempt_at_ns,
        "the task does not expire before it is due"
    );
}
