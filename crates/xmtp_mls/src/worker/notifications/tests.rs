use super::*;
use crate::{
    client::notifications::{NotificationChannel, NotificationOverride, NotificationState},
    tester,
    utils::test::MlsGroupExt,
};
use std::sync::Arc;
use xmtp_common::{
    StreamHandle,
    time::{Duration, timeout},
    wait_for_eq,
};
use xmtp_db::{consent_record::ConsentState, group::GroupMembershipState};

pub(crate) mod support;
use support::Call;

pub(crate) fn config() -> NotificationConfig {
    NotificationConfig::new(NotificationChannel::Http {
        url: "https://127.0.0.1/push".into(),
        signing_key: vec![7; 32],
    })
}

fn synthetic_desired(count: u128) -> Desired {
    (1..=count)
        .map(|index| {
            let id = GroupId::try_from(index.to_le_bytes().as_slice()).unwrap();
            (
                Topic::new_group_message(id).cloned_vec(),
                DesiredTopic {
                    group_id: Some(id),
                    include_commits: false,
                },
            )
        })
        .collect()
}

/// Drive the same diff and response transaction against a bounded scripted backend.
async fn send_synthetic_batch(
    client: &crate::Client<support::Context>,
    desired: &Desired,
) -> Result<(), NotificationError> {
    let db = client.context.db();
    let record = db.notification_record()?;
    let epoch = crate::utils::time::hmac_epoch();
    let (topics, removes) = diff(desired, &db.uploaded_topics()?, epoch, &[3; 32], false);
    let adds: Vec<_> = topics
        .into_iter()
        .map(|topic| UploadedTopic {
            topic,
            hmac_epoch_base: Some(epoch - 1),
            include_commits: false,
            root_key_fingerprint: vec![3; 32],
            stale: false,
        })
        .collect();
    let request = UpdateSubscriptionsRequest {
        recipient_id: record.push_recipient_id.unwrap(),
        recipient_secret: record.push_recipient_secret.unwrap(),
        adds: adds
            .iter()
            .map(|row| Subscription {
                topic: row.topic.clone(),
                hmac_epoch_base: epoch - 1,
                hmac_keys: vec![vec![1; 42]; 3],
                include_commits: false,
            })
            .collect(),
        removes: removes.clone(),
    };
    assert!(request.adds.len() + request.removes.len() <= REQUEST_TOPICS);
    let response = client.context.api().update_subscriptions(request).await?;
    confirm(
        &client.context,
        record.push_generation,
        &config(),
        &response,
        &adds,
        &removes,
    )?;
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_repair_reaches_2500_topics_in_three_turns() {
    let (client, peer) = support::client().await;
    client.enable_notifications(config()).await?;
    peer.state.lock().extra = 1;
    let desired = synthetic_desired(2500);
    send_synthetic_batch(&client, &desired).await?;
    assert_eq!(
        client
            .db()
            .uploaded_topics()?
            .iter()
            .filter(|row| row.stale)
            .count(),
        1000
    );
    send_synthetic_batch(&client, &desired).await?;
    send_synthetic_batch(&client, &desired).await?;
    assert_eq!(peer.calls(Call::Update), 3);
    assert_eq!(peer.state.lock().subscriptions.len(), 2500);
    assert_eq!(client.db().uploaded_topics()?.len(), 2500);
    assert!(client.db().notification_record()?.push_repairing);
    assert_eq!(
        client
            .db()
            .uploaded_topics()?
            .iter()
            .filter(|row| row.stale)
            .count(),
        500
    );
    send_synthetic_batch(&client, &desired).await?;
    assert!(!client.db().notification_record()?.push_repairing);
    assert!(client.db().uploaded_topics()?.iter().all(|row| !row.stale));
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_repair_stale_marks_survive_database_restore() {
    tester!(alix, disable_workers);
    let db = alix.db();
    let desired = synthetic_desired(2500);
    let mut rows: Vec<_> = desired
        .keys()
        .map(|topic| UploadedTopic {
            topic: topic.clone(),
            hmac_epoch_base: Some(crate::utils::time::hmac_epoch() - 1),
            include_commits: false,
            root_key_fingerprint: vec![3; 32],
            stale: true,
        })
        .collect();
    db.confirm_uploaded_topics(&rows, &[])?;
    let mut record = db.notification_record()?;
    record.push_state = 1;
    record.push_config = Some(encode(&config())?);
    record.push_repairing = true;
    db.save_notification_record(&record)?;
    for row in &mut rows[..1000] {
        row.stale = false;
    }
    confirm(
        &alix.context,
        0,
        &config(),
        &RecipientState {
            topic_count: 2501,
            channel: config().channel_id(),
            expires_at_ns: time::now_ns() + support::TEST_TTL_NS,
        },
        &rows[..1000],
        &[],
    )?;
    let snapshot = Arc::new(alix.db_snapshot());
    tester!(restored, snapshot: snapshot, disable_workers);
    assert!(restored.db().notification_record()?.push_repairing);
    let uploaded = restored.db().uploaded_topics()?;
    assert_eq!(uploaded.iter().filter(|row| row.stale).count(), 1500);
    let (next, _) = diff(
        &desired,
        &uploaded,
        crate::utils::time::hmac_epoch(),
        &[3; 32],
        false,
    );
    assert_eq!(next.len(), 1000);
    assert!(
        next.iter()
            .all(|topic| uploaded.iter().any(|row| &row.topic == topic && row.stale))
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_desired_set_uses_real_membership_and_each_rule() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    bo_group.update_consent_state(ConsentState::Allowed)?;
    let sync = crate::groups::MlsGroup::create_and_insert(
        alix.context.clone(),
        xmtp_proto::types::ConversationType::Sync,
        Default::default(),
        Default::default(),
        None,
    )?;
    let mut config = config();
    config.include_welcomes = false;
    let wanted = desired(&alix.context, &config)?;
    assert!(wanted.contains_key(&Topic::new_group_message(group.group_id).cloned_vec()));
    assert!(!wanted.contains_key(&Topic::new_group_message(sync.group_id).cloned_vec()));
    group.update_consent_state(ConsentState::Denied)?;
    assert!(desired(&alix.context, &config)?.is_empty());
    group.set_notifications(NotificationOverride::Enabled)?;
    assert_eq!(desired(&alix.context, &config)?.len(), 1);
    group.set_notifications(NotificationOverride::Disabled)?;
    config.consent_states = vec![ConsentState::Denied];
    assert!(desired(&alix.context, &config)?.is_empty());
    sync.set_notifications(NotificationOverride::Disabled)?;
    config.include_sync_groups = true;
    assert!(
        desired(&alix.context, &config)?
            .contains_key(&Topic::new_group_message(sync.group_id).cloned_vec())
    );
    group.remove_members(&[bo.inbox_id()]).await?;
    bo_group.sync().await?;
    assert!(!bo_group.is_active()?);
    bo_group.set_notifications(NotificationOverride::Enabled)?;
    assert!(
        !desired(&bo.context, &config)?
            .contains_key(&Topic::new_group_message(group.group_id).cloned_vec())
    );
    // The storage membership flag alone does not decide active MLS membership.
    assert_ne!(
        bo.db()
            .find_group(&group.group_id)?
            .unwrap()
            .membership_state,
        GroupMembershipState::Restored
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_reuploads_keys_flags_and_root_from_request_snapshot() {
    let (client, peer) = support::client().await;
    let group = client.create_group(None, None)?;
    let mut config = config();
    config.include_welcomes = false;
    client.enable_notifications(config.clone()).await?;
    run(&client.context).await?;
    let topic = Topic::new_group_message(group.group_id).cloned_vec();
    let uploaded = client.db().uploaded_topics()?;
    assert_eq!(uploaded.len(), 1);
    assert_eq!(
        peer.state.lock().subscriptions[&topic].hmac_keys,
        group
            .hmac_keys(-1..=1)?
            .into_iter()
            .map(|key| key.key.to_vec())
            .collect::<Vec<_>>()
    );
    let mut old_window = uploaded[0].clone();
    old_window.hmac_epoch_base = Some(crate::utils::time::hmac_epoch() - 2);
    client.db().confirm_uploaded_topics(&[old_window], &[])?;
    run(&client.context).await?;
    assert_eq!(peer.calls(Call::Update), 2);
    let old_root = client.db().uploaded_topics()?[0]
        .root_key_fingerprint
        .clone();
    StoredUserPreferences::store_hmac_key(&client.db(), &[8; 42], None)?;
    run(&client.context).await?;
    assert_ne!(
        client.db().uploaded_topics()?[0].root_key_fingerprint,
        old_root
    );
    config.include_commits = true;
    client.enable_notifications(config).await?;
    run(&client.context).await?;
    assert!(peer.state.lock().subscriptions[&topic].include_commits);
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_resource_exhaustion_suppresses_adds_but_keeps_removes() {
    let (client, peer) = support::client().await;
    let group = client.create_group(None, None)?;
    let mut config = config();
    config.include_welcomes = false;
    client.enable_notifications(config.clone()).await?;
    run(&client.context).await?;
    peer.state.lock().limit = Some(1);
    client.create_group(None, None)?;
    run(&client.context).await?;
    assert!(client.db().notification_record()?.push_suppressed.is_some());
    let calls = peer.calls(Call::Update);
    run(&client.context).await?;
    assert_eq!(peer.calls(Call::Update), calls);
    group.set_notifications(NotificationOverride::Disabled)?;
    run(&client.context).await?;
    assert_eq!(peer.state.lock().subscriptions.len(), 1);
    assert!(
        !peer
            .state
            .lock()
            .subscriptions
            .contains_key(&Topic::new_group_message(group.group_id).cloned_vec())
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_suppressed_batch_still_removes_without_a_rule_change() {
    let (client, peer) = support::client().await;
    let removed = client.create_group(None, None)?;
    let mut config = config();
    config.include_welcomes = false;
    client.enable_notifications(config).await?;
    run(&client.context).await?;
    peer.state.lock().limit = Some(1);
    client.create_group(None, None)?;
    let extra = client.create_group(None, None)?;
    removed.set_notifications(NotificationOverride::Disabled)?;

    // The combined delta exceeds the limit and leaves the old upload in place.
    run(&client.context).await?;
    let suppressed = client.db().notification_record()?.push_suppressed.unwrap();
    assert_eq!(peer.state.lock().subscriptions.len(), 1);
    run(&client.context).await?;
    assert!(peer.state.lock().subscriptions.is_empty());
    assert!(client.db().uploaded_topics()?.is_empty());
    assert_eq!(
        client.db().notification_record()?.push_suppressed,
        Some(suppressed)
    );
    let calls = peer.calls(Call::Update);
    run(&client.context).await?;
    assert_eq!(peer.calls(Call::Update), calls);

    // A rule change makes the desired set small enough and permits adds again.
    extra.set_notifications(NotificationOverride::Disabled)?;
    run(&client.context).await?;
    assert_eq!(peer.state.lock().subscriptions.len(), 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_renewal_recreation_and_not_found_repair() {
    let (client, peer) = support::client().await;
    client.create_group(None, None)?;
    client.enable_notifications(config()).await?;
    run(&client.context).await?;
    let before = client.db().uploaded_topics()?.len();
    peer.state.lock().subscriptions.clear();
    let mut record = client.db().notification_record()?;
    record.push_deadlines = Some(encode(&Deadlines::default())?);
    client.db().save_notification_record(&record)?;
    run(&client.context).await?;
    assert_eq!(peer.calls(Call::Register), 2);
    assert!(client.db().notification_record()?.push_repairing);
    assert_eq!(
        client
            .db()
            .uploaded_topics()?
            .iter()
            .filter(|row| row.stale)
            .count(),
        before
    );
    run(&client.context).await?;
    assert_eq!(peer.state.lock().subscriptions.len(), before);
    peer.state.lock().registered = false;
    client.create_group(None, None)?;
    run(&client.context).await?;
    assert!(client.db().uploaded_topics()?.is_empty());
    assert!(deadlines(&client.db().notification_record()?)?.registration_owed);
    let updates = peer.calls(Call::Update);
    run(&client.context).await?;
    assert_eq!(peer.calls(Call::Register), 3);
    assert_eq!(peer.calls(Call::Update), updates);
    run(&client.context).await?;
    assert_eq!(peer.state.lock().subscriptions.len(), before + 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_stale_failure_cannot_fail_a_new_configuration() {
    let (client, peer) = support::client().await;
    client.enable_notifications(config()).await?;
    let mut record = client.db().notification_record()?;
    record.push_deadlines = Some(encode(&Deadlines::default())?);
    client.db().save_notification_record(&record)?;
    {
        let mut server = peer.state.lock();
        server.pause_next = true;
        server.next_error = Some(tonic::Code::InvalidArgument);
    }
    let context = client.context.clone();
    let task = xmtp_common::spawn(None, async move { run(&context).await });
    timeout(Duration::from_secs(5), peer.entered.notified()).await?;
    let changed_client = client.clone();
    let mut replacement = config();
    replacement.metadata = b"new configuration".to_vec();
    let enable = xmtp_common::spawn(None, async move {
        changed_client.enable_notifications(replacement).await
    });
    wait_for_eq(
        || async { client.db().notification_record().unwrap().push_generation },
        record.push_generation + 1,
    )
    .await?;
    peer.release.notify_one();
    task.join().await??;
    enable.join().await??;
    assert!(matches!(
        client.notification_state()?,
        NotificationState::Enabled
    ));
    assert!(
        client
            .db()
            .notification_record()?
            .push_failed_error
            .is_none()
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_task_is_durable_coalesced_and_retries_after_sixty_seconds() {
    use crate::worker::tasks::TaskWorker;
    let (client, peer) = support::client().await;
    client.enable_notifications(config()).await?;
    for _ in 0..20 {
        wake(&client.context)?;
    }
    let tasks = client.db().get_tasks()?;
    let rows: Vec<_> = tasks
        .into_iter()
        .filter(|row| row.data_hash == task_hash().as_ref())
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].expires_at_ns, i64::MAX);
    assert_eq!(rows[0].max_attempts, i32::MAX);
    peer.state.lock().next_error = Some(tonic::Code::Unavailable);
    let before = time::now_ns();
    TaskWorker::run_and_reschedule_task(rows[0].clone(), &client.context).await?;
    let row = client
        .db()
        .get_tasks()?
        .into_iter()
        .find(|row| row.data_hash == task_hash().as_ref())
        .unwrap();
    assert_eq!(row.attempts, 1);
    assert!(row.next_attempt_at_ns >= before + RETRY_NS);
    assert!(row.next_attempt_at_ns <= time::now_ns() + RETRY_NS);
    let retry_at = row.next_attempt_at_ns;
    let calls = peer.calls(Call::Update);
    for _ in 0..20 {
        wake(&client.context)?;
    }
    let row = client
        .db()
        .get_tasks()?
        .into_iter()
        .find(|row| row.data_hash == task_hash().as_ref())
        .unwrap();
    assert_eq!(row.next_attempt_at_ns, retry_at);
    TaskWorker::run_and_reschedule_task(row.clone(), &client.context).await?;
    assert_eq!(peer.calls(Call::Update), calls);
    // Make the stored retry due without waiting for wall-clock time.
    client
        .db()
        .update_task(row.id, row.attempts, before, before)?;
    let mut due = row;
    due.next_attempt_at_ns = before;
    TaskWorker::run_and_reschedule_task(due, &client.context).await?;
    let row = client
        .db()
        .get_tasks()?
        .into_iter()
        .find(|row| row.data_hash == task_hash().as_ref())
        .unwrap();
    assert_eq!(row.attempts, 0);
    let calls = peer.calls(Call::Update);
    let before = time::now_ns();
    let TaskOutcome::RescheduleAt(next) = run(&client.context).await? else {
        panic!("enabled task must remain");
    };
    assert_eq!(peer.calls(Call::Update), calls);
    assert!(next >= before + NS_IN_HOUR);
    client.db().update_task(row.id, 0, before, next)?;
    wake(&client.context)?;
    let advanced = client
        .db()
        .get_tasks()?
        .into_iter()
        .find(|task| task.data_hash == task_hash().as_ref())
        .unwrap();
    assert!(advanced.next_attempt_at_ns < next);
    // A missed hint is repaired by the same local scan when the task next runs.
    client.create_group(None, None)?;
    run(&client.context).await?;
    assert_eq!(peer.calls(Call::Update), calls + 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_stale_upload_is_removed_after_configuration_change() {
    let (client, peer) = support::client().await;
    let group = client.create_group(None, None)?;
    let topic = Topic::new_group_message(group.group_id).cloned_vec();
    client.enable_notifications(config()).await?;
    let generation = client.db().notification_record()?.push_generation;
    peer.state.lock().pause_next = true;
    let context = client.context.clone();
    let upload = xmtp_common::spawn(None, async move { run(&context).await });
    timeout(Duration::from_secs(5), peer.entered.notified()).await?;

    let other = client.clone();
    let mut replacement = config();
    replacement.consent_states.clear();
    replacement.include_welcomes = false;
    let enable = xmtp_common::spawn(None, async move {
        other.enable_notifications(replacement).await
    });
    wait_for_eq(
        || async { client.db().notification_record().unwrap().push_generation },
        generation + 1,
    )
    .await?;
    peer.release.notify_one();
    upload.join().await??;
    enable.join().await??;

    assert!(peer.state.lock().subscriptions.contains_key(&topic));
    assert!(
        client
            .db()
            .uploaded_topics()?
            .iter()
            .any(|row| row.topic == topic)
    );
    run(&client.context).await?;
    assert!(peer.state.lock().subscriptions.is_empty());
    assert!(client.db().uploaded_topics()?.is_empty());
    assert_eq!(peer.calls(Call::Update), 2);
    assert!(matches!(
        client.notification_state()?,
        NotificationState::Enabled
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_disable_discards_an_in_flight_success() {
    let (client, peer) = support::client().await;
    client.create_group(None, None)?;
    client.enable_notifications(config()).await?;
    peer.state.lock().pause_next = true;
    let context = client.context.clone();
    let upload = xmtp_common::spawn(None, async move { run(&context).await });
    timeout(Duration::from_secs(5), peer.entered.notified()).await?;
    let other = client.clone();
    let disable = xmtp_common::spawn(None, async move { other.disable_notifications().await });
    wait_for_eq(
        || async { client.db().notification_record().unwrap().push_state },
        0,
    )
    .await?;
    peer.release.notify_one();
    assert_eq!(upload.join().await??, TaskOutcome::Done);
    disable.join().await??;
    assert!(client.db().uploaded_topics()?.is_empty());
    assert!(client.db().notification_record()?.push_last_state.is_none());
    assert_eq!(peer.calls(Call::Unregister), 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_queued_disable_cannot_unregister_a_newer_enable() {
    let (client, peer) = support::client().await;
    client.enable_notifications(config()).await?;
    let generation = client.db().notification_record()?.push_generation;
    peer.state.lock().pause_next = true;
    let context = client.context.clone();
    let upload = xmtp_common::spawn(None, async move { run(&context).await });
    timeout(Duration::from_secs(5), peer.entered.notified()).await?;

    let other = client.clone();
    let disable = xmtp_common::spawn(None, async move { other.disable_notifications().await });
    wait_for_eq(
        || async { client.db().notification_record().unwrap().push_generation },
        generation + 1,
    )
    .await?;
    let other = client.clone();
    let mut replacement = config();
    replacement.metadata = b"newer configuration".to_vec();
    let enable = xmtp_common::spawn(None, async move {
        other.enable_notifications(replacement).await
    });
    wait_for_eq(
        || async { client.db().notification_record().unwrap().push_generation },
        generation + 2,
    )
    .await?;
    peer.release.notify_one();
    upload.join().await??;
    disable.join().await??;
    enable.join().await??;

    assert_eq!(peer.calls(Call::Unregister), 0);
    assert!(peer.state.lock().registered);
    let record = client.db().notification_record()?;
    assert_eq!(
        decode::<NotificationConfig>(record.push_config.as_deref().unwrap())?.metadata,
        b"newer configuration"
    );
    assert!(matches!(
        client.notification_state()?,
        NotificationState::Enabled
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_request_keeps_the_root_key_snapshot() {
    let (client, peer) = support::client().await;
    let group = client.create_group(None, None)?;
    client.enable_notifications(config()).await?;
    peer.state.lock().pause_next = true;
    let context = client.context.clone();
    let upload = xmtp_common::spawn(None, async move { run(&context).await });
    timeout(Duration::from_secs(5), peer.entered.notified()).await?;
    let previous = StoredUserPreferences::load(client.db())?.hmac_key.unwrap();
    StoredUserPreferences::store_hmac_key(&client.db(), &[9; 42], None)?;
    peer.release.notify_one();
    upload.join().await??;
    let topic = Topic::new_group_message(group.group_id).cloned_vec();
    let row = client
        .db()
        .uploaded_topics()?
        .into_iter()
        .find(|row| row.topic == topic)
        .unwrap();
    assert_eq!(
        row.root_key_fingerprint,
        xmtp_common::sha256_array(&previous)
    );
    run(&client.context).await?;
    let row = client
        .db()
        .uploaded_topics()?
        .into_iter()
        .find(|row| row.topic == topic)
        .unwrap();
    assert_eq!(
        row.root_key_fingerprint,
        xmtp_common::sha256_array(&[9; 42])
    );
    assert_eq!(peer.calls(Call::Update), 2);
}

xmtp_common::if_native! {
    #[xmtp_common::test(unwrap_try = true)]
    async fn notification_stalled_request_bounds_runner_delay_and_keeps_messaging_live() {
        use crate::worker::{key_package_maintenance as kp, tasks::TaskWorker};
        let (client, peer) = support::client().await;
        tester!(bo, disable_workers);
        let group = client.create_group(None, None)?;
        client.enable_notifications(config()).await?;
        client.db().queue_key_package_rotation()?;
        let now = time::now_ns();
        client.db().create_or_ignore_task(kp::kp_seed(kp::kp_rotation_proto(), now + 5 * NS_IN_SEC)?)?;
        peer.state.lock().pause_next = true;
        let context = client.context.clone();
        let runner = xmtp_common::spawn(None, async move { TaskWorker::new(context).run().await });
        timeout(Duration::from_secs(5), peer.entered.notified()).await?;
        let entered_at = std::time::Instant::now();
        // Network waits cannot hold the database writer or the group state lock.
        timeout(Duration::from_secs(5), async {
            group.update_consent_state(ConsentState::Denied)?;
            group.send_message(b"notification wait does not block messages", Default::default()).await?;
            let invited = bo.create_group(None, None)?;
            invited.add_members(&[client.inbox_id()]).await?;
            let welcomes = client.sync_welcomes().await?;
            assert!(welcomes.iter().any(|item| item.group_id == invited.group_id));
            Ok::<_, crate::groups::GroupError>(())
        }).await??;
        assert!(client.db().next_key_package_rotation_ns()?.unwrap() < now + NS_IN_HOUR);
        timeout(Duration::from_secs(32), async {
            loop {
                if client.db().next_key_package_rotation_ns().unwrap().unwrap() > now + NS_IN_HOUR { break; }
                time::sleep(Duration::from_millis(50)).await;
            }
        }).await?;
        assert!(entered_at.elapsed() < Duration::from_secs(35));
        assert!(peer.calls(Call::Publish) > 0);
        runner.abort_handle().end();
    }
}
