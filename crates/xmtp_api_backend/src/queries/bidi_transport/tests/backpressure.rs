use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn slow_lease_is_dropped_without_blocking_siblings() {
    assert_full_lease_does_not_clone();
    let (transport, servers) = transport();
    let shared = group_topic(b"g1");
    let mut slow = transport.lease(vec![(shared.clone(), 0)], 1).await?;
    let mut fast = transport.lease(vec![(shared.clone(), 0)], 8).await?;
    let mut server = take_server(&servers);
    let initial = server.next_mutate().await;
    server.ack_empty(initial.id);
    for lease in [&mut slow, &mut fast] {
        assert!(matches!(
            recv(lease).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    let (m1, m2, m3) = (
        group_msg(1, b"g1"),
        group_msg(2, b"g1"),
        group_msg(3, b"g1"),
    );
    server.send(messages(vec![m1.clone()], vec![]));
    server.send(messages(vec![m2.clone()], vec![]));
    server.send(messages(vec![m3.clone()], vec![]));

    for expected in [&m1, &m2, &m3] {
        match recv(&mut fast).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![expected.clone()]),
            _ => panic!("fast lease must receive every delivery"),
        }
    }
    assert!(matches!(
        recv(&mut slow).await,
        Some(LeaseEvent::GroupMessages(_))
    ));
    assert!(
        recv(&mut slow).await.is_none(),
        "wedged lease must be closed"
    );
}

/// Count payload copies separately from delivery to prove that a full queue
/// cannot allocate another batch before the lease is dropped.
fn assert_full_lease_does_not_clone() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountedMessage {
        envelope: ServerEnvelope,
        copies: Arc<AtomicUsize>,
    }

    impl Clone for CountedMessage {
        fn clone(&self) -> Self {
            self.copies.fetch_add(1, Ordering::SeqCst);
            Self {
                envelope: self.envelope.clone(),
                copies: self.copies.clone(),
            }
        }
    }

    let mut ledger = Ledger::<BackendBinding>::default();
    let topic = group_topic(b"g1");
    let subs = vec![(topic.clone(), 0)];
    let (slow, _slow_events) = mpsc::channel(1);
    slow.try_send(LeaseEvent::CatchUpComplete).unwrap();
    let slow_id = ledger.register(&subs, slow);
    let (fast, mut fast_events) = mpsc::channel(1);
    ledger.register(&subs, fast);
    let (id, _) = ledger.prepare_adds(subs).remove(0);
    ledger.applied(id, vec![(topic, 1)]);
    let copies = Arc::new(AtomicUsize::new(0));
    let dropped = ledger.demux(
        vec![CountedMessage {
            envelope: group_msg(1, b"g1"),
            copies: copies.clone(),
        }],
        DeliveryKind::Group,
        |message| BackendBinding::group_topic(&message.envelope),
        |message| BackendBinding::group_cursor(&message.envelope),
        |messages| {
            LeaseEvent::GroupMessages(
                messages
                    .into_iter()
                    .map(|message| message.envelope)
                    .collect(),
            )
        },
    );
    assert_eq!(dropped, vec![slow_id]);
    assert_eq!(
        copies.load(Ordering::SeqCst),
        1,
        "only the ready lease may copy the payload"
    );
    match fast_events.try_recv().unwrap() {
        LeaseEvent::GroupMessages(messages) => assert_eq!(messages, vec![group_msg(1, b"g1")]),
        _ => panic!("the ready lease must receive the message"),
    }
}
