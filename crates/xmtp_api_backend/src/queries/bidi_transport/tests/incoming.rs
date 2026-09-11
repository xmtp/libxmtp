use super::*;

fn raw_group(sequence: u64) -> ServerEnvelope {
    let mut envelope = group_msg(sequence, &[7; 16]);
    envelope.meta.as_mut().unwrap().message_hash = Some(backend_v1::MessageHash {
        hash: Some(backend_v1::message_hash::Hash::Sha256(vec![8; 32])),
    });
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

#[rstest::rstest]
#[case::queue(limits(), true)]
#[case::rows(IncomingBatchLimits { max_rows: 1, ..limits() }, false)]
#[case::bytes(IncomingBatchLimits { max_bytes: 1, ..limits() }, false)]
#[xmtp_common::test(unwrap_try = true)]
async fn raw_capacity_error_survives_a_full_delivery_channel(
    #[case] limits: IncomingBatchLimits,
    #[case] retryable: bool,
) {
    let topic = group_topic(&[7; 16]);
    let (transport, servers) = transport();
    let mut lease = transport
        .lease_ordered(vec![(topic.clone(), 0)], 1, limits)
        .await
        .unwrap();
    let mut server = take_server(&servers);
    let update = server.next_mutate().await;
    server.ack(update.id, vec![(topic.clone(), 10)]);
    // The registration occupies the only slot. The delivery must not be copied.
    server.send(messages(vec![raw_group(8), raw_group(9)], vec![]));
    xmtp_common::wait_for_some(|| async { lease.incoming_failure.lock().is_some().then_some(()) })
        .await;
    assert!(matches!(
        incoming(&mut lease).await.unwrap(),
        IncomingEvent::Registered { .. }
    ));
    let error = incoming(&mut lease).await.unwrap_err();
    assert_eq!(error.is_retryable(), retryable);
    if retryable {
        assert!(matches!(error, TransportError::Backpressure));
    } else {
        assert!(matches!(error, TransportError::Capacity));
    }
    assert!(lease.next_incoming().await.is_none());
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
