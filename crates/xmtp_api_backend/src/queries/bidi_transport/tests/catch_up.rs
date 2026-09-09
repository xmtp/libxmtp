use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn target_zero_completes_only_after_applied() {
    let (transport, servers) = transport();
    let topic = group_topic(b"empty");
    let mut lease = transport.lease(vec![(topic.clone(), 5)], 8).await?;
    let mut server = take_server(&servers);
    let update = server.next_mutate().await;
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(100), lease.next())
            .await
            .is_err()
    );
    server.ack(update.id, vec![(topic, 0)]);
    assert!(matches!(
        recv(&mut lease).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
    server.send(messages(vec![group_msg(6, b"empty")], vec![]));
    match recv(&mut lease).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(6, b"empty")]),
        _ => panic!("the empty registration must continue receiving messages"),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn target_equal_to_floor_needs_no_delivery() {
    let (transport, servers) = transport();
    let topic = group_topic(b"equal");
    let mut lease = transport.lease(vec![(topic.clone(), 5)], 8).await?;
    let mut server = take_server(&servers);
    let update = server.next_mutate().await;
    server.ack(update.id, vec![(topic, 5)]);
    assert!(matches!(
        recv(&mut lease).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
    server.send(messages(
        vec![group_msg(5, b"equal"), group_msg(6, b"equal")],
        vec![],
    ));
    match recv(&mut lease).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(6, b"equal")]),
        _ => panic!("the floor must remain exclusive after completion"),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_topic_absent_from_targets_inherits_the_registration() {
    // Drive the same ledger through a no-op add acknowledgement. Normal
    // lease joins need no update when the registration can already serve them.
    let topic = group_topic(b"shared");
    let mut ledger = Ledger::<BackendBinding>::default();
    let (tx_a, mut events_a) = mpsc::channel(8);
    let alpha = ledger.register(&[(topic.clone(), 0)], tx_a);
    let (initial_id, initial) = ledger.prepare_adds(vec![(topic.clone(), 0)]).remove(0);
    let (api, mut server) = mock_pair();
    let mut connection = BidiConnection::open(&api, initial).await?;
    server.next_mutate().await;
    assert!(matches!(
        connection.next().await,
        Some(Event::Started { .. })
    ));
    server.ack(initial_id, vec![(topic.clone(), 10)]);
    let Some(Event::Applied { id, targets }) = connection.next().await else {
        panic!("expected Applied");
    };
    assert!(ledger.applied(id, targets).is_empty());
    assert!(ledger.recheck().is_empty());

    let (tx_b, mut events_b) = mpsc::channel(8);
    let beta = ledger.register(&[(topic.clone(), 5)], tx_b);
    assert!(ledger.join(beta, vec![(topic.clone(), 5)]).is_empty());
    let (noop_id, noop) = ledger.prepare_adds(vec![(topic.clone(), 5)]).remove(0);
    connection.mutate(noop).await?;
    server.next_mutate().await;
    server.ack_empty(noop_id);
    let Some(Event::Applied { id, targets }) = connection.next().await else {
        panic!("expected Applied");
    };
    assert_eq!(id, noop_id);
    assert!(
        targets.is_empty(),
        "an active topic is absent from added_targets"
    );
    assert!(ledger.applied(id, targets).is_empty());
    assert!(ledger.recheck().is_empty());
    assert_eq!(ledger.leases[&alpha].unmet, 1);
    assert_eq!(ledger.leases[&beta].unmet, 1);
    assert!(events_a.try_recv().is_err());
    assert!(
        events_b.try_recv().is_err(),
        "an empty target list is not completion"
    );

    server.send(messages(vec![group_msg(10, b"shared")], vec![]));
    let Some(Event::GroupMessages { messages }) = connection.next().await else {
        panic!("expected messages");
    };
    assert!(
        ledger
            .demux(
                messages,
                DeliveryKind::Group,
                BackendBinding::group_topic,
                BackendBinding::group_cursor,
                LeaseEvent::GroupMessages
            )
            .is_empty()
    );
    assert!(ledger.recheck().is_empty());
    for events in [&mut events_a, &mut events_b] {
        match events.recv().await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(10, b"shared")])
            }
            _ => panic!("both holders must receive the target"),
        }
        assert!(matches!(
            events.recv().await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn two_holders_keep_independent_monotonic_floors() {
    let (transport, servers) = transport();
    let topic = group_topic(b"shared");
    let mut alpha = transport.lease(vec![(topic.clone(), 0)], 8).await?;
    let mut beta = transport.lease(vec![(topic.clone(), 5)], 8).await?;
    let mut server = take_server(&servers);
    let initial = server.next_mutate().await;
    server.ack(initial.id, vec![(topic, 10)]);
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(100), server.from_client.recv())
            .await
            .is_err(),
        "a holder at or above the wire floor needs no update"
    );
    let all: Vec<_> = [1, 2, 5, 6, 10]
        .into_iter()
        .map(|id| group_msg(id, b"shared"))
        .collect();
    server.send(messages(all.clone(), vec![]));
    match recv(&mut alpha).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, all),
        _ => panic!("alpha must receive its full suffix"),
    }
    match recv(&mut beta).await {
        Some(LeaseEvent::GroupMessages(got)) => {
            assert_eq!(got, vec![group_msg(6, b"shared"), group_msg(10, b"shared")])
        }
        _ => panic!("beta must receive only messages above its floor"),
    }
    for lease in [&mut alpha, &mut beta] {
        assert!(matches!(
            recv(lease).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn lower_cursor_readd_routes_queued_messages_at_applied_boundaries() {
    let (transport, servers) = transport();
    let topic = group_topic(b"shared");
    let mut alpha = transport.lease(vec![(topic.clone(), 50)], 8).await?;
    let mut server = take_server(&servers);
    let initial = server.next_mutate().await;
    server.ack(initial.id, vec![(topic.clone(), 60)]);
    let first: Vec<_> = (51..=60).map(|id| group_msg(id, b"shared")).collect();
    server.send(messages(first.clone(), vec![]));
    match recv(&mut alpha).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, first),
        _ => panic!("alpha must receive its initial history"),
    }
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));

    let mut beta = transport.lease(vec![(topic.clone(), 40)], 8).await?;
    let remove = server.next_mutate().await;
    assert!(remove.adds.is_empty());
    assert_eq!(
        remove.removes,
        vec![backend_v1::Topic {
            topic: topic.cloned_vec()
        }]
    );
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(100), server.from_client.recv())
            .await
            .is_err(),
        "the add must wait for the remove acknowledgement"
    );
    // This message was queued by the old registration before removal.
    server.send(messages(vec![group_msg(61, b"shared")], vec![]));
    match recv(&mut alpha).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(61, b"shared")]),
        _ => panic!("the old holder must receive the queued message"),
    }
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(100), beta.next())
            .await
            .is_err(),
        "the new holder must not receive the old registration"
    );

    server.ack_empty(remove.id);
    let add = server.next_mutate().await;
    assert!(add.id > remove.id);
    assert!(add.removes.is_empty());
    assert_eq!(add.adds.len(), 1);
    assert_eq!(
        add.adds[0].topic.as_ref().unwrap().topic,
        topic.cloned_vec()
    );
    assert_eq!(add.adds[0].cursor.as_ref().unwrap().sequence_id, 40);
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(100), beta.next())
            .await
            .is_err()
    );
    server.ack(add.id, vec![(topic, 62)]);
    let replayed: Vec<_> = (41..=62).map(|id| group_msg(id, b"shared")).collect();
    server.send(messages(replayed.clone(), vec![]));
    match recv(&mut alpha).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(62, b"shared")]),
        _ => panic!("alpha must skip all overlap, including queued message 61"),
    }
    match recv(&mut beta).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, replayed),
        _ => panic!("beta must receive every message from 41, including 61"),
    }
    assert!(matches!(
        recv(&mut beta).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(100), alpha.next())
            .await
            .is_err(),
        "an old holder must not receive another completion"
    );
}

#[xmtp_common::test(flavor = "current_thread", unwrap_try = true)]
async fn unknown_applied_warns_without_disturbing_delivery() {
    let log = xmtp_common::traced_test::TestWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(log.clone())
        .with_ansi(false)
        .without_time()
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let (transport, servers) = transport();
    let mut lease = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut server = take_server(&servers);
    let update = server.next_mutate().await;
    let unknown_id = update.id + 1;
    server.send(subscribe_response::Response::Applied(
        subscribe_response::Applied {
            id: unknown_id,
            added_targets: vec![],
        },
    ));
    server.ack(update.id, vec![(group_topic(b"g1"), 2)]);
    server.send(messages(
        vec![group_msg(1, b"g1"), group_msg(2, b"g1")],
        vec![],
    ));
    match recv(&mut lease).await {
        Some(LeaseEvent::GroupMessages(got)) => {
            assert_eq!(got, vec![group_msg(1, b"g1"), group_msg(2, b"g1")])
        }
        _ => panic!("an unknown acknowledgement must not disturb delivery"),
    }
    assert!(matches!(
        recv(&mut lease).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
    server.send(subscribe_response::Response::Applied(
        subscribe_response::Applied {
            id: update.id,
            added_targets: vec![],
        },
    ));
    server.send(messages(
        vec![group_msg(2, b"g1"), group_msg(3, b"g1")],
        vec![],
    ));
    match recv(&mut lease).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(3, b"g1")]),
        _ => panic!("a repeated acknowledgement must preserve the delivery guard"),
    }
    let output = log.as_string();
    let warnings: Vec<_> = output
        .lines()
        .filter(|line| line.contains("received Applied for an unknown update"))
        .collect();
    assert_eq!(warnings.len(), 2);
    for id in [unknown_id, update.id] {
        assert!(
            warnings
                .iter()
                .any(|line| line.contains("WARN") && line.contains(&format!("id={id}")))
        );
    }
    assert!(lease.events.try_recv().is_err());
}
