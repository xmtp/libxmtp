use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn first_lease_opens_the_wire_with_its_cursored_adds() {
    let (transport, servers) = transport();
    assert!(servers.lock().unwrap().is_empty(), "wire must open lazily");

    let group = group_topic(b"g1");
    let welcome = welcome_topic(b"i1");
    let _lease = transport
        .lease(
            vec![(group.clone(), 5), (welcome.clone(), 0)],
            DEFAULT_LEASE_DEPTH,
        )
        .await?;

    let mut server = take_server(&servers);
    let mutate = server.next_mutate().await;
    assert_eq!(mutate.adds.len(), 2);
    assert_eq!(
        mutate.adds[0].topic.as_ref().unwrap().topic,
        group.cloned_vec()
    );
    assert_eq!(mutate.adds[0].cursor.as_ref().unwrap().sequence_id, 5);
    assert_eq!(
        mutate.adds[1].topic.as_ref().unwrap().topic,
        welcome.cloned_vec()
    );
    assert_eq!(mutate.adds[1].cursor.as_ref().unwrap().sequence_id, 0);
    assert!(mutate.removes.is_empty());
    assert_ne!(mutate.id, 0, "update IDs must be nonzero");
}

#[xmtp_common::test(unwrap_try = true)]
async fn second_lease_is_a_cursored_re_add_on_the_open_wire() {
    let (transport, servers) = transport();
    let shared = group_topic(b"g1");
    let _first = transport.lease(vec![(shared.clone(), 40)], 8).await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack_empty(first.id);
    let fresh = group_topic(b"g2");
    let _second = transport
        .lease(vec![(shared.clone(), 7), (fresh.clone(), 0)], 8)
        .await?;
    assert!(
        servers.lock().unwrap().is_empty(),
        "second lease must use the open wire"
    );
    let remove = server.next_mutate().await;
    assert!(remove.adds.is_empty());
    assert_eq!(
        remove.removes,
        vec![backend_v1::Topic {
            topic: shared.cloned_vec()
        }]
    );
    let fresh_add = server.next_mutate().await;
    assert_eq!(fresh_add.adds.len(), 1);
    assert_eq!(
        fresh_add.adds[0].topic.as_ref().unwrap().topic,
        fresh.cloned_vec()
    );
    assert_eq!(fresh_add.adds[0].cursor.as_ref().unwrap().sequence_id, 0);
    server.ack_empty(remove.id);
    server.ack_empty(fresh_add.id);
    let second = server.next_mutate().await;
    assert_eq!(second.adds.len(), 1);
    assert_eq!(
        second.adds[0].topic.as_ref().unwrap().topic,
        shared.cloned_vec()
    );
    assert_eq!(second.adds[0].cursor.as_ref().unwrap().sequence_id, 7);
    assert!(first.id < remove.id && remove.id < fresh_add.id && fresh_add.id < second.id);
}

#[xmtp_common::test(unwrap_try = true)]
async fn deliveries_demux_by_topic() {
    let (transport, servers) = transport();
    let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut beta = transport.lease(vec![(group_topic(b"g2"), 0)], 8).await?;
    let mut inst = transport.lease(vec![(welcome_topic(b"i1"), 0)], 8).await?;
    let mut server = take_server(&servers);

    for _ in 0..3 {
        let wave = server.next_mutate().await;
        server.ack_empty(wave.id);
    }
    for lease in [&mut alpha, &mut beta, &mut inst] {
        assert!(matches!(
            recv(lease).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    let (m1, m2, m3) = (
        group_msg(1, b"g1"),
        group_msg(2, b"g2"),
        group_msg(3, b"g1"),
    );
    let unroutable = GroupMessage::default();
    let w = welcome_msg(9, b"i1");
    server.send(messages(
        vec![m1.clone(), unroutable, m2.clone(), m3.clone()],
        vec![w.clone()],
    ));

    match recv(&mut alpha).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![m1, m3]),
        _ => panic!("alpha expected its two group messages"),
    }
    match recv(&mut beta).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![m2]),
        _ => panic!("beta expected its one group message"),
    }
    match recv(&mut inst).await {
        Some(LeaseEvent::WelcomeMessages(got)) => assert_eq!(got, vec![w]),
        _ => panic!("inst expected its welcome"),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn rotation_ordered_replay_delivers_every_topic() {
    let (transport, servers) = transport();
    let mut alpha = transport
        .lease(vec![(group_topic(b"g1"), 0), (group_topic(b"g2"), 0)], 8)
        .await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack(
        first.id,
        vec![(group_topic(b"g1"), 6), (group_topic(b"g2"), 4)],
    );

    server.send(messages(
        vec![group_msg(5, b"g1"), group_msg(6, b"g1")],
        vec![],
    ));
    server.send(messages(
        vec![group_msg(3, b"g2"), group_msg(4, b"g2")],
        vec![],
    ));

    match recv(&mut alpha).await {
        Some(LeaseEvent::GroupMessages(got)) => {
            assert_eq!(got, vec![group_msg(5, b"g1"), group_msg(6, b"g1")])
        }
        _ => panic!("expected the first rotation turn"),
    }
    match recv(&mut alpha).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
            got,
            vec![group_msg(3, b"g2"), group_msg(4, b"g2")],
            "a later turn opening below an earlier turn's ids must still deliver"
        ),
        _ => panic!("expected the second rotation turn"),
    }
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn covered_live_frame_is_dropped() {
    let (transport, servers) = transport();
    let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack(first.id, vec![(group_topic(b"g1"), 2)]);
    server.send(messages(
        vec![group_msg(1, b"g1"), group_msg(2, b"g1")],
        vec![],
    ));
    match recv(&mut alpha).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got.len(), 2),
        _ => panic!("alpha expected its replay"),
    }
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));

    server.send(messages(
        vec![group_msg(2, b"g1"), group_msg(3, b"g1")],
        vec![],
    ));
    match recv(&mut alpha).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
            got,
            vec![group_msg(3, b"g1")],
            "a covered live frame must be dropped, not re-delivered"
        ),
        _ => panic!("alpha expected only the fresh live message"),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn shared_topic_fans_out_to_every_lease() {
    let (transport, servers) = transport();
    let shared = group_topic(b"g1");
    let mut alpha = transport.lease(vec![(shared.clone(), 0)], 8).await?;
    let mut beta = transport.lease(vec![(shared.clone(), 0)], 8).await?;
    let mut server = take_server(&servers);

    for _ in 0..1 {
        let wave = server.next_mutate().await;
        server.ack_empty(wave.id);
    }
    let m = group_msg(1, b"g1");
    server.send(messages(vec![m.clone()], vec![]));

    for lease in [&mut alpha, &mut beta] {
        assert!(matches!(
            recv(lease).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        match recv(lease).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![m.clone()]),
            _ => panic!("both leases must receive the shared delivery"),
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn markers_route_to_their_owners() {
    let (transport, servers) = transport();
    let (ga, gb) = (group_topic(b"g1"), group_topic(b"g2"));
    let mut alpha = transport.lease(vec![(ga.clone(), 0)], 8).await?;
    let mut beta = transport.lease(vec![(gb.clone(), 0)], 8).await?;
    let mut server = take_server(&servers);
    let update_a = server.next_mutate().await;
    let update_b = server.next_mutate().await;
    server.ack(update_a.id, vec![(ga, 1)]);
    server.ack(update_b.id, vec![(gb, 2)]);
    server.send(messages(vec![group_msg(2, b"g2")], vec![]));
    match recv(&mut beta).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(2, b"g2")]),
        _ => panic!("beta expected only its own topic"),
    }
    assert!(matches!(
        recv(&mut beta).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(100), alpha.next())
            .await
            .is_err(),
        "beta's completion must not complete alpha"
    );
    server.send(messages(vec![group_msg(1, b"g1")], vec![]));
    match recv(&mut alpha).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(1, b"g1")]),
        _ => panic!("alpha expected only its own topic"),
    }
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(100), beta.next())
            .await
            .is_err(),
        "alpha's completion must not complete beta again"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn deref_is_refcounted_and_last_lease_closes_the_wire() {
    let (transport, servers) = transport();
    let (shared, exclusive) = (group_topic(b"g1"), group_topic(b"g2"));
    let alpha = transport
        .lease(vec![(shared.clone(), 0), (exclusive.clone(), 0)], 8)
        .await?;
    let beta = transport.lease(vec![(shared.clone(), 0)], 8).await?;
    let mut server = take_server(&servers);
    let initial = server.next_mutate().await;
    server.ack_empty(initial.id);

    drop(alpha);
    let removal = server.next_mutate().await;
    assert!(removal.adds.is_empty());
    assert_eq!(
        removal.removes,
        vec![backend_v1::Topic {
            topic: exclusive.cloned_vec()
        }],
        "the shared topic still has a holder and must stay"
    );

    drop(beta);
    server.request_stream_ended().await;

    xmtp_common::time::sleep(Duration::from_millis(50)).await; // let a would-be abort land
    server.send(started(30_000));

    let _again = transport.lease(vec![(shared, 3)], 8).await?;
    let mut second = take_server(&servers);
    assert_eq!(second.next_mutate().await.adds.len(), 1);
}

#[xmtp_common::test(flavor = "current_thread", unwrap_try = true)]
async fn wire_session_span_closes_with_a_reason_on_every_release_path() {
    use tracing_subscriber::layer::SubscriberExt;

    let closes = WireCloseLog::default();
    let _guard =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(closes.clone()));

    let (suspended, servers_a) = transport();
    let alpha = suspended.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut first = take_server(&servers_a);
    first.next_mutate().await;
    suspended.suspend().await?;
    assert_eq!(close_reasons(&closes, 1).await[0], "suspend");
    drop((alpha, first, suspended));

    let (dying, servers_b) = transport();
    let beta = dying.lease(vec![(group_topic(b"g2"), 0)], 8).await?;
    let mut second = take_server(&servers_b);
    second.next_mutate().await;
    drop(second);
    assert_eq!(close_reasons(&closes, 2).await[1], "wire_end");
    drop((beta, dying));

    let (retiring, servers_c) = transport();
    let gamma = retiring.lease(vec![(group_topic(b"g3"), 0)], 8).await?;
    let mut third = take_server(&servers_c);
    third.next_mutate().await;
    drop(gamma);
    assert_eq!(close_reasons(&closes, 3).await[2], "idle");
    drop((third, retiring));
}

#[xmtp_common::test(unwrap_try = true)]
async fn deref_purges_only_the_dropped_leases_unsent_updates() {
    let mut ledger = Ledger::<BackendBinding>::default();
    let (a, _events_a) = mpsc::channel(8);
    let (b, _events_b) = mpsc::channel(8);
    let (g1, g3, g4) = (group_topic(b"g1"), group_topic(b"g3"), group_topic(b"g4"));
    let alpha = ledger.register(&[(g1.clone(), 0), (g4.clone(), 0)], a);
    let beta = ledger.register(&[(g3.clone(), 0)], b);
    let mut outbox = Outbox::default();
    outbox
        .updates
        .extend(ledger.prepare_adds(vec![(g1.clone(), 0)]));
    outbox
        .updates
        .extend(ledger.prepare_removes(vec![group_topic(b"g2")]));
    outbox.updates.extend(ledger.prepare_adds(vec![(g3, 0)]));
    outbox
        .updates
        .extend(ledger.prepare_adds(vec![(g4.clone(), 0)]));
    let mut task = ledger_task(ledger, outbox);
    let removed: HashSet<_> = task.drop_leases(vec![alpha]).into_iter().collect();
    assert_eq!(removed, HashSet::from([g1, g4]));
    let remaining: Vec<_> = task.outbox.updates.iter().map(|(id, _)| *id).collect();
    assert_eq!(remaining, vec![2, 3]);
    assert_eq!(
        task.ledger
            .pending_updates
            .keys()
            .copied()
            .collect::<HashSet<_>>(),
        HashSet::from([2, 3])
    );
    assert!(task.ledger.leases.contains_key(&beta));
}

#[xmtp_common::test(unwrap_try = true)]
async fn empty_lease_is_refused_without_opening_the_wire() {
    let (transport, servers) = transport();
    let refused = transport.lease(vec![], 8).await;
    assert!(matches!(refused, Err(TransportError::Empty)));
    assert!(servers.lock().unwrap().is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_retire_remove_is_acked_without_closing_the_transport() {
    let (transport, servers) = transport();
    let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let beta = transport.lease(vec![(group_topic(b"g2"), 0)], 8).await?;
    let mut server = take_server(&servers);
    for _ in 0..2 {
        let wave = server.next_mutate().await;
        server.ack_empty(wave.id);
    }
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));

    drop(beta);
    let removes = server.next_mutate().await;
    assert!(removes.adds.is_empty());
    assert_eq!(
        removes.removes,
        vec![backend_v1::Topic {
            topic: group_topic(b"g2").cloned_vec()
        }]
    );
    assert_ne!(removes.id, 0, "a remove update must have a nonzero ID");

    server.ack_empty(removes.id);
    server.send(messages(vec![group_msg(7, b"g1")], vec![]));
    match recv(&mut alpha).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(7, b"g1")]),
        _ => panic!("alpha must keep receiving after a retire ack"),
    }
    let _gamma = transport
        .lease(vec![(group_topic(b"g3"), 0)], 8)
        .await
        .expect("the transport must survive a retire ack");
}
