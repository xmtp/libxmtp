//! Incoming controller tests: receipt.

use super::*;

// verifies: PROC-002
#[xmtp_common::test(unwrap_try = true)]
async fn receipt_acknowledgement_follows_storage_and_never_uses_the_target() {
    let alix = EngineTestPeer::new().await;
    let topic = Topic::new_group_message(GroupId::generate());
    let mut controller = controller(alix.context.clone());
    add_scope(&mut controller, 1, &topic);
    controller.registered([(topic.clone(), Cursor(90))].into());
    controller.refresh_statuses();
    assert_eq!(
        controller.state.statuses.lock()[&1].topics[0].received,
        Cursor(0)
    );

    let acknowledged = Arc::new(Mutex::new(Vec::new()));
    let observed = acknowledged.clone();
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        move |cursors| observed.lock().push(cursors),
    ));
    let batch = OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: Cursor(0),
        envelopes: [10, 20]
            .into_iter()
            .map(|sequence| wire::ServerEnvelope {
                meta: Some(meta(&topic, sequence)),
                envelope: Some(wire::ClientEnvelope::default()),
            })
            .map(Into::into)
            .collect(),
    };
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(batch.clone()))));
    controller.refresh_statuses();
    let status = controller.state.statuses.lock()[&1].topics[0].clone();
    assert_eq!(status.received, Cursor(20));
    assert_eq!(status.processed, Cursor(0));
    assert_eq!(status.processing, IncomingProcessing::Pending);
    assert_eq!(acknowledged.lock()[0][&topic], Cursor(20));
    // A replay after an unacknowledged delivery has no duplicate durable rows.
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(batch))));
    let pending = alix
        .context
        .db()
        .pending_states_through(&topic_key(&topic)?, Cursor(90))?;
    assert_eq!(pending.len(), 2);
    assert_eq!(acknowledged.lock().len(), 2);

    // A retryable storage failure must also discard uncommitted transport progress.
    controller.receive_error(
        topic.clone(),
        xmtp_mls_core::mls_store::MlsStoreError::Storage(
            xmtp_db::stream_storage::StreamStorageError::HeadChanged.into(),
        )
        .into(),
    );
    assert!(controller.transport.subscription().is_none());
    assert!(controller.transport.registered.is_empty());
    assert!(!controller.receipt(&topic).blocked());
    controller.refresh_statuses();
    assert_eq!(
        controller.state.statuses.lock()[&1].processing,
        IncomingProcessing::Pending
    );

    // Refuse a skipped prefix, but let this controller repair it from durable F.
    let missing = xmtp_db::stream_storage::StreamStorageError::MissingPrefix {
        after: 80,
        received: 20,
    };
    assert!(!missing.is_retryable());
    controller.receive_error(
        topic.clone(),
        xmtp_mls_core::mls_store::MlsStoreError::Storage(missing.into()).into(),
    );
    assert!(!controller.receipt(&topic).blocked());
    controller.refresh_statuses();
    let snapshot = controller.state.statuses.lock()[&1].clone();
    assert_eq!(snapshot.processing, IncomingProcessing::Pending);
    assert!(snapshot.topics[0].error.as_ref().unwrap().is_retryable());
    assert_eq!(snapshot.topics[0].received, Cursor(20));
    assert_eq!(snapshot.topics[0].processed, Cursor(0));
    controller.start_open();
    assert!(
        !controller.transport.is_opening(),
        "topic reconciliation preserves backoff"
    );
    controller.read_queue.push_back(topic.clone());
    controller
        .topics
        .entry(topic.clone())
        .or_default()
        .receipt
        .last_read = None;
    controller.start_read();
    assert!(
        controller.read.is_some(),
        "the missing prefix can be queried"
    );
}

// verifies: PROC-020
#[xmtp_common::test(unwrap_try = true)]
async fn each_kind_keeps_its_budget_and_only_committed_chunks_are_acknowledged() {
    use xmtp_db::{TransactionOutcome, XmtpMlsStorageProvider};

    let alix = EngineTestPeer::new().await;
    let client = alix
        .with_stream_policy(|settings| {
            settings.max_fetched_rows = 8;
            settings.max_fetched_bytes = 4096;
            settings.max_admission_rows = 1;
            settings.max_admission_bytes = 1024;
            settings.group_pending.rows = 2;
            settings.welcome_pending.bytes = 1;
        })
        .await;
    let topic = Topic::new_group_message(GroupId::generate());
    let welcome = Topic::new_welcome_message(client.installation_public_key());
    let key = topic_key(&topic)?;
    let mut controller = controller(client.context.clone());
    add_scope(&mut controller, 1, &topic);
    add_scope(&mut controller, 2, &welcome);
    controller.transport.requested = [topic.clone(), welcome.clone()].into();
    let limits = controller
        .context
        .incoming_runtime()
        .policy()
        .fetched_limits();
    assert_eq!(limits.max_rows, 8);
    assert_eq!(limits.max_bytes, 4096);
    let acknowledged = Arc::new(Mutex::new(Vec::new()));
    let observed = acknowledged.clone();
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        move |cursors| observed.lock().push(cursors),
    ));
    let batch = OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: Cursor(0),
        envelopes: [10, 20, 30]
            .into_iter()
            .map(|sequence| wire::ServerEnvelope {
                meta: Some(meta(&topic, sequence)),
                envelope: Some(wire::ClientEnvelope::default()),
            })
            .map(Into::into)
            .collect(),
    };
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(batch.clone()))));
    assert_eq!(
        client.context.db().topic_progress(&key)?.received,
        Cursor(20)
    );
    assert_eq!(acknowledged.lock().len(), 2);
    assert_eq!(acknowledged.lock()[0][&topic], Cursor(10));
    assert_eq!(acknowledged.lock()[1][&topic], Cursor(20));
    assert!(controller.receipt(&topic).paused);
    assert!(!controller.receipt(&welcome).paused);
    assert_eq!(controller.transport.permanent_failures, 0);
    assert!(controller.transport.subscription().is_none());

    xmtp_mls_core::state_tx::state_write(client.context.mls_storage(), |tx| {
        let storage = tx.storage();
        for sequence in [10, 20] {
            storage
                .db()
                .complete_pending_envelope(&key, Cursor(sequence))?;
        }
        Ok::<_, xmtp_db::StorageError>(TransactionOutcome::Continue(()))
    })?;
    controller.reconcile()?;
    assert!(!controller.receipt(&topic).paused);
    let observed = acknowledged.clone();
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        move |cursors| observed.lock().push(cursors),
    ));
    // An overlapping replay must use each chunk's wire start, not its durable receipt.
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(batch))));
    assert_eq!(
        client.context.db().topic_progress(&key)?.received,
        Cursor(30)
    );
    assert_eq!(acknowledged.lock().last().unwrap()[&topic], Cursor(30));
    assert_eq!(
        client
            .context
            .db()
            .pending_states_through(&key, Cursor(30))?
            .len(),
        1
    );
    assert!(!controller.receipt(&topic).blocked());

    // Run the production loop with one retained head and a legal eight-row batch.
    // The fake transport exposes each new registration and never replays on its own.
    use xmtp_common::{StreamHandle, wait_for_eq};
    use xmtp_db::incoming_envelope::IncomingRetry;

    let receiver = EngineTestPeer::new().await;
    let group = receiver.create_group(None, None)?;
    let client = receiver
        .with_stream_policy(|settings| {
            settings.max_fetched_rows = 8;
            settings.max_admission_rows = 8;
            settings.max_pending_rows_per_topic = 8;
            settings.active_database_poll_interval = Duration::from_millis(10);
            settings.receiver_fallback_interval = Duration::from_millis(100);
        })
        .await;
    let topic = Topic::new_group_message(group.group_id);
    let key = topic_key(&topic)?;
    let before = client.context.db().topic_progress(&key)?;
    let retained = Cursor(before.received.0 + 10);
    let replayed = Cursor(retained.0 + 8);
    let later = Cursor(replayed.0 + 1);
    let envelope = |cursor: Cursor| wire::ServerEnvelope {
        meta: Some(meta(&topic, cursor.0)),
        envelope: Some(wire::ClientEnvelope {
            payload: Some(wire::client_envelope::Payload::GroupMessage(
                wire::GroupMessage {
                    data: vec![0, 1, 0],
                    ..Default::default()
                },
            )),
        }),
    };
    let limits = client
        .context
        .incoming_runtime()
        .policy()
        .incoming_limits(NetworkEntityKind::Group);
    MlsStore::new(client.context.clone()).admit_incoming_batch(
        &OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: before.received,
            envelopes: vec![envelope(retained).into()],
        },
        limits,
    )?;
    client.context.db().set_incoming_retry(
        &key,
        retained,
        &IncomingRetry {
            retry_at_ns: xmtp_common::time::now_ns() + 30 * xmtp_common::NS_IN_SEC,
            blocked: false,
            error_code: Some("identity_dependency".into()),
            retry_expires_at_ns: None,
        },
    )?;
    let (commands, receiver) = mpsc::unbounded_channel();
    let state = Arc::new(SharedState::default());
    let mut running = Controller::new(client.context.clone(), receiver, state.clone());
    add_scope(&mut running, 1, &topic);
    let (opened, mut registrations) = mpsc::unbounded_channel();
    let acknowledged = Arc::new(Mutex::new(Vec::new()));
    let observed = acknowledged.clone();
    let acknowledged_topic = topic.clone();
    running.transport.factory = Some(Arc::new(
        move |cursors: TopicCursor, limits: IncomingBatchLimits| -> SubscriptionFuture {
            assert_eq!(limits.max_rows, 8);
            let (send, receive) = mpsc::unbounded_channel();
            opened.send((cursors, send)).unwrap();
            let observed = observed.clone();
            let topic = acknowledged_topic.clone();
            Box::pin(async move {
                let events = futures::stream::unfold(receive, |mut receive| async move {
                    receive.recv().await.map(|event| (event, receive))
                });
                Ok(IncomingSubscription::new(
                    Box::pin(events),
                    move |cursors| {
                        if let Some(received) = cursors.get(&topic) {
                            observed.lock().push(*received);
                        }
                    },
                ))
            })
        },
    ));
    let task = xmtp_common::spawn(None, running.run());
    let (starts, first) = xmtp_common::time::timeout(Duration::from_secs(5), registrations.recv())
        .await?
        .unwrap();
    assert_eq!(starts[&topic], retained);
    first.send(Ok(IncomingEvent::Registered {
        starts,
        targets: [(topic.clone(), replayed)].into(),
    }))?;
    let batch = OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: retained,
        envelopes: (1..=8)
            .map(|offset| envelope(Cursor(retained.0 + offset)))
            .map(Into::into)
            .collect(),
    };
    first.send(Ok(IncomingEvent::OrderedBatch(batch.clone())))?;
    xmtp_common::time::timeout(Duration::from_secs(5), first.closed()).await?;
    assert_eq!(client.context.db().topic_progress(&key)?.received, retained);
    assert_eq!(
        client.context.db().topic_progress(&key)?.processed,
        before.processed
    );
    assert_eq!(
        client
            .context
            .db()
            .pending_states_through(&key, later)?
            .len(),
        1
    );
    assert!(acknowledged.lock().is_empty());

    // Make the retained head runnable. The same controller frees capacity and reopens.
    client.context.db().set_incoming_retry(
        &key,
        retained,
        &IncomingRetry {
            retry_at_ns: 0,
            blocked: false,
            error_code: None,
            retry_expires_at_ns: None,
        },
    )?;
    wait_for_eq(
        || async { client.context.db().topic_progress(&key).unwrap().processed },
        retained,
    )
    .await?;
    let (starts, resumed) =
        xmtp_common::time::timeout(Duration::from_secs(5), registrations.recv())
            .await?
            .unwrap();
    assert_eq!(starts[&topic], retained);
    resumed.send(Ok(IncomingEvent::Registered {
        starts,
        targets: [(topic.clone(), later)].into(),
    }))?;
    resumed.send(Ok(IncomingEvent::OrderedBatch(batch)))?;
    wait_for_eq(
        || async { client.context.db().topic_progress(&key).unwrap().processed },
        replayed,
    )
    .await?;
    resumed.send(Ok(IncomingEvent::OrderedBatch(OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: replayed,
        envelopes: vec![envelope(later).into()],
    })))?;
    wait_for_eq(
        || async { client.context.db().topic_progress(&key).unwrap().processed },
        later,
    )
    .await?;
    assert_eq!(client.context.db().topic_progress(&key)?.received, later);
    assert!(
        client
            .context
            .db()
            .pending_states_through(&key, later)?
            .is_empty()
    );
    assert_eq!(*acknowledged.lock(), vec![replayed, later]);
    let snapshot = state.statuses.lock()[&1].clone();
    assert_eq!(snapshot.scope_generation, 1);
    assert_eq!(snapshot.connection_generation, 2);
    assert_eq!(snapshot.topics[0].target, Some(replayed));
    assert_ne!(snapshot.processing, IncomingProcessing::Blocked);
    commands.send(Command::Release(1))?;
    task.join().await?;
}

// verifies: PROC-001
#[xmtp_common::test(unwrap_try = true)]
async fn byte_chunks_validate_the_complete_input_before_receipt() {
    let alix = EngineTestPeer::new().await;
    let topic = Topic::new_group_message(GroupId::generate());
    let envelope = |sequence| wire::ServerEnvelope {
        meta: Some(meta(&topic, sequence)),
        envelope: Some(wire::ClientEnvelope::default()),
    };
    let max_bytes = envelope(10).encoded_len() + 2;
    let client = alix
        .with_stream_policy(|settings| {
            settings.max_admission_rows = 8;
            settings.max_admission_bytes = max_bytes as u64;
        })
        .await;
    let mut controller = controller(client.context.clone());
    let key = topic_key(&topic)?;
    // Unknown fields that fit when decoded still count at their received size.
    let unknown = xmtp_proto::types::future_welcome_envelope(&meta(&topic, 10), &[9; 8]);
    let unknown = ReceivedEnvelope::from_raw(unknown)?;
    assert!(unknown.decode()?.encoded_len() <= max_bytes && unknown.raw().len() > max_bytes);
    for envelopes in [vec![envelope(10).into(), envelope(5).into()], vec![unknown]] {
        let batch = OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: Cursor(0),
            envelopes,
        };
        assert!(controller.admit_received_batch(batch).is_err());
    }
    assert_eq!(
        client.context.db().topic_progress(&key)?.received,
        Cursor(0)
    );
    controller.admit_received_batch(OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: Cursor(0),
        envelopes: vec![envelope(10).into(), envelope(20).into()],
    })?;
    assert_eq!(
        client.context.db().topic_progress(&key)?.received,
        Cursor(20)
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn replacement_cancels_old_obligations_and_retains_notifications() {
    let (commands, _receiver) = mpsc::unbounded_channel();
    let coordinator = Arc::new(IncomingCoordinator {
        commands,
        generations: AtomicU64::new(0),
        state: Arc::new(SharedState::default()),
    });
    let lease = coordinator.acquire(IncomingScope::Topics(vec![]));
    let old = lease.replace_topics(vec![Topic::new_group_message(GroupId::generate())]);
    assert_eq!(old.processing, IncomingProcessing::Cancelled);
    let current = lease.snapshot();
    assert!(current.scope_generation > old.scope_generation);
    assert_eq!(
        current.previous.unwrap().processing,
        IncomingProcessing::Cancelled
    );
    xmtp_common::time::timeout(Duration::from_secs(1), lease.changed()).await?;
}

/// Once durable receipt reaches a topic's fixed target, the controller does
/// not read receipt progress for that target again.
#[xmtp_common::test(unwrap_try = true)]
async fn receipt_that_reaches_a_fixed_target_is_read_once() {
    let alix = EngineTestPeer::new().await;
    let topic = Topic::new_group_message(GroupId::generate());
    let mut controller = controller(alix.context.clone());
    add_scope(&mut controller, 1, &topic);
    controller.read_queue.push_back(topic.clone());
    admit_empty(&mut controller, &topic, &[10]);
    controller.registered([(topic, Cursor(10))].into());
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    for _ in 0..100 {
        controller.start_read();
    }
    assert!(controller.read.is_none());
    assert_eq!(
        controller
            .receipt_progress_reads
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
}
