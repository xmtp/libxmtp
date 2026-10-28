use super::*;
use crate::{
    client::notifications::{
        NotificationChannel, NotificationOverride, NotificationState, encode_config,
    },
    tester,
    utils::test::MlsGroupExt,
};
use std::sync::Arc;
use xmtp_common::{
    RetryableError, StreamHandle,
    time::{Duration, timeout},
    wait_for_eq,
};
use xmtp_db::{consent_record::ConsentState, group::GroupMembershipState};

pub(crate) mod support;
use support::Call;

pub(crate) fn config() -> NotificationConfig {
    NotificationConfig::new(NotificationChannel::Http {
        url: "https://127.0.0.1/push".into(),
        signing_key: vec![7; 32].into(),
    })
}

// verifies: EVENT-001, EVENT-010, EVENT-024
#[xmtp_common::test(unwrap_try = true)]
async fn stored_terminal_notification_failure_emits_once() {
    use xmtp_events::{ClientEvent, EventFilter, EventKind};

    tester!(alix, disable_workers);
    let mut record = alix.db().notification_record()?;
    record.push_state = 1;
    record.push_generation += 1;
    alix.db().save_notification_record(&record)?;
    let events = alix
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::NotificationsFailed]), Some(4));
    assert!(record_error(
        &alix.context,
        record.push_generation,
        &NotificationError::PermissionDenied,
        None,
    )?);
    assert!(!record_error(
        &alix.context,
        record.push_generation,
        &NotificationError::PermissionDenied,
        None,
    )?);
    assert_eq!(alix.db().notification_record()?.push_state, 2);
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::NotificationsFailed(failure)), ..
        }] if failure.cause == NotificationError::PermissionDenied.to_string()
    ));
}

// verifies: EVENT-001, EVENT-010, EVENT-024
#[xmtp_common::test(unwrap_try = true)]
async fn background_notification_failure_emits_once() {
    use xmtp_events::{ClientEvent, EventFilter, EventKind};

    let (client, peer) = support::client().await;
    client.enable_notifications(config()).await?;
    let events = client
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::NotificationsFailed]), Some(4));
    peer.state.lock().next_error = Some(tonic::Code::InvalidArgument);
    assert_eq!(run(&client.context).await?, TaskOutcome::Done);
    assert_eq!(client.db().notification_record()?.push_state, 2);
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::NotificationsFailed(failure)), ..
        }] if failure.cause == NotificationError::InvalidArgument.to_string()
    ));
    assert_eq!(run(&client.context).await?, TaskOutcome::Done);
    assert!(events.drain().is_empty());
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

#[xmtp_common::test(unwrap_try = true)]
async fn notification_repair_reaches_2500_topics_in_three_turns() {
    use crate::worker::tasks::TaskWorker;
    let (client, peer) = support::client().await;
    // The welcome topic and 2,499 real MLS groups form the desired set.
    for _ in 0..2499 {
        client.create_group(None, None)?;
    }
    client.enable_notifications(config()).await?;
    peer.state.lock().extra = 1;
    wake(&client.context)?;
    let next_task = || {
        client
            .db()
            .get_tasks()
            .unwrap()
            .into_iter()
            .find(|task| task.data_hash == task_hash().as_ref())
            .unwrap()
    };
    TaskWorker::run_and_reschedule_task(next_task(), &client.context).await?;
    assert_eq!(
        client
            .db()
            .uploaded_topics()?
            .iter()
            .filter(|row| row.stale)
            .count(),
        1000
    );
    TaskWorker::run_and_reschedule_task(next_task(), &client.context).await?;
    TaskWorker::run_and_reschedule_task(next_task(), &client.context).await?;
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
    TaskWorker::run_and_reschedule_task(next_task(), &client.context).await?;
    assert!(!client.db().notification_record()?.push_repairing);
    assert!(client.db().uploaded_topics()?.iter().all(|row| !row.stale));
    assert_eq!(
        peer.state.lock().batches,
        vec![(1000, 0), (1000, 0), (1000, 0), (500, 0)]
    );
    TaskWorker::run_and_reschedule_task(next_task(), &client.context).await?;
    assert_eq!(peer.calls(Call::Update), 4);
    assert_eq!(next_task().attempts, 0);
    assert!(next_task().next_attempt_at_ns > time::now_ns());
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
    record.push_config = Some(encode_config(&config())?);
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
            .iter()
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
    assert!(client.db().notification_record()?.push_suppressed.is_none());
    assert!(
        !peer
            .state
            .lock()
            .subscriptions
            .contains_key(&Topic::new_group_message(group.group_id).cloned_vec())
    );

    // Returning to the old desired set must make a new request. Its old
    // fingerprint cannot stay suppressed after the successful update above.
    let calls = peer.calls(Call::Update);
    group.set_notifications(NotificationOverride::Enabled)?;
    run(&client.context).await?;
    assert_eq!(peer.calls(Call::Update), calls + 1);
    assert!(client.db().notification_record()?.push_suppressed.is_some());
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
    assert!(client.db().notification_record()?.push_suppressed.is_none());
    let calls = peer.calls(Call::Update);
    run(&client.context).await?;
    assert_eq!(peer.calls(Call::Update), calls + 1);
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
    assert!(client.db().notification_record()?.push_suppressed.is_none());
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
    replacement.include_commits = true;
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

/// A rate-limited add is not the topic limit: the state stays enabled, adds
/// are not suppressed, and no request is sent before the delay ends.
// verifies: PUSH-267, RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_rate_limit_keeps_state_enabled_and_waits_the_delay() {
    use crate::worker::tasks::TaskWorker;
    let delay = Duration::from_secs(120);
    let mapped = NotificationError::from(xmtp_api::dyn_err(
        xmtp_proto::api::ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(
            xmtp_api_grpc::error::rate_limited_status("push", delay),
        )),
    ));
    assert!(matches!(mapped, NotificationError::Api(_)));
    assert!(mapped.is_retryable());

    let (client, peer) = support::client().await;
    let mut config = config();
    config.include_welcomes = false;
    client.enable_notifications(config).await?;
    run(&client.context).await?;
    client.create_group(None, None)?;
    wake(&client.context)?;
    let task = || {
        client
            .db()
            .get_tasks()
            .unwrap()
            .into_iter()
            .find(|row| row.data_hash == task_hash().as_ref())
            .unwrap()
    };
    let mut due = task();
    due.next_attempt_at_ns = 0;
    peer.state.lock().rate_limit = Some((Call::Update, delay));
    let calls = peer.calls(Call::Update);
    let before = time::now_ns();
    TaskWorker::run_and_reschedule_task(due, &client.context).await?;
    assert_eq!(peer.calls(Call::Update), calls + 1);
    let row = task();
    assert_eq!(row.attempts, 1);
    assert!(row.next_attempt_at_ns >= before + delay.as_nanos() as i64);
    assert!(matches!(
        client.notification_state()?,
        NotificationState::Enabled
    ));
    let record = client.db().notification_record()?;
    assert!(record.push_suppressed.is_none());
    assert!(record.push_failed_error.is_none());

    // A wake that makes the task run early still sends nothing inside the delay.
    let TaskOutcome::RescheduleAt(next) = run(&client.context).await? else {
        panic!("enabled task must remain");
    };
    assert_eq!(peer.calls(Call::Update), calls + 1);
    assert!(next >= before + delay.as_nanos() as i64);
}

/// A rate limit maps to the retryable `Api` error, also through the preflight
/// wrapper. The detail on another code keeps that code's PUSH-267 row.
// verifies: PUSH-267, RATE-041
#[xmtp_common::test(unwrap_try = true)]
fn notification_rate_limits_map_to_api() {
    use xmtp_api_grpc::error::{GrpcError, rate_limited_status};
    use xmtp_proto::api::ApiClientError;
    let delay = Duration::from_secs(5);
    let preflight = xmtp_api::ApiError::Preflight(xmtp_api::preflight::PreflightError::new(
        ApiClientError::client(GrpcError::Status(rate_limited_status("request", delay))),
    ));
    let mapped = NotificationError::from(preflight);
    assert!(matches!(mapped, NotificationError::Api(_)));
    assert_eq!(mapped.rate_limit_delay(), Some(delay));

    let limited = rate_limited_status("request", delay);
    let denied = tonic::Status::with_details(
        tonic::Code::PermissionDenied,
        limited.message(),
        limited.details().to_vec().into(),
    );
    let mapped = NotificationError::from(xmtp_api::dyn_err(ApiClientError::client(
        GrpcError::Status(denied),
    )));
    assert!(matches!(mapped, NotificationError::PermissionDenied));
}

/// A refused Unregister records its delay even though the client is now
/// disabled, so the task waits it out after a later enable.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_a_refused_unregister_records_its_delay() {
    let (client, peer) = support::client().await;
    client.enable_notifications(config()).await?;
    peer.state.lock().rate_limit = Some((Call::Unregister, Duration::from_secs(600)));
    assert!(client.disable_notifications().await.is_err());
    let record = client.db().notification_record()?;
    assert!(deadlines(&record)?.registration_rate_limited_until_ns > time::now_ns());
}

/// A hold that cannot be stored does not replace the refusal: the caller
/// still sees the rate limit and its delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_a_failed_hold_write_keeps_the_rate_limit() {
    let (client, peer) = support::client().await;
    client.enable_notifications(config()).await?;
    let delay = Duration::from_secs(600);
    {
        let mut state = peer.state.lock();
        state.rate_limit = Some((Call::Unregister, delay));
        state.pause_next = true;
    }
    let disabling = client.clone();
    let disable = xmtp_common::spawn(None, async move { disabling.disable_notifications().await });
    timeout(Duration::from_secs(5), peer.entered.notified()).await?;
    let mut record = client.db().notification_record()?;
    record.push_deadlines = Some(vec![0xff]);
    client.db().save_notification_record(&record)?;
    peer.release.notify_one();
    let error = disable.join().await?.unwrap_err();
    assert_eq!(error.rate_limit_delay(), Some(delay));
}

/// A Register hold that cannot be stored does not replace the refusal: the
/// app still sees the rate limit and its delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_a_failed_registration_hold_write_keeps_the_rate_limit() {
    let (client, peer) = support::client().await;
    let delay = Duration::from_secs(600);
    {
        let mut state = peer.state.lock();
        state.rate_limit = Some((Call::Register, delay));
        state.pause_next = true;
    }
    let enabling = client.clone();
    let enable = xmtp_common::spawn(None, async move {
        enabling.enable_notifications(config()).await
    });
    timeout(Duration::from_secs(5), peer.entered.notified()).await?;
    let mut record = client.db().notification_record()?;
    record.push_deadlines = Some(vec![0xff]);
    client.db().save_notification_record(&record)?;
    peer.release.notify_one();
    let error = enable.join().await?.unwrap_err();
    assert_eq!(error.rate_limit_delay(), Some(delay));
}

/// An update hold does not postpone a registration renewal that falls due
/// inside it: the task wakes for the renewal.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_an_update_hold_keeps_the_renewal_due() {
    let (client, peer) = support::client().await;
    let mut config = config();
    config.include_welcomes = false;
    client.enable_notifications(config).await?;
    run(&client.context).await?;
    client.create_group(None, None)?;
    peer.state.lock().rate_limit = Some((Call::Update, Duration::from_secs(600)));
    assert!(run(&client.context).await.is_err());
    let renewal = time::now_ns() + 10 * xmtp_common::NS_IN_SEC;
    let mut record = client.db().notification_record()?;
    let mut held = deadlines(&record)?;
    held.renewal_ns = renewal;
    record.push_deadlines = Some(encode(&held)?);
    client.db().save_notification_record(&record)?;
    let TaskOutcome::RescheduleAt(at) = run(&client.context).await? else {
        panic!("the task waits");
    };
    assert!(
        at <= renewal,
        "the renewal is not postponed by the update hold"
    );
}

/// A refused owed Unregister holds registration only: once an enable has
/// registered, the task still sends subscription updates.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_a_registration_hold_leaves_updates_running() {
    let (client, peer) = support::client().await;
    client.enable_notifications(config()).await?;
    run(&client.context).await?;
    peer.state.lock().rate_limit = Some((Call::Unregister, Duration::from_secs(600)));
    assert!(client.disable_notifications().await.is_err());
    client.enable_notifications(config()).await?;
    client.create_group(None, None)?;
    wake(&client.context)?;
    let updates = peer.calls(Call::Update);
    run(&client.context).await?;
    assert_eq!(peer.calls(Call::Update), updates + 1);
}

/// An update hold ends with its delay: the next turn after it sends.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_an_update_hold_ends_with_its_delay() {
    let (client, peer) = support::client().await;
    let mut config = config();
    config.include_welcomes = false;
    client.enable_notifications(config).await?;
    run(&client.context).await?;
    client.create_group(None, None)?;
    peer.state.lock().rate_limit = Some((Call::Update, Duration::from_secs(600)));
    assert!(run(&client.context).await.is_err());
    let updates = peer.calls(Call::Update);
    assert!(matches!(
        run(&client.context).await?,
        TaskOutcome::RescheduleAt(_)
    ));
    assert_eq!(peer.calls(Call::Update), updates, "held");
    // The delay ends.
    let mut record = client.db().notification_record()?;
    let mut held = deadlines(&record)?;
    held.rate_limited_until_ns = time::now_ns() - 1;
    record.push_deadlines = Some(encode(&held)?);
    client.db().save_notification_record(&record)?;
    run(&client.context).await?;
    assert_eq!(peer.calls(Call::Update), updates + 1);
}

/// Enabling again, or disabling and enabling, does not end a pending delay:
/// the task still sends no subscription update inside it.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_reenable_keeps_a_pending_rate_limit_delay() {
    let delay = Duration::from_secs(600);
    let (client, peer) = support::client().await;
    let mut config = config();
    config.include_welcomes = false;
    client.enable_notifications(config.clone()).await?;
    run(&client.context).await?;
    client.create_group(None, None)?;
    peer.state.lock().rate_limit = Some((Call::Update, delay));
    let before = time::now_ns();
    assert!(run(&client.context).await.is_err());
    let updates = peer.calls(Call::Update);

    client.enable_notifications(config.clone()).await?;
    wake(&client.context)?;
    let TaskOutcome::RescheduleAt(next) = run(&client.context).await? else {
        panic!("enabled task must remain");
    };
    assert_eq!(peer.calls(Call::Update), updates);
    assert!(next >= before + delay.as_nanos() as i64);

    client.disable_notifications().await?;
    client.enable_notifications(config).await?;
    wake(&client.context)?;
    let TaskOutcome::RescheduleAt(next) = run(&client.context).await? else {
        panic!("enabled task must remain");
    };
    assert_eq!(peer.calls(Call::Update), updates);
    assert!(next >= before + delay.as_nanos() as i64);
}

/// The owed reset and the registration share one 30-second deadline. A
/// registration refused with a delay that ends past it returns the rate limit
/// at once, instead of waiting until the turn times out and losing the delay.
// verifies: RATE-041, PUSH-267
#[xmtp_common::test(unwrap_try = true)]
async fn notification_registration_after_a_slow_reset_keeps_its_rate_limit() {
    let delay = Duration::from_secs(28);
    let (client, peer) = support::client().await;
    client.enable_notifications(config()).await?;
    peer.state.lock().next_error = Some(tonic::Code::Unavailable);
    assert!(client.disable_notifications().await.is_err());
    assert!(client.db().notification_record()?.push_unregister_owed);
    {
        let mut state = peer.state.lock();
        state.slow = Some((Call::Unregister, Duration::from_secs(3)));
        state.rate_limit = Some((Call::Register, delay));
    }
    let started = xmtp_common::time::Instant::now();
    let error = client.enable_notifications(config()).await.unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(20));
    assert!(matches!(error, NotificationError::Api(_)));
    assert_eq!(error.rate_limit_delay(), Some(delay));
    let record = client.db().notification_record()?;
    assert!(deadlines(&record)?.registration_rate_limited_until_ns > time::now_ns());
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_successful_reenable_resets_old_retry_backoff() {
    use crate::worker::tasks::TaskWorker;
    let (client, peer) = support::client().await;
    client.create_group(None, None)?;
    client.enable_notifications(config()).await?;
    wake(&client.context)?;
    let task = || {
        client
            .db()
            .get_tasks()
            .unwrap()
            .into_iter()
            .find(|row| row.data_hash == task_hash().as_ref())
            .unwrap()
    };
    peer.state.lock().next_error = Some(tonic::Code::Unavailable);
    TaskWorker::run_and_reschedule_task(task(), &client.context).await?;
    assert_eq!(task().attempts, 1);
    assert!(peer.state.lock().subscriptions.is_empty());
    // Simulate an existing long retry without waiting through each failure.
    let retry_at = time::now_ns() + NS_IN_HOUR;
    let row = task();
    client
        .db()
        .update_task(row.id, 5, row.last_attempted_at_ns, retry_at)?;

    // Neither ordinary hints nor an unsuccessful enable bypass the retry.
    wake(&client.context)?;
    assert_eq!(task().next_attempt_at_ns, retry_at);
    peer.state.lock().next_error = Some(tonic::Code::Unavailable);
    assert!(client.enable_notifications(config()).await.is_err());
    wake(&client.context)?;
    assert_eq!(task().attempts, 5);
    assert_eq!(task().next_attempt_at_ns, retry_at);

    client.enable_notifications(config()).await?;
    // The runner consumes the successful registration hint.
    wake(&client.context)?;
    assert_eq!(task().attempts, 0);
    assert!(task().next_attempt_at_ns <= time::now_ns());
    TaskWorker::run_and_reschedule_task(task(), &client.context).await?;
    assert_eq!(peer.state.lock().subscriptions.len(), 2);
    assert_eq!(client.db().uploaded_topics()?.len(), 2);
    assert_eq!(peer.calls(Call::Update), 2);
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_later_failure_cancels_registration_retry_reset() {
    use crate::worker::tasks::TaskWorker;
    let (client, peer) = support::client().await;
    client.enable_notifications(config()).await?;
    wake(&client.context)?;
    client.enable_notifications(config()).await?;
    // A due timer can run before the runner consumes the registration hint.
    let row = client
        .db()
        .get_tasks()?
        .into_iter()
        .find(|row| row.data_hash == task_hash().as_ref())
        .unwrap();
    peer.state.lock().next_error = Some(tonic::Code::Unavailable);
    TaskWorker::run_and_reschedule_task(row, &client.context).await?;
    let failed = client
        .db()
        .get_tasks()?
        .into_iter()
        .find(|row| row.data_hash == task_hash().as_ref())
        .unwrap();
    assert_eq!(failed.attempts, 1);
    wake(&client.context)?;
    let after = client
        .db()
        .get_tasks()?
        .into_iter()
        .find(|row| row.data_hash == task_hash().as_ref())
        .unwrap();
    assert_eq!(after.attempts, failed.attempts);
    assert_eq!(after.next_attempt_at_ns, failed.next_attempt_at_ns);
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_revocation_discards_a_prepared_batch() {
    for override_change in [false, true] {
        let (client, peer) = support::client().await;
        let group = client.create_group(None, None)?;
        let mut config = config();
        config.include_welcomes = false;
        client.enable_notifications(config).await?;
        let changed_group = group.clone();
        let _hook = support::on_before_request(move || {
            if override_change {
                changed_group
                    .set_notifications(NotificationOverride::Disabled)
                    .unwrap();
            } else {
                changed_group
                    .update_consent_state(ConsentState::Denied)
                    .unwrap();
            }
        });
        let TaskOutcome::RescheduleAt(next) = run(&client.context).await? else {
            panic!("a changed desired set must be scanned again");
        };
        assert!(next <= time::now_ns());
        assert!(!group.notifications_enabled()?);
        assert_eq!(peer.calls(Call::Update), 0);
        assert!(client.db().uploaded_topics()?.is_empty());
        run(&client.context).await?;
        assert_eq!(peer.calls(Call::Update), 0);
        assert!(peer.state.lock().subscriptions.is_empty());
    }
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

// verifies: PUSH-266
#[xmtp_common::test(unwrap_try = true)]
async fn notification_disable_during_a_reset_cannot_register_again() {
    let (client, peer) = support::client().await;
    client.enable_notifications(config()).await?;
    peer.state.lock().next_error = Some(tonic::Code::Unavailable);
    assert!(
        client
            .disable_notifications()
            .await
            .unwrap_err()
            .is_retryable()
    );

    let before_enable = peer.state.lock().calls.len();
    peer.state.lock().pause_next = true;
    let other = client.clone();
    let enable = xmtp_common::spawn(
        None,
        async move { other.enable_notifications(config()).await },
    );
    timeout(Duration::from_secs(5), peer.entered.notified()).await?;
    let other = client.clone();
    let disable = xmtp_common::spawn(None, async move { other.disable_notifications().await });
    wait_for_eq(
        || async { client.db().notification_record().unwrap().push_state },
        0,
    )
    .await?;
    peer.release.notify_one();
    assert!(matches!(enable.join().await??, NotificationState::Disabled));
    disable.join().await??;

    assert_eq!(
        &peer.state.lock().calls[before_enable..],
        &[Call::Unregister, Call::Unregister]
    );
    assert!(!peer.state.lock().registered);
    assert!(!client.db().notification_record()?.push_unregister_owed);
    assert!(client.db().uploaded_topics()?.is_empty());
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
    replacement.include_commits = true;
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

    assert!(
        peer.state
            .lock()
            .calls
            .ends_with(&[Call::Unregister, Call::Register])
    );
    assert!(peer.state.lock().registered);
    let record = client.db().notification_record()?;
    assert!(stored_config(&record)?.include_commits);
    assert!(matches!(
        client.notification_state()?,
        NotificationState::Enabled
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_superseded_disable_reconciles_previously_uploaded_topics() {
    let (client, peer) = support::client().await;
    let group = client.create_group(None, None)?;
    let topic = Topic::new_group_message(group.group_id).cloned_vec();
    let mut initial = config();
    initial.include_welcomes = false;
    client.enable_notifications(initial.clone()).await?;
    run(&client.context).await?;
    assert!(peer.state.lock().subscriptions.contains_key(&topic));
    assert_eq!(client.db().uploaded_topics()?.len(), 1);

    let mut record = client.db().notification_record()?;
    let generation = record.push_generation;
    record.push_deadlines = Some(encode(&Deadlines::default())?);
    client.db().save_notification_record(&record)?;
    peer.state.lock().pause_next = true;
    let context = client.context.clone();
    let renewal = xmtp_common::spawn(None, async move { run(&context).await });
    timeout(Duration::from_secs(5), peer.entered.notified()).await?;
    assert_eq!(peer.calls(Call::Register), 2);

    let other = client.clone();
    let disable = xmtp_common::spawn(None, async move { other.disable_notifications().await });
    wait_for_eq(
        || async { client.db().notification_record().unwrap().push_generation },
        generation + 1,
    )
    .await?;
    assert!(client.db().uploaded_topics()?.is_empty());
    assert!(matches!(
        client.notification_state()?,
        NotificationState::Disabled
    ));

    initial.consent_states.clear();
    let other = client.clone();
    let enable = xmtp_common::spawn(
        None,
        async move { other.enable_notifications(initial).await },
    );
    wait_for_eq(
        || async { client.db().notification_record().unwrap().push_generation },
        generation + 2,
    )
    .await?;
    peer.release.notify_one();
    renewal.join().await??;
    disable.join().await??;
    enable.join().await??;

    assert_eq!(peer.calls(Call::Unregister), 1);
    assert_eq!(peer.calls(Call::Register), 3);
    assert!(
        peer.state
            .lock()
            .calls
            .ends_with(&[Call::Unregister, Call::Register])
    );
    assert!(peer.state.lock().subscriptions.is_empty());
    assert!(client.db().uploaded_topics()?.is_empty());
    run(&client.context).await?;
    assert!(peer.state.lock().subscriptions.is_empty());
    assert!(client.db().uploaded_topics()?.is_empty());
    assert_eq!(peer.state.lock().batches, vec![(1, 0)]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_two_disables_remove_topics_before_a_newer_enable() {
    let (client, peer) = support::client().await;
    client.create_group(None, None)?;
    let mut config = config();
    config.include_welcomes = false;
    client.enable_notifications(config.clone()).await?;
    run(&client.context).await?;
    assert_eq!(client.db().uploaded_topics()?.len(), 1);
    assert_eq!(peer.state.lock().subscriptions.len(), 1);

    let generation = client.db().notification_record()?.push_generation;
    let guard = client
        .context
        .task_channels()
        .notification_request
        .lock()
        .await;
    let mut first = std::pin::pin!(client.disable_notifications());
    assert!(futures::poll!(first.as_mut()).is_pending());
    assert!(client.db().uploaded_topics()?.is_empty());
    let mut second = std::pin::pin!(client.disable_notifications());
    assert!(futures::poll!(second.as_mut()).is_pending());
    assert_eq!(
        client.db().notification_record()?.push_generation,
        generation + 2
    );
    drop(guard);

    // Poll only D1. D2 has cleared an empty set but has not resumed at the lock.
    first.await?;
    assert!(matches!(
        client.notification_state()?,
        NotificationState::Disabled
    ));
    config.consent_states.clear();
    let mut enable = std::pin::pin!(client.enable_notifications(config));
    assert!(futures::poll!(enable.as_mut()).is_pending());
    assert_eq!(
        client.db().notification_record()?.push_generation,
        generation + 3
    );
    second.await?;
    enable.await?;

    run(&client.context).await?;
    assert!(peer.state.lock().subscriptions.is_empty());
    assert!(client.db().uploaded_topics()?.is_empty());
    assert_eq!(peer.calls(Call::Unregister), 1);
    assert_eq!(peer.calls(Call::Register), 2);
    assert_eq!(peer.calls(Call::Update), 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_in_flight_deltas_survive_disable_enable_interleavings() {
    for remove in [false, true] {
        for two_disables in [false, true] {
            for enable_before_confirm in [false, true] {
                let (client, peer) = support::client().await;
                let group = client.create_group(None, None)?;
                let topic = Topic::new_group_message(group.group_id).cloned_vec();
                let mut initial = config();
                initial.include_welcomes = false;
                client.enable_notifications(initial.clone()).await?;
                if remove {
                    run(&client.context).await?;
                    initial.consent_states.clear();
                    client.enable_notifications(initial.clone()).await?;
                }
                peer.state.lock().pause_next = true;
                let context = client.context.clone();
                let upload = xmtp_common::spawn(None, async move { run(&context).await });
                timeout(Duration::from_secs(5), peer.entered.notified()).await?;

                let mut first = std::pin::pin!(client.disable_notifications());
                assert!(futures::poll!(first.as_mut()).is_pending());
                let mut second = std::pin::pin!(client.disable_notifications());
                if two_disables {
                    assert!(futures::poll!(second.as_mut()).is_pending());
                }
                assert!(client.db().uploaded_topics()?.is_empty());

                let mut replacement = config();
                replacement.include_welcomes = false;
                if !remove {
                    replacement.consent_states.clear();
                }
                let mut enable = std::pin::pin!(client.enable_notifications(replacement));
                if enable_before_confirm {
                    assert!(futures::poll!(enable.as_mut()).is_pending());
                }
                peer.release.notify_one();
                upload.join().await??;
                if !enable_before_confirm {
                    // Finish confirmation while Disabled. Do not poll either
                    // disable again until the newer enable has changed state.
                    assert!(matches!(
                        client.notification_state()?,
                        NotificationState::Disabled
                    ));
                    assert!(client.db().uploaded_topics()?.is_empty());
                    assert!(futures::poll!(enable.as_mut()).is_pending());
                }
                first.await?;
                if two_disables {
                    second.await?;
                }
                enable.await?;
                assert_eq!(peer.calls(Call::Unregister), 1);
                assert!(peer.state.lock().subscriptions.is_empty());
                assert!(client.db().uploaded_topics()?.is_empty());

                run(&client.context).await?;
                assert_eq!(peer.state.lock().subscriptions.contains_key(&topic), remove);
                assert_eq!(client.db().uploaded_topics()?.len(), usize::from(remove));
                assert_eq!(peer.calls(Call::Update), if remove { 3 } else { 1 });
            }
        }
    }
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
        xmtp_common::sha256_array(previous.as_slice())
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
    // verifies: PUSH-240
    #[xmtp_common::test(unwrap_try = true)]
    async fn notification_stalled_request_bounds_runner_delay_and_keeps_messaging_live() {
        use crate::worker::{key_package_maintenance as kp, tasks::TaskWorker};
        const PHASE_DELAY: Duration = Duration::from_secs(20);
        const MAINTENANCE_WAIT: Duration = Duration::from_secs(42);

        for reset in [false, true] {
            let (client, peer) = support::client().await;
            tester!(bo, disable_workers);
            let group = client.create_group(None, None)?;
            client.enable_notifications(config()).await?;
            if reset {
                peer.state.lock().next_error = Some(tonic::Code::Unavailable);
                assert!(client.disable_notifications().await.unwrap_err().is_retryable());
                peer.state.lock().next_error = Some(tonic::Code::Unavailable);
                assert!(client.enable_notifications(config()).await.unwrap_err().is_retryable());
                assert!(client.db().notification_record()?.push_unregister_owed);
                assert!(matches!(client.notification_state()?, NotificationState::Enabled));
            }
            client.db().queue_key_package_rotation()?;
            let now = time::now_ns();
            client.db().create_or_ignore_task(kp::kp_seed(kp::kp_rotation_proto(), now + 5 * NS_IN_SEC)?)?;
            peer.state.lock().pause_next = true;
            let mut task_worker = TaskWorker::new(client.context.clone());
            let runner = xmtp_common::spawn(None, async move { task_worker.run().await });
            timeout(Duration::from_secs(5), peer.entered.notified()).await?;
            let entered_at = time::Instant::now();
            let phases = reset.then(|| {
                let peer = peer.clone();
                xmtp_common::spawn(None, async move {
                    time::sleep(PHASE_DELAY).await;
                    peer.state.lock().pause_next = true;
                    peer.release.notify_one();
                    timeout(Duration::from_secs(5), peer.entered.notified()).await.unwrap();
                    time::sleep(PHASE_DELAY).await;
                    peer.release.notify_one();
                })
            });
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
            // Let both requests finish on the weaker code, then check the bound
            // from maintenance becoming due five seconds after the turn starts.
            timeout(MAINTENANCE_WAIT, async {
                loop {
                    if client.db().next_key_package_rotation_ns().unwrap().unwrap() > now + NS_IN_HOUR { break; }
                    time::sleep(Duration::from_millis(50)).await;
                }
            }).await?;
            let elapsed = entered_at.elapsed();
            assert!(elapsed < Duration::from_secs(35), "reset={reset}: key package maintenance waited {elapsed:?} after the notification turn started");
            assert!(peer.calls(Call::Publish) > 0);
            if reset {
                assert_eq!(peer.calls(Call::Unregister), 3);
                assert_eq!(peer.calls(Call::Register), 2);
                let record = client.db().notification_record()?;
                assert!(!record.push_unregister_owed);
                assert!(record.push_last_state.is_none());
                assert!(matches!(client.notification_state()?, NotificationState::Enabled));
                assert!(!peer.state.lock().registered);
            }
            if let Some(phases) = phases {
                phases.abort_handle().end();
            }
            runner.abort_handle().end();
            if reset {
                run(&client.context).await?;
                assert_eq!(peer.calls(Call::Unregister), 3);
                assert_eq!(peer.calls(Call::Register), 3);
                assert!(peer.state.lock().registered);
                assert!(client.db().notification_record()?.push_last_state.is_some());
            }
        }
    }
}

/// The backend rejects a batch with more than `REQUEST_TOPICS` adds and
/// removes before it applies any, and that rejection is terminal, so removes
/// take their share of the batch first, and adds of new or stale topics fill
/// only the rest.
#[xmtp_common::test]
fn diff_keeps_adds_and_removes_within_the_batch_bound() {
    let epoch = crate::utils::time::hmac_epoch();
    let topics = synthetic_desired(2100);
    let uploaded = |list: &[&Vec<u8>], stale| -> Vec<_> {
        list.iter()
            .map(|&topic| UploadedTopic {
                topic: topic.clone(),
                hmac_epoch_base: Some(epoch - 1),
                include_commits: false,
                root_key_fingerprint: vec![3; 32],
                stale,
            })
            .collect()
    };
    let keys: Vec<_> = topics.keys().collect();
    // (desired and not uploaded, desired and uploaded stale, uploaded and not
    // desired) -> (adds, removes)
    for (new, stale, unwanted, expected) in [
        (700, 0, 700, (300, 700)),
        (0, 0, 1500, (0, REQUEST_TOPICS)),
        (0, 600, 600, (400, 600)),
    ] {
        let (new_topics, rest) = keys.split_at(new);
        let (stale_topics, rest) = rest.split_at(stale);
        let desired: Desired = new_topics
            .iter()
            .chain(stale_topics)
            .map(|&topic| (topic.clone(), topics[topic].clone()))
            .collect();
        let mut rows = uploaded(stale_topics, true);
        rows.extend(uploaded(&rest[..unwanted], false));
        let (adds, removes) = diff(&desired, &rows, epoch, &[3; 32], false);
        assert_eq!((adds.len(), removes.len()), expected);
        assert!(adds.iter().all(|topic| desired.contains_key(topic)));
        assert!(removes.iter().all(|topic| !desired.contains_key(topic)));
    }
}
