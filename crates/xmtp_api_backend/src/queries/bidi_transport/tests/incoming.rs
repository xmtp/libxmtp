use super::*;

fn raw_group(sequence: u64) -> ServerEnvelope {
    let mut envelope = group_msg(sequence, &[7; 16]);
    envelope.meta.as_mut().unwrap().message_hash = Some(backend_v1::MessageHash {
        hash: Some(backend_v1::message_hash::Hash::Sha256(vec![8; 32])),
    });
    envelope
}

fn raw_group_on(sequence: u64, group_id: &[u8]) -> ServerEnvelope {
    let mut envelope = raw_group(sequence);
    envelope.meta.as_mut().unwrap().topic = group_msg(sequence, group_id).meta.unwrap().topic;
    envelope
}

fn limits() -> IncomingBatchLimits {
    IncomingBatchLimits {
        max_rows: 16,
        max_bytes: 4096,
    }
}

async fn incoming(lease: &mut TopicLease<BackendBinding>) -> Result<IncomingEvent, TransportError> {
    xmtp_common::time::timeout(WAIT, lease.next_incoming())
        .await
        .unwrap()
        .unwrap()
}

async fn incoming_frame(receiver: &mut mpsc::Receiver<IncomingFrame>) -> IncomingFrame {
    xmtp_common::time::timeout(WAIT, receiver.recv())
        .await
        .expect("timed out waiting for an incoming frame")
        .expect("incoming frame sender closed")
}

#[xmtp_common::test(unwrap_try = true)]
async fn raw_reconnect_replays_uncommitted_delivery_and_uses_received_floor() {
    let topic = group_topic(&[7; 16]);
    let (transport, servers) = transport();
    let mut lease = transport
        .lease_ordered(vec![(topic.clone(), 2)], 8, limits())
        .await?;
    let mut server = take_server(&servers);
    let update = server.next_mutate().await;
    server.ack(update.id, vec![(topic.clone(), 20)]);
    let IncomingEvent::Registered { starts, targets } = incoming(&mut lease).await? else {
        panic!("registration");
    };
    assert_eq!(starts[&topic], Cursor(2));
    assert_eq!(targets[&topic], Cursor(20));
    server.send(messages(vec![raw_group(8)], vec![]));
    let IncomingEvent::OrderedBatch(batch) = incoming(&mut lease).await? else {
        panic!("batch");
    };
    assert_eq!(batch.after, Cursor(2));
    // No receipt acknowledgement: storage did not commit this delivery.
    drop(server);
    assert!(matches!(
        incoming(&mut lease).await?,
        IncomingEvent::Disconnected
    ));
    xmtp_common::wait_for_some(|| async { (!servers.lock().unwrap().is_empty()).then_some(()) })
        .await;
    let mut server = take_server(&servers);
    let update = server.next_mutate().await;
    assert_eq!(update.adds[0].cursor.as_ref().unwrap().sequence_id, 2);
    server.ack(update.id, vec![(topic.clone(), 20)]);
    incoming(&mut lease).await?;
    server.send(messages(vec![raw_group(8)], vec![]));
    let IncomingEvent::OrderedBatch(replayed) = incoming(&mut lease).await? else {
        panic!("replayed batch");
    };
    assert_eq!(replayed.after, Cursor(2));
    assert_eq!(replayed.envelopes, batch.envelopes);
    // Unary recovery can commit beyond the latest stream delivery.
    lease.acknowledge_received([(topic.clone(), Cursor(20))].into());
    transport.suspend().await?;
    let resumed = transport.clone();
    let resume = tokio::spawn(async move { resumed.resume().await });
    xmtp_common::wait_for_some(|| async { (!servers.lock().unwrap().is_empty()).then_some(()) })
        .await;
    let mut server = take_server(&servers);
    let update = server.next_mutate().await;
    assert_eq!(update.adds[0].cursor.as_ref().unwrap().sequence_id, 20);
    server.ack(update.id, vec![(topic.clone(), 20)]);
    resume.await??;
}

#[xmtp_common::test(unwrap_try = true)]
async fn raw_full_delivery_channel_pauses_without_advancing_cursors() {
    let first_topic = group_topic(&[7; 16]);
    let second_topic = group_topic(&[8; 16]);
    let mut ledger = Ledger::<BackendBinding>::default();
    let (events, _event_receiver) = mpsc::channel(1);
    let id = ledger.register(
        &[(first_topic.clone(), 0), (second_topic.clone(), 0)],
        events,
    );
    let (sender, mut receiver) = mpsc::channel(1);
    let limits = IncomingBatchLimits {
        max_rows: 1,
        ..limits()
    };
    ledger.leases.get_mut(&id).unwrap().incoming = Some((sender, limits));
    let (update, _) = ledger
        .prepare_adds(vec![(first_topic.clone(), 0), (second_topic.clone(), 0)])
        .remove(0);
    ledger.applied(
        update,
        vec![(first_topic.clone(), 10), (second_topic.clone(), 10)],
    );

    // Registered occupies the only slot. Both envelopes must stay pending.
    assert!(
        ledger
            .demux_incoming(&[raw_group(8), raw_group_on(9, &[8; 16])], |message| {
                message
            },)
            .unwrap()
            .is_empty()
    );
    let lease = ledger.leases.get(&id).unwrap();
    assert_eq!(lease.delivered[&first_topic], 0);
    assert_eq!(lease.delivered[&second_topic], 0);
    assert!(lease.incoming_failure.lock().is_none());
    assert!(ledger.has_pending_incoming());

    // Free one slot at a time. Each flush hands off one ordered chunk, then
    // advances only the cursor for that accepted chunk.
    incoming_frame(&mut receiver).await.unwrap();
    assert!(
        ledger
            .flush_incoming(xmtp_common::time::Instant::now())
            .is_empty()
    );
    let Ok(first_events) = incoming_frame(&mut receiver).await else {
        panic!("first queued delivery")
    };
    let IncomingEvent::OrderedBatch(first) = first_events.into_iter().next().unwrap() else {
        panic!("first ordered batch")
    };
    assert_eq!(
        (first.topic, first.after, first.envelopes),
        (first_topic.clone(), Cursor(0), vec![raw_group(8)])
    );
    assert_eq!(ledger.leases[&id].delivered[&first_topic], 8);
    assert_eq!(ledger.leases[&id].delivered[&second_topic], 0);
    assert!(
        ledger
            .flush_incoming(xmtp_common::time::Instant::now())
            .is_empty()
    );
    let Ok(second_events) = incoming_frame(&mut receiver).await else {
        panic!("second queued delivery")
    };
    let IncomingEvent::OrderedBatch(second) = second_events.into_iter().next().unwrap() else {
        panic!("second ordered batch")
    };
    assert_eq!(
        (second.topic, second.after, second.envelopes),
        (
            second_topic.clone(),
            Cursor(0),
            vec![raw_group_on(9, &[8; 16])]
        )
    );
    assert_eq!(ledger.leases[&id].delivered[&second_topic], 9);
    assert!(!ledger.has_pending_incoming());
}

#[xmtp_common::test(unwrap_try = true)]
async fn raw_incoming_splitter_covers_each_envelope_once_in_order() {
    const ENVELOPES: u64 = 5;
    const ROWS: usize = 2;
    let topic = group_topic(&[7; 16]);
    let mut ledger = Ledger::<BackendBinding>::default();
    let (events, _event_receiver) = mpsc::channel(1);
    let id = ledger.register(&[(topic.clone(), 0)], events);
    let (sender, mut receiver) = mpsc::channel(1);
    ledger.leases.get_mut(&id).unwrap().incoming = Some((
        sender,
        IncomingBatchLimits {
            max_rows: ROWS,
            ..limits()
        },
    ));
    let (update, _) = ledger.prepare_adds(vec![(topic.clone(), 0)]).remove(0);
    ledger.applied(update, vec![(topic.clone(), ENVELOPES)]);
    incoming_frame(&mut receiver).await.unwrap(); // Registered.

    let source: Vec<_> = (1..=ENVELOPES).map(raw_group).collect();
    assert!(
        ledger
            .demux_incoming(&source, |message| message)
            .unwrap()
            .is_empty()
    );
    let mut received = Vec::new();
    let mut deliveries = 0;
    let mut after = 0;
    while received.len() < source.len() {
        let Ok(events) = incoming_frame(&mut receiver).await else {
            panic!("queued delivery")
        };
        assert_eq!(events.len(), 1);
        let IncomingEvent::OrderedBatch(batch) = events.into_iter().next().unwrap() else {
            panic!("ordered batch")
        };
        assert!(batch.envelopes.len() <= ROWS);
        assert_eq!(batch.after, Cursor(after));
        after = batch
            .envelopes
            .last()
            .unwrap()
            .meta
            .as_ref()
            .unwrap()
            .cursor
            .as_ref()
            .unwrap()
            .sequence_id;
        received.extend(batch.envelopes);
        deliveries += 1;
        ledger.flush_incoming(xmtp_common::time::Instant::now());
    }
    assert_eq!(deliveries, 3);
    assert_eq!(received, source);
    assert_eq!(ledger.leases[&id].delivered[&topic], ENVELOPES);
}

#[xmtp_common::test(unwrap_try = true)]
async fn raw_incoming_byte_splitter_covers_each_envelope_once_in_order() {
    const ENVELOPES: u64 = 3;
    let topic = group_topic(&[7; 16]);
    let mut ledger = Ledger::<BackendBinding>::default();
    let (events, _event_receiver) = mpsc::channel(1);
    let id = ledger.register(&[(topic.clone(), 0)], events);
    let (sender, mut receiver) = mpsc::channel(1);
    let source: Vec<_> = (1..=ENVELOPES).map(raw_group).collect();
    ledger.leases.get_mut(&id).unwrap().incoming = Some((
        sender,
        IncomingBatchLimits {
            max_rows: ENVELOPES as usize,
            max_bytes: source[0].encoded_len(),
        },
    ));
    let (update, _) = ledger.prepare_adds(vec![(topic.clone(), 0)]).remove(0);
    ledger.applied(update, vec![(topic.clone(), ENVELOPES)]);
    incoming_frame(&mut receiver).await.unwrap(); // Registered.

    assert!(
        ledger
            .demux_incoming(&source, |message| message)
            .unwrap()
            .is_empty()
    );
    let mut received = Vec::new();
    while received.len() < source.len() {
        let Ok(events) = incoming_frame(&mut receiver).await else {
            panic!("queued delivery")
        };
        let IncomingEvent::OrderedBatch(batch) = events.into_iter().next().unwrap() else {
            panic!("ordered batch")
        };
        assert!(batch.envelopes.len() <= 1);
        received.extend(batch.envelopes);
        ledger.flush_incoming(xmtp_common::time::Instant::now());
    }
    assert_eq!(received, source);
}

#[xmtp_common::test(unwrap_try = true)]
async fn raw_single_envelope_over_the_byte_limit_fails_loudly() {
    let topic = group_topic(&[7; 16]);
    let envelope = raw_group(8);
    let mut ledger = Ledger::<BackendBinding>::default();
    let (events, _event_receiver) = mpsc::channel(1);
    let id = ledger.register(&[(topic.clone(), 0)], events);
    let (sender, mut receiver) = mpsc::channel(1);
    ledger.leases.get_mut(&id).unwrap().incoming = Some((
        sender,
        IncomingBatchLimits {
            max_rows: 1,
            max_bytes: envelope.encoded_len() - 1,
        },
    ));
    let (update, _) = ledger.prepare_adds(vec![(topic.clone(), 0)]).remove(0);
    ledger.applied(update, vec![(topic, 10)]);
    incoming_frame(&mut receiver).await.unwrap(); // Registered.

    assert_eq!(
        ledger.demux_incoming(&[envelope], |message| message)?,
        vec![id]
    );
    let failure = ledger.leases[&id].incoming_failure.lock();
    assert!(matches!(failure.as_ref(), Some(TransportError::Capacity)));
    assert!(failure.as_ref().unwrap().is_retryable());
}

#[xmtp_common::test(unwrap_try = true)]
async fn raw_registration_pauses_when_its_delivery_channel_is_full() {
    let topic = group_topic(&[7; 16]);
    let mut ledger = Ledger::<BackendBinding>::default();
    let (events, _event_receiver) = mpsc::channel(1);
    let id = ledger.register(&[(topic.clone(), 0)], events);
    let (sender, mut receiver) = mpsc::channel(1);
    sender.try_send(Ok(vec![])).unwrap();
    ledger.leases.get_mut(&id).unwrap().incoming = Some((sender, limits()));
    let (update, _) = ledger.prepare_adds(vec![(topic.clone(), 0)]).remove(0);
    ledger.applied(update, vec![(topic.clone(), 10)]);

    assert!(ledger.has_pending_incoming());
    assert!(ledger.leases[&id].paused_at.is_some());
    assert!(ledger.leases[&id].incoming_failure.lock().is_none());
    incoming_frame(&mut receiver).await.unwrap();
    assert!(
        ledger
            .flush_incoming(xmtp_common::time::Instant::now())
            .is_empty()
    );
    assert!(matches!(
        incoming_frame(&mut receiver).await.unwrap().as_slice(),
        [IncomingEvent::Registered { starts, targets }]
            if starts[&topic] == Cursor(0) && targets[&topic] == Cursor(10)
    ));
    assert!(!ledger.has_pending_incoming());
}

#[xmtp_common::test(unwrap_try = true)]
async fn raw_transport_resumes_wire_reads_after_an_incoming_slot_frees() {
    let topic = group_topic(&[7; 16]);
    let (transport, servers) = transport();
    let mut lease = transport
        .lease_ordered(
            vec![(topic.clone(), 0)],
            1,
            IncomingBatchLimits {
                max_rows: 1,
                ..limits()
            },
        )
        .await?;
    let mut server = take_server(&servers);
    let update = server.next_mutate().await;
    server.ack(update.id, vec![(topic, 10)]);
    assert!(matches!(
        incoming(&mut lease).await?,
        IncomingEvent::Registered { .. }
    ));

    server.send(messages(vec![raw_group(8)], vec![]));
    server.send(messages(vec![raw_group(9)], vec![]));
    let next_topic = group_topic(&[8; 16]);
    let mut next = transport
        .lease_ordered(
            vec![(next_topic.clone(), 0)],
            1,
            IncomingBatchLimits {
                max_rows: 1,
                ..limits()
            },
        )
        .await?;
    let update = server.next_mutate().await;
    server.ack(update.id, vec![(next_topic, 10)]);
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(50), next.next_incoming())
            .await
            .is_err(),
        "a full lease must gate the queued registration behind its pending frame"
    );
    let IncomingEvent::OrderedBatch(first) = incoming(&mut lease).await? else {
        panic!("first batch")
    };
    assert_eq!(first.after, Cursor(0));
    assert_eq!(first.envelopes, vec![raw_group(8)]);

    let IncomingEvent::OrderedBatch(second) = incoming(&mut lease).await? else {
        panic!("second batch")
    };
    assert_eq!(second.after, Cursor(8));
    assert_eq!(second.envelopes, vec![raw_group(9)]);
    assert!(matches!(
        incoming(&mut next).await?,
        IncomingEvent::Registered { .. }
    ));
    assert!(lease.incoming_failure.lock().is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn raw_incoming_pause_timeout_drops_the_stalled_lease() {
    let topic = group_topic(&[7; 16]);
    let mut ledger = Ledger::<BackendBinding>::default();
    let (events, _event_receiver) = mpsc::channel(1);
    let id = ledger.register(&[(topic.clone(), 0)], events);
    let (sender, _receiver) = mpsc::channel(1);
    ledger.leases.get_mut(&id).unwrap().incoming = Some((sender, limits()));
    let (update, _) = ledger.prepare_adds(vec![(topic.clone(), 0)]).remove(0);
    ledger.applied(update, vec![(topic.clone(), 10)]);
    assert!(
        ledger
            .demux_incoming(&[raw_group(8)], |message| message)
            .unwrap()
            .is_empty()
    );

    let paused_at = ledger.leases[&id].paused_at.unwrap();
    assert_eq!(
        ledger.flush_incoming(paused_at + INCOMING_PAUSE_TIMEOUT),
        vec![id]
    );
    assert!(matches!(
        ledger.leases[&id].incoming_failure.lock().as_ref(),
        Some(TransportError::Backpressure)
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn raw_task_wakes_at_an_incoming_pause_deadline() {
    let topic = group_topic(&[7; 16]);
    let mut ledger = Ledger::<BackendBinding>::default();
    let (events, _event_receiver) = mpsc::channel(1);
    let id = ledger.register(&[(topic.clone(), 0)], events);
    let (sender, _receiver) = mpsc::channel(1);
    ledger.leases.get_mut(&id).unwrap().incoming = Some((sender, limits()));
    let (update, _) = ledger.prepare_adds(vec![(topic.clone(), 0)]).remove(0);
    ledger.applied(update, vec![(topic, 10)]);
    ledger.demux_incoming(&[raw_group(8)], |message| message)?;

    let mut task = ledger_task(ledger, Outbox::default());
    let (commands, receiver) = mpsc::unbounded_channel();
    task.cmds = receiver;
    task.lease_cmds = commands.downgrade();
    task.ledger.leases.get_mut(&id).unwrap().paused_at = Some(
        xmtp_common::time::Instant::now() - INCOMING_PAUSE_TIMEOUT + Duration::from_millis(10),
    );

    assert!(matches!(
        xmtp_common::time::timeout(WAIT, task.next_step()).await?,
        Step::Retry
    ));
    assert_eq!(
        task.ledger
            .flush_incoming(xmtp_common::time::Instant::now()),
        vec![id]
    );
    drop(commands);
}

#[xmtp_common::test(unwrap_try = true)]
async fn raw_multi_topic_frame_fits_one_queue_slot() {
    const TOPICS: u8 = 100;
    let topics: Vec<_> = (0..TOPICS).map(|id| group_topic(&[id; 16])).collect();
    let (transport, servers) = transport();
    let mut lease = transport
        .lease_ordered(
            topics.iter().cloned().map(|topic| (topic, 0)).collect(),
            1,
            IncomingBatchLimits {
                max_rows: TOPICS.into(),
                max_bytes: 1024 * 1024,
            },
        )
        .await?;
    let mut server = take_server(&servers);
    let update = server.next_mutate().await;
    server.ack(
        update.id,
        topics.iter().cloned().map(|topic| (topic, 1)).collect(),
    );
    assert!(matches!(
        incoming(&mut lease).await?,
        IncomingEvent::Registered { .. }
    ));
    let envelopes: Vec<_> = (0..TOPICS)
        .map(|id| {
            let mut envelope = raw_group(1);
            envelope.meta.as_mut().unwrap().topic = group_msg(1, &[id; 16]).meta.unwrap().topic;
            envelope
        })
        .collect();
    server.send(messages(envelopes.clone(), vec![]));
    let mut received = HashMap::new();
    for _ in 0..TOPICS {
        let IncomingEvent::OrderedBatch(batch) = incoming(&mut lease).await? else {
            panic!("ordered batch");
        };
        assert_eq!(batch.after, Cursor(0));
        assert!(received.insert(batch.topic, batch.envelopes).is_none());
    }
    for (topic, envelope) in topics.into_iter().zip(envelopes) {
        assert_eq!(received.remove(&topic), Some(vec![envelope]));
    }
    assert!(received.is_empty());
    assert!(lease.incoming_failure.lock().is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn raw_bad_order_blocks_the_complete_frame() {
    let topic = group_topic(&[7; 16]);
    let (transport, servers) = transport();
    let mut lease = transport
        .lease_ordered(vec![(topic.clone(), 0)], 8, limits())
        .await?;
    let mut server = take_server(&servers);
    let update = server.next_mutate().await;
    server.ack(update.id, vec![(topic, 10)]);
    incoming(&mut lease).await?;
    server.send(messages(vec![raw_group(8), raw_group(7)], vec![]));
    assert!(matches!(
        incoming(&mut lease).await,
        Err(TransportError::Protocol("cursor order"))
    ));
    assert!(lease.next_incoming().await.is_none());
}

#[xmtp_common::test(flavor = "current_thread", unwrap_try = true)]
async fn cancelled_add_waits_for_pending_remove_before_replacement() {
    let anchor = group_topic(b"anchor");
    let topic = group_topic(&[7; 16]);
    let mut ledger = Ledger::<BackendBinding>::default();
    let (sender, _anchor_events) = mpsc::channel(8);
    ledger.register(&[(anchor.clone(), 0)], sender);
    let (initial_id, initial) = ledger.prepare_adds(vec![(anchor.clone(), 0)]).remove(0);
    let (api, mut server) = mock_pair();
    let wire = BidiConnection::open(&api, initial).await?;
    let mut task = ledger_task(ledger, Outbox::default());
    let (commands, receiver) = mpsc::unbounded_channel();
    task.lease_cmds = commands.downgrade();
    task.cmds = receiver;
    task.conn = Some(wire);
    assert_eq!(server.next_mutate().await.id, initial_id);
    task.wire_event(Event::Applied {
        id: initial_id,
        targets: vec![(anchor, 0)],
    });

    // The older reader queues a remove while the original add is still unsent.
    let mut original = Vec::new();
    for floor in [10, 0] {
        let (reply, response) = oneshot::channel();
        task.lease(vec![(topic.clone(), floor)], 8, Some(limits()), reply)
            .await;
        original.push(response.await??);
    }
    for lease in original {
        task.deref(lease.id);
    }
    task.flush_outbox();
    let removal = server.next_mutate().await;
    assert!(removal.adds.is_empty());
    assert_eq!(removal.removes.len(), 1);
    assert_eq!(removal.removes[0].topic, topic.cloned_vec());

    // A new holder arrives before that removal is acknowledged.
    let (reply, response) = oneshot::channel();
    task.lease(vec![(topic.clone(), 0)], 8, Some(limits()), reply)
        .await;
    let mut replacement = response.await??;
    task.wire_event(Event::Applied {
        id: removal.id,
        targets: vec![],
    });
    task.flush_outbox();
    let addition = server.next_mutate().await;
    assert!(addition.removes.is_empty());
    assert_eq!(addition.adds.len(), 1);
    assert_eq!(addition.adds[0].topic.as_ref()?.topic, topic.cloned_vec());
    assert_eq!(addition.adds[0].cursor.as_ref()?.sequence_id, 0);
    task.wire_event(Event::Applied {
        id: addition.id,
        targets: vec![(topic.clone(), 8)],
    });
    let IncomingEvent::Registered { starts, targets } = incoming(&mut replacement).await? else {
        panic!("replacement registration");
    };
    assert_eq!(starts, [(topic.clone(), Cursor(0))].into());
    assert_eq!(targets, [(topic.clone(), Cursor(8))].into());
    task.wire_event(Event::GroupMessages {
        messages: vec![raw_group(8)],
    });
    let IncomingEvent::OrderedBatch(batch) = incoming(&mut replacement).await? else {
        panic!("replacement batch");
    };
    assert_eq!(batch.topic, topic);
    assert_eq!(batch.after, Cursor(0));
    assert_eq!(batch.envelopes, vec![raw_group(8)]);
    assert!(replacement.incoming_failure.lock().is_none());
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(50), server.from_client.recv())
            .await
            .is_err(),
        "the replacement must send exactly one add after the pending remove"
    );
}
