//! Notification worker tests: failures and repair.

use super::*;

// verifies: EVENT-001, EVENT-010, EVENT-024
#[xmtp_common::test(unwrap_try = true)]
async fn stored_terminal_notification_failure_emits_once() {
    use xmtp_events::{ClientEvent, EventFilter, EventKind};

    let alix = EngineTestPeer::new().await;
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

    let (client, peer) = support::peer().await;
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

#[xmtp_common::test(unwrap_try = true)]
async fn notification_repair_stale_marks_survive_database_restore() {
    let alix = EngineTestPeer::new().await;
    let db = alix.db();
    let desired = synthetic_desired(2500);
    let mut rows: Vec<_> = desired
        .keys()
        .map(|topic| UploadedTopic {
            topic: topic.clone(),
            hmac_epoch_base: Some(xmtp_mls_core::hmac_epoch::hmac_epoch() - 1),
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
        false,
    )?;
    let snapshot = Arc::new(alix.db_snapshot());
    let restored = alix.from_snapshot(&snapshot, |_| {}).await;
    assert!(restored.db().notification_record()?.push_repairing);
    let uploaded = restored.db().uploaded_topics()?;
    assert_eq!(uploaded.iter().filter(|row| row.stale).count(), 1500);
    let (next, _, _) = diff(
        &desired,
        &uploaded,
        xmtp_mls_core::hmac_epoch::hmac_epoch(),
        &[3; 32],
        false,
    );
    assert_eq!(next.len(), 1000);
    assert!(
        next.iter()
            .all(|topic| uploaded.iter().any(|row| &row.topic == topic && row.stale))
    );
}

xmtp_common::if_native! {
    // verifies: PUSH-267
    #[xmtp_common::test(unwrap_try = true)]
    async fn notification_uncertain_update_survives_restore_and_generation_change() {
        use xmtp_db::ConnectionExt;
        for applied in [false, true] {
            for (kept, disabled) in [(false, false), (true, false), (false, true)] {
                let (client, peer) = support::peer().await;
                let removed = client.create_group(None, None)?;
                let removed_topic = Topic::new_group_message(removed.group_id).cloned_vec();
                let mut config = config();
                config.include_welcomes = false;
                client.enable_notifications(config.clone()).await?;
                run(&client.context).await?;
                removed.set_notifications(NotificationOverride::Disabled)?;
                let added = client.create_group(None, None)?;
                let added_topic = Topic::new_group_message(added.group_id).cloned_vec();
                // Fixture networking must finish before native timers are paused.
                tokio::time::pause();
                peer.state.lock().pause_next = !applied;
                peer.state.lock().lose_update_reply = applied;
                let context = client.context.clone();
                let upload = xmtp_common::spawn(None, async move { run(&context).await });
                peer.entered.notified().await;
                assert_eq!(peer.state.lock().subscriptions.contains_key(&added_topic), applied);
                assert_eq!(peer.state.lock().subscriptions.contains_key(&removed_topic), !applied);
                // Enabling saves a newer generation before waiting on the request lock.
                let generation = client.db().notification_record()?.push_generation;
                let mut enable = std::pin::pin!(client.enable_notifications(config));
                let mut disable = std::pin::pin!(client.disable_notifications());
                if disabled { assert!(futures::poll!(disable.as_mut()).is_pending()); }
                let changed_generation = applied && kept;
                if changed_generation {
                    assert!(futures::poll!(enable.as_mut()).is_pending());
                    assert_eq!(client.db().notification_record()?.push_generation, generation + 1);
                }
                tokio::time::advance(Duration::from_secs(31)).await;
                let result = upload.join().await?;
                if disabled {
                    assert_eq!(result?, TaskOutcome::Done);
                    assert!(client.db().uploaded_topics()?.is_empty());
                    disable.await?;
                    tokio::time::resume();
                    assert!(peer.state.lock().subscriptions.is_empty());
                    assert_eq!(run(&client.context).await?, TaskOutcome::Done);
                    continue;
                }
                if changed_generation { result?; } else { assert!(result.is_err()); }
                let rows = client.db().uploaded_topics()?;
                assert!(rows.iter().any(|row| row.topic == added_topic && row.stale), "uncertain add must be stale");
                assert!(rows.iter().any(|row| row.topic == removed_topic), "unconfirmed remove must remain");
                enable.await?;
                tokio::time::resume();
                if !kept {
                    added.set_notifications(NotificationOverride::Disabled)?;
                }
                let snapshot = client.db().raw_query(|conn| Ok(conn.serialize_database_to_buffer().to_vec()))?;
                let restored = support::client_from_snapshot(&client, &snapshot, peer.clone()).await;
                run(&restored.context).await?;
                let expected = if kept { vec![added_topic] } else { vec![] };
                assert_eq!(peer.state.lock().subscriptions.keys().cloned().collect::<Vec<_>>(), expected);
                assert_eq!(restored.db().uploaded_topics()?.iter().map(|row| row.topic.clone()).collect::<Vec<_>>(), expected);
                assert!(restored.db().uploaded_topics()?.iter().all(|row| !row.stale));
                assert!(!restored.db().notification_record()?.push_repairing);
                let calls = peer.calls(Call::Update);
                for _ in 0..3 { run(&restored.context).await?; }
                assert_eq!(peer.calls(Call::Update), calls, "convergence must not restart full repairs");
                assert_eq!(peer.state.lock().batches.last(), Some(&(usize::from(kept), 1 + usize::from(!kept))));
            }
        }
    }
}
