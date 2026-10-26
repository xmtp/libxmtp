use super::*;

fn strict_limits() -> MutateLimits {
    MutateLimits::from_limits(&xmtp_configuration::LimitsConfiguration {
        max_update_frames_per_second: 1,
        max_update_burst: 1,
        ..Default::default()
    })
}

fn transport_within(mutate: MutateLimits) -> (BidiTransport<BackendBinding>, Servers) {
    let servers: Servers = Arc::default();
    let sink = servers.clone();
    let transport = BidiTransport::new_within(
        move |initial| {
            let (api, server) = mock_pair();
            sink.lock().unwrap().push(server);
            async move {
                BidiConnection::open(&api, initial)
                    .await
                    .map_err(OpenError::new)
            }
        },
        false,
        mutate,
    );
    (transport, servers)
}

// verifies: CONF-093
#[xmtp_common::test(unwrap_try = true)]
async fn strict_update_limits_pace_a_shared_wire_rejoin() {
    let (transport, servers) = transport_within(strict_limits());
    let shared = group_topic(b"shared");
    let other = group_topic(b"other");
    let _first = transport.lease(vec![(shared.clone(), 40)], 8).await?;
    let mut server = take_server(&servers);
    let opening = server.next_mutate().await;
    let mut backend_budget = Bucket::new(1, 1);
    assert!(backend_budget.take(), "backend accepts the opening Update");
    server.ack_empty(opening.id);

    let _second = transport
        .lease(vec![(shared.clone(), 7), (other.clone(), 0)], 8)
        .await?;
    let remove = server.next_mutate().await;
    assert_eq!(remove.removes.len(), 1);
    assert!(remove.adds.is_empty());
    // The backend counts arrival after a radio or scheduling delay, while the
    // client spent its token when it queued the frame.
    xmtp_common::time::sleep(Duration::from_millis(200)).await;
    assert!(backend_budget.take(), "backend accepts the delayed remove");
    server.ack_empty(remove.id);

    let add = server.next_mutate().await;
    assert!(add.adds.iter().any(|entry| {
        entry
            .topic
            .as_ref()
            .is_some_and(|topic| topic.topic == other.cloned_vec())
    }));
    assert!(backend_budget.take(), "backend accepts the paced add");
    server.ack_empty(add.id);

    drop(server);
    let mut reopened = wait_for_server(&servers).await;
    let replay = reopened.next_mutate().await;
    let mut backend_budget = Bucket::new(1, 1);
    assert!(backend_budget.take(), "backend accepts the reopened Update");
    assert!(replay.id > add.id);
    reopened.ack_empty(replay.id);

    let third = group_topic(b"third");
    let _third = transport.lease(vec![(third.clone(), 0)], 8).await?;
    let add = reopened.next_mutate().await;
    assert!(add.adds.iter().any(|entry| {
        entry
            .topic
            .as_ref()
            .is_some_and(|topic| topic.topic == third.cloned_vec())
    }));
    assert!(backend_budget.take(), "backend accepts an add after reopen");
    reopened.ack_empty(add.id);
}

#[xmtp_common::test(unwrap_try = true)]
async fn open_wire_span_charges_the_snapshot_update_burst() {
    let ledger = Ledger {
        mutate: strict_limits(),
        ..Ledger::default()
    };
    let mut task = ledger_task(ledger, Outbox::default());
    task.open_wire_span();
    assert!(task.update_budget.wait() > Duration::from_secs(1));
}
