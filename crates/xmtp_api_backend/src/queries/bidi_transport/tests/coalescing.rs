use super::*;

#[xmtp_common::test(flavor = "current_thread", unwrap_try = true)]
async fn queued_leases_coalesce_during_dial_and_deliver_once() {
    const QUEUED_LEASES: usize = 150;
    const REGISTRATION_UPDATES: usize = 2;
    let (dial_started, started) = oneshot::channel();
    let (release, dial_gate) = oneshot::channel();
    let dial = Arc::new(Mutex::new(Some((dial_started, dial_gate))));
    let servers: Servers = Arc::default();
    let sink = servers.clone();
    let transport = BidiTransport::new(
        move |initial| {
            let (started, gate) = dial.lock().unwrap().take().expect("only one dial");
            let (api, server) = mock_pair();
            sink.lock().unwrap().push(server);
            started.send(()).unwrap();
            async move {
                gate.await.unwrap();
                BidiConnection::open(&api, initial)
                    .await
                    .map_err(OpenError::new)
            }
        },
        false,
    );
    let (reply, opening) = oneshot::channel();
    transport
        .cmds
        .send(Cmd::Lease {
            subs: vec![(group_topic(b"anchor"), 0)],
            depth: 8,
            reply,
        })
        .unwrap();
    started.await?;
    let mut replies = Vec::new();
    for index in 0..QUEUED_LEASES {
        let (reply, lease) = oneshot::channel();
        transport
            .cmds
            .send(Cmd::Lease {
                subs: vec![(group_topic(&index.to_le_bytes()), 0)],
                depth: 8,
                reply,
            })
            .unwrap();
        replies.push(lease);
    }
    release.send(()).unwrap();
    let mut anchor = xmtp_common::time::timeout(WAIT, opening).await???;
    let mut leases = Vec::new();
    for reply in replies {
        leases.push(xmtp_common::time::timeout(WAIT, reply).await???);
    }
    let mut server = take_server(&servers);
    let initial = server.next_mutate().await;
    let batch = server.next_mutate().await;
    assert_eq!(initial.adds.len(), 1);
    assert_eq!(batch.adds.len(), QUEUED_LEASES);
    assert!(initial.id < batch.id);
    assert!(initial.removes.is_empty() && batch.removes.is_empty());
    let topics: HashSet<_> = batch
        .adds
        .iter()
        .map(|query| query.topic.as_ref().unwrap().topic.clone())
        .collect();
    assert_eq!(topics.len(), QUEUED_LEASES);
    assert_eq!(
        topics,
        (0..QUEUED_LEASES)
            .map(|index| group_topic(&index.to_le_bytes()).cloned_vec())
            .collect()
    );
    for update in [&initial, &batch] {
        let targets = update
            .adds
            .iter()
            .map(|query| {
                (
                    Topic::try_from(query.topic.as_ref().unwrap().topic.clone()).unwrap(),
                    1,
                )
            })
            .collect();
        server.ack(update.id, targets);
    }
    let first: Vec<_> = (0..QUEUED_LEASES)
        .map(|index| group_msg(1, &index.to_le_bytes()))
        .collect();
    server.send(messages(vec![group_msg(1, b"anchor")], vec![]));
    server.send(messages(first.clone(), vec![]));
    let second: Vec<_> = (0..QUEUED_LEASES)
        .map(|index| group_msg(2, &index.to_le_bytes()))
        .collect();
    server.send(messages(first, vec![]));
    server.send(messages(second, vec![]));
    for (index, lease) in leases.iter_mut().enumerate() {
        match recv(lease).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(1, &index.to_le_bytes())])
            }
            _ => panic!("each lease must receive its first message"),
        }
        assert!(matches!(
            recv(lease).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        match recv(lease).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(2, &index.to_le_bytes())])
            }
            _ => panic!("replay must not repeat the first message"),
        }
        assert!(lease.events.try_recv().is_err());
    }
    assert!(matches!(
        recv(&mut anchor).await,
        Some(LeaseEvent::GroupMessages(_))
    ));
    assert!(matches!(
        recv(&mut anchor).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
    xmtp_common::time::timeout(WAIT, transport.resume()).await??;
    assert_eq!(server.updates.len(), REGISTRATION_UPDATES);
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(50), server.from_client.recv())
            .await
            .is_err(),
        "150 queued leases need exactly two registration updates including the opening lease"
    );

    drop(leases);
    let removes = server.next_mutate().await;
    assert!(removes.id > batch.id);
    assert!(removes.adds.is_empty());
    assert_eq!(removes.removes.len(), QUEUED_LEASES);
    server.ack_empty(removes.id);
    xmtp_common::time::timeout(WAIT, transport.resume()).await??;
    assert_eq!(server.updates.len(), REGISTRATION_UPDATES + 1);
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(50), server.from_client.recv())
            .await
            .is_err(),
        "all queued derefs must share one remove update"
    );
}

#[rstest::rstest]
#[case::topic_cap(false)]
#[case::byte_budget(true)]
#[xmtp_common::test(flavor = "current_thread", unwrap_try = true)]
async fn coalescing_keeps_limits_boundaries_and_ack_ids(#[case] byte_limited: bool) {
    let mut ledger = Ledger::<BackendBinding>::default();
    if byte_limited {
        ledger.chunk_bytes = 2 * topic_wire_cost(&group_topic(b"a"));
    } else {
        ledger.chunk_cap = 2;
    }
    let (_, initial) = ledger
        .prepare_adds(vec![(group_topic(b"anchor"), 0)])
        .remove(0);
    let (api, mut server) = mock_pair();
    let wire = BidiConnection::open(&api, initial).await.unwrap();
    let mut task = ledger_task(ledger, Outbox::default());
    task.conn = Some(wire);
    for name in [b"a", b"b", b"c"] {
        task.outbox
            .updates
            .extend(task.ledger.prepare_adds(vec![(group_topic(name), 0)]));
    }
    for name in [b"d", b"e", b"f"] {
        task.outbox
            .updates
            .extend(task.ledger.prepare_removes(vec![group_topic(name)]));
    }
    for _ in 0..2 {
        task.outbox
            .updates
            .extend(task.ledger.prepare_adds(vec![(group_topic(b"g"), 0)]));
    }
    task.flush_outbox();
    assert!(task.outbox.is_empty());
    let first = server.next_mutate().await;
    server.ack_empty(first.id);
    task.ledger
        .applied(first.id, vec![(group_topic(b"anchor"), 0)]);
    let expected = [
        (2, vec![b"a", b"b"], vec![]),
        (4, vec![b"c"], vec![]),
        (5, vec![], vec![b"d", b"e"]),
        (7, vec![], vec![b"f"]),
        (8, vec![b"g"], vec![]),
        (9, vec![b"g"], vec![]),
    ];
    assert_eq!(
        task.ledger
            .pending_updates
            .keys()
            .copied()
            .collect::<HashSet<_>>(),
        expected.iter().map(|(id, _, _)| *id).collect()
    );
    let mut last_id = first.id;
    for (id, adds, removes) in expected {
        let update = server.next_mutate().await;
        assert_eq!(update.id, id);
        assert!(update.id > last_id);
        last_id = update.id;
        assert_eq!(
            update
                .adds
                .iter()
                .map(|add| add.topic.as_ref().unwrap().topic.clone())
                .collect::<Vec<_>>(),
            adds.iter()
                .map(|name| group_topic(*name).cloned_vec())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            update
                .removes
                .iter()
                .map(|topic| topic.topic.clone())
                .collect::<Vec<_>>(),
            removes
                .iter()
                .map(|name| group_topic(*name).cloned_vec())
                .collect::<Vec<_>>()
        );
        assert!(update.adds.is_empty() || update.removes.is_empty());
        assert!(update.adds.len() + update.removes.len() <= task.ledger.chunk_cap);
        assert!(update.encoded_len() <= task.ledger.chunk_bytes);
        server.ack_empty(id);
        task.ledger.applied(
            id,
            adds.iter().map(|name| (group_topic(*name), 0)).collect(),
        );
    }
    assert!(task.ledger.pending_updates.is_empty());
}

#[xmtp_common::test(flavor = "current_thread", unwrap_try = true)]
async fn coalescing_commits_ack_ids_only_after_wire_acceptance() {
    let mut ledger = Ledger::<BackendBinding>::default();
    let (_, initial) = ledger
        .prepare_adds(vec![(group_topic(b"anchor"), 0)])
        .remove(0);
    let (api, mut server) = mock_pair();
    let wire = BidiConnection::open(&api, initial).await?;
    for index in 0..crate::queries::bidi::COMMAND_BUFFER {
        let (_, update) = ledger
            .prepare_removes(vec![group_topic(&index.to_le_bytes())])
            .remove(0);
        assert!(wire.try_mutate(update).is_ok());
    }
    let mut task = ledger_task(ledger, Outbox::default());
    task.conn = Some(wire);
    let (first_id, first) = task
        .ledger
        .prepare_adds(vec![(group_topic(b"a"), 0)])
        .remove(0);
    let (second_id, second) = task
        .ledger
        .prepare_adds(vec![(group_topic(b"b"), 0)])
        .remove(0);
    task.outbox
        .updates
        .extend([(first_id, first), (second_id, second)]);
    task.flush_outbox();
    assert_eq!(
        task.outbox
            .updates
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        vec![first_id, second_id]
    );
    assert_eq!(task.ledger.pending_updates[&first_id].adds.len(), 1);
    assert_eq!(task.ledger.pending_updates[&second_id].adds.len(), 1);

    for _ in 0..=crate::queries::bidi::COMMAND_BUFFER {
        server.next_mutate().await;
    }
    task.flush_outbox();
    assert!(task.outbox.is_empty());
    let merged = server.next_mutate().await;
    assert_eq!(merged.id, first_id);
    assert_eq!(
        merged
            .adds
            .iter()
            .map(|add| add.topic.as_ref().unwrap().topic.clone())
            .collect::<Vec<_>>(),
        vec![
            group_topic(b"a").cloned_vec(),
            group_topic(b"b").cloned_vec()
        ]
    );
    assert_eq!(task.ledger.pending_updates[&first_id].adds.len(), 2);
    assert!(!task.ledger.pending_updates.contains_key(&second_id));
}
