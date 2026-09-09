use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn a_lease_over_the_cap_splits_into_bounded_frames() {
    const CHUNK_CAP: usize = 2;
    let (transport, servers) = transport_capped(CHUNK_CAP, MAX_MUTATE_BYTES);
    let topics: Vec<(Topic, u64)> = (0..(CHUNK_CAP as u32 + 1))
        .map(|i| (group_topic(&i.to_le_bytes()), 0))
        .collect();
    let mut lease = transport.lease(topics, DEFAULT_LEASE_DEPTH).await?;

    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    let second = server.next_mutate().await;
    assert_eq!(first.adds.len(), CHUNK_CAP, "first frame is capped");
    assert_eq!(second.adds.len(), 1, "the overflow rides a second frame");
    assert_ne!(first.id, second.id, "each chunk has its own update ID");

    server.ack_empty(first.id);
    let quiet = xmtp_common::time::timeout(Duration::from_millis(200), lease.next()).await;
    assert!(
        quiet.is_err(),
        "a chunked lease must not report caught-up until its last chunk completes"
    );

    server.ack_empty(second.id);
    assert!(
        matches!(recv(&mut lease).await, Some(LeaseEvent::CatchUpComplete)),
        "the last chunk's completion catches the lease up"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_reconnect_resume_over_the_cap_splits_into_bounded_frames() {
    const CHUNK_CAP: usize = 2;
    let (transport, servers) = transport_capped(CHUNK_CAP, MAX_MUTATE_BYTES);
    let topics: Vec<(Topic, u64)> = (0..(CHUNK_CAP as u32 + 1))
        .map(|i| (group_topic(&i.to_le_bytes()), 0))
        .collect();
    let mut lease = transport.lease(topics, DEFAULT_LEASE_DEPTH).await?;

    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    let second = server.next_mutate().await;
    server.ack_empty(first.id);
    server.ack_empty(second.id);
    assert!(matches!(
        recv(&mut lease).await,
        Some(LeaseEvent::CatchUpComplete)
    ));

    drop(server);
    let mut reconnect = wait_for_server(&servers).await;
    let resume_a = reconnect.next_mutate().await;
    let resume_b = reconnect.next_mutate().await;
    assert_eq!(resume_a.adds.len(), CHUNK_CAP, "resume frame is capped");
    assert_eq!(
        resume_b.adds.len(),
        1,
        "the overflow rides a second resume frame"
    );
    assert_ne!(
        resume_a.id, resume_b.id,
        "each resume chunk has its own update ID"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn an_over_cap_lease_still_catching_up_survives_a_wire_death() {
    let (transport, servers) = transport_capped(2, usize::MAX);
    let topics: Vec<(Topic, u64)> = (0..3u32)
        .map(|i| (group_topic(&i.to_le_bytes()), 0))
        .collect();
    let mut lease = transport.lease(topics, DEFAULT_LEASE_DEPTH).await?;

    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    let second = server.next_mutate().await;
    assert_eq!(first.adds.len() + second.adds.len(), 3);
    server.ack_empty(first.id);
    drop(server);

    let mut reconnect = wait_for_server(&servers).await;
    let resume_a = reconnect.next_mutate().await;
    let resume_b = reconnect.next_mutate().await;
    assert_eq!(
        resume_a.adds.len() + resume_b.adds.len(),
        3,
        "the whole lease is re-issued after the death, not just the unfinished chunk"
    );
    for id in [resume_a.id, resume_b.id] {
        assert!(
            id != first.id && id != second.id,
            "reopened chunks carry fresh update IDs"
        );
    }

    reconnect.ack_empty(resume_a.id);
    let quiet = xmtp_common::time::timeout(Duration::from_millis(200), lease.next()).await;
    assert!(
        quiet.is_err(),
        "a re-chunked lease stays catching-up until its LAST reopened chunk completes"
    );
    reconnect.ack_empty(resume_b.id);
    assert!(matches!(
        recv(&mut lease).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn an_over_cap_lease_survives_suspend_and_resume() {
    let (transport, servers) = transport_capped(2, usize::MAX);
    let topics: Vec<(Topic, u64)> = (0..3u32)
        .map(|i| (group_topic(&i.to_le_bytes()), 0))
        .collect();
    let mut lease = transport.lease(topics, DEFAULT_LEASE_DEPTH).await?;

    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    let second = server.next_mutate().await;
    server.ack_empty(first.id);
    server.ack_empty(second.id);
    assert!(matches!(
        recv(&mut lease).await,
        Some(LeaseEvent::CatchUpComplete)
    ));

    transport.suspend().await?;
    drop(server);
    let mut resume = tokio::spawn({
        let transport = transport.clone();
        async move { transport.resume().await }
    });

    let mut reconnect = wait_for_server(&servers).await;
    let resume_a = reconnect.next_mutate().await;
    let resume_b = reconnect.next_mutate().await;
    assert_eq!(
        resume_a.adds.len() + resume_b.adds.len(),
        3,
        "the whole lease is re-subscribed on resume, chunked into bounded frames"
    );
    reconnect.ack_empty(resume_a.id);
    let pending = xmtp_common::time::timeout(Duration::from_millis(200), &mut resume).await;
    assert!(
        pending.is_err(),
        "resume() must not resolve until every resumed chunk is caught up"
    );
    reconnect.ack_empty(resume_b.id);
    xmtp_common::time::timeout(WAIT, resume).await?.unwrap()?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_lease_over_the_byte_budget_splits_into_bounded_frames() {
    let per_topic = topic_wire_cost(&group_topic(&0u32.to_le_bytes()));
    let (transport, servers) = transport_capped(usize::MAX, 2 * per_topic);
    let topics: Vec<(Topic, u64)> = (0..5u32)
        .map(|i| (group_topic(&i.to_le_bytes()), 0))
        .collect();
    let _lease = transport.lease(topics, DEFAULT_LEASE_DEPTH).await?;

    let mut server = take_server(&servers);
    let mut subscribed = 0;
    while subscribed < 5 {
        let mutate = server.next_mutate().await;
        assert!(
            !mutate.adds.is_empty() && mutate.adds.len() <= 2,
            "a byte-budgeted frame holds 1–2 adds, got {}",
            mutate.adds.len()
        );
        subscribed += mutate.adds.len();
    }
    assert_eq!(subscribed, 5, "every topic is subscribed across the frames");
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_mass_unsubscribe_chunks_the_removes_update() {
    let (transport, servers) = transport_capped(2, usize::MAX);
    let keeper = transport
        .lease(vec![(group_topic(b"keep"), 0)], DEFAULT_LEASE_DEPTH)
        .await?;
    let mut server = take_server(&servers);
    let k = server.next_mutate().await;
    server.ack_empty(k.id);

    let big = transport
        .lease(
            (0..3u32)
                .map(|i| (group_topic(&i.to_le_bytes()), 0))
                .collect(),
            DEFAULT_LEASE_DEPTH,
        )
        .await?;
    let a = server.next_mutate().await;
    let b = server.next_mutate().await;
    server.ack_empty(a.id);
    server.ack_empty(b.id);

    drop(big);
    let mut removed = 0;
    while removed < 3 {
        let mutate = server.next_mutate().await;
        if mutate.removes.is_empty() {
            continue; // ignore a trailing add/echo frame
        }
        assert!(
            mutate.removes.len() <= 2,
            "each removes frame respects the cap, got {}",
            mutate.removes.len()
        );
        removed += mutate.removes.len();
    }
    assert_eq!(removed, 3, "every dropped topic is unsubscribed");
    drop(keeper);
}

/// P3-STR-013, P3-TST-002: a full wire refuses one more topic without a frame.
#[xmtp_common::test(unwrap_try = true)]
async fn a_lease_cannot_push_the_wire_past_the_topic_limit() {
    let (transport, servers) = transport();
    let limit = xmtp_configuration::BACKEND_DEFAULT_MAX_STREAM_TOPICS;
    let topics = (0..limit)
        .map(|id| {
            let mut group = [0u8; 16];
            group[..8].copy_from_slice(&(id as u64).to_le_bytes());
            (group_topic(&group), 0)
        })
        .collect();
    let _full = transport.lease(topics, 8).await?;
    let mut server = take_server(&servers);
    let initial = server.next_mutate().await;
    assert_eq!(initial.adds.len(), limit);
    let extra = group_topic(&[0xff; 16]);
    assert!(matches!(
        transport.lease(vec![(extra, 0)], 8).await,
        Err(TransportError::TooManyTopics)
    ));
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(100), server.from_client.recv())
            .await
            .is_err(),
        "a refused lease must send no update"
    );
    assert!(
        servers.lock().unwrap().is_empty(),
        "a refused lease must open no extra wire"
    );
}
