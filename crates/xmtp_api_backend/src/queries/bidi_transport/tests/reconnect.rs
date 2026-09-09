use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn wire_death_reopens_from_lease_floors() {
    let (transport, servers) = transport();
    let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack(first.id, vec![(group_topic(b"g1"), 2)]);
    server.send(messages(
        vec![group_msg(1, b"g1"), group_msg(2, b"g1")],
        vec![],
    ));
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::GroupMessages(_))
    ));
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));

    drop(server); // the wire dies mid-stream

    let mut second = wait_for_server(&servers).await;
    let resume = second.next_mutate().await;
    assert_eq!(resume.adds.len(), 1, "one resume update");
    assert_eq!(
        resume.adds[0].topic.as_ref().unwrap().topic,
        group_topic(b"g1").cloned_vec()
    );
    assert_eq!(
        resume.adds[0].cursor.as_ref().unwrap().sequence_id,
        0,
        "resume at the meet of last-seen and the lease floor"
    );

    second.ack(resume.id, vec![(group_topic(b"g1"), 3)]);
    second.send(messages(
        vec![
            group_msg(1, b"g1"),
            group_msg(2, b"g1"),
            group_msg(3, b"g1"),
        ],
        vec![],
    ));
    match recv(&mut alpha).await {
        Some(LeaseEvent::GroupMessages(got)) => {
            assert_eq!(got, vec![group_msg(3, b"g1")])
        }
        _ => panic!("the lease must survive the flap and see only new messages"),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn command_traffic_does_not_postpone_the_reconnect() {
    let (transport, servers) = transport();
    let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack_empty(first.id);
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));

    drop(server); // the wire dies

    let churn = {
        let transport = transport.clone();
        tokio::spawn(async move {
            loop {
                let lease = transport.lease(vec![(group_topic(b"g9"), 0)], 4).await;
                drop(lease);
                xmtp_common::time::sleep(Duration::from_millis(40)).await;
            }
        })
    };
    let _second = wait_for_server(&servers).await;
    churn.abort();
}

#[xmtp_common::test(unwrap_try = true)]
async fn half_open_wire_is_reaped_and_reconnected() {
    let (transport, servers) = transport();
    let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack(first.id, vec![(group_topic(b"g1"), 1)]);
    server.send(started(150));
    server.send(messages(vec![group_msg(1, b"g1")], vec![]));
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::GroupMessages(_))
    ));
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));

    server.next_ping().await;

    let mut second = wait_for_server(&servers).await;
    let resume = second.next_mutate().await;
    assert_eq!(resume.adds.len(), 1);
    assert_eq!(resume.adds[0].cursor.as_ref().unwrap().sequence_id, 0);

    second.ack(resume.id, vec![(group_topic(b"g1"), 2)]);
    second.send(messages(vec![group_msg(2, b"g1")], vec![]));
    match recv(&mut alpha).await {
        Some(LeaseEvent::GroupMessages(got)) => {
            assert_eq!(got, vec![group_msg(2, b"g1")])
        }
        _ => panic!("the lease must survive a half-open wire invisibly"),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn reconnect_folds_a_caught_up_holders_floor_when_nothing_was_delivered() {
    let (transport, servers) = transport();
    let topic = group_topic(b"g1");
    let mut gamma = transport.lease(vec![(topic.clone(), 5)], 8).await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack_empty(first.id);
    assert!(matches!(
        recv(&mut gamma).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
    drop(server); // the wire dies...

    let _alpha = transport.lease(vec![(topic.clone(), 50)], 8).await?;
    let mut second = wait_for_server(&servers).await;
    let reissue = second.next_mutate().await;
    assert_eq!(
        reissue
            .adds
            .iter()
            .map(|add| (
                add.topic.as_ref().unwrap().topic.clone(),
                add.cursor.as_ref().unwrap().sequence_id
            ))
            .collect::<Vec<_>>(),
        vec![(topic.cloned_vec(), 5)],
        "the re-add anchors at the caught-up holder's floor"
    );

    second.ack(reissue.id, vec![(topic.clone(), 30)]);
    second.send(messages(vec![group_msg(30, b"g1")], vec![]));
    match recv(&mut gamma).await {
        Some(LeaseEvent::GroupMessages(got)) => {
            assert_eq!(got, vec![group_msg(30, b"g1")])
        }
        _ => panic!("gamma expected its outage gap"),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn deref_during_a_dead_wire_keeps_the_topic_off_the_reconnect() {
    let (transport, servers) = transport();
    let _alpha = transport.lease(vec![(group_topic(b"g1"), 3)], 8).await?;
    let beta = transport.lease(vec![(group_topic(b"g2"), 7)], 8).await?;
    let server = take_server(&servers);
    drop(server); // the wire dies

    drop(beta); // deref lands while there is no wire to send a remove on

    let mut second = wait_for_server(&servers).await;
    let reissue = second.next_mutate().await;
    assert_eq!(
        reissue
            .adds
            .iter()
            .map(|add| (
                add.topic.as_ref().unwrap().topic.clone(),
                add.cursor.as_ref().unwrap().sequence_id
            ))
            .collect::<Vec<_>>(),
        vec![(group_topic(b"g1").cloned_vec(), 3)],
        "the dropped lease's topic must not ride the reconnect"
    );
    assert!(
        reissue.removes.is_empty(),
        "nothing to remove on a fresh wire"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn open_failure_surfaces_and_registers_nothing() {
    let transport = BidiTransport::<BackendBinding>::new(
        |_initial| async { Err(OpenError::unretryable(Refused)) },
        false,
    );
    let denied = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await;
    assert!(matches!(denied, Err(TransportError::Open(_))));
    let again = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await;
    assert!(matches!(again, Err(TransportError::Open(_))));
}

#[xmtp_common::test(unwrap_try = true)]
async fn unretryable_reconnect_closes_every_lease() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let servers: Servers = Arc::default();
    let dials = Arc::new(AtomicUsize::new(0));
    let transport: BidiTransport<BackendBinding> = {
        let sink = servers.clone();
        let dials = dials.clone();
        BidiTransport::new(
            move |initial| {
                let n = dials.fetch_add(1, Ordering::SeqCst);
                let sink = sink.clone();
                async move {
                    if n > 0 {
                        return Err(OpenError::new(Refused));
                    }
                    let (api, server) = mock_pair();
                    sink.lock().unwrap().push(server);
                    BidiConnection::open(&api, initial)
                        .await
                        .map_err(OpenError::new)
                }
            },
            false,
        )
    };

    let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack_empty(first.id);
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));

    drop(server); // the wire dies; the reconnect dial is refused

    assert!(
        recv(&mut alpha).await.is_none(),
        "an unretryable reconnect must end every lease, not redial forever"
    );
    assert_eq!(dials.load(Ordering::SeqCst), 2, "no dial after the refusal");
    let denied = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await;
    assert!(matches!(denied, Err(TransportError::Closed)));
}
