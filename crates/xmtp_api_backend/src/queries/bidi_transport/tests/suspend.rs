use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn suspend_half_closes_and_resume_completes_at_catch_up() {
    let (transport, servers) = transport();
    let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack(first.id, vec![(group_topic(b"g1"), 1)]);
    server.send(messages(vec![group_msg(1, b"g1")], vec![]));
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::GroupMessages(_))
    ));
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));

    transport.suspend().await?;
    server.request_stream_ended().await;

    let resumed = tokio::spawn({
        let transport = transport.clone();
        async move { transport.resume().await }
    });
    let mut second = wait_for_server(&servers).await;
    let resume = second.next_mutate().await;
    assert_eq!(resume.adds.len(), 1);
    assert_eq!(
        resume.adds[0].cursor.as_ref().unwrap().sequence_id,
        0,
        "resume at the meet of the kept position and floor"
    );
    second.ack(resume.id, vec![(group_topic(b"g1"), 2)]);
    xmtp_common::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !resumed.is_finished(),
        "Applied alone does not meet the target"
    );

    second.send(messages(
        vec![group_msg(1, b"g1"), group_msg(2, b"g1")],
        vec![],
    ));
    match recv(&mut alpha).await {
        Some(LeaseEvent::GroupMessages(got)) => {
            assert_eq!(got, vec![group_msg(2, b"g1")])
        }
        _ => panic!("the lease must survive suspend/resume invisibly"),
    }
    xmtp_common::time::timeout(WAIT, resumed).await?.unwrap()?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_resumes_join_one_catch_up_update() {
    let (transport, servers) = transport();
    let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack_empty(first.id);
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));

    transport.suspend().await?;
    server.request_stream_ended().await;

    let resume_a = tokio::spawn({
        let transport = transport.clone();
        async move { transport.resume().await }
    });
    let resume_b = tokio::spawn({
        let transport = transport.clone();
        async move { transport.resume().await }
    });

    let mut second = wait_for_server(&servers).await;
    let resume = second.next_mutate().await;
    second.ack_empty(resume.id);
    xmtp_common::time::timeout(WAIT, resume_a).await?.unwrap()?;
    xmtp_common::time::timeout(WAIT, resume_b).await?.unwrap()?;
    assert!(
        servers.lock().unwrap().is_empty(),
        "the second resume must join the pending update"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn suspended_transport_stays_off_the_network() {
    let (transport, servers) = transport();
    let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack_empty(first.id);
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
    transport.suspend().await?;
    server.request_stream_ended().await;
    let mut beta = transport.lease(vec![(group_topic(b"g2"), 0)], 8).await?;
    xmtp_common::time::sleep(Duration::from_millis(400)).await;
    assert!(
        servers.lock().unwrap().is_empty(),
        "suspended must mean off the network"
    );
    let resumed = tokio::spawn({
        let transport = transport.clone();
        async move { transport.resume().await }
    });
    let mut second = wait_for_server(&servers).await;
    let resume = second.next_mutate().await;
    let got: HashSet<_> = resume
        .adds
        .iter()
        .map(|add| add.topic.as_ref().unwrap().topic.clone())
        .collect();
    assert_eq!(
        got,
        HashSet::from([
            group_topic(b"g1").cloned_vec(),
            group_topic(b"g2").cloned_vec()
        ]),
        "one update covers all leased topics"
    );
    assert_eq!(resume.adds.len(), 2);
    assert!(
        servers.lock().unwrap().is_empty(),
        "one wire serves both leases"
    );
    second.ack(resume.id, vec![(group_topic(b"g2"), 1)]);
    xmtp_common::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !resumed.is_finished(),
        "resume must wait for the parked lease's target too"
    );
    second.send(messages(vec![group_msg(1, b"g2")], vec![]));
    match recv(&mut beta).await {
        Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(1, b"g2")]),
        _ => panic!("beta must receive its catch-up"),
    }
    assert!(matches!(
        recv(&mut beta).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
    xmtp_common::time::timeout(WAIT, resumed).await?.unwrap()?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_born_suspended_transport_parks_the_first_lease() {
    let (transport, servers) = transport_born(true);

    let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    xmtp_common::time::sleep(Duration::from_millis(400)).await;
    assert!(
        servers.lock().unwrap().is_empty(),
        "a born-suspended transport must keep its first lease parked"
    );

    let resumed = tokio::spawn({
        let transport = transport.clone();
        async move { transport.resume().await }
    });
    let mut server = wait_for_server(&servers).await;
    let wave = server.next_mutate().await;
    assert_eq!(
        wave.adds
            .iter()
            .map(|add| &add.topic.as_ref().unwrap().topic)
            .collect::<Vec<_>>(),
        vec![&group_topic(b"g1").cloned_vec()],
        "the parked lease's adds open the wire"
    );
    server.ack_empty(wave.id);
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
    xmtp_common::time::timeout(WAIT, resumed).await?.unwrap()?;

    server.send(messages(vec![group_msg(1, b"g1")], vec![]));
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::GroupMessages(_))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn dropping_the_last_lease_settles_resume_waiters() {
    let (transport, servers) = transport();
    let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack_empty(first.id);
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));

    transport.suspend().await?;
    server.request_stream_ended().await;

    let resumed = tokio::spawn({
        let transport = transport.clone();
        async move { transport.resume().await }
    });
    let mut second = wait_for_server(&servers).await;
    let _resume = second.next_mutate().await;
    drop(alpha);
    xmtp_common::time::timeout(WAIT, resumed).await?.unwrap()?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn suspend_preempts_a_stuck_dial() {
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
                    if n == 0 {
                        std::future::pending::<()>().await;
                        unreachable!("the hung dial never resolves");
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

    let leased = tokio::spawn({
        let transport = transport.clone();
        async move { transport.lease(vec![(group_topic(b"g1"), 0)], 8).await }
    });
    xmtp_common::wait_for_ge(|| async { dials.load(Ordering::SeqCst) }, 1).await?;

    xmtp_common::time::timeout(WAIT, transport.suspend()).await??;
    let mut alpha = xmtp_common::time::timeout(WAIT, leased).await?.unwrap()?;

    let resumed = tokio::spawn({
        let transport = transport.clone();
        async move { transport.resume().await }
    });
    let mut server = wait_for_server(&servers).await;
    let resume = server.next_mutate().await;
    server.ack_empty(resume.id);
    xmtp_common::time::timeout(WAIT, resumed).await?.unwrap()?;
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_preempting_suspend_outranks_a_deferred_resume() {
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
                    if n == 0 {
                        std::future::pending::<()>().await;
                        unreachable!("the hung dial never resolves");
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

    let leased = tokio::spawn({
        let transport = transport.clone();
        async move { transport.lease(vec![(group_topic(b"g1"), 0)], 8).await }
    });
    xmtp_common::wait_for_ge(|| async { dials.load(Ordering::SeqCst) }, 1).await?;

    let first_resume = tokio::spawn({
        let transport = transport.clone();
        async move { transport.resume().await }
    });
    xmtp_common::time::sleep(Duration::from_millis(20)).await;
    xmtp_common::time::timeout(WAIT, transport.suspend()).await??;
    xmtp_common::time::timeout(WAIT, first_resume)
        .await?
        .unwrap()?;
    let mut alpha = xmtp_common::time::timeout(WAIT, leased).await?.unwrap()?;

    xmtp_common::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        dials.load(Ordering::SeqCst),
        1,
        "a resume deferred before the suspend must not redial"
    );

    let second_resume = tokio::spawn({
        let transport = transport.clone();
        async move { transport.resume().await }
    });
    let mut server = wait_for_server(&servers).await;
    let resume = server.next_mutate().await;
    server.ack_empty(resume.id);
    xmtp_common::time::timeout(WAIT, second_resume)
        .await?
        .unwrap()?;
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn resume_with_nothing_to_do_resolves_immediately() {
    let (transport, servers) = transport();
    transport.suspend().await?;
    xmtp_common::time::timeout(WAIT, transport.resume()).await??;

    let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack_empty(first.id);
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));

    xmtp_common::time::timeout(WAIT, transport.resume()).await??;
    server.send(messages(vec![group_msg(1, b"g1")], vec![]));
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::GroupMessages(_))
    ));
    assert!(
        servers.lock().unwrap().is_empty(),
        "resume on a live wire must not open a second one"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn lease_during_a_dead_wire_rides_the_resume_open() {
    let (transport, servers) = transport();
    let _alpha = transport.lease(vec![(group_topic(b"g1"), 3)], 8).await?;
    let server = take_server(&servers);
    drop(server);

    let mut beta = transport.lease(vec![(group_topic(b"g2"), 7)], 8).await?;
    let mut second = wait_for_server(&servers).await;

    let mut adds: Vec<(Vec<u8>, u64)> = Vec::new();
    let mut waves = Vec::new();
    while adds.len() < 2 {
        let mutate = second.next_mutate().await;
        waves.push(mutate.id);
        adds.extend(mutate.adds.iter().map(|add| {
            (
                add.topic.as_ref().unwrap().topic.clone(),
                add.cursor.as_ref().unwrap().sequence_id,
            )
        }));
    }
    adds.sort();
    let mut expected = vec![
        (group_topic(b"g1").cloned_vec(), 3),
        (group_topic(b"g2").cloned_vec(), 7),
    ];
    expected.sort();
    assert_eq!(adds, expected);
    assert!(
        servers.lock().unwrap().is_empty(),
        "one wire serves everyone"
    );

    for wave in waves {
        second.ack_empty(wave);
    }
    assert!(matches!(
        recv(&mut beta).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
}
