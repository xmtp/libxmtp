//! Notification worker tests: enable disable.

use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn notification_revocation_discards_a_prepared_batch() {
    for override_change in [false, true] {
        let (client, peer) = support::peer().await;
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
    let (client, peer) = support::peer().await;
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
    let (client, peer) = support::peer().await;
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
    let (client, peer) = support::peer().await;
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
    let (client, peer) = support::peer().await;
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
    let (client, peer) = support::peer().await;
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
    let (client, peer) = support::peer().await;
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
                let (client, peer) = support::peer().await;
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

/// Stored deadlines that cannot be decoded do not stop an app from turning
/// notifications off or back on: they hold nothing.
#[xmtp_common::test(unwrap_try = true)]
async fn notification_undecodable_deadlines_do_not_block_enable_or_disable() {
    let (client, _peer) = support::peer().await;
    client.enable_notifications(config()).await?;
    let mut record = client.db().notification_record()?;
    record.push_deadlines = Some(vec![0xff]);
    client.db().save_notification_record(&record)?;
    client.disable_notifications().await?;
    let mut record = client.db().notification_record()?;
    record.push_deadlines = Some(vec![0xff]);
    client.db().save_notification_record(&record)?;
    client.enable_notifications(config()).await?;
}
