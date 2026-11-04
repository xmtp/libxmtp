//! Notification worker tests: desired set.

use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn notification_desired_set_uses_real_membership_and_each_rule() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    bo_group.update_consent_state(ConsentState::Allowed)?;
    let sync = xmtp_mls_engine::groups::MlsGroup::create_and_insert(
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
    let (client, peer) = support::peer().await;
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
    old_window.hmac_epoch_base = Some(xmtp_mls_core::hmac_epoch::hmac_epoch() - 2);
    client.db().confirm_uploaded_topics(&[old_window], &[])?;
    run(&client.context).await?;
    assert_eq!(peer.calls(Call::Update), 2);
    let old_root = client.db().uploaded_topics()?[0]
        .root_key_fingerprint
        .clone();
    xmtp_db::user_preferences::store_test_root_key(
        &client.db(),
        &[8; 42],
        xmtp_common::time::now_ns() + xmtp_common::NS_IN_DAY,
    )?;
    run(&client.context).await?;
    assert_ne!(
        client.db().uploaded_topics()?[0].root_key_fingerprint,
        old_root
    );
    config.include_commits = true;
    client.enable_notifications(config).await?;
    run(&client.context).await?;
    assert!(peer.state.lock().subscriptions[&topic].include_commits);

    peer.state.lock().limit = Some(1);
    let extra = client.create_group(None, None)?;
    let extra_topic = Topic::new_group_message(extra.group_id).cloned_vec();
    run(&client.context).await?;
    assert!(client.db().notification_record()?.push_suppressed.is_some());
    let keys = peer.state.lock().subscriptions[&topic].hmac_keys.clone();
    xmtp_db::user_preferences::store_test_root_key(
        &client.db(),
        &[9; 42],
        time::now_ns() + 2 * xmtp_common::NS_IN_DAY,
    )?;
    let calls = peer.calls(Call::Update);
    run(&client.context).await?;
    assert_eq!(
        peer.calls(Call::Update),
        calls + 1,
        "cap must permit key-only repair"
    );
    assert_ne!(peer.state.lock().subscriptions[&topic].hmac_keys, keys);
    assert!(!peer.state.lock().subscriptions.contains_key(&extra_topic));
    assert!(client.db().notification_record()?.push_suppressed.is_some());
    run(&client.context).await?;
    assert_eq!(
        peer.calls(Call::Update),
        calls + 1,
        "key-only success must not release new topics"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_resource_exhaustion_suppresses_adds_but_keeps_removes() {
    let (client, peer) = support::peer().await;
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
    let (client, peer) = support::peer().await;
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
    let (client, peer) = support::peer().await;
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
    let (client, peer) = support::peer().await;
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
async fn notification_request_keeps_the_root_key_snapshot() {
    let (client, peer) = support::peer().await;
    let group = client.create_group(None, None)?;
    client.enable_notifications(config()).await?;
    peer.state.lock().pause_next = true;
    let context = client.context.clone();
    let upload = xmtp_common::spawn(None, async move { run(&context).await });
    timeout(Duration::from_secs(5), peer.entered.notified()).await?;
    let previous = StoredUserPreferences::load(client.db())?.hmac_key.unwrap();
    xmtp_db::user_preferences::store_test_root_key(
        &client.db(),
        &[9; 42],
        xmtp_common::time::now_ns() + xmtp_common::NS_IN_DAY,
    )?;
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

/// The backend rejects a batch with more than `REQUEST_TOPICS` adds and
/// removes before it applies any, and that rejection is terminal, so removes
/// take their share of the batch first, and adds of new or stale topics fill
/// only the rest.
#[xmtp_common::test]
fn diff_keeps_adds_and_removes_within_the_batch_bound() {
    let epoch = xmtp_mls_core::hmac_epoch::hmac_epoch();
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
    for (new, stale, unwanted, suppress, repair, expected) in [
        (700, 0, 700, false, "stale", (300, 700)),
        (0, 0, 1500, false, "stale", (0, REQUEST_TOPICS)),
        (0, 600, 600, false, "stale", (400, 600)),
        (700, 600, 600, true, "stale", (400, 600)),
        (700, 600, 600, true, "root", (400, 600)),
        (700, 600, 600, true, "epoch", (400, 600)),
        (700, 600, 600, true, "flags", (400, 600)),
    ] {
        let (new_topics, rest) = keys.split_at(new);
        let (stale_topics, rest) = rest.split_at(stale);
        let desired: Desired = new_topics
            .iter()
            .chain(stale_topics)
            .map(|&topic| (topic.clone(), topics[topic].clone()))
            .collect();
        let mut rows = uploaded(stale_topics, repair == "stale");
        for row in &mut rows {
            match repair {
                "root" => row.root_key_fingerprint = vec![4; 32],
                "epoch" => row.hmac_epoch_base = Some(epoch - 2),
                "flags" => row.include_commits = true,
                _ => {}
            }
        }
        rows.extend(uploaded(&rest[..unwanted], false));
        let (adds, removes, adds_new_topic) = diff(&desired, &rows, epoch, &[3; 32], suppress);
        assert_eq!((adds.len(), removes.len()), expected);
        assert_eq!(adds_new_topic, !suppress && new > 0 && expected.0 > 0);
        let expected_adds: Vec<_> = new_topics
            .iter()
            .take(if suppress { 0 } else { new })
            .chain(stale_topics)
            .take(expected.0)
            .map(|topic| (*topic).clone())
            .collect();
        assert_eq!(adds, expected_adds, "{repair}, suppress={suppress}");
        assert_eq!(
            removes,
            rest[..expected.1]
                .iter()
                .map(|topic| (*topic).clone())
                .collect::<Vec<_>>()
        );
        assert!(adds.len() + removes.len() <= REQUEST_TOPICS);
    }
}
